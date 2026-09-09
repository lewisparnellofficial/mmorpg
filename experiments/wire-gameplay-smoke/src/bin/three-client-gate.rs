//! Deterministic three-role typed-session gate for the first shared-zone slice.
//!
//! Start `mmorpg-server` with `--wire-address` before running this binary.
//! This validates three distinct authenticated characters, one shared world,
//! party privacy, and the versioned snapshot summary boundary.

use mmorpg_client_transport::{WireConnection, WireConnectionConfig};
use mmorpg_content::starter_catalog;
use mmorpg_wire::{ClientCommand, RoleCode, ServerEvent, ServerMessage, WorldSnapshot};
use std::env;

const DEFAULT_ADDRESS: &str = "127.0.0.1:4001";

struct GateClient {
    player_id: u64,
    connection: WireConnection,
}

fn main() {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let digest = starter_catalog().content_digest();
    let mut tank = connect(&address, 2, RoleCode::Tank, digest);
    let mut healer = connect(&address, 3, RoleCode::Healer, digest);
    let mut damage = connect(&address, 1, RoleCode::DamageDealer, digest);

    assert_ne!(tank.player_id, healer.player_id);
    assert_ne!(tank.player_id, damage.player_id);
    assert_ne!(healer.player_id, damage.player_id);

    let tank_snapshot = snapshot(&mut tank);
    let healer_snapshot = snapshot(&mut healer);
    let damage_snapshot = snapshot(&mut damage);
    assert_private_snapshot(&tank_snapshot, tank.player_id, None);
    assert_private_snapshot(&healer_snapshot, healer.player_id, None);
    assert_private_snapshot(&damage_snapshot, damage.player_id, None);

    tank.connection
        .send_typed_command(&ClientCommand::InvitePartyMember {
            target_id: healer.player_id,
        })
        .expect("tank party invite should send");
    let party_id = match expect_event(&mut tank.connection, "party invite", |event| {
        matches!(event, ServerEvent::PartyInviteCreated { .. })
    }) {
        ServerEvent::PartyInviteCreated { party_id, .. } => party_id,
        _ => unreachable!(),
    };
    expect_event(
        &mut healer.connection,
        "party invite delivery",
        |event| matches!(event, ServerEvent::PartyInviteCreated { party_id: id, .. } if *id == party_id),
    );

    healer
        .connection
        .send_typed_command(&ClientCommand::AcceptPartyInvite { party_id })
        .expect("healer party acceptance should send");
    let accepted_party = match expect_event(&mut healer.connection, "party acceptance", |event| {
        matches!(event, ServerEvent::PartyInviteAccepted { .. })
    }) {
        ServerEvent::PartyInviteAccepted { party, .. } => party,
        _ => unreachable!(),
    };
    assert_eq!(accepted_party.member_ids.len(), 2);
    expect_event(&mut tank.connection, "party acceptance delivery", |event| {
        matches!(event, ServerEvent::PartyInviteAccepted { party, .. }
            if party.party_id == party_id && party.member_ids.len() == 2)
    });

    let tank_snapshot = snapshot(&mut tank);
    let healer_snapshot = snapshot(&mut healer);
    let damage_snapshot = snapshot(&mut damage);
    assert_private_snapshot(&tank_snapshot, tank.player_id, Some(party_id));
    assert_private_snapshot(&healer_snapshot, healer.player_id, Some(party_id));
    assert_private_snapshot(&damage_snapshot, damage.player_id, None);

    tank.connection
        .send_typed_command(&ClientCommand::SelectTarget { target_id: 2 })
        .expect("tank target command should send");
    expect_event(&mut tank.connection, "tank target selection", |event| {
        matches!(
            event,
            ServerEvent::TargetSelected {
                player_id,
                target_id: 2
            } if *player_id == tank.player_id
        )
    });
    tank.connection
        .send_typed_command(&ClientCommand::Taunt)
        .expect("tank taunt should send");
    expect_event(&mut tank.connection, "tank taunt", |event| {
        matches!(
            event,
            ServerEvent::TauntResolved {
                player_id,
                target_id: 2
            } if *player_id == tank.player_id
        )
    });

    defeat_and_loot(&mut damage, 2, 902);
    expect_event(&mut tank.connection, "first enemy respawn", |event| {
        matches!(event, ServerEvent::EnemyRespawned { enemy_id: 2, .. })
    });
    defeat_and_loot(&mut damage, 2, 903);

    println!(
        "three-client gate: success (tank={}, healer={}, damage={}, party={}, enemy_generations=2)",
        tank.player_id, healer.player_id, damage.player_id, party_id
    );
}

fn defeat_and_loot(client: &mut GateClient, enemy_id: u64, operation_id: u64) {
    client
        .connection
        .send_typed_command(&ClientCommand::SelectTarget {
            target_id: enemy_id,
        })
        .expect("damage target command should send");
    expect_event(&mut client.connection, "damage target selection", |event| {
        matches!(
            event,
            ServerEvent::TargetSelected {
                player_id,
                target_id
            } if *player_id == client.player_id && *target_id == enemy_id
        )
    });

    let mut defeated = false;
    for _ in 0..16 {
        client
            .connection
            .send_typed_command(&ClientCommand::BasicAttack)
            .expect("damage attack should send");
        let result = expect_event(&mut client.connection, "damage attack", |event| {
            matches!(
                event,
                ServerEvent::AttackResolved {
                    player_id,
                    target_id,
                    ..
                } if *player_id == client.player_id && *target_id == enemy_id
            ) || matches!(event, ServerEvent::CommandRejected { .. })
        });
        if matches!(
            result,
            ServerEvent::AttackResolved {
                target_health: 0,
                ..
            }
        ) {
            defeated = true;
            break;
        }
    }
    assert!(defeated, "enemy {enemy_id} did not die within 16 attacks");
    expect_event(
        &mut client.connection,
        "enemy defeat",
        |event| matches!(event, ServerEvent::EnemyDefeated { enemy_id: defeated } if *defeated == enemy_id),
    );
    client
        .connection
        .send_typed_command(&ClientCommand::Retryable {
            operation_id,
            command: Box::new(ClientCommand::LootEnemy { enemy_id }),
        })
        .expect("generation loot command should send");
    expect_event(&mut client.connection, "generation loot", |event| {
        matches!(
            event,
            ServerEvent::LootRewarded {
                player_id,
                enemy_id: looted,
                ..
            } if *player_id == client.player_id && *looted == enemy_id
        )
    });
}

fn connect(address: &str, character_id: u64, role: RoleCode, digest: [u8; 32]) -> GateClient {
    let mut connection = WireConnection::connect(&WireConnectionConfig::new(address))
        .unwrap_or_else(|error| panic!("cannot connect character {character_id}: {error}"));
    expect_message(&mut connection, "welcome", |message| {
        matches!(message, ServerMessage::Welcome { .. })
    });
    connection
        .send_typed_command(&ClientCommand::Authenticate {
            token: "dev-local".to_owned(),
        })
        .expect("authentication should send");
    expect_message(&mut connection, "authentication", |message| {
        matches!(message, ServerMessage::Authenticated { .. })
    });
    connection
        .send_typed_command(&ClientCommand::ListCharacters)
        .expect("character list should send");
    let characters = match expect_message(&mut connection, "character list", |message| {
        matches!(message, ServerMessage::CharacterList { .. })
    }) {
        ServerMessage::CharacterList { characters, .. } => characters,
        _ => unreachable!(),
    };
    let selected = characters
        .iter()
        .find(|character| character.character_id == character_id)
        .expect("requested development character should exist");
    assert_eq!(selected.role, role);
    connection
        .send_typed_command(&ClientCommand::SelectCharacter { character_id })
        .expect("character selection should send");
    expect_message(
        &mut connection,
        "character selection",
        |message| matches!(message, ServerMessage::CharacterSelected { character_id: id, .. } if *id == character_id),
    );
    connection
        .send_typed_command(&ClientCommand::ContentDigest { digest })
        .expect("content digest should send");
    expect_message(&mut connection, "content acceptance", |message| {
        matches!(message, ServerMessage::ContentAccepted { .. })
    });
    connection
        .send_typed_command(&ClientCommand::EnterWorld)
        .expect("enter world should send");
    let player_id = match expect_message(&mut connection, "connected", |message| {
        matches!(message, ServerMessage::Connected { .. })
    }) {
        ServerMessage::Connected { player_id, .. } => player_id,
        _ => unreachable!(),
    };
    GateClient {
        player_id,
        connection,
    }
}

fn snapshot(client: &mut GateClient) -> WorldSnapshot {
    client
        .connection
        .send_typed_command(&ClientCommand::Snapshot)
        .expect("snapshot should send");
    match expect_message(&mut client.connection, "snapshot", |message| {
        matches!(message, ServerMessage::Snapshot(_))
    }) {
        ServerMessage::Snapshot(snapshot) => snapshot,
        _ => unreachable!(),
    }
}

fn assert_private_snapshot(snapshot: &WorldSnapshot, player_id: u64, party_id: Option<u64>) {
    assert_eq!(snapshot.player_count, 1);
    assert_eq!(snapshot.players.len(), 1);
    assert_eq!(snapshot.players[0].player_id, player_id);
    assert_eq!(
        snapshot.party.as_ref().map(|party| party.party_id),
        party_id
    );
}

fn expect_message(
    connection: &mut WireConnection,
    label: &str,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> ServerMessage {
    loop {
        let message = connection
            .read_server_message()
            .unwrap_or_else(|error| panic!("failed while waiting for {label}: {error}"));
        if predicate(&message) {
            return message;
        }
    }
}

fn expect_event(
    connection: &mut WireConnection,
    label: &str,
    predicate: impl Fn(&ServerEvent) -> bool,
) -> ServerEvent {
    loop {
        let message = expect_message(connection, label, |message| {
            matches!(message, ServerMessage::Event(_))
        });
        let ServerMessage::Event(event) = message else {
            unreachable!()
        };
        if predicate(&event) {
            return event;
        }
    }
}
