use mmorpg_core::{Command, EntityId, Event, ItemId, PartyId, QuestId, Role, World};
use mmorpg_server::test_support::{
    ParsedLine, encode_snapshot_text, format_event, format_machine_snapshot, parse_line,
};
use mmorpg_server::{
    WireClient, event_recipient, event_visible_to_player, wire_snapshot_for_player,
};
use mmorpg_wire::{ServerEvent, ServerMessage, ZoneAreaCode};
use std::net::{TcpListener, TcpStream};

#[test]
fn event_format_is_human_readable() {
    let event = Event::EnemyDefeated {
        enemy_id: EntityId(9),
    };
    assert_eq!(format_event(&event), "EVENT enemy_defeated id=9");
}

#[test]
fn snapshot_command_is_available_without_a_bound_player() {
    assert!(matches!(
        parse_line("snapshot", None),
        Ok(ParsedLine::Snapshot)
    ));
}

#[test]
fn private_events_and_snapshots_are_scoped_to_the_bound_player() {
    let player_id = EntityId(7);
    let other_id = EntityId(8);
    assert_eq!(
        event_recipient(&Event::ItemPurchased {
            player_id,
            vendor_id: EntityId(1),
            item_id: ItemId::TOWN_RATION,
            quantity: 1,
            total_price: 2,
            gold_remaining: 18,
        }),
        Some(player_id)
    );
    assert_eq!(
        event_recipient(&Event::EnemyDefeated { enemy_id: other_id }),
        None
    );

    let mut world = World::new_starter_zone();
    world.step([
        Command::JoinPlayer {
            name: "One".to_owned(),
            role: Role::Tank,
        },
        Command::JoinPlayer {
            name: "Two".to_owned(),
            role: Role::Healer,
        },
    ]);
    let snapshot = wire_snapshot_for_player(&world, EntityId(5));
    assert_eq!(snapshot.players.len(), 1);
    assert_eq!(snapshot.players[0].player_id, 5);
    assert_eq!(snapshot.player_count, 1);
}

#[test]
fn party_events_do_not_leak_to_unrelated_players() {
    let mut world = World::new_starter_zone();
    world.step([
        Command::JoinPlayer {
            name: "One".to_owned(),
            role: Role::Tank,
        },
        Command::JoinPlayer {
            name: "Two".to_owned(),
            role: Role::Healer,
        },
        Command::JoinPlayer {
            name: "Stranger".to_owned(),
            role: Role::DamageDealer,
        },
    ]);
    let invite = world.step([Command::InvitePartyMember {
        player_id: EntityId(5),
        target_id: EntityId(6),
    }]);
    assert!(event_visible_to_player(
        &invite[0],
        Some(EntityId(5)),
        &world
    ));
    assert!(event_visible_to_player(
        &invite[0],
        Some(EntityId(6)),
        &world
    ));
    assert!(!event_visible_to_player(
        &invite[0],
        Some(EntityId(7)),
        &world
    ));

    let accepted = world.step([Command::AcceptPartyInvite {
        player_id: EntityId(6),
        party_id: PartyId(1),
    }]);
    assert!(event_visible_to_player(
        &accepted[0],
        Some(EntityId(5)),
        &world
    ));
    assert!(event_visible_to_player(
        &accepted[0],
        Some(EntityId(6)),
        &world
    ));
    assert!(!event_visible_to_player(
        &accepted[0],
        Some(EntityId(7)),
        &world
    ));
}

#[test]
fn public_events_are_filtered_by_nearby_interest() {
    let mut world = World::new_starter_zone();
    world.step([
        Command::JoinPlayer {
            name: "Near".to_owned(),
            role: Role::Tank,
        },
        Command::JoinPlayer {
            name: "Far".to_owned(),
            role: Role::DamageDealer,
        },
    ]);
    for _ in 0..172 {
        world.step([Command::Move {
            player_id: EntityId(6),
            dx: 0.35,
            dy: 0.0,
        }]);
    }
    let movement = Event::PlayerMoved {
        player_id: EntityId(6),
        position: world.player(EntityId(6)).unwrap().position,
        area: mmorpg_core::ZoneArea::Field,
    };
    assert!(!event_visible_to_player(
        &movement,
        Some(EntityId(5)),
        &world
    ));
    assert!(event_visible_to_player(
        &movement,
        Some(EntityId(6)),
        &world
    ));
}

#[test]
fn replaceable_position_events_coalesce_before_flush() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let _peer = TcpStream::connect(address).expect("connect test peer");
    let (stream, _) = listener.accept().expect("accept test peer");
    let mut client = WireClient::new(1, stream);
    client.queue_replaceable_server_message(
        5,
        ServerMessage::Event(ServerEvent::PlayerMoved {
            player_id: 5,
            position: mmorpg_wire::PositionState { x: 1.0, y: 0.0 },
            area: ZoneAreaCode::Town,
        }),
    );
    client.queue_replaceable_server_message(
        5,
        ServerMessage::Event(ServerEvent::PlayerMoved {
            player_id: 5,
            position: mmorpg_wire::PositionState { x: 2.0, y: 0.0 },
            area: ZoneAreaCode::Town,
        }),
    );
    assert_eq!(client.replaceable_events.len(), 1);
    assert!(matches!(
        client.replaceable_events.get(&5),
        Some(ServerMessage::Event(ServerEvent::PlayerMoved { position, .. }))
            if position.x == 2.0
    ));
}

#[test]
fn machine_snapshot_has_stable_bootstrap_records() {
    let mut world = World::new_starter_zone();
    world.step([Command::JoinPlayer {
        name: "Aria".to_owned(),
        role: Role::DamageDealer,
    }]);

    assert_eq!(
        format_machine_snapshot(&world),
        vec![
            "TEMP_SNAPSHOT_BEGIN version=2",
            "TEMP_SNAPSHOT WORLD tick=1 players=1 npcs=4 enemies=3 vendors=1",
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0.0,0.0 health=100 max_health=100 gold=20 capacity=16 target=none",
            "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0.0,0.0 health=1 max_health=1",
            "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=25.0,0.0 health=100 max_health=100",
            "TEMP_SNAPSHOT NPC id=3 template_id=2 name=Field%20Wolf kind=enemy position=31.0,6.0 health=100 max_health=100",
            "TEMP_SNAPSHOT NPC id=4 template_id=2 name=Field%20Wolf kind=enemy position=31.0,-6.0 health=100 max_health=100",
            "TEMP_SNAPSHOT_END",
        ]
    );
}

#[test]
fn machine_snapshot_includes_player_inventory_and_quest_state() {
    let mut world = World::new_starter_zone();
    world.step([Command::JoinPlayer {
        name: "Aria".to_owned(),
        role: Role::DamageDealer,
    }]);
    let player_id = world.players().next().expect("player joined").id;
    let vendor_id = world
        .npcs()
        .find(|npc| npc.kind == mmorpg_core::NpcKind::Vendor)
        .expect("starter vendor exists")
        .id;

    world.step([Command::BuyItem {
        player_id,
        vendor_id,
        item_id: ItemId::TOWN_RATION,
        quantity: 2,
    }]);
    world.step([Command::AcceptQuest {
        player_id,
        npc_id: vendor_id,
        quest_id: QuestId::CLEAR_THE_FIELD,
    }]);

    let lines = format_machine_snapshot(&world);
    assert!(
        lines
            .iter()
            .any(|line| { line == "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=2" })
    );
    assert!(lines.iter().any(|line| {
        line == "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=0/3 status=Accepted"
    }));
}

#[test]
fn machine_snapshot_text_encoding_preserves_record_boundaries() {
    assert_eq!(
        encode_snapshot_text("Name with\tcontrols%"),
        "Name%20with%09controls%25"
    );
}
