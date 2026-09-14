use mmorpg_core::{Command, EntityId, Event, ItemId, QuestId, Role};
use mmorpg_server::test_support::{ParsedLineExt, parse_line};
use mmorpg_server::{
    AuthenticatedSession, ClientOrigin, DEFAULT_CAST_TIME_TICKS, DEFAULT_COMBAT_COOLDOWN_TICKS,
    DEFAULT_TICK_HZ, DISCONNECT_GRACE_TICKS, DetachedCharacter, MAX_COMMANDS_PER_TICK,
    MAX_PENDING_COMMANDS, MAX_REPLACEABLE_EVENTS, MAX_WIRE_CLIENTS, MAX_WIRE_OUTPUT_BYTES,
    PendingCommand, Server, SimulationTimingStats, WireClient, interleave_wire_commands,
    wire_command_to_core,
};
use mmorpg_wire::{
    ClientCommand as WireCommand, SequencedServerMessage, ServerEvent, ServerMessage, ZoneAreaCode,
    decode_one,
};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

#[test]
fn development_protocol_requires_a_bound_player_for_gameplay() {
    assert!(parse_line("move 1 0", None).is_err());
    assert!(parse_line("attack", None).is_err());
    assert!(parse_line("connect Alice tank", None).is_ok());
}

#[test]
fn development_protocol_only_allows_a_client_to_move_its_own_player() {
    let own = Some(EntityId(7));
    assert!(parse_line("move 7 1 0", own).is_ok());
    assert!(parse_line("move 8 1 0", own).is_err());
}

#[test]
fn development_protocol_parses_economy_commands_for_bound_player() {
    let own = Some(EntityId(7));
    assert_eq!(
        parse_line("vendor 1", own).unwrap_command(),
        Command::ListVendor {
            player_id: EntityId(7),
            vendor_id: EntityId(1),
        }
    );
    assert_eq!(
        parse_line("buy 1 2 3", own).unwrap_command(),
        Command::BuyItem {
            player_id: EntityId(7),
            vendor_id: EntityId(1),
            item_id: ItemId(2),
            quantity: 3,
        }
    );
    assert_eq!(
        parse_line("loot 12", own).unwrap_command(),
        Command::LootEnemy {
            player_id: EntityId(7),
            enemy_id: EntityId(12),
        }
    );
    assert!(parse_line("buy 1 2 0", own).is_err());
    assert!(parse_line("loot 12", None).is_err());
}

#[test]
fn development_protocol_parses_quest_commands_for_bound_player() {
    let own = Some(EntityId(7));
    assert_eq!(
        parse_line("quest-offers 1", own).unwrap_command(),
        Command::ListQuestOffers {
            player_id: EntityId(7),
            npc_id: EntityId(1),
        }
    );
    assert_eq!(
        parse_line("accept-quest 1 1", own).unwrap_command(),
        Command::AcceptQuest {
            player_id: EntityId(7),
            npc_id: EntityId(1),
            quest_id: QuestId(1),
        }
    );
    assert_eq!(
        parse_line("turn-in-quest 1 1", own).unwrap_command(),
        Command::TurnInQuest {
            player_id: EntityId(7),
            npc_id: EntityId(1),
            quest_id: QuestId(1),
        }
    );
    assert!(parse_line("accept-quest 1 nope", own).is_err());
}

#[test]
fn line_and_typed_adapters_preserve_the_starter_transcript() {
    let player_id = EntityId(7);
    let transcript = [
        ("move 1.5 -2", WireCommand::Move { dx: 1.5, dy: -2.0 }),
        ("target 12", WireCommand::SelectTarget { target_id: 12 }),
        ("attack", WireCommand::BasicAttack),
        ("vendor 1", WireCommand::ListVendor { vendor_id: 1 }),
        (
            "buy 1 2 3",
            WireCommand::BuyItem {
                vendor_id: 1,
                item_id: 2,
                quantity: 3,
            },
        ),
        ("loot 12", WireCommand::LootEnemy { enemy_id: 12 }),
        ("quest-offers 1", WireCommand::ListQuestOffers { npc_id: 1 }),
        (
            "accept-quest 1 1",
            WireCommand::AcceptQuest {
                npc_id: 1,
                quest_id: 1,
            },
        ),
        (
            "turn-in-quest 1 1",
            WireCommand::TurnInQuest {
                npc_id: 1,
                quest_id: 1,
            },
        ),
    ];

    for (line, typed) in transcript {
        let line_command = parse_line(line, Some(player_id)).unwrap_command();
        let typed_command =
            wire_command_to_core(typed, player_id).expect("typed transcript should adapt");
        assert_eq!(line_command, typed_command, "transcript diverged at {line}");
    }
}

#[test]
fn typed_pending_command_queue_has_a_hard_bound() {
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    for _ in 0..(MAX_PENDING_COMMANDS + 1) {
        server.enqueue_pending_wire_command(
            99,
            Command::Move {
                player_id: EntityId(5),
                dx: 1.0,
                dy: 0.0,
            },
        );
    }
    assert_eq!(server.commands.len(), MAX_PENDING_COMMANDS);
}

#[test]
fn simulation_tick_command_budget_rejects_overflow_without_debt() {
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    for _ in 0..=MAX_COMMANDS_PER_TICK {
        server.commands.push_back(PendingCommand {
            origin: ClientOrigin::Wire(1),
            command: Command::Move {
                player_id: EntityId(5),
                dx: 1.0,
                dy: 0.0,
            },
            operation: None,
        });
    }

    let accepted = server.take_tick_commands();
    assert_eq!(accepted.len(), MAX_COMMANDS_PER_TICK);
    assert!(server.commands.is_empty());
}

#[test]
fn live_server_uses_fixed_deferred_combat_timing() {
    let server = Server::new(DEFAULT_TICK_HZ, false, None);

    assert_eq!(server.combat_timing.tick_hz(), DEFAULT_TICK_HZ as u32);
    assert_eq!(
        server.combat_timing.cast_time_ticks(),
        DEFAULT_CAST_TIME_TICKS
    );
    assert_eq!(
        server.combat_timing.cooldown_ticks(),
        DEFAULT_COMBAT_COOLDOWN_TICKS
    );
}

#[test]
fn request_wrapper_correlates_an_immediate_session_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, true, None);
    server.wire_clients.push(WireClient::new(1, stream));

    server.handle_wire_command(
        1,
        WireCommand::Request {
            request_id: 19,
            command: Box::new(WireCommand::Authenticate {
                token: "dev-local".to_owned(),
            }),
        },
    );

    let frame: Vec<_> = server.wire_clients[0].output.drain(..).collect();
    let decoded = decode_one(&frame).expect("correlated response should decode");
    let sequenced = SequencedServerMessage::decode_payload(&decoded.envelope.payload)
        .expect("server response should carry a sequence");
    assert_eq!(sequenced.sequence, 1);
    assert_eq!(
        sequenced.message,
        ServerMessage::Response {
            request_id: 19,
            message: Box::new(ServerMessage::Authenticated {
                account_id: 1,
                session_id: 1,
            }),
        }
    );
}

#[test]
fn request_wrapper_correlates_an_asynchronous_join_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, true, None);
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 1,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    server.wire_clients.push(client);

    server.handle_wire_command(
        1,
        WireCommand::Request {
            request_id: 27,
            command: Box::new(WireCommand::EnterWorld),
        },
    );
    server.next_tick = Instant::now() - Duration::from_millis(1);
    server.advance_if_due();

    let frame: Vec<_> = server.wire_clients[0].output.drain(..).collect();
    let mut correlated_join = false;
    let mut remaining = frame.as_slice();
    while !remaining.is_empty() {
        let decoded = decode_one(remaining).expect("join response should decode");
        let sequenced = SequencedServerMessage::decode_payload(&decoded.envelope.payload)
            .expect("join response should carry a sequence");
        if sequenced.message
            == (ServerMessage::Response {
                request_id: 27,
                message: Box::new(ServerMessage::Connected {
                    player_id: 5,
                    role: mmorpg_wire::RoleCode::DamageDealer,
                }),
            })
        {
            correlated_join = true;
            break;
        }
        remaining = &remaining[decoded.consumed..];
    }
    assert!(
        correlated_join,
        "asynchronous join response lost its request ID"
    );
}

#[test]
fn simulation_timing_stats_count_deadline_misses_and_track_maximum() {
    let mut stats = SimulationTimingStats::default();
    stats.record(Duration::from_millis(2), Duration::from_millis(5));
    stats.record(Duration::from_millis(8), Duration::from_millis(5));

    assert_eq!(stats.ticks, 2);
    assert_eq!(stats.deadline_misses, 1);
    assert_eq!(stats.max_duration, Duration::from_millis(8));
}

#[test]
fn wire_accepts_are_bounded_before_session_creation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    let mut peers = Vec::new();
    for _ in 0..MAX_WIRE_CLIENTS {
        let peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        assert!(server.add_wire_client(stream));
        peers.push(peer);
    }
    let rejected_peer = TcpStream::connect(address).expect("connect rejected peer");
    let (rejected_stream, _) = listener.accept().expect("accept rejected peer");
    assert!(!server.add_wire_client(rejected_stream));
    assert_eq!(server.wire_clients.len(), MAX_WIRE_CLIENTS);
    drop(rejected_peer);
    drop(peers);
}

#[test]
fn typed_intake_interleaves_clients_before_authoritative_application() {
    let commands = interleave_wire_commands(vec![
        vec![
            (1, WireCommand::Move { dx: 1.0, dy: 0.0 }),
            (1, WireCommand::BasicAttack),
            (1, WireCommand::Snapshot),
        ],
        vec![(2, WireCommand::Move { dx: -1.0, dy: 0.0 })],
    ]);
    let owners: Vec<_> = commands.iter().map(|(owner, _)| *owner).collect();
    assert_eq!(owners, vec![1, 2, 1, 1]);
}

#[test]
fn active_character_fence_is_scoped_to_account_and_character() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    let mut first = WireClient::new(1, stream);
    first.authenticated = Some(AuthenticatedSession {
        account_id: 7,
        session_id: 1,
    });
    first.selected_character_id = Some(42);
    server.wire_clients.push(first);

    assert!(server.character_reserved_by_other(2, 7, 42));
    assert!(!server.character_reserved_by_other(2, 8, 42));
    assert!(!server.character_reserved_by_other(2, 7, 43));
    assert!(!server.character_reserved_by_other(1, 7, 42));
}

#[test]
fn disconnected_character_rebinds_within_grace_without_joining_again() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    let player = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    server.detached_characters.insert(
        (1, 1),
        DetachedCharacter {
            player_id: player,
            expires_at_tick: DISCONNECT_GRACE_TICKS,
        },
    );
    let mut client = WireClient::new(1, stream);
    client.authenticated = Some(AuthenticatedSession {
        account_id: 1,
        session_id: 2,
    });
    client.selected_character_id = Some(1);
    client.content_compatible = true;
    server.wire_clients.push(client);

    server.handle_wire_command(1, WireCommand::EnterWorld);

    assert_eq!(server.wire_clients[0].player_id, Some(player));
    assert!(server.detached_characters.is_empty());
    assert!(server.commands.is_empty());
}

#[test]
fn disconnected_character_expires_into_an_authoritative_leave() {
    let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
    let player = server
        .world
        .step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }])
        .into_iter()
        .find_map(|event| match event {
            Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("test player should join");
    server.detached_characters.insert(
        (1, 1),
        DetachedCharacter {
            player_id: player,
            expires_at_tick: 0,
        },
    );

    server.expire_detached_characters();

    assert!(server.detached_characters.is_empty());
    assert!(matches!(
        server.commands.front().map(|pending| &pending.command),
        Some(Command::LeavePlayer { player_id }) if *player_id == player
    ));
}

#[test]
fn slow_client_delivery_is_bounded_and_evicted() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut client = WireClient::new(1, stream);

    for _ in 0..20_000 {
        client.queue_server_message(&ServerMessage::Welcome {
            server: "mmorpg".to_owned(),
        });
        if client.closed {
            break;
        }
    }

    assert!(client.closed, "a saturated client must be evicted");
    assert!(client.output.len() <= MAX_WIRE_OUTPUT_BYTES);

    let replaceable_listener =
        TcpListener::bind("127.0.0.1:0").expect("bind replaceable test listener");
    let replaceable_address = replaceable_listener
        .local_addr()
        .expect("replaceable listener address");
    let _replaceable_peer =
        TcpStream::connect(replaceable_address).expect("connect replaceable test peer");
    let (replaceable_stream, _) = replaceable_listener
        .accept()
        .expect("accept replaceable test peer");
    let mut replaceable_client = WireClient::new(2, replaceable_stream);
    for key in 0..=MAX_REPLACEABLE_EVENTS as u64 {
        replaceable_client.queue_replaceable_server_message(
            key,
            ServerMessage::Event(ServerEvent::PlayerMoved {
                player_id: key,
                position: mmorpg_wire::PositionState { x: 0.0, y: 0.0 },
                area: ZoneAreaCode::Town,
            }),
        );
    }
    assert!(replaceable_client.closed);
    assert_eq!(
        replaceable_client.replaceable_events.len(),
        MAX_REPLACEABLE_EVENTS
    );
}
