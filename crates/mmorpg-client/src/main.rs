//! Minimal interactive Linux client technology spike.
//!
//! Bevy owns presentation and input. A dedicated TCP worker owns all socket
//! I/O so render and input systems never wait on the server. The default line
//! connection remains available for local compatibility. An
//! opt-in wire address uses typed command frames and typed server messages;
//! both paths feed the same renderer-independent presentation model.

use bevy::prelude::*;
use mmorpg_client_adapter::{
    apply_event as apply_presentation_event, apply_snapshot as apply_presentation_snapshot,
    apply_wire_message,
};
use mmorpg_client_model::{ClientEntity, ClientWorld};
use mmorpg_client_protocol::{
    EntityId, NpcKind, ServerEvent, ServerLine, Snapshot, SnapshotAssembler, decode_server_line,
};
use mmorpg_content::{ItemId, QuestId, item_definition, starter_catalog};
use mmorpg_wire::{
    ClientCommand as WireCommand, DecodeError as WireDecodeError, Envelope, MessageKind,
    ServerMessage, decode_one,
};
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
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
const COMMAND_QUEUE_CAPACITY: usize = 64;
const MAX_DEFERRED_COMMANDS: usize = 32;
const MAX_OUTGOING_LINES: usize = 64;
const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
    Move {
        dx: f32,
        dy: f32,
    },
    Target(EntityId),
    Attack,
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
}

#[derive(Debug)]
enum NetworkEvent {
    Status(String),
    ServerLine(String),
    ServerMessage(ServerMessage),
}

#[derive(Resource)]
struct NetworkBridge {
    command_tx: SyncSender<ClientCommand>,
    event_rx: Arc<Mutex<Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
struct MovementRepeat(Timer);

fn main() {
    let mut arguments = std::env::args().skip(1);
    let server_address = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_SERVER_ADDRESS.to_owned());
    let mut wire_address = None;
    while let Some(argument) = arguments.next() {
        if argument != "--wire-address" {
            eprintln!("unknown argument '{argument}'");
            return;
        }
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
    let display_address = wire_address
        .clone()
        .unwrap_or_else(|| server_address.clone());
    let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
    let (event_tx, event_rx) = mpsc::channel();
    if let Some(wire_address) = wire_address {
        spawn_wire_network_worker(wire_address, command_rx, event_tx);
    } else {
        spawn_network_worker(server_address, command_rx, event_tx);
    }

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
        .insert_resource(ClientState::new(display_address))
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

fn setup_ui(mut commands: Commands) {
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
        let Some(address) = server_address
            .to_socket_addrs()
            .ok()
            .and_then(|mut addresses| addresses.next())
        else {
            let _ = event_tx.send(NetworkEvent::Status(
                "connection address could not be resolved".to_owned(),
            ));
            return;
        };
        let mut stream = match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
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
                Ok(command) if session_ready => queue_command_bounded(&mut outgoing, command),
                Ok(command) if deferred_commands.len() < MAX_DEFERRED_COMMANDS => {
                    deferred_commands.push_back(command)
                }
                Ok(_) => {}
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
                                    queue_command_bounded(&mut outgoing, command);
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

/// Runs the typed wire transport on the same dedicated worker used by the
/// legacy line path. Socket ownership remains outside Bevy's render and input
/// systems, and the bounded frame buffer rejects malformed or oversized data
/// before it reaches the presentation adapter.
fn spawn_wire_network_worker(
    server_address: String,
    command_rx: Receiver<ClientCommand>,
    event_tx: Sender<NetworkEvent>,
) {
    thread::spawn(move || {
        let _ = event_tx.send(NetworkEvent::Status(format!(
            "connecting to typed wire {server_address}"
        )));
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
        let mut stream = match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = event_tx.send(NetworkEvent::Status(format!(
                    "wire connection failed: {error}"
                )));
                return;
            }
        };
        if let Err(error) = stream.set_nonblocking(true) {
            let _ = event_tx.send(NetworkEvent::Status(format!(
                "cannot configure wire socket: {error}"
            )));
            return;
        }

        let _ = event_tx.send(NetworkEvent::Status(
            "typed wire socket connected".to_owned(),
        ));
        let mut outgoing = VecDeque::<Vec<u8>>::new();
        queue_wire_command(
            &mut outgoing,
            WireCommand::Join {
                name: PLAYER_NAME.to_owned(),
                role: mmorpg_wire::RoleCode::DamageDealer,
            },
        );
        let mut incoming = Vec::new();
        let mut session_ready = false;
        let mut deferred_commands = VecDeque::new();

        loop {
            match command_rx.try_recv() {
                Err(TryRecvError::Disconnected) => break,
                Ok(command) if session_ready => {
                    queue_wire_command(&mut outgoing, client_command_to_wire(command))
                }
                Ok(command) if deferred_commands.len() < MAX_DEFERRED_COMMANDS => {
                    deferred_commands.push_back(command)
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => {}
            }

            if !flush_outgoing(&mut stream, &mut outgoing) {
                let _ = event_tx.send(NetworkEvent::Status("typed wire write failed".to_owned()));
                break;
            }

            let mut buffer = [0_u8; 4096];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "typed wire server closed the connection".to_owned(),
                        ));
                        return;
                    }
                    Ok(bytes_read) => {
                        incoming.extend_from_slice(&buffer[..bytes_read]);
                        if incoming.len() > MAX_WIRE_INPUT_BYTES {
                            let _ = event_tx.send(NetworkEvent::Status(
                                "typed wire input buffer exceeded its limit".to_owned(),
                            ));
                            return;
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "typed wire read failed: {error}"
                        )));
                        return;
                    }
                }
            }

            loop {
                let decoded = match decode_one(&incoming) {
                    Ok(decoded) => decoded,
                    Err(WireDecodeError::Truncated { .. }) => break,
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "typed wire frame rejected: {error}"
                        )));
                        return;
                    }
                };
                let consumed = decoded.consumed;
                let kind = decoded.envelope.kind;
                let payload = decoded.envelope.payload;
                incoming.drain(..consumed);
                if kind != MessageKind::Event {
                    let _ = event_tx.send(NetworkEvent::Status(
                        "typed wire server sent a non-event message".to_owned(),
                    ));
                    return;
                }
                let message = match ServerMessage::decode_payload(&payload) {
                    Ok(message) => message,
                    Err(error) => {
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "typed wire server message rejected: {error}"
                        )));
                        return;
                    }
                };
                if let ServerMessage::Connected { .. } = message {
                    session_ready = true;
                    queue_wire_command(&mut outgoing, WireCommand::Snapshot);
                    while let Some(command) = deferred_commands.pop_front() {
                        queue_wire_command(&mut outgoing, client_command_to_wire(command));
                    }
                }
                if event_tx.send(NetworkEvent::ServerMessage(message)).is_err() {
                    return;
                }
            }

            thread::sleep(Duration::from_millis(5));
        }
    });
}

fn client_command_to_wire(command: ClientCommand) -> WireCommand {
    match command {
        ClientCommand::Move { dx, dy } => WireCommand::Move { dx, dy },
        ClientCommand::Target(EntityId(target_id)) => WireCommand::SelectTarget { target_id },
        ClientCommand::Attack => WireCommand::BasicAttack,
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

fn queue_command_line(outgoing: &mut VecDeque<Vec<u8>>, command: &str) {
    let mut line = command.as_bytes().to_vec();
    line.push(b'\n');
    outgoing.push_back(line);
}

fn queue_command_bounded(outgoing: &mut VecDeque<Vec<u8>>, command: ClientCommand) {
    if outgoing.len() >= MAX_OUTGOING_LINES {
        return;
    }
    match command {
        ClientCommand::Move { dx, dy } => queue_command_line(outgoing, &format!("move {dx} {dy}")),
        ClientCommand::Target(EntityId(id)) => {
            queue_command_line(outgoing, &format!("target {id}"));
        }
        ClientCommand::Attack => queue_command_line(outgoing, "attack"),
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
                state.connection_status = status.clone();
                state.log(status);
            }
            NetworkEvent::ServerLine(line) => apply_server_line(&mut state, &line),
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
    let player = state.player_id.and_then(|id| state.presentation.player(id));
    let target = player.and_then(|player| player.target).map_or_else(
        || "none".to_owned(),
        |target| {
            state.presentation.npc(target).map_or_else(
                || target.0.to_string(),
                |npc| format!("{} ({}/{})", target.0, npc.health, npc.max_health),
            )
        },
    );
    let player_summary = state.player_id.map_or_else(
        || "not connected".to_owned(),
        |id| match state.presentation.player(id) {
            Some(player) => format!(
                "player {}  hp {}/{}  target {}",
                id.0, player.health, player.max_health, target
            ),
            None => format!("player {}  waiting for snapshot", id.0),
        },
    );
    let tick = state
        .presentation
        .world_tick()
        .map_or_else(|| "-".to_owned(), |tick| tick.to_string());
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
        "server: {} ({})\n{}  tick {}\n{}\n{}\n{}\n{}\n\ncontrols: WASD move | Tab target | Space attack | L loot | V vendor | B buy | O offers | E accept | R turn in{}\n\n{}",
        state.connection_status,
        state.server_address,
        player_summary,
        tick,
        inventory,
        quests,
        vendor,
        offers,
        notification,
        logs,
    )
}

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
    match message {
        ServerMessage::Welcome { server } => state.log(format!("server greeted {server}")),
        ServerMessage::Connected { player_id, .. } => {
            state.player_id = Some(EntityId(*player_id));
            state.connection_status = "authenticated typed development session".to_owned();
            state.log(format!("connected as {player_id}"));
        }
        ServerMessage::Error { message } => state.log(format!("rejected: {message}")),
        ServerMessage::Event(_) | ServerMessage::Snapshot(_) => {}
    }
    if let Err(error) = apply_wire_message(&mut state.presentation, message) {
        state.log(format!("authoritative presentation rejected: {error}"));
    }
}

fn apply_snapshot(state: &mut ClientState, snapshot: Snapshot) {
    if let Err(error) = apply_presentation_snapshot(&mut state.presentation, &snapshot) {
        state.log(format!("authoritative presentation rejected: {error}"));
        return;
    }
}

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

    #[test]
    fn projects_player_and_npc_state_only_from_server_lines() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(&mut state, "TEMP_SNAPSHOT_BEGIN version=1");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT WORLD tick=10 players=1 npcs=1 enemies=1 vendors=0",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=4.0,-2.0 health=90 max_health=100 gold=20 target=2",
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
            "TEMP_SNAPSHOT_BEGIN version=1",
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
    fn projects_the_machine_snapshot_records_used_by_the_graphical_client() {
        let mut state = ClientState::new(DEFAULT_SERVER_ADDRESS.to_owned());
        apply_server_line(&mut state, "CONNECTED player_id=5 role=damage");
        apply_server_line(&mut state, "TEMP_SNAPSHOT_BEGIN version=1");
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT WORLD tick=17 players=1 npcs=2 enemies=1 vendors=1",
        );
        apply_server_line(
            &mut state,
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=3.0,-4.0 health=88 max_health=100 gold=20 target=2",
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
