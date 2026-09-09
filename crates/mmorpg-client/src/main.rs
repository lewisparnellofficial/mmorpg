//! Minimal interactive Linux client technology spike.
//!
//! Bevy owns presentation and input. A dedicated TCP worker owns all socket
//! I/O so render and input systems never wait on the server. Typed command
//! frames and typed server messages feed the renderer-independent presentation
//! model.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::window::{PresentMode, PrimaryWindow, WindowFocused};
use mmorpg_client_adapter::apply_wire_message;
#[cfg(test)]
use mmorpg_client_adapter::{
    apply_event as apply_presentation_event, apply_snapshot as apply_presentation_snapshot,
};
use mmorpg_client_model::{ClientEntity, ClientWorld};
use mmorpg_client_protocol::{EntityId, NpcKind, SnapshotAssembler};
#[cfg(test)]
use mmorpg_client_protocol::{ServerEvent, ServerLine, Snapshot, decode_server_line};
use mmorpg_client_secure_input::{
    ActionId, AddonId, Generation, NativePress, NodeId, SecureInputRegistry,
};
use mmorpg_client_session::{Session as TypedSession, SessionInput, SessionOutput, SessionState};
use mmorpg_content::{ItemId, QuestId, item_definition, starter_catalog};
use mmorpg_wire::{
    CharacterSummary, ClientCommand as WireCommand, DecodeError as WireDecodeError, Envelope,
    MessageKind, RoleCode, SequencedServerMessage, ServerMessage, decode_one,
};
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use ui_scripting_spike::{AddonPolicy, AddonRunner};

const FIELD_SIZE: Vec2 = Vec2::new(40.0, 30.0);
const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:4000";
const DEV_AUTH_TOKEN: &str = "dev-local";
const MOVEMENT_STEP: f32 = 0.35;
const MOVEMENT_REPEAT_SECONDS: f32 = 0.05;
const MAX_LOG_LINES: usize = 6;
const COMMAND_QUEUE_CAPACITY: usize = 64;
const MAX_DEFERRED_COMMANDS: usize = 32;
const MAX_OUTGOING_LINES: usize = 64;
const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
const FRAME_TIME_SAMPLE_CAPACITY: usize = 6000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderBackendChoice {
    Automatic,
    Vulkan,
    Gl,
}

impl RenderBackendChoice {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Automatic),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "gl" | "opengl" | "gles" => Some(Self::Gl),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Automatic => "auto",
            Self::Vulkan => "vulkan",
            Self::Gl => "gl",
        }
    }
}

fn render_plugin(choice: RenderBackendChoice) -> RenderPlugin {
    let Some(backends) = (match choice {
        RenderBackendChoice::Automatic => None,
        RenderBackendChoice::Vulkan => Some(Backends::VULKAN),
        RenderBackendChoice::Gl => Some(Backends::GL),
    }) else {
        return RenderPlugin::default();
    };
    RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
            backends: Some(backends),
            ..default()
        })),
        ..default()
    }
}

#[derive(Component)]
struct StarterNpc {
    id: EntityId,
}

#[derive(Component)]
struct PlayerMarker;

#[derive(Component)]
struct StatusText;

#[derive(Resource, Clone)]
struct NpcPresentationAssets {
    mesh: Handle<Mesh>,
    vendor_material: Handle<StandardMaterial>,
    enemy_material: Handle<StandardMaterial>,
}

#[derive(Resource, Debug)]
struct ClientState {
    server_address: String,
    connection_status: String,
    player_id: Option<EntityId>,
    connected_role: Option<RoleCode>,
    available_characters: Vec<CharacterSummary>,
    selected_character_id: Option<u64>,
    target_cursor: usize,
    logs: VecDeque<String>,
    snapshot: SnapshotAssembler,
    presentation: ClientWorld,
}

impl ClientState {
    fn new(server_address: String) -> Self {
        Self {
            server_address,
            connection_status: "connecting".to_owned(),
            player_id: None,
            connected_role: None,
            available_characters: Vec::new(),
            selected_character_id: None,
            target_cursor: 0,
            logs: VecDeque::from(["WASD move  Tab target  Space attack".to_owned()]),
            snapshot: SnapshotAssembler::new(),
            presentation: ClientWorld::default(),
        }
    }

    fn log(&mut self, message: impl Into<String>) {
        if self.logs.len() == MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(message.into());
    }
}

#[derive(Debug)]
enum ClientCommand {
    SelectCharacter {
        character_id: u64,
    },
    Move {
        dx: f32,
        dy: f32,
    },
    Target(EntityId),
    Attack,
    Taunt,
    Heal(EntityId),
    ListVendor(EntityId),
    ListQuestOffers(EntityId),
    BuyItem {
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    },
    AcceptQuest {
        npc_id: EntityId,
        quest_id: QuestId,
    },
    TurnInQuest {
        npc_id: EntityId,
        quest_id: QuestId,
    },
    Loot(EntityId),
    InvitePartyMember(EntityId),
    AcceptPartyInvite(u64),
}

#[derive(Debug)]
enum NetworkEvent {
    Status(String),
    ServerMessage(ServerMessage),
}

#[derive(Resource)]
struct NetworkBridge {
    command_tx: SyncSender<ClientCommand>,
    event_rx: Arc<Mutex<Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
struct MovementRepeat(Timer);

#[derive(Clone, Resource)]
struct ScriptedUiPresentation {
    default_label: String,
    addon_label: String,
    default_node_id: u64,
    addon_node_id: u64,
}

fn build_scripted_ui_presentation(
    addon_root: Option<&Path>,
) -> Result<ScriptedUiPresentation, String> {
    if let Some(addon_root) = addon_root {
        let repository = ui_scripting_spike::PackageRepository::new(addon_root);
        let known_capabilities = std::collections::BTreeSet::new();
        let mut runners = repository
            .load_all(AddonPolicy::default(), &known_capabilities)
            .map_err(|error| format!("cannot load addon repository: {error}"))?;
        if runners.len() < 2 {
            return Err("addon repository must contain at least two packages".to_owned());
        }
        println!(
            "SCRIPTED_UI source=repository root={}",
            addon_root.display()
        );
        let mut default_ui = runners.remove(0);
        let mut addon = runners.remove(0);
        return scripted_ui_from_runners(&mut default_ui, &mut addon);
    }
    let default_source = r#"
        local panel = ui.create_panel("Default UI secure attack")
        ui.set_position(panel, 12, 12)
    "#;
    let addon_source = r#"
        local panel = ui.create_panel("Addon secure attack presentation")
        ui.set_position(panel, 12, 42)
    "#;
    let mut default_ui = AddonRunner::load("default-ui", default_source, AddonPolicy::default())
        .expect("default UI addon must load during client startup");
    let mut addon = AddonRunner::load("starter-addon", addon_source, AddonPolicy::default())
        .expect("ordinary UI addon must load during client startup");
    scripted_ui_from_runners(&mut default_ui, &mut addon)
}

fn scripted_ui_from_runners(
    default_ui: &mut AddonRunner,
    addon: &mut AddonRunner,
) -> Result<ScriptedUiPresentation, String> {
    let default_node = default_ui
        .snapshot()
        .nodes
        .first()
        .ok_or_else(|| "default UI addon must describe one panel".to_owned())
        .cloned()?;
    let addon_node = addon
        .snapshot()
        .nodes
        .first()
        .ok_or_else(|| "ordinary UI addon must describe one panel".to_owned())
        .cloned()?;
    default_ui
        .secure_input(default_node.id, "basic_attack")
        .map_err(|error| format!("default UI secure action rejected: {error}"))?;
    addon
        .secure_input(addon_node.id, "basic_attack")
        .map_err(|error| format!("addon secure action rejected: {error}"))?;
    println!(
        "SCRIPTED_UI default_node={} addon_node={} action=basic_attack",
        default_node.id, addon_node.id
    );
    Ok(ScriptedUiPresentation {
        default_label: default_node.text,
        addon_label: addon_node.text,
        default_node_id: default_node.id,
        addon_node_id: addon_node.id,
    })
}

#[derive(Resource)]
struct AcceptanceSmoke {
    enabled: bool,
    step: usize,
    timer: Timer,
}

#[derive(Resource)]
struct FrameTimeStats {
    enabled: bool,
    collecting: bool,
    samples_ms: Vec<f64>,
    warmup_timer: Timer,
    report_timer: Timer,
}

impl FrameTimeStats {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            collecting: false,
            samples_ms: Vec::with_capacity(FRAME_TIME_SAMPLE_CAPACITY),
            report_timer: Timer::from_seconds(5.0, TimerMode::Once),
            warmup_timer: Timer::from_seconds(2.0, TimerMode::Once),
        }
    }

    fn report(&mut self) {
        if self.samples_ms.is_empty() {
            println!("frame_time_stats samples=0");
            return;
        }
        self.samples_ms.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            let index = ((self.samples_ms.len() - 1) as f64 * fraction).round() as usize;
            self.samples_ms[index]
        };
        let max = *self.samples_ms.last().expect("non-empty frame samples");
        println!(
            "frame_time_stats samples={} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3}",
            self.samples_ms.len(),
            percentile(0.50),
            percentile(0.95),
            percentile(0.99),
            max,
        );
    }
}

impl AcceptanceSmoke {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            step: 0,
            timer: Timer::from_seconds(0.25, TimerMode::Repeating),
        }
    }
}

#[derive(Resource)]
struct SecureInputState {
    registry: SecureInputRegistry,
    default_addon: AddonId,
    default_node: NodeId,
    node_generation: Generation,
    attack_action: ActionId,
    next_physical_event_id: u64,
    secure_attack_min: Vec2,
    secure_attack_max: Vec2,
}

impl SecureInputState {
    fn new(scripted_ui: &ScriptedUiPresentation) -> Self {
        let default_addon = AddonId::new(1).expect("default addon ID cannot be zero");
        let default_node = NodeId::new(scripted_ui.default_node_id)
            .expect("scripted default action node cannot be zero");
        let addon = AddonId::new(2).expect("addon ID cannot be zero");
        let addon_node = NodeId::new(scripted_ui.addon_node_id)
            .expect("scripted addon action node cannot be zero");
        let node_generation = Generation::new(1).expect("default node generation cannot be zero");
        let attack_action = ActionId::new(1).expect("attack action ID cannot be zero");
        let mut registry = SecureInputRegistry::new();
        registry
            .register(default_addon, default_node, node_generation, attack_action)
            .expect("default secure attack binding must be valid");
        registry
            .register(addon, addon_node, node_generation, attack_action)
            .expect("addon secure attack binding must be valid");
        Self {
            registry,
            default_addon,
            default_node,
            node_generation,
            attack_action,
            next_physical_event_id: 0,
            secure_attack_min: Vec2::new(12.0, 12.0),
            secure_attack_max: Vec2::new(300.0, 68.0),
        }
    }

    fn next_physical_event_id(&mut self) -> u64 {
        self.next_physical_event_id = self.next_physical_event_id.saturating_add(1).max(1);
        self.next_physical_event_id
    }

    fn refresh_default_binding(&mut self) {
        self.registry.unload_addon(self.default_addon);
        self.registry
            .register(
                self.default_addon,
                self.default_node,
                self.node_generation,
                self.attack_action,
            )
            .expect("default secure attack binding must be valid after focus change");
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let server_address = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_SERVER_ADDRESS.to_owned());
    let mut wire_address = None;
    let mut preferred_character_id = None;
    let mut addon_root = None;
    let mut acceptance_smoke = false;
    let mut frame_time_stats = false;
    let mut render_backend = RenderBackendChoice::Automatic;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--wire-address" => {
                if wire_address.is_some() {
                    eprintln!("--wire-address may only be specified once");
                    return;
                }
                wire_address = arguments.next();
                if wire_address.is_none() {
                    eprintln!("--wire-address requires an address");
                    return;
                }
            }
            "--character-id" => {
                if preferred_character_id.is_some() {
                    eprintln!("--character-id may only be specified once");
                    return;
                }
                let Some(value) = arguments.next() else {
                    eprintln!("--character-id requires a numeric character ID");
                    return;
                };
                match value.parse::<u64>() {
                    Ok(0) | Err(_) => {
                        eprintln!("--character-id must be a non-zero numeric character ID");
                        return;
                    }
                    Ok(character_id) => preferred_character_id = Some(character_id),
                }
            }
            "--acceptance-smoke" => acceptance_smoke = true,
            "--frame-time-stats" => frame_time_stats = true,
            "--addon-root" => {
                if addon_root.is_some() {
                    eprintln!("--addon-root may only be specified once");
                    return;
                }
                addon_root = arguments.next();
                if addon_root.is_none() {
                    eprintln!("--addon-root requires a package repository path");
                    return;
                }
            }
            "--render-backend" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--render-backend requires auto, vulkan, or gl");
                    return;
                };
                let Some(choice) = RenderBackendChoice::parse(&value) else {
                    eprintln!("--render-backend must be auto, vulkan, or gl (got {value})");
                    return;
                };
                render_backend = choice;
            }
            _ => {
                eprintln!("unknown argument '{argument}'");
                return;
            }
        }
    }
    let typed_address = wire_address.unwrap_or_else(|| server_address.clone());
    println!("render_backend_request={}", render_backend.label());
    let scripted_ui = match build_scripted_ui_presentation(addon_root.as_deref().map(Path::new)) {
        Ok(scripted_ui) => scripted_ui,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    if addon_root.is_none() {
        println!("SCRIPTED_UI source=built-in");
    }
    let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel();
    spawn_wire_network_worker(
        typed_address.clone(),
        preferred_character_id,
        command_rx,
        event_tx,
    );

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(render_plugin(render_backend))
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "MMORPG Client — interactive slice".to_owned(),
                        resolution: (1280, 720).into(),
                        // Prefer low-latency presentation while allowing the
                        // platform to select a supported swapchain mode.
                        present_mode: PresentMode::AutoNoVsync,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgb(0.08, 0.12, 0.18)))
        .insert_resource(ClientState::new(typed_address))
        .insert_resource(scripted_ui.clone())
        .insert_resource(NetworkBridge {
            command_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
        })
        .insert_resource(MovementRepeat(Timer::from_seconds(
            MOVEMENT_REPEAT_SECONDS,
            TimerMode::Repeating,
        )))
        .insert_resource(AcceptanceSmoke::new(acceptance_smoke))
        .insert_resource(FrameTimeStats::new(frame_time_stats))
        .insert_resource(SecureInputState::new(&scripted_ui))
        .add_systems(Startup, setup_scene)
        .add_systems(Startup, setup_ui)
        .add_systems(
            Update,
            (
                consume_network_events,
                sample_frame_time,
                acceptance_smoke_input,
                secure_window_focus,
                native_secure_pointer_input,
                keyboard_input,
                sync_authoritative_presentation,
                update_status_text,
            )
                .chain(),
        )
        .run();
}

fn sample_frame_time(time: Res<Time>, mut stats: ResMut<FrameTimeStats>) {
    if !stats.enabled {
        return;
    }
    if !stats.collecting {
        if stats.warmup_timer.tick(time.delta()).just_finished() {
            stats.collecting = true;
            stats.report_timer.reset();
        }
        return;
    }
    if stats.samples_ms.len() < FRAME_TIME_SAMPLE_CAPACITY {
        stats.samples_ms.push(time.delta_secs_f64() * 1000.0);
    }
    if stats.report_timer.tick(time.delta()).just_finished() {
        stats.report();
    }
}

fn secure_window_focus(
    mut focus_events: MessageReader<WindowFocused>,
    mut secure_input: ResMut<SecureInputState>,
) {
    for event in focus_events.read() {
        // Any native focus transition invalidates bindings minted against the
        // previous focused surface. The registry deliberately does not
        // distinguish focus gain from loss; the next presentation commit must
        // re-register a binding before it can be activated.
        secure_input.registry.focus_changed();
        if event.focused {
            secure_input.refresh_default_binding();
        }
    }
}

fn acceptance_smoke_input(
    time: Res<Time>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut smoke: ResMut<AcceptanceSmoke>,
) {
    if !smoke.enabled
        || state.player_id.is_none()
        || !smoke.timer.tick(time.delta()).just_finished()
    {
        return;
    }
    let role = state.connected_role;
    let command = match role {
        Some(RoleCode::Tank) => match smoke.step {
            0 => ClientCommand::Target(EntityId(4)),
            8..=9 => ClientCommand::InvitePartyMember(EntityId(7)),
            10 => ClientCommand::Target(EntityId(4)),
            11..=15 => ClientCommand::Taunt,
            _ => {
                smoke.step = smoke.step.saturating_add(1);
                return;
            }
        },
        Some(RoleCode::Healer) => match smoke.step {
            8 => ClientCommand::AcceptPartyInvite(1),
            15..=25 => ClientCommand::Heal(EntityId(6)),
            _ => {
                smoke.step = smoke.step.saturating_add(1);
                return;
            }
        },
        Some(RoleCode::DamageDealer) => match smoke.step {
            0 => ClientCommand::ListVendor(EntityId(1)),
            1 => ClientCommand::ListQuestOffers(EntityId(1)),
            2 => ClientCommand::BuyItem {
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 1,
            },
            3 => ClientCommand::AcceptQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
            4..=32 => ClientCommand::Move { dx: 0.35, dy: 0.0 },
            33 => ClientCommand::Target(EntityId(2)),
            34..=42 => ClientCommand::Attack,
            43 => ClientCommand::Loot(EntityId(2)),
            44 => ClientCommand::Target(EntityId(3)),
            45..=53 => ClientCommand::Attack,
            54 => ClientCommand::Loot(EntityId(3)),
            55 => ClientCommand::Target(EntityId(4)),
            56..=64 => ClientCommand::Attack,
            65 => ClientCommand::Loot(EntityId(4)),
            66..=94 => ClientCommand::Move { dx: -0.35, dy: 0.0 },
            95 => ClientCommand::TurnInQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
            _ => return,
        },
        None => return,
    };
    send_command(&bridge, &mut state, command);
    smoke.step = smoke.step.saturating_add(1);
}

fn native_secure_pointer_input(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut secure_input: ResMut<SecureInputState>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < secure_input.secure_attack_min.x
        || cursor.x > secure_input.secure_attack_max.x
        || cursor.y < secure_input.secure_attack_min.y
        || cursor.y > secure_input.secure_attack_max.y
    {
        return;
    }

    let physical_event_id = secure_input.next_physical_event_id();
    let default_addon = secure_input.default_addon;
    let default_node = secure_input.default_node;
    let node_generation = secure_input.node_generation;
    let attack_action = secure_input.attack_action;
    match secure_input.registry.dispatch(
        default_addon,
        default_node,
        node_generation,
        NativePress::PrimaryPointer { physical_event_id },
    ) {
        Ok(trusted) => {
            let (action, _, _, _) = trusted.consume();
            if action == attack_action {
                send_command(&bridge, &mut state, ClientCommand::Attack);
            }
        }
        Err(error) => state.log(error.to_string()),
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let catalog = starter_catalog();
    catalog
        .validate()
        .expect("the compiled starter catalog must validate before client startup");

    println!(
        "loaded zone '{}' with {} NPC definitions and {} spawn placements",
        catalog.zones[0].name,
        catalog.npcs.len(),
        catalog.zones[0].spawns.len()
    );

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 24.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-8.0, 18.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let field_material = materials.add(Color::srgb(0.22, 0.45, 0.19));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(FIELD_SIZE.x, FIELD_SIZE.y))),
        MeshMaterial3d(field_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    let town_material = materials.add(Color::srgb(0.58, 0.42, 0.24));
    let roof_material = materials.add(Color::srgb(0.36, 0.12, 0.08));

    // The town is intentionally made from primitive meshes: this spike tests
    // the runtime and content boundary, not the art pipeline.
    spawn_block(
        &mut commands,
        &mut meshes,
        town_material.clone(),
        Vec3::new(-9.0, 0.3, -2.0),
        Vec3::new(5.0, 0.6, 4.0),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        town_material.clone(),
        Vec3::new(-3.0, 0.3, -2.0),
        Vec3::new(4.0, 0.6, 4.0),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        roof_material.clone(),
        Vec3::new(-9.0, 1.8, -2.0),
        Vec3::new(5.4, 2.4, 4.4),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        roof_material,
        Vec3::new(-3.0, 1.8, -2.0),
        Vec3::new(4.4, 2.4, 4.4),
    );

    // NPC presentation entities are created only after the server publishes
    // a completed snapshot. Catalog spawn order is not an entity identity.
    commands.insert_resource(NpcPresentationAssets {
        mesh: meshes.add(Capsule3d::new(0.45, 1.0)),
        vendor_material: materials.add(Color::srgb(0.95, 0.78, 0.16)),
        enemy_material: materials.add(Color::srgb(0.72, 0.72, 0.76)),
    });

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.1))),
        MeshMaterial3d(materials.add(Color::srgb(0.18, 0.46, 0.95))),
        Transform::from_xyz(0.0, 0.85, 0.0),
        PlayerMarker,
    ));
}

fn setup_ui(mut commands: Commands, scripted_ui: Res<ScriptedUiPresentation>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(12.0),
                left: px(12.0),
                width: px(560.0),
                padding: UiRect::all(px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.88)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("connecting..."),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                StatusText,
            ));
            parent
                .spawn((
                    Node {
                        width: px(260.0),
                        height: px(32.0),
                        margin: UiRect::top(px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.32, 0.58, 0.95)),
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new(format!(
                            "{} / {} — click or Space",
                            scripted_ui.default_label, scripted_ui.addon_label
                        )),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

/// Runs the typed wire transport on a dedicated worker. Socket ownership
/// remains outside Bevy's render and input systems, and the bounded frame
/// buffer rejects malformed or oversized data before it reaches the
/// presentation adapter.
fn spawn_wire_network_worker(
    server_address: String,
    preferred_character_id: Option<u64>,
    command_rx: Receiver<ClientCommand>,
    event_tx: Sender<NetworkEvent>,
) {
    thread::spawn(move || {
        let Some(address) = server_address
            .to_socket_addrs()
            .ok()
            .and_then(|mut addresses| addresses.next())
        else {
            let _ = event_tx.send(NetworkEvent::Status(
                "wire address could not be resolved".to_owned(),
            ));
            return;
        };
        let mut deferred_commands = VecDeque::new();

        loop {
            if event_tx
                .send(NetworkEvent::Status(format!(
                    "connecting to typed wire {server_address}"
                )))
                .is_err()
            {
                return;
            }
            let mut stream = match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(stream) => stream,
                Err(error) => {
                    if event_tx
                        .send(NetworkEvent::Status(format!(
                            "wire connection failed: {error}; retrying"
                        )))
                        .is_err()
                    {
                        return;
                    }
                    thread::sleep(RECONNECT_DELAY);
                    continue;
                }
            };
            if let Err(error) = stream.set_nonblocking(true) {
                if event_tx
                    .send(NetworkEvent::Status(format!(
                        "cannot configure wire socket: {error}; retrying"
                    )))
                    .is_err()
                {
                    return;
                }
                thread::sleep(RECONNECT_DELAY);
                continue;
            }
            if event_tx
                .send(NetworkEvent::Status(
                    "typed wire socket connected".to_owned(),
                ))
                .is_err()
            {
                return;
            }
            let mut outgoing = VecDeque::<Vec<u8>>::new();
            let mut session = TypedSession::with_content_digest(
                DEV_AUTH_TOKEN,
                starter_catalog().content_digest(),
            );
            queue_session_outputs(&mut outgoing, session.handle(SessionInput::Connect));
            let mut incoming = Vec::new();

            'connection: loop {
                match command_rx.try_recv() {
                    Err(TryRecvError::Disconnected) => return,
                    Ok(ClientCommand::SelectCharacter { character_id }) => queue_session_outputs(
                        &mut outgoing,
                        session.handle(SessionInput::SelectCharacter(character_id)),
                    ),
                    Ok(command) if session.state() == SessionState::Ready => {
                        queue_session_outputs(
                            &mut outgoing,
                            session.handle(SessionInput::Intent(client_command_to_wire(command))),
                        );
                    }
                    Ok(command) if deferred_commands.len() < MAX_DEFERRED_COMMANDS => {
                        deferred_commands.push_back(command)
                    }
                    Ok(_) | Err(TryRecvError::Empty) => {}
                }

                if !flush_outgoing(&mut stream, &mut outgoing) {
                    let _ = event_tx.send(NetworkEvent::Status(
                        "typed wire write failed; reconnecting".to_owned(),
                    ));
                    break 'connection;
                }

                let mut buffer = [0_u8; 4096];
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => {
                            let _ = event_tx.send(NetworkEvent::Status(
                                "typed wire server closed the connection; reconnecting".to_owned(),
                            ));
                            break 'connection;
                        }
                        Ok(bytes_read) => {
                            incoming.extend_from_slice(&buffer[..bytes_read]);
                            if incoming.len() > MAX_WIRE_INPUT_BYTES {
                                let _ = event_tx.send(NetworkEvent::Status(
                                    "typed wire input buffer exceeded its limit; reconnecting"
                                        .to_owned(),
                                ));
                                break 'connection;
                            }
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                        Err(error) => {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire read failed: {error}; reconnecting"
                            )));
                            break 'connection;
                        }
                    }
                }

                loop {
                    let decoded = match decode_one(&incoming) {
                        Ok(decoded) => decoded,
                        Err(WireDecodeError::Truncated { .. }) => break,
                        Err(error) => {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire frame rejected: {error}; reconnecting"
                            )));
                            break 'connection;
                        }
                    };
                    let consumed = decoded.consumed;
                    let kind = decoded.envelope.kind;
                    let payload = decoded.envelope.payload;
                    incoming.drain(..consumed);
                    if kind != MessageKind::Event {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "typed wire server sent a non-event message; reconnecting".to_owned(),
                        ));
                        break 'connection;
                    }
                    let (sequence, message) = match SequencedServerMessage::decode_payload(&payload)
                    {
                        Ok(message) => (Some(message.sequence), message.message),
                        Err(_) => match ServerMessage::decode_payload(&payload) {
                            Ok(message) => (None, message),
                            Err(error) => {
                                let _ = event_tx.send(NetworkEvent::Status(format!(
                                    "typed wire server message rejected: {error}; reconnecting"
                                )));
                                break 'connection;
                            }
                        },
                    };
                    let session_input = match sequence {
                        Some(sequence) => SessionInput::Sequenced {
                            sequence,
                            message: message.clone(),
                        },
                        None => SessionInput::Server(message.clone()),
                    };
                    let session_outputs = session.handle(session_input);
                    if session_outputs
                        .iter()
                        .any(|output| matches!(output, SessionOutput::Rejected { .. }))
                    {
                        if let ServerMessage::Error { message } = &message {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire session rejected message: {message}"
                            )));
                        }
                    }
                    queue_session_outputs(&mut outgoing, session_outputs);
                    if let ServerMessage::CharacterSelected {
                        character_id,
                        name,
                        role,
                    } = &message
                    {
                        println!("server selected character {character_id} {name} role={role:?}");
                    }
                    if let ServerMessage::Connected { player_id, role } = &message {
                        println!("server connected player {player_id} role={role:?}");
                    }
                    if matches!(message, ServerMessage::CharacterList { ref characters, .. } if characters.is_empty())
                    {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "authenticated account has no characters".to_owned(),
                        ));
                        continue;
                    }
                    if let ServerMessage::CharacterList { characters, .. } = &message
                        && let Some(character_id) = preferred_character_id
                        && characters
                            .iter()
                            .any(|character| character.character_id == character_id)
                    {
                        queue_session_outputs(
                            &mut outgoing,
                            session.handle(SessionInput::SelectCharacter(character_id)),
                        );
                        println!("selecting startup character {character_id}");
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "selecting startup character {character_id}"
                        )));
                    }
                    if matches!(message, ServerMessage::Snapshot(_)) {
                        while let Some(command) = deferred_commands.pop_front() {
                            queue_session_outputs(
                                &mut outgoing,
                                session
                                    .handle(SessionInput::Intent(client_command_to_wire(command))),
                            );
                        }
                    }
                    if event_tx.send(NetworkEvent::ServerMessage(message)).is_err() {
                        return;
                    }
                }

                thread::sleep(Duration::from_millis(5));
            }
            let _ = session.handle(SessionInput::Disconnected);
            deferred_commands.clear();
            thread::sleep(RECONNECT_DELAY);
        }
    });
}

fn client_command_to_wire(command: ClientCommand) -> WireCommand {
    match command {
        ClientCommand::SelectCharacter { character_id } => {
            WireCommand::SelectCharacter { character_id }
        }
        ClientCommand::Move { dx, dy } => WireCommand::Move { dx, dy },
        ClientCommand::Target(EntityId(target_id)) => WireCommand::SelectTarget { target_id },
        ClientCommand::Attack => WireCommand::BasicAttack,
        ClientCommand::Taunt => WireCommand::Taunt,
        ClientCommand::Heal(EntityId(target_id)) => WireCommand::Heal { target_id },
        ClientCommand::ListVendor(EntityId(vendor_id)) => WireCommand::ListVendor { vendor_id },
        ClientCommand::ListQuestOffers(EntityId(npc_id)) => WireCommand::ListQuestOffers { npc_id },
        ClientCommand::BuyItem {
            vendor_id: EntityId(vendor_id),
            item_id,
            quantity,
        } => WireCommand::BuyItem {
            vendor_id,
            item_id: item_id.0,
            quantity,
        },
        ClientCommand::AcceptQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => WireCommand::AcceptQuest {
            npc_id,
            quest_id: quest_id.0,
        },
        ClientCommand::TurnInQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => WireCommand::TurnInQuest {
            npc_id,
            quest_id: quest_id.0,
        },
        ClientCommand::Loot(EntityId(enemy_id)) => WireCommand::LootEnemy { enemy_id },
        ClientCommand::InvitePartyMember(EntityId(target_id)) => {
            WireCommand::InvitePartyMember { target_id }
        }
        ClientCommand::AcceptPartyInvite(party_id) => WireCommand::AcceptPartyInvite { party_id },
    }
}

fn queue_wire_command(outgoing: &mut VecDeque<Vec<u8>>, command: WireCommand) {
    if outgoing.len() >= MAX_OUTGOING_LINES {
        return;
    }
    let Ok(payload) = command.encode_payload() else {
        return;
    };
    let Ok(frame) =
        Envelope::new(MessageKind::Command, payload).and_then(|envelope| envelope.encode())
    else {
        return;
    };
    outgoing.push_back(frame);
}

fn queue_session_outputs(outgoing: &mut VecDeque<Vec<u8>>, outputs: Vec<SessionOutput>) {
    for output in outputs {
        if let SessionOutput::Send(command) = output {
            queue_wire_command(outgoing, command);
        }
    }
}

#[cfg(test)]
fn queue_command_line(outgoing: &mut VecDeque<Vec<u8>>, command: &str) {
    let mut line = command.as_bytes().to_vec();
    line.push(b'\n');
    outgoing.push_back(line);
}

#[cfg(test)]
fn queue_command_bounded(outgoing: &mut VecDeque<Vec<u8>>, command: ClientCommand) {
    if outgoing.len() >= MAX_OUTGOING_LINES {
        return;
    }
    match command {
        // Character selection is a typed-wire session transition. The legacy
        // development line protocol has no account or character catalog.
        ClientCommand::SelectCharacter { .. } => {}
        ClientCommand::Move { dx, dy } => queue_command_line(outgoing, &format!("move {dx} {dy}")),
        ClientCommand::Target(EntityId(id)) => {
            queue_command_line(outgoing, &format!("target {id}"));
        }
        ClientCommand::Attack => queue_command_line(outgoing, "attack"),
        ClientCommand::Taunt => queue_command_line(outgoing, "taunt"),
        ClientCommand::Heal(EntityId(id)) => {
            queue_command_line(outgoing, &format!("heal {id}"));
        }
        ClientCommand::ListVendor(EntityId(id)) => {
            queue_command_line(outgoing, &format!("vendor {id}"));
        }
        ClientCommand::ListQuestOffers(EntityId(id)) => {
            queue_command_line(outgoing, &format!("quest-offers {id}"));
        }
        ClientCommand::BuyItem {
            vendor_id: EntityId(vendor_id),
            item_id,
            quantity,
        } => queue_command_line(outgoing, &format!("buy {vendor_id} {item_id} {quantity}")),
        ClientCommand::AcceptQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => queue_command_line(outgoing, &format!("accept-quest {npc_id} {quest_id}")),
        ClientCommand::TurnInQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => queue_command_line(outgoing, &format!("turn-in-quest {npc_id} {quest_id}")),
        ClientCommand::Loot(EntityId(enemy_id)) => {
            queue_command_line(outgoing, &format!("loot {enemy_id}"));
        }
        ClientCommand::InvitePartyMember(_) | ClientCommand::AcceptPartyInvite(_) => {}
    }
}

fn flush_outgoing(stream: &mut TcpStream, outgoing: &mut VecDeque<Vec<u8>>) -> bool {
    while let Some(line) = outgoing.front_mut() {
        match stream.write(line) {
            Ok(0) => return false,
            Ok(bytes_written) => {
                line.drain(..bytes_written);
                if line.is_empty() {
                    outgoing.pop_front();
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(_) => return false,
        }
    }
    true
}

fn consume_network_events(bridge: Res<NetworkBridge>, mut state: ResMut<ClientState>) {
    let Ok(receiver) = bridge.event_rx.lock() else {
        state.connection_status = "network bridge unavailable".to_owned();
        return;
    };
    while let Ok(event) = receiver.try_recv() {
        match event {
            NetworkEvent::Status(status) => {
                if let Err(error) = state.snapshot.finish() {
                    state.log(format!("discarded incomplete snapshot: {error}"));
                }
                if status.starts_with("connecting to typed wire") {
                    state.player_id = None;
                    state.connected_role = None;
                    state.available_characters.clear();
                    state.selected_character_id = None;
                    state.presentation = ClientWorld::default();
                }
                state.connection_status = status.clone();
                state.log(status);
            }
            NetworkEvent::ServerMessage(message) => apply_server_message(&mut state, &message),
        }
    }
}

fn keyboard_input(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut repeat: ResMut<MovementRepeat>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut secure_input: ResMut<SecureInputState>,
) {
    if input.just_pressed(KeyCode::Enter)
        && state.player_id.is_none()
        && state.selected_character_id.is_none()
        && let Some(character_id) = state
            .available_characters
            .first()
            .map(|character| character.character_id)
    {
        state.selected_character_id = Some(character_id);
        state.connection_status = "selecting character".to_owned();
        send_command(
            &bridge,
            &mut state,
            ClientCommand::SelectCharacter { character_id },
        );
    }

    let mut dx = 0.0;
    let mut dy = 0.0;
    if input.pressed(KeyCode::KeyA) {
        dx -= MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyD) {
        dx += MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyS) {
        dy -= MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyW) {
        dy += MOVEMENT_STEP;
    }
    let movement_length = dx.hypot(dy);
    if movement_length > MOVEMENT_STEP {
        let scale = MOVEMENT_STEP / movement_length;
        dx *= scale;
        dy *= scale;
    }

    let repeating = repeat.0.tick(time.delta()).just_finished();
    if dx != 0.0 || dy != 0.0 {
        if (input.just_pressed(KeyCode::KeyA)
            || input.just_pressed(KeyCode::KeyD)
            || input.just_pressed(KeyCode::KeyS)
            || input.just_pressed(KeyCode::KeyW))
            || repeating
        {
            send_command(&bridge, &mut state, ClientCommand::Move { dx, dy });
        }
    } else {
        repeat.0.reset();
    }

    if input.just_pressed(KeyCode::Tab)
        && let Some(target_id) = next_target(&mut state)
    {
        send_command(&bridge, &mut state, ClientCommand::Target(target_id));
    }
    if input.just_pressed(KeyCode::Space) {
        let physical_event_id = secure_input.next_physical_event_id();
        let default_addon = secure_input.default_addon;
        let default_node = secure_input.default_node;
        let node_generation = secure_input.node_generation;
        let attack_action = secure_input.attack_action;
        match secure_input.registry.dispatch(
            default_addon,
            default_node,
            node_generation,
            NativePress::Key {
                physical_event_id,
                repeat: false,
            },
        ) {
            Ok(trusted) => {
                let (action, _, _, _) = trusted.consume();
                if action == attack_action {
                    send_command(&bridge, &mut state, ClientCommand::Attack);
                }
            }
            Err(error) => state.log(error.to_string()),
        }
    }
    if input.just_pressed(KeyCode::KeyL)
        && let Some(target_id) = state
            .player_id
            .and_then(|id| state.presentation.player(id))
            .and_then(|player| player.target)
    {
        send_command(&bridge, &mut state, ClientCommand::Loot(target_id));
    }
    if input.just_pressed(KeyCode::KeyV)
        && let Some(vendor_id) = first_vendor_id(&state)
    {
        send_command(&bridge, &mut state, ClientCommand::ListVendor(vendor_id));
    }
    if input.just_pressed(KeyCode::KeyO)
        && let Some(vendor_id) = first_vendor_id(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::ListQuestOffers(vendor_id),
        );
    }
    if input.just_pressed(KeyCode::KeyB)
        && let Some((vendor_id, item_id)) = first_vendor_listing(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::BuyItem {
                vendor_id,
                item_id,
                quantity: 1,
            },
        );
    }
    if input.just_pressed(KeyCode::KeyE)
        && let Some((npc_id, quest_id)) = first_quest_offer(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::AcceptQuest { npc_id, quest_id },
        );
    }
    if input.just_pressed(KeyCode::KeyR)
        && let Some((npc_id, quest_id)) = first_player_quest(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::TurnInQuest { npc_id, quest_id },
        );
    }
}

fn send_command(bridge: &NetworkBridge, state: &mut ClientState, command: ClientCommand) {
    match bridge.command_tx.try_send(command) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => {
            state.log("input queue full; command dropped".to_owned());
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            state.connection_status = "network worker stopped".to_owned();
            state.log("network worker stopped".to_owned());
        }
    }
}

fn next_target(state: &mut ClientState) -> Option<EntityId> {
    let ids: Vec<_> = state
        .presentation
        .entities()
        .filter_map(|entity| match entity {
            ClientEntity::Npc(npc) => Some(npc.id),
            ClientEntity::Player(_) => None,
        })
        .collect();
    let target = ids.get(state.target_cursor % ids.len().max(1)).copied()?;
    state.target_cursor = (state.target_cursor + 1) % ids.len();
    Some(target)
}

fn first_vendor_id(state: &ClientState) -> Option<EntityId> {
    state
        .presentation
        .entities()
        .find_map(|entity| match entity {
            ClientEntity::Npc(npc) if npc.kind == NpcKind::Vendor => Some(npc.id),
            _ => None,
        })
}

fn first_quest_offer(state: &ClientState) -> Option<(EntityId, QuestId)> {
    let npc_id = first_vendor_id(state)?;
    let quest_id = state
        .presentation
        .quest_offers(npc_id)?
        .first()
        .map(|offer| offer.quest_id)?;
    Some((npc_id, quest_id))
}

fn first_vendor_listing(state: &ClientState) -> Option<(EntityId, ItemId)> {
    let vendor_id = first_vendor_id(state)?;
    let item_id = state
        .presentation
        .vendor_listings(vendor_id)?
        .first()
        .map(|listing| listing.item_id)?;
    Some((vendor_id, item_id))
}

fn first_player_quest(state: &ClientState) -> Option<(EntityId, QuestId)> {
    let player_id = state.player_id?;
    let quest_id = state
        .presentation
        .player(player_id)?
        .quests
        .first()
        .map(|quest| quest.quest_id)?;
    Some((first_vendor_id(state)?, quest_id))
}

fn sync_authoritative_presentation(
    mut commands: Commands,
    state: Res<ClientState>,
    assets: Res<NpcPresentationAssets>,
    mut player_query: Query<&mut Transform, (With<PlayerMarker>, Without<StarterNpc>)>,
    mut npc_query: Query<(Entity, &StarterNpc, &mut Transform), Without<PlayerMarker>>,
) {
    if let Some(player_id) = state.player_id
        && let Some(player) = state.presentation.player(player_id)
        && let Ok(mut transform) = player_query.single_mut()
    {
        transform.translation.x = player.position.x;
        transform.translation.z = player.position.y;
    }
    let mut rendered_ids = BTreeMap::new();
    for (entity, marker, mut transform) in &mut npc_query {
        if let Some(ClientEntity::Npc(npc)) = state.presentation.entity(marker.id) {
            transform.translation.x = npc.position.x;
            transform.translation.z = npc.position.y;
            transform.translation.y = if npc.defeated { 0.2 } else { 0.75 };
            rendered_ids.insert(marker.id, entity);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for entity in state.presentation.entities() {
        let ClientEntity::Npc(npc) = entity else {
            continue;
        };
        if rendered_ids.contains_key(&npc.id) {
            continue;
        }
        let material = match npc.kind {
            NpcKind::Vendor => assets.vendor_material.clone(),
            NpcKind::Enemy => assets.enemy_material.clone(),
        };
        commands.spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(
                npc.position.x,
                if npc.defeated { 0.2 } else { 0.75 },
                npc.position.y,
            ),
            StarterNpc { id: npc.id },
        ));
    }
}

fn update_status_text(state: Res<ClientState>, mut query: Query<&mut Text, With<StatusText>>) {
    let Ok(mut text) = query.single_mut() else {
        return;
    };
    text.0 = format_hud_text(&state);
}

fn format_hud_text(state: &ClientState) -> String {
    let character_selection = if state.player_id.is_some() {
        String::new()
    } else if state.available_characters.is_empty() {
        "characters: waiting for account response".to_owned()
    } else if state.selected_character_id.is_some() {
        "characters: selection sent; entering world".to_owned()
    } else {
        format!(
            "characters: {} — press Enter to select",
            state
                .available_characters
                .iter()
                .map(|character| format!("{} ({:?})", character.name, character.role))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let player = state.player_id.and_then(|id| state.presentation.player(id));
    let target = player.and_then(|player| player.target).map_or_else(
        || "none".to_owned(),
        |target| {
            state.presentation.npc(target).map_or_else(
                || target.0.to_string(),
                |npc| {
                    let threat = state
                        .presentation
                        .enemy_threat_target(target)
                        .map_or_else(|| "none".to_owned(), |player_id| player_id.0.to_string());
                    format!(
                        "{} ({}/{}) threat={threat}",
                        target.0, npc.health, npc.max_health
                    )
                },
            )
        },
    );
    let player_summary = state.player_id.map_or_else(
        || "not connected".to_owned(),
        |id| match state.presentation.player(id) {
            Some(player) => format!(
                "player {} ({:?})  hp {}/{}  target {}",
                id.0, player.role, player.health, player.max_health, target
            ),
            None => format!("player {}  waiting for snapshot", id.0),
        },
    );
    let party = state.presentation.parties().next().map_or_else(
        || "party: none".to_owned(),
        |party| {
            format!(
                "party {}  leader {}  members {}",
                party.id.0,
                party.leader_id.0,
                party
                    .member_ids
                    .iter()
                    .map(|member_id| member_id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
    );
    let tick = state
        .presentation
        .world_tick()
        .map_or_else(|| "-".to_owned(), |tick| tick.to_string());
    let combat_status = state.player_id.map_or_else(
        || "combat: unavailable".to_owned(),
        |player_id| {
            state.presentation.combat_ready_tick(player_id).map_or_else(
                || "combat: ready".to_owned(),
                |ready_tick| format!("combat: cooldown through tick {ready_tick}"),
            )
        },
    );
    let logs = state.logs.iter().cloned().collect::<Vec<_>>().join("\n");
    let inventory = player.map_or_else(
        || "inventory: waiting for snapshot".to_owned(),
        |player| {
            let stacks = player
                .inventory
                .stacks()
                .map(|stack| {
                    let name = item_definition(stack.item_id)
                        .map(|definition| definition.name)
                        .unwrap_or("unknown item");
                    format!("{name} x{}", stack.quantity)
                })
                .collect::<Vec<_>>();
            if stacks.is_empty() {
                format!(
                    "inventory ({}/{}): empty",
                    player.inventory.used_slots(),
                    player.inventory.capacity()
                )
            } else {
                format!(
                    "inventory ({}/{}): {}",
                    player.inventory.used_slots(),
                    player.inventory.capacity(),
                    stacks.join(", ")
                )
            }
        },
    );
    let quests = player.map_or_else(
        || "quests: waiting for snapshot".to_owned(),
        |player| {
            if player.quests.is_empty() {
                return "quests: none".to_owned();
            }
            player
                .quests
                .iter()
                .map(|quest| {
                    let name = starter_catalog()
                        .quests
                        .iter()
                        .find(|definition| definition.id == quest.quest_id)
                        .map(|definition| definition.name)
                        .unwrap_or("unknown quest");
                    let required = quest
                        .required_count
                        .map_or_else(|| "?".to_owned(), |count| count.to_string());
                    format!(
                        "{name} {}/{} ({:?})",
                        quest.progress, required, quest.status
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        },
    );
    let vendor = first_vendor_id(state).map_or_else(
        || "vendor: not discovered".to_owned(),
        |vendor_id| match state.presentation.vendor_listings(vendor_id) {
            Some(listings) if !listings.is_empty() => format!(
                "vendor: {}",
                listings
                    .iter()
                    .map(|listing| {
                        format!(
                            "{} {}g ({} left)",
                            listing.name, listing.unit_price, listing.remaining_quantity
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "vendor: press V to list stock".to_owned(),
        },
    );
    let offers = first_vendor_id(state).map_or_else(
        || "offers: not discovered".to_owned(),
        |vendor_id| match state.presentation.quest_offers(vendor_id) {
            Some(offers) if !offers.is_empty() => format!(
                "offers: {}",
                offers
                    .iter()
                    .map(|offer| offer.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "offers: press O to ask the town NPC".to_owned(),
        },
    );
    let notification = state.presentation.last_notification().map_or_else(
        || "".to_owned(),
        |notification| format!("\nnotice: {notification:?}"),
    );
    format!(
        "server: {} ({})\n{}\n{}\n{}  tick {}\n{}\n{}\n{}\n{}\n{}\n\ncontrols: Enter select character | WASD move | Tab target | Space attack | L loot | V vendor | B buy | O offers | E accept | R turn in{}\n\n{}",
        state.connection_status,
        state.server_address,
        character_selection,
        party,
        player_summary,
        tick,
        combat_status,
        inventory,
        quests,
        vendor,
        offers,
        notification,
        logs,
    )
}

#[cfg(test)]
fn apply_server_line(state: &mut ClientState, line: &str) {
    // TEMPORARY ADAPTER: the development protocol is not production-ready,
    // but all gameplay state changes still pass through its strict decoder.
    if line.starts_with("TEMP_SNAPSHOT") {
        match state.snapshot.push_line(line) {
            Ok(Some(snapshot)) => apply_snapshot(state, snapshot),
            Ok(None) => {}
            Err(error) => state.log(format!("snapshot rejected: {error}")),
        }
        return;
    }
    match decode_server_line(line) {
        Ok(ServerLine::SnapshotBegin { .. } | ServerLine::SnapshotEnd) => {}
        Ok(ServerLine::World(_)) => {}
        Ok(ServerLine::Connected(connected)) => {
            state.player_id = Some(connected.player_id);
            state.connection_status = "authenticated development session".to_owned();
            state.log(format!("connected as {}", connected.player_id.0));
        }
        // Legacy diagnostic records are intentionally not projected. The
        // graphical client consumes complete TEMP_SNAPSHOT frames and typed
        // EVENT records so its renderer has one authoritative state source.
        Ok(
            ServerLine::Player(_) | ServerLine::Npc(_) | ServerLine::Item(_) | ServerLine::Quest(_),
        ) => {}
        Ok(ServerLine::Event(event)) => apply_event(state, event),
        Err(_) => match line.split_whitespace().next() {
            Some("ERR") => state.log(line.to_owned()),
            Some("WELCOME") => state.log("server greeted client"),
            Some("TYPE") => {}
            Some(_) | None => state.log(line.to_owned()),
        },
    }
}

fn apply_server_message(state: &mut ClientState, message: &ServerMessage) {
    let is_snapshot = matches!(message, ServerMessage::Snapshot(_));
    match message {
        ServerMessage::Response { message, .. } => return apply_server_message(state, message),
        ServerMessage::Welcome { server } => state.log(format!("server greeted {server}")),
        ServerMessage::Authenticated { account_id, .. } => {
            state.log(format!("authenticated account {account_id}"));
        }
        ServerMessage::CharacterList { characters, .. } => {
            state.available_characters = characters.clone();
            state.selected_character_id = None;
            state.connection_status = "select a character with Enter".to_owned();
            state.log(format!(
                "received {} available character(s)",
                characters.len()
            ));
        }
        ServerMessage::CharacterSelected { name, role, .. } => {
            state.connection_status = "character selected; entering world".to_owned();
            state.log(format!("selected character {name} ({role:?})"));
        }
        ServerMessage::Connected {
            player_id, role, ..
        } => {
            state.player_id = Some(EntityId(*player_id));
            state.connected_role = Some(*role);
            state.connection_status = "authenticated typed development session".to_owned();
            state.log(format!("connected as {player_id}"));
        }
        ServerMessage::ContentAccepted { .. } => {
            state.connection_status = "content compatible; entering world".to_owned();
            state.log("content compatibility accepted".to_owned());
        }
        ServerMessage::ContentMismatch { .. } => {
            state.connection_status = "content mismatch".to_owned();
            state.log("content compatibility rejected".to_owned());
        }
        ServerMessage::Error { message } => state.log(format!("rejected: {message}")),
        ServerMessage::Event(event) => {
            println!("GRAPHICAL_EVENT {event:?}");
        }
        ServerMessage::SkippedEvent { .. } | ServerMessage::Snapshot(_) => {}
    }
    if let Err(error) = apply_wire_message(&mut state.presentation, message) {
        state.log(format!("authoritative presentation rejected: {error}"));
    } else if is_snapshot {
        log_graphical_bootstrap(state);
    }
}

fn log_graphical_bootstrap(state: &ClientState) {
    let Some(player_id) = state.player_id else {
        return;
    };
    let Some(player) = state.presentation.player(player_id) else {
        return;
    };
    let quest = player
        .quests
        .iter()
        .find(|quest| quest.quest_id == QuestId::CLEAR_THE_FIELD);
    let (progress, status) = quest.map_or((0, "None".to_owned()), |quest| {
        (quest.progress, format!("{:?}", quest.status))
    });
    println!(
        "GRAPHICAL_BOOTSTRAP player={} gold={} clear_field_progress={} status={}",
        player_id.0, player.gold, progress, status
    );
}

#[cfg(test)]
fn apply_snapshot(state: &mut ClientState, snapshot: Snapshot) {
    if let Err(error) = apply_presentation_snapshot(&mut state.presentation, &snapshot) {
        state.log(format!("authoritative presentation rejected: {error}"));
        return;
    }
}

#[cfg(test)]
fn apply_event(state: &mut ClientState, event: ServerEvent) {
    match apply_presentation_event(&mut state.presentation, &event) {
        Ok(_) => {}
        Err(error) => state.log(format!("authoritative presentation rejected: {error}")),
    }
    match event {
        ServerEvent::TargetSelected {
            player_id,
            target_id,
        } if Some(player_id) == state.player_id => {
            state.log(format!("target {}", target_id.0));
        }
        ServerEvent::AttackResolved { player_id, .. } if Some(player_id) == state.player_id => {
            state.log("server resolved attack".to_owned());
        }
        ServerEvent::EnemyDefeated { .. } => {
            state.log("enemy defeated; server decides rewards".to_owned());
        }
        ServerEvent::CommandRejected { reason } => state.log(format!("rejected: {reason}")),
        ServerEvent::PlayerJoined { .. } => {}
        _ => {}
    }
}

fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(material),
        Transform::from_translation(position),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    #[test]
    fn secure_attack_binding_rejects_replay_and_refreshes_after_focus_change() {
        let scripted_ui = build_scripted_ui_presentation(None).expect("built-in UI must load");
        let mut secure = SecureInputState::new(&scripted_ui);
        let first = secure
            .registry
            .dispatch(
                secure.default_addon,
                secure.default_node,
                secure.node_generation,
                NativePress::Key {
                    physical_event_id: 1,
                    repeat: false,
                },
            )
            .unwrap();
        assert_eq!(first.consume().0, secure.attack_action);
        assert!(
            secure
                .registry
                .dispatch(
                    secure.default_addon,
                    secure.default_node,
                    secure.node_generation,
                    NativePress::Key {
                        physical_event_id: 1,
                        repeat: false,
                    },
                )
                .is_err()
        );

        secure.registry.focus_changed();
        assert!(
            secure
                .registry
                .dispatch(
                    secure.default_addon,
                    secure.default_node,
                    secure.node_generation,
                    NativePress::Key {
                        physical_event_id: 2,
                        repeat: false,
                    },
                )
                .is_err()
        );
        secure.refresh_default_binding();
        assert!(
            secure
                .registry
                .dispatch(
                    secure.default_addon,
                    secure.default_node,
                    secure.node_generation,
                    NativePress::Key {
                        physical_event_id: 2,
                        repeat: false,
                    },
                )
                .is_ok()
        );
    }

    #[test]
    fn projects_player_and_npc_state_only_from_server_lines() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(&mut state, "TEMP_SNAPSHOT_BEGIN version=2");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT WORLD tick=10 players=1 npcs=1 enemies=1 vendors=0",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=4.0,-2.0 health=90 max_health=100 gold=20 capacity=16 target=2",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=24.0,0.0 health=100 max_health=100",
        );
        apply_server_line(&mut state, "TEMP_SNAPSHOT_END");
        apply_server_line(
            &mut state,
            "EVENT player_moved id=5 pos=6.00,-1.00 area=Field",
        );
        apply_server_line(
            &mut state,
            "EVENT attack player=5 target=2 damage=25 target_hp=75",
        );

        assert_eq!(state.player_id, Some(EntityId(5)));
        assert_eq!(
            state.presentation.player(EntityId(5)).unwrap().position,
            mmorpg_client_protocol::Position::new(6.0, -1.0)
        );
        assert_eq!(state.presentation.player(EntityId(5)).unwrap().health, 90);
        assert_eq!(
            state.presentation.player(EntityId(5)).unwrap().target,
            Some(EntityId(2))
        );
        assert_eq!(state.presentation.npc(EntityId(2)).unwrap().health, 75);
    }

    #[test]
    fn tab_targeting_cycles_server_known_entities() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        for line in [
            "TEMP_SNAPSHOT_BEGIN version=2",
            "TEMP_SNAPSHOT WORLD tick=1 players=0 npcs=2 enemies=2 vendors=0",
            "TEMP_SNAPSHOT NPC id=4 template_id=2 name=Wolf_4 kind=enemy position=0.0,0.0 health=100 max_health=100",
            "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Wolf_2 kind=enemy position=0.0,0.0 health=100 max_health=100",
            "TEMP_SNAPSHOT_END",
        ] {
            apply_server_line(&mut state, line);
        }

        assert_eq!(next_target(&mut state), Some(EntityId(2)));
        assert_eq!(next_target(&mut state), Some(EntityId(4)));
        assert_eq!(next_target(&mut state), Some(EntityId(2)));
    }

    #[test]
    fn bounded_command_queue_drops_commands_after_the_outgoing_limit() {
        let mut outgoing = VecDeque::new();
        for _ in 0..MAX_OUTGOING_LINES {
            queue_command_bounded(&mut outgoing, ClientCommand::Attack);
        }
        queue_command_bounded(&mut outgoing, ClientCommand::Attack);

        assert_eq!(outgoing.len(), MAX_OUTGOING_LINES);
    }

    #[test]
    fn starter_loop_commands_encode_to_server_intents() {
        let mut outgoing = VecDeque::new();
        queue_command_bounded(&mut outgoing, ClientCommand::ListVendor(EntityId(1)));
        queue_command_bounded(&mut outgoing, ClientCommand::ListQuestOffers(EntityId(1)));
        queue_command_bounded(
            &mut outgoing,
            ClientCommand::BuyItem {
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 1,
            },
        );
        queue_command_bounded(
            &mut outgoing,
            ClientCommand::AcceptQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
        );
        queue_command_bounded(
            &mut outgoing,
            ClientCommand::TurnInQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
        );
        queue_command_bounded(&mut outgoing, ClientCommand::Loot(EntityId(2)));

        let lines = outgoing
            .into_iter()
            .map(|line| String::from_utf8(line).expect("client commands are UTF-8"))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "vendor 1\n",
                "quest-offers 1\n",
                "buy 1 2 1\n",
                "accept-quest 1 1\n",
                "turn-in-quest 1 1\n",
                "loot 2\n",
            ]
        );
    }

    #[test]
    fn typed_character_list_requires_a_user_selection_intent() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_message(
            &mut state,
            &ServerMessage::CharacterList {
                account_id: 1,
                characters: vec![CharacterSummary {
                    character_id: 7,
                    name: "Aria".to_owned(),
                    role: mmorpg_wire::RoleCode::DamageDealer,
                }],
            },
        );

        assert_eq!(state.available_characters.len(), 1);
        assert_eq!(state.available_characters[0].character_id, 7);
        assert_eq!(state.selected_character_id, None);
        assert!(format_hud_text(&state).contains("press Enter to select"));
        assert!(matches!(
            client_command_to_wire(ClientCommand::SelectCharacter { character_id: 7 }),
            WireCommand::SelectCharacter { character_id: 7 }
        ));

        let mut legacy_outgoing = VecDeque::new();
        queue_command_bounded(
            &mut legacy_outgoing,
            ClientCommand::SelectCharacter { character_id: 7 },
        );
        assert!(legacy_outgoing.is_empty());
    }

    #[test]
    fn wire_worker_reconnects_through_preferred_character_selection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should have an address")
            .to_string();
        let (snapshot_tx, snapshot_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for (session_id, player_id) in [(1, 5), (2, 6)] {
                let (mut stream, _) = listener.accept().expect("worker should connect");
                serve_test_authenticated_connection(
                    &mut stream,
                    session_id,
                    player_id,
                    &snapshot_tx,
                );
            }
        });

        let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel();
        spawn_wire_network_worker(address, Some(7), command_rx, event_tx);

        let character_list = wait_for_test_character_list(&event_rx);
        assert_eq!(character_list[0].character_id, 7);

        assert_eq!(wait_for_test_connected(&event_rx), 5);
        snapshot_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should request a bootstrap snapshot after entering world");

        let character_list = wait_for_test_character_list(&event_rx);
        assert_eq!(character_list[0].character_id, 7);
        assert_eq!(wait_for_test_connected(&event_rx), 6);
        snapshot_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should request a bootstrap snapshot after reconnecting");
        drop(command_tx);
        server.join().expect("test wire server should finish");
    }

    fn serve_test_authenticated_connection(
        stream: &mut TcpStream,
        session_id: u64,
        player_id: u64,
        snapshot_tx: &mpsc::Sender<()>,
    ) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("server timeout should configure");
        assert!(matches!(
            read_test_wire_command(stream),
            WireCommand::Authenticate { token } if token == DEV_AUTH_TOKEN
        ));
        send_test_wire_message(
            stream,
            &ServerMessage::Authenticated {
                account_id: 1,
                session_id,
            },
        );
        assert!(matches!(
            read_test_wire_command(stream),
            WireCommand::ListCharacters
        ));
        send_test_wire_message(
            stream,
            &ServerMessage::CharacterList {
                account_id: 1,
                characters: vec![CharacterSummary {
                    character_id: 7,
                    name: "Aria".to_owned(),
                    role: mmorpg_wire::RoleCode::DamageDealer,
                }],
            },
        );
        assert!(matches!(
            read_test_wire_command(stream),
            WireCommand::SelectCharacter { character_id: 7 }
        ));
        send_test_wire_message(
            stream,
            &ServerMessage::CharacterSelected {
                character_id: 7,
                name: "Aria".to_owned(),
                role: mmorpg_wire::RoleCode::DamageDealer,
            },
        );
        let digest = match read_test_wire_command(stream) {
            WireCommand::ContentDigest { digest } => digest,
            command => panic!("expected content digest, received {command:?}"),
        };
        send_test_wire_message(stream, &ServerMessage::ContentAccepted { digest });
        assert!(matches!(
            read_test_wire_command(stream),
            WireCommand::EnterWorld
        ));
        send_test_wire_message(
            stream,
            &ServerMessage::Connected {
                player_id,
                role: mmorpg_wire::RoleCode::DamageDealer,
            },
        );
        assert!(matches!(
            read_test_wire_command(stream),
            WireCommand::Snapshot
        ));
        snapshot_tx
            .send(())
            .expect("test should observe the bootstrap request");
    }

    fn wait_for_test_character_list(
        event_rx: &mpsc::Receiver<NetworkEvent>,
    ) -> Vec<CharacterSummary> {
        loop {
            let event = event_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("worker should report the character list");
            if let NetworkEvent::ServerMessage(ServerMessage::CharacterList {
                characters, ..
            }) = event
            {
                return characters;
            }
        }
    }

    fn wait_for_test_connected(event_rx: &mpsc::Receiver<NetworkEvent>) -> u64 {
        loop {
            let event = event_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("worker should enter the selected character");
            if let NetworkEvent::ServerMessage(ServerMessage::Connected { player_id, .. }) = event {
                return player_id;
            }
        }
    }

    fn read_test_wire_command(stream: &mut TcpStream) -> WireCommand {
        let mut length = [0_u8; mmorpg_wire::LENGTH_PREFIX_LEN];
        stream
            .read_exact(&mut length)
            .expect("test client should write frame length");
        let body_length = u32::from_be_bytes(length) as usize;
        let mut frame = Vec::with_capacity(mmorpg_wire::LENGTH_PREFIX_LEN + body_length);
        frame.extend_from_slice(&length);
        frame.resize(mmorpg_wire::LENGTH_PREFIX_LEN + body_length, 0);
        stream
            .read_exact(&mut frame[mmorpg_wire::LENGTH_PREFIX_LEN..])
            .expect("test client should write a complete frame");
        let decoded = decode_one(&frame).expect("test client frame should decode");
        assert_eq!(decoded.envelope.kind, MessageKind::Command);
        WireCommand::decode_payload(&decoded.envelope.payload)
            .expect("test client command payload should decode")
    }

    fn send_test_wire_message(stream: &mut TcpStream, message: &ServerMessage) {
        let payload = message
            .encode_payload()
            .expect("test server message should encode");
        let frame = Envelope::new(MessageKind::Event, payload)
            .expect("test envelope should build")
            .encode()
            .expect("test envelope should encode");
        stream
            .write_all(&frame)
            .expect("test server should write complete frame");
    }

    #[test]
    fn typed_mode_wraps_client_intents_in_versioned_command_frames() {
        let mut outgoing = VecDeque::new();
        queue_wire_command(
            &mut outgoing,
            client_command_to_wire(ClientCommand::BuyItem {
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 2,
            }),
        );
        let frame = outgoing.pop_front().expect("wire frame should be queued");
        let decoded = decode_one(&frame).expect("wire frame should decode");
        assert_eq!(decoded.envelope.kind, MessageKind::Command);
        assert_eq!(
            WireCommand::decode_payload(&decoded.envelope.payload),
            Ok(WireCommand::BuyItem {
                vendor_id: 1,
                item_id: 2,
                quantity: 2,
            })
        );
    }

    #[test]
    fn render_backend_selector_accepts_only_documented_values() {
        assert_eq!(
            RenderBackendChoice::parse("auto"),
            Some(RenderBackendChoice::Automatic)
        );
        assert_eq!(
            RenderBackendChoice::parse("vk"),
            Some(RenderBackendChoice::Vulkan)
        );
        assert_eq!(
            RenderBackendChoice::parse("opengl"),
            Some(RenderBackendChoice::Gl)
        );
        assert_eq!(RenderBackendChoice::parse("metal"), None);
        assert_eq!(RenderBackendChoice::Gl.label(), "gl");
    }

    #[test]
    fn projects_the_machine_snapshot_records_used_by_the_graphical_client() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(&mut state, "TEMP_SNAPSHOT_BEGIN version=2");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT WORLD tick=17 players=1 npcs=2 enemies=1 vendors=1",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=3.0,-4.0 health=88 max_health=100 gold=20 capacity=16 target=2",
        );
        apply_server_line(&mut state, "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=2");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=1/3 status=Accepted",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0.0,0.0 health=1 max_health=1",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=24.0,0.0 health=100 max_health=100",
        );
        apply_server_line(&mut state, "TEMP_SNAPSHOT_END");

        assert_eq!(state.presentation.world_tick(), Some(17));
        assert_eq!(
            state.presentation.player(EntityId(5)).unwrap().position,
            mmorpg_client_protocol::Position::new(3.0, -4.0)
        );
        assert_eq!(state.presentation.player(EntityId(5)).unwrap().gold, 20);
        assert_eq!(
            state
                .presentation
                .player(EntityId(5))
                .unwrap()
                .inventory
                .quantity(mmorpg_content::ItemId::TOWN_RATION),
            2
        );
        assert_eq!(
            state.presentation.player(EntityId(5)).unwrap().quests[0].progress,
            1
        );
        apply_server_line(
            &mut state,
            "EVENT vendor_listed player=5 vendor=1 listings=item=2 name=Town_Ration price=2 stock=98 max_stack=20;item=3 name=Minor_Healing_Potion price=8 stock=10 max_stack=5",
        );
        apply_server_line(
            &mut state,
            "EVENT quest_offers player=5 npc=1 quests=id=1 name=Clear_the_Field",
        );
        let hud = format_hud_text(&state);
        assert!(hud.contains("player 5 (DamageDealer)  hp 88/100  target 2"));
        assert!(hud.contains("party: none"));
        assert!(hud.contains("combat: ready"));
        assert!(hud.contains("inventory (1/16): Town Ration x2"));
        assert!(hud.contains("Clear the Field 1/3 (Accepted)"));
        assert!(hud.contains("vendor: Town Ration 2g (98 left)"));
        assert!(hud.contains("offers: Clear the Field"));
        assert_eq!(
            state.presentation.npc(EntityId(2)).unwrap().template_id,
            mmorpg_content::NpcTemplateId::FIELD_WOLF
        );
    }
}
