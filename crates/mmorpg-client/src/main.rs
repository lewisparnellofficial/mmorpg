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
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use ui_scripting_spike::{AddonPolicy, AddonRunner};

mod addon;
mod config;
mod diagnostics;
mod input;
mod network;
mod presentation;
mod scene;
mod types;
mod ui;

pub(crate) use addon::*;
pub(crate) use config::*;
pub(crate) use diagnostics::*;
pub(crate) use input::*;
pub(crate) use network::*;
pub(crate) use presentation::*;
pub(crate) use scene::*;
pub(crate) use types::*;
pub(crate) use ui::*;

fn main() {
    let Some(config) = ClientConfig::parse() else {
        return;
    };
    let ClientConfig {
        typed_address,
        preferred_character_id,
        addon_root,
        addon_process_host,
        addon_process_package_root,
        acceptance_smoke,
        frame_time_stats,
        render_backend,
        ..
    } = config;
    let (scripted_ui, addon_process_supervisor) = if let Some(host_path) = addon_process_host {
        match build_process_scripted_ui_presentation(
            Path::new(&host_path),
            addon_process_package_root.as_deref().map(Path::new),
        ) {
            Ok(result) => (result.0, Some(result.1)),
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        }
    } else {
        match build_scripted_ui_presentation(addon_root.as_deref().map(Path::new)) {
            Ok(scripted_ui) => (scripted_ui, None),
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        }
    };
    if addon_root.is_none() && addon_process_supervisor.is_none() {
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
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(render_plugin(render_backend))
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "MMORPG Client — interactive slice".to_owned(),
                    resolution: (1280, 720).into(),
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
    );
    if let Some(supervisor) = addon_process_supervisor {
        app.insert_resource(supervisor);
    }
    app.run();
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
    fn render_backend_selector_keeps_automatic_and_explicit_masks_distinct() {
        assert_eq!(RenderBackendChoice::Automatic.backends(), None);
        assert_eq!(
            RenderBackendChoice::Vulkan.backends(),
            Some(Backends::VULKAN)
        );
        assert_eq!(RenderBackendChoice::Gl.backends(), Some(Backends::GL));
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
