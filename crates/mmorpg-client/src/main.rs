//! Minimal interactive Linux client technology spike.
//!
//! Bevy owns presentation and input. A dedicated TCP worker owns all socket
//! I/O so render and input systems never wait on the server. The line parser
//! below is deliberately temporary: the development server's `snapshot`
//! command provides a bounded bootstrap stream, while live gameplay still
//! arrives as development event lines. This adapter must be replaced by the
//! production wire protocol when that boundary is implemented.

use bevy::prelude::*;
use mmorpg_content::{NpcArchetype, starter_catalog};
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const FIELD_SIZE: Vec2 = Vec2::new(40.0, 30.0);
const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:4000";
const PLAYER_NAME: &str = "Aria";
const PLAYER_ROLE: &str = "damage";
const MOVEMENT_STEP: f32 = 2.0;
const MOVEMENT_REPEAT_SECONDS: f32 = 0.12;
const MAX_LOG_LINES: usize = 6;

#[derive(Component)]
struct StarterNpc {
    id: EntityId,
}

#[derive(Component)]
struct PlayerMarker;

#[derive(Component)]
struct StatusText;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct EntityId(u64);

#[derive(Clone, Copy, Debug, Default)]
struct NpcState {
    position: Vec2,
    health: u32,
    max_health: u32,
}

#[derive(Resource, Debug)]
struct ClientState {
    server_address: String,
    connection_status: String,
    player_id: Option<EntityId>,
    player_position: Vec2,
    player_health: u32,
    player_max_health: u32,
    player_target: Option<EntityId>,
    world_tick: Option<u64>,
    npcs: BTreeMap<EntityId, NpcState>,
    target_cursor: usize,
    logs: VecDeque<String>,
}

impl ClientState {
    fn new(server_address: String) -> Self {
        Self {
            server_address,
            connection_status: "connecting".to_owned(),
            player_id: None,
            player_position: Vec2::ZERO,
            player_health: 0,
            player_max_health: 0,
            player_target: None,
            world_tick: None,
            npcs: BTreeMap::new(),
            target_cursor: 0,
            logs: VecDeque::from(["WASD move  Tab target  Space attack".to_owned()]),
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
    Move { dx: f32, dy: f32 },
    Target(EntityId),
    Attack,
}

#[derive(Debug)]
enum NetworkEvent {
    Status(String),
    ServerLine(String),
}

#[derive(Resource)]
struct NetworkBridge {
    command_tx: Sender<ClientCommand>,
    event_rx: Arc<Mutex<Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
struct MovementRepeat(Timer);

fn main() {
    let server_address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_SERVER_ADDRESS.to_owned());
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    spawn_network_worker(server_address.clone(), command_rx, event_tx);

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MMORPG Client — interactive slice".to_owned(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.12, 0.18)))
        .insert_resource(ClientState::new(server_address))
        .insert_resource(NetworkBridge {
            command_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
        })
        .insert_resource(MovementRepeat(Timer::from_seconds(
            MOVEMENT_REPEAT_SECONDS,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup_scene)
        .add_systems(Startup, setup_ui)
        .add_systems(
            Update,
            (
                consume_network_events,
                keyboard_input,
                sync_authoritative_presentation,
                update_status_text,
            )
                .chain(),
        )
        .run();
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

    let npc_materials = [
        materials.add(Color::srgb(0.95, 0.78, 0.16)),
        materials.add(Color::srgb(0.72, 0.72, 0.76)),
    ];

    for (index, spawn) in catalog.zones[0].spawns.iter().enumerate() {
        let definition = catalog
            .npcs
            .iter()
            .find(|npc| npc.id == spawn.npc_template_id)
            .expect("every spawn must reference an NPC definition");
        let material = match definition.archetype {
            NpcArchetype::Vendor | NpcArchetype::QuestGiver => npc_materials[0].clone(),
            NpcArchetype::Enemy => npc_materials[1].clone(),
        };

        // Content coordinates use x/y; the 3D presentation maps content y to
        // world z. A primitive marker keeps every starter NPC visible.
        commands.spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.45, 1.0))),
            MeshMaterial3d(material),
            Transform::from_xyz(spawn.x, 0.75, spawn.y),
            StarterNpc {
                id: EntityId(index as u64 + 1),
            },
        ));

        println!(
            "  instantiated {} '{}' at ({:.1}, {:.1})",
            match definition.archetype {
                NpcArchetype::Vendor => "vendor",
                NpcArchetype::Enemy => "enemy",
                NpcArchetype::QuestGiver => "quest giver",
            },
            definition.name,
            spawn.x,
            spawn.y
        );
    }

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.1))),
        MeshMaterial3d(materials.add(Color::srgb(0.18, 0.46, 0.95))),
        Transform::from_xyz(0.0, 0.85, 0.0),
        PlayerMarker,
    ));
}

fn setup_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(12.0),
                left: px(12.0),
                width: px(460.0),
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
        });
}

fn spawn_network_worker(
    server_address: String,
    command_rx: Receiver<ClientCommand>,
    event_tx: Sender<NetworkEvent>,
) {
    thread::spawn(move || {
        let _ = event_tx.send(NetworkEvent::Status(format!(
            "connecting to {server_address}"
        )));
        let mut stream = match TcpStream::connect(&server_address) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = event_tx.send(NetworkEvent::Status(format!("connection failed: {error}")));
                return;
            }
        };
        if let Err(error) = stream.set_nonblocking(true) {
            let _ = event_tx.send(NetworkEvent::Status(format!(
                "cannot configure socket: {error}"
            )));
            return;
        }

        let _ = event_tx.send(NetworkEvent::Status("socket connected".to_owned()));
        let mut outgoing = VecDeque::<Vec<u8>>::new();
        queue_command_line(
            &mut outgoing,
            &format!("connect {PLAYER_NAME} {PLAYER_ROLE}"),
        );
        let mut incoming = Vec::new();
        let mut state_requested = false;
        let mut session_ready = false;
        let mut deferred_commands = VecDeque::new();

        loop {
            match command_rx.try_recv() {
                Err(TryRecvError::Disconnected) => break,
                Ok(command) if session_ready => queue_command(&mut outgoing, command),
                Ok(command) => deferred_commands.push_back(command),
                Err(TryRecvError::Empty) => {}
            }

            if !flush_outgoing(&mut stream, &mut outgoing) {
                let _ = event_tx.send(NetworkEvent::Status("connection write failed".to_owned()));
                break;
            }

            let mut buffer = [0_u8; 4096];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "server closed the connection".to_owned(),
                        ));
                        return;
                    }
                    Ok(bytes_read) => {
                        incoming.extend_from_slice(&buffer[..bytes_read]);
                        while let Some(newline) = incoming.iter().position(|byte| *byte == b'\n') {
                            let line = incoming.drain(..=newline).collect::<Vec<_>>();
                            let line = String::from_utf8_lossy(&line[..line.len() - 1])
                                .trim_end_matches('\r')
                                .to_owned();
                            if line.starts_with("CONNECTED ") && !state_requested {
                                session_ready = true;
                                queue_command_line(&mut outgoing, "snapshot");
                                state_requested = true;
                                while let Some(command) = deferred_commands.pop_front() {
                                    queue_command(&mut outgoing, command);
                                }
                            }
                            if event_tx.send(NetworkEvent::ServerLine(line)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "connection read failed: {error}"
                        )));
                        return;
                    }
                }
            }

            thread::sleep(Duration::from_millis(5));
        }
    });
}

fn queue_command_line(outgoing: &mut VecDeque<Vec<u8>>, command: &str) {
    let mut line = command.as_bytes().to_vec();
    line.push(b'\n');
    outgoing.push_back(line);
}

fn queue_command(outgoing: &mut VecDeque<Vec<u8>>, command: ClientCommand) {
    match command {
        ClientCommand::Move { dx, dy } => queue_command_line(outgoing, &format!("move {dx} {dy}")),
        ClientCommand::Target(EntityId(id)) => {
            queue_command_line(outgoing, &format!("target {id}"));
        }
        ClientCommand::Attack => queue_command_line(outgoing, "attack"),
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
                state.connection_status = status.clone();
                state.log(status);
            }
            NetworkEvent::ServerLine(line) => apply_server_line(&mut state, &line),
        }
    }
}

fn keyboard_input(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut repeat: ResMut<MovementRepeat>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
) {
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
        send_command(&bridge, &mut state, ClientCommand::Attack);
    }
}

fn send_command(bridge: &NetworkBridge, state: &mut ClientState, command: ClientCommand) {
    if bridge.command_tx.send(command).is_err() {
        state.connection_status = "network worker stopped".to_owned();
        state.log("network worker stopped".to_owned());
    }
}

fn next_target(state: &mut ClientState) -> Option<EntityId> {
    let ids: Vec<_> = state.npcs.keys().copied().collect();
    let target = ids.get(state.target_cursor % ids.len().max(1)).copied()?;
    state.target_cursor = (state.target_cursor + 1) % ids.len();
    Some(target)
}

fn sync_authoritative_presentation(
    state: Res<ClientState>,
    mut player_query: Query<&mut Transform, (With<PlayerMarker>, Without<StarterNpc>)>,
    mut npc_query: Query<(&StarterNpc, &mut Transform), Without<PlayerMarker>>,
) {
    if let Ok(mut transform) = player_query.single_mut() {
        transform.translation.x = state.player_position.x;
        transform.translation.z = state.player_position.y;
    }
    for (marker, mut transform) in &mut npc_query {
        if let Some(npc) = state.npcs.get(&marker.id) {
            transform.translation.x = npc.position.x;
            transform.translation.z = npc.position.y;
            transform.translation.y = if npc.health == 0 { 0.2 } else { 0.75 };
        }
    }
}

fn update_status_text(state: Res<ClientState>, mut query: Query<&mut Text, With<StatusText>>) {
    let Ok(mut text) = query.single_mut() else {
        return;
    };
    let target = state.player_target.map_or_else(
        || "none".to_owned(),
        |target| {
            state.npcs.get(&target).map_or_else(
                || target.0.to_string(),
                |npc| format!("{} ({}/{})", target.0, npc.health, npc.max_health),
            )
        },
    );
    let player = state.player_id.map_or_else(
        || "not connected".to_owned(),
        |id| {
            format!(
                "player {}  hp {}/{}  target {}",
                id.0, state.player_health, state.player_max_health, target
            )
        },
    );
    let tick = state
        .world_tick
        .map_or_else(|| "-".to_owned(), |tick| tick.to_string());
    let logs = state.logs.iter().cloned().collect::<Vec<_>>().join("\n");
    text.0 = format!(
        "server: {} ({})\n{}  tick {}\n\n{}",
        state.connection_status, state.server_address, player, tick, logs
    );
}

fn apply_server_line(state: &mut ClientState, line: &str) {
    // TEMPORARY ADAPTER: these diagnostics are not a stable protocol. Keep
    // all gameplay state changes in this function sourced from server lines.
    let fields = line.split_whitespace().collect::<Vec<_>>();
    match fields.first().copied() {
        Some("TEMP_SNAPSHOT_BEGIN") | Some("TEMP_SNAPSHOT_END") => {}
        Some("TEMP_SNAPSHOT") => apply_snapshot_record(state, &fields),
        Some("WORLD") => {
            state.world_tick = field(&fields, "tick").and_then(|value| value.parse().ok());
        }
        Some("CONNECTED") => {
            state.player_id = field(&fields, "player_id").and_then(parse_entity_id);
            state.connection_status = "authenticated development session".to_owned();
            state.log(format!(
                "connected as {}",
                state
                    .player_id
                    .map_or_else(|| "unknown".to_owned(), |id| id.0.to_string())
            ));
        }
        Some("PLAYER") => apply_player_state(state, &fields),
        Some("NPC") => apply_npc_state(state, &fields),
        Some("EVENT") => apply_event(state, &fields, line),
        Some("ERR") => state.log(line.to_owned()),
        Some("WELCOME") => state.log("server greeted client"),
        Some("TYPE") => {}
        Some(_) | None => state.log(line.to_owned()),
    }
}

fn apply_snapshot_record(state: &mut ClientState, fields: &[&str]) {
    match fields.get(1).copied() {
        Some("WORLD") => {
            state.world_tick = field(fields, "tick").and_then(|value| value.parse().ok());
        }
        Some("PLAYER") => apply_snapshot_player_state(state, fields),
        Some("NPC") => apply_snapshot_npc_state(state, fields),
        _ => state.log("invalid temporary snapshot record"),
    }
}

fn apply_snapshot_player_state(state: &mut ClientState, fields: &[&str]) {
    let Some(id) = field(fields, "id").and_then(parse_entity_id) else {
        return;
    };
    if Some(id) != state.player_id {
        return;
    }
    if let Some(position) = field(fields, "position").and_then(parse_position) {
        state.player_position = position;
    }
    if let (Some(health), Some(max_health)) = (
        field(fields, "health").and_then(|value| value.parse().ok()),
        field(fields, "max_health").and_then(|value| value.parse().ok()),
    ) {
        state.player_health = health;
        state.player_max_health = max_health;
    }
    state.player_target = field(fields, "target").and_then(parse_optional_entity_id);
}

fn apply_snapshot_npc_state(state: &mut ClientState, fields: &[&str]) {
    let Some(id) = field(fields, "id").and_then(parse_entity_id) else {
        return;
    };
    let Some(position) = field(fields, "position").and_then(parse_position) else {
        return;
    };
    let (Some(health), Some(max_health)) = (
        field(fields, "health").and_then(|value| value.parse().ok()),
        field(fields, "max_health").and_then(|value| value.parse().ok()),
    ) else {
        return;
    };
    state.npcs.insert(
        id,
        NpcState {
            position,
            health,
            max_health,
        },
    );
}

fn apply_player_state(state: &mut ClientState, fields: &[&str]) {
    let Some(id) = field(fields, "id").and_then(parse_entity_id) else {
        return;
    };
    if Some(id) != state.player_id {
        return;
    }
    if let Some(position) = field(fields, "pos").and_then(parse_position) {
        state.player_position = position;
    }
    if let Some((health, max_health)) = field(fields, "hp").and_then(parse_health) {
        state.player_health = health;
        state.player_max_health = max_health;
    }
    state.player_target = field(fields, "target").and_then(parse_optional_entity_id);
}

fn apply_npc_state(state: &mut ClientState, fields: &[&str]) {
    let Some(id) = field(fields, "id").and_then(parse_entity_id) else {
        return;
    };
    let Some(position) = field(fields, "pos").and_then(parse_position) else {
        return;
    };
    let Some((health, max_health)) = field(fields, "hp").and_then(parse_health) else {
        return;
    };
    state.npcs.insert(
        id,
        NpcState {
            position,
            health,
            max_health,
        },
    );
}

fn apply_event(state: &mut ClientState, fields: &[&str], line: &str) {
    let Some(kind) = fields.get(1).copied() else {
        return;
    };
    match kind {
        "player_moved" if event_player_is_local(state, fields) => {
            if let Some(position) = field(fields, "pos").and_then(parse_position) {
                state.player_position = position;
            }
        }
        "target_selected" if event_player_is_local(state, fields) => {
            state.player_target = field(fields, "target").and_then(parse_entity_id);
            state.log(format!(
                "target {}",
                state
                    .player_target
                    .map_or_else(|| "none".to_owned(), |id| id.0.to_string())
            ));
        }
        "attack" if event_player_is_local(state, fields) => {
            if let Some(target) = field(fields, "target").and_then(parse_entity_id)
                && let Some(npc) = state.npcs.get_mut(&target)
                && let Some(health) =
                    field(fields, "target_hp").and_then(|value| value.parse().ok())
            {
                npc.health = health;
            }
            state.log("server resolved attack".to_owned());
        }
        "enemy_defeated" => {
            if let Some(enemy) = field(fields, "id").and_then(parse_entity_id)
                && let Some(npc) = state.npcs.get_mut(&enemy)
            {
                npc.health = 0;
            }
            state.log("enemy defeated; server decides rewards".to_owned());
        }
        "rejected" | "transaction_rejected" | "quest_rejected" => state.log(line.to_owned()),
        "item_purchased" | "loot_rewarded" | "quest_rewarded" => state.log(line.to_owned()),
        _ => {}
    }
}

fn event_player_is_local(state: &ClientState, fields: &[&str]) -> bool {
    field(fields, "player")
        .or_else(|| field(fields, "id"))
        .and_then(parse_entity_id)
        == state.player_id
}

fn field<'a>(fields: &'a [&str], key: &str) -> Option<&'a str> {
    fields.iter().find_map(|field| {
        let (field_key, value) = field.split_once('=')?;
        (field_key == key).then_some(value)
    })
}

fn parse_entity_id(value: &str) -> Option<EntityId> {
    value
        .parse::<u64>()
        .ok()
        .map(EntityId)
        .filter(|id| id.0 != 0)
}

fn parse_optional_entity_id(value: &str) -> Option<EntityId> {
    (value != "none").then(|| parse_entity_id(value)).flatten()
}

fn parse_position(value: &str) -> Option<Vec2> {
    let (x, y) = value.split_once(',')?;
    Some(Vec2::new(x.parse().ok()?, y.parse().ok()?))
}

fn parse_health(value: &str) -> Option<(u32, u32)> {
    let (health, max_health) = value.split_once('/')?;
    Some((health.parse().ok()?, max_health.parse().ok()?))
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

    #[test]
    fn projects_player_and_npc_state_only_from_server_lines() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(
            &mut state,
            "PLAYER id=5 name=Aria role=damage pos=4.00,-2.00 hp=90/100 gold=20 target=2",
        );
        apply_server_line(
            &mut state,
            "NPC id=2 name=Field kind=Enemy pos=24.00,0.00 hp=100/100",
        );
        apply_server_line(
            &mut state,
            "EVENT player_moved id=5 pos=6.00,-1.00 area=Field",
        );
        apply_server_line(
            &mut state,
            "EVENT attack player=5 target=2 damage=25 target_hp=75",
        );

        assert_eq!(state.player_id, Some(EntityId(5)));
        assert_eq!(state.player_position, Vec2::new(6.0, -1.0));
        assert_eq!(state.player_health, 90);
        assert_eq!(state.player_target, Some(EntityId(2)));
        assert_eq!(state.npcs[&EntityId(2)].health, 75);
    }

    #[test]
    fn tab_targeting_cycles_server_known_entities() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        state.npcs.insert(EntityId(4), NpcState::default());
        state.npcs.insert(EntityId(2), NpcState::default());

        assert_eq!(next_target(&mut state), Some(EntityId(2)));
        assert_eq!(next_target(&mut state), Some(EntityId(4)));
        assert_eq!(next_target(&mut state), Some(EntityId(2)));
    }

    #[test]
    fn projects_the_machine_snapshot_records_used_by_the_graphical_client() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(&mut state, "TEMP_SNAPSHOT_BEGIN version=1");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT WORLD tick=17 players=1 npcs=4 enemies=3 vendors=1",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=3.0,-4.0 health=88 max_health=100 gold=20 target=2",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=24.0,0.0 health=100 max_health=100",
        );

        assert_eq!(state.world_tick, Some(17));
        assert_eq!(state.player_position, Vec2::new(3.0, -4.0));
        assert_eq!(state.player_health, 88);
        assert_eq!(state.player_target, Some(EntityId(2)));
        assert_eq!(state.npcs[&EntityId(2)].position, Vec2::new(24.0, 0.0));
        assert_eq!(state.npcs[&EntityId(2)].health, 100);
    }
}
