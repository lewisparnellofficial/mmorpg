//! End-to-end validation of the typed wire gameplay path.
//!
//! Start `mmorpg-server` with its optional wire listener before running this
//! tool. The tool intentionally uses the public blocking transport from a
//! separate process, so this exercises framing, server session binding, the
//! authoritative world, and client-facing typed payloads together.

use mmorpg_client_transport::{WireConnection, WireConnectionConfig};
use mmorpg_content::starter_catalog;
use mmorpg_wire::{ClientCommand, ServerEvent, ServerMessage, WorldSnapshot};
use std::env;

const DEFAULT_ADDRESS: &str = "127.0.0.1:4001";

fn main() {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let address = arguments
        .iter()
        .find(|argument| argument.as_str() != "--expect-restored")
        .cloned()
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let expect_restored = arguments
        .iter()
        .any(|argument| argument == "--expect-restored");
    let mut connection = WireConnection::connect(&WireConnectionConfig::new(&address))
        .unwrap_or_else(|error| panic!("cannot connect to typed wire listener {address}: {error}"));

    expect_message(&mut connection, "welcome", |message| {
        matches!(message, ServerMessage::Welcome { .. })
    });
    connection
        .send_typed_command(&ClientCommand::Join {
            name: "WireSmoke".to_owned(),
            role: mmorpg_wire::RoleCode::DamageDealer,
        })
        .expect("legacy join command should encode and send");
    match expect_message(
        &mut connection,
        "unauthenticated join rejection",
        |message| matches!(message, ServerMessage::Error { .. }),
    ) {
        ServerMessage::Error { message } => {
            assert_eq!(message, "authenticate before sending wire commands");
        }
        _ => unreachable!("predicate selected an error message"),
    }
    connection
        .send_typed_command(&ClientCommand::Authenticate {
            token: "dev-local".to_owned(),
        })
        .expect("authentication command should encode and send");
    match expect_message(&mut connection, "authenticated", |message| {
        matches!(message, ServerMessage::Authenticated { .. })
    }) {
        ServerMessage::Authenticated {
            account_id,
            session_id,
        } => {
            assert_eq!(account_id, 1);
            assert_ne!(session_id, 0);
        }
        _ => unreachable!("predicate selected authenticated message"),
    }
    connection
        .send_typed_command(&ClientCommand::EnterWorld)
        .expect("premature enter-world command should encode and send");
    match expect_message(
        &mut connection,
        "premature enter-world rejection",
        |message| matches!(message, ServerMessage::Error { .. }),
    ) {
        ServerMessage::Error { message } => {
            assert_eq!(message, "select a character before entering world");
        }
        _ => unreachable!("predicate selected an error message"),
    }
    connection
        .send_typed_command(&ClientCommand::ListCharacters)
        .expect("character-list command should encode and send");
    let character_id = match expect_message(&mut connection, "character list", |message| {
        matches!(message, ServerMessage::CharacterList { .. })
    }) {
        ServerMessage::CharacterList {
            account_id,
            characters,
        } => {
            assert_eq!(account_id, 1);
            let character = characters
                .first()
                .expect("development account should have a character");
            assert_eq!(character.name, "Aria");
            assert_eq!(character.role, mmorpg_wire::RoleCode::DamageDealer);
            character.character_id
        }
        _ => unreachable!("predicate selected character list"),
    };
    connection
        .send_typed_command(&ClientCommand::SelectCharacter { character_id })
        .expect("character-selection command should encode and send");
    match expect_message(&mut connection, "character selected", |message| {
        matches!(message, ServerMessage::CharacterSelected { .. })
    }) {
        ServerMessage::CharacterSelected {
            character_id: selected_id,
            name,
            role,
        } => {
            assert_eq!(selected_id, character_id);
            assert_eq!(name, "Aria");
            assert_eq!(role, mmorpg_wire::RoleCode::DamageDealer);
        }
        _ => unreachable!("predicate selected character-selected message"),
    }
    let digest = starter_catalog().content_digest();
    connection
        .send_typed_command(&ClientCommand::ContentDigest { digest })
        .expect("content digest command should encode and send");
    match expect_message(&mut connection, "content accepted", |message| {
        matches!(message, ServerMessage::ContentAccepted { .. })
    }) {
        ServerMessage::ContentAccepted { digest: accepted } => assert_eq!(accepted, digest),
        _ => unreachable!("predicate selected content acceptance"),
    }
    connection
        .send_typed_command(&ClientCommand::EnterWorld)
        .expect("enter-world command should encode and send");
    let player_id = match expect_message(&mut connection, "connected", |message| {
        matches!(message, ServerMessage::Connected { .. })
    }) {
        ServerMessage::Connected { player_id, .. } => player_id,
        _ => unreachable!("predicate selected connected message"),
    };
    expect_message(&mut connection, "player joined", |message| {
        matches!(
            message,
            ServerMessage::Event(ServerEvent::PlayerJoined { .. })
        )
    });

    connection
        .send_typed_command(&ClientCommand::Snapshot)
        .expect("snapshot command should send");
    let snapshot = match expect_message(&mut connection, "snapshot", |message| {
        matches!(message, ServerMessage::Snapshot(_))
    }) {
        ServerMessage::Snapshot(snapshot) => snapshot,
        _ => unreachable!("predicate selected snapshot"),
    };
    validate_snapshot(&snapshot, player_id);
    if expect_restored {
        validate_restored_snapshot(&snapshot, player_id);
        println!("wire restart smoke: restored character state successfully");
        return;
    }

    connection
        .send_typed_command(&ClientCommand::ListVendor { vendor_id: 1 })
        .expect("vendor command should send");
    let listings = match expect_event(&mut connection, "vendor listing", |event| {
        matches!(event, ServerEvent::VendorListed { .. })
    }) {
        ServerEvent::VendorListed { listings, .. } => listings,
        _ => unreachable!("predicate selected vendor listing"),
    };
    assert!(listings.iter().any(|listing| listing.item_id == 2));

    connection
        .send_typed_command(&ClientCommand::Retryable {
            operation_id: 101,
            command: Box::new(ClientCommand::BuyItem {
                vendor_id: 1,
                item_id: 2,
                quantity: 1,
            }),
        })
        .expect("purchase command should send");
    let purchase = expect_event(&mut connection, "purchase", |event| {
        matches!(event, ServerEvent::ItemPurchased { .. })
    });
    match purchase {
        ServerEvent::ItemPurchased {
            player_id: purchase_player,
            item_id,
            quantity,
            ..
        } => {
            assert_eq!(purchase_player, player_id);
            assert_eq!(item_id, 2);
            assert_eq!(quantity, 1);
        }
        _ => unreachable!("predicate selected purchase"),
    }

    connection
        .send_typed_command(&ClientCommand::ListQuestOffers { npc_id: 1 })
        .expect("quest offers command should send");
    let offers = expect_event(&mut connection, "quest offers", |event| {
        matches!(event, ServerEvent::QuestOffersListed { .. })
    });
    assert!(matches!(
        offers,
        ServerEvent::QuestOffersListed { quests, .. } if quests.iter().any(|quest| quest.quest_id == 1)
    ));

    connection
        .send_typed_command(&ClientCommand::AcceptQuest {
            npc_id: 1,
            quest_id: 1,
        })
        .expect("quest acceptance command should send");
    expect_event(&mut connection, "quest acceptance", |event| {
        matches!(
            event,
            ServerEvent::QuestAccepted {
                player_id: accepted_player,
                quest_id: 1,
                ..
            } if *accepted_player == player_id
        )
    });

    for enemy_id in [2, 3, 4] {
        connection
            .send_typed_command(&ClientCommand::SelectTarget {
                target_id: enemy_id,
            })
            .expect("target command should send");
        expect_event(&mut connection, "target selection", |event| {
            matches!(
                event,
                ServerEvent::TargetSelected {
                    player_id: selected_player,
                    target_id,
                } if *selected_player == player_id && *target_id == enemy_id
            )
        });

        let mut defeated = false;
        for _ in 0..16 {
            connection
                .send_typed_command(&ClientCommand::BasicAttack)
                .expect("attack command should send");
            let attack = expect_event(&mut connection, "attack resolution", |event| {
                matches!(
                    event,
                    ServerEvent::AttackResolved {
                        player_id: attacker,
                        target_id,
                        ..
                    } if *attacker == player_id && *target_id == enemy_id
                ) || matches!(event, ServerEvent::CommandRejected { .. })
            });
            if matches!(
                attack,
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
            &mut connection,
            "enemy defeat",
            |event| matches!(event, ServerEvent::EnemyDefeated { enemy_id: defeated } if *defeated == enemy_id),
        );

        connection
            .send_typed_command(&ClientCommand::Retryable {
                operation_id: 200 + enemy_id,
                command: Box::new(ClientCommand::LootEnemy { enemy_id }),
            })
            .expect("loot command should send");
        expect_event(&mut connection, "loot reward", |event| {
            matches!(
                event,
                ServerEvent::LootRewarded {
                    player_id: looter,
                    enemy_id: looted,
                    ..
                } if *looter == player_id && *looted == enemy_id
            )
        });
    }

    connection
        .send_typed_command(&ClientCommand::Retryable {
            operation_id: 301,
            command: Box::new(ClientCommand::TurnInQuest {
                npc_id: 1,
                quest_id: 1,
            }),
        })
        .expect("quest turn-in command should send");
    expect_event(&mut connection, "quest reward", |event| {
        matches!(
            event,
            ServerEvent::QuestRewarded {
                player_id: rewarded_player,
                quest_id: 1,
                ..
            } if *rewarded_player == player_id
        )
    });

    println!(
        "wire gameplay smoke: success (player={player_id}, enemies=3, vendor_purchase=1, quest=1)"
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
            unreachable!("predicate selected an event envelope");
        };
        if predicate(&event) {
            return event;
        }
    }
}

fn validate_snapshot(snapshot: &WorldSnapshot, player_id: u64) {
    assert_eq!(snapshot.player_count, 1);
    assert_eq!(snapshot.npc_count, 4);
    assert_eq!(snapshot.enemy_count, 3);
    assert_eq!(snapshot.vendor_count, 1);
    assert!(
        snapshot
            .players
            .iter()
            .any(|player| player.player_id == player_id)
    );
    for entity_id in [1, 2, 3, 4] {
        assert!(snapshot.npcs.iter().any(|npc| npc.entity_id == entity_id));
    }
}

fn validate_restored_snapshot(snapshot: &WorldSnapshot, player_id: u64) {
    let player = snapshot
        .players
        .iter()
        .find(|player| player.player_id == player_id)
        .expect("restored player should appear in snapshot");
    assert!(player.gold > 20, "quest reward gold should survive restart");
    assert!(
        player
            .inventory
            .iter()
            .any(|stack| stack.item_id == 2 && stack.quantity >= 1)
    );
    assert!(
        player
            .quests
            .iter()
            .any(|quest| quest.quest_id == 1
                && quest.status == mmorpg_wire::QuestStatusCode::Rewarded)
    );
}
