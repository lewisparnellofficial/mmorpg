//! End-to-end validation of the authoritative starter slice and client
//! presentation projection.
//!
//! This experiment deliberately uses the public APIs of `mmorpg-core` and
//! `mmorpg-client-model` as a small replay adapter would. It does not access
//! private world state or fabricate gameplay results on the client.

use mmorpg_client_model::{ClientEntity, ClientWorld};
use mmorpg_content::{ItemId, QuestId};
use mmorpg_core::{Command, EntityId, Event, NpcKind, Role, World};

/// Summary of the state that the replay proved was projected successfully.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayReport {
    pub player_id: EntityId,
    pub vendor_id: EntityId,
    pub enemy_ids: Vec<EntityId>,
    pub applied_events: usize,
    pub projected_position: (f32, f32),
    pub projected_area: mmorpg_core::ZoneArea,
    pub projected_gold: u32,
    pub projected_pelts: u32,
    pub projected_rations: u32,
    pub projected_potions: u32,
    pub defeated_enemies: usize,
    pub completed_quest: QuestId,
    pub vendor_potions_remaining: u32,
}

/// Runs the starter-zone replay and returns the resulting client-facing
/// projection summary.
pub fn run_replay() -> ReplayReport {
    let mut world = World::new_starter_zone();
    let vendor_id = world
        .npcs()
        .find(|npc| npc.kind == NpcKind::Vendor)
        .expect("starter world must contain a vendor")
        .id;
    let enemy_ids: Vec<_> = world
        .npcs()
        .filter(|npc| npc.kind == NpcKind::Enemy)
        .map(|npc| npc.id)
        .collect();
    assert_eq!(
        enemy_ids.len(),
        3,
        "starter world must contain three enemies"
    );

    let mut client = ClientWorld::default();
    let mut applied_events = 0;

    // Bootstrap NPC state through the separate authoritative snapshot path.
    for npc in world.npcs() {
        client.apply_npc_snapshot(npc);
    }

    let join_events = world.step([Command::JoinPlayer {
        name: "Replay Tank".to_owned(),
        role: Role::DamageDealer,
    }]);
    applied_events += apply_events(&mut client, &join_events);
    let player_id = joined_player_id(&join_events);

    // Town interactions establish vendor and quest UI state, then exercise a
    // server-authoritative purchase before leaving town.
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::ListVendor {
            player_id,
            vendor_id,
        }],
    );
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::ListQuestOffers {
            player_id,
            npc_id: vendor_id,
        }],
    );
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::MINOR_HEALING_POTION,
            quantity: 1,
        }],
    );
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::AcceptQuest {
            player_id,
            npc_id: vendor_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }],
    );

    // Leave town and move into attack range. Each movement command stays
    // within the core's per-command movement limit.
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::Move {
            player_id,
            dx: 10.0,
            dy: 0.0,
        }],
    );
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::Move {
            player_id,
            dx: 10.0,
            dy: 0.0,
        }],
    );
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::Move {
            player_id,
            dx: 4.0,
            dy: 0.0,
        }],
    );

    // Kill and loot all three wolves. DamageDealer damage is 12, so nine
    // authoritative attacks are required for each 100-health enemy.
    for &enemy_id in &enemy_ids {
        applied_events += apply_step(
            &mut world,
            &mut client,
            [Command::SelectTarget {
                player_id,
                target_id: enemy_id,
            }],
        );
        for _ in 0..9 {
            applied_events += apply_step(
                &mut world,
                &mut client,
                [Command::BasicAttack { player_id }],
            );
        }
        applied_events += apply_step(
            &mut world,
            &mut client,
            [Command::LootEnemy {
                player_id,
                enemy_id,
            }],
        );
    }

    // Return to town and turn in the now-completed quest.
    for dx in [-10.0, -10.0, -4.0] {
        applied_events += apply_step(
            &mut world,
            &mut client,
            [Command::Move {
                player_id,
                dx,
                dy: 0.0,
            }],
        );
    }
    applied_events += apply_step(
        &mut world,
        &mut client,
        [Command::TurnInQuest {
            player_id,
            npc_id: vendor_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }],
    );

    let player = client
        .player(player_id)
        .expect("player join must be projected");
    let completed_quest = player
        .quests
        .iter()
        .find(|quest| quest.quest_id == QuestId::CLEAR_THE_FIELD)
        .filter(|quest| quest.status == mmorpg_core::QuestStatus::Rewarded)
        .map(|quest| quest.quest_id)
        .expect("quest completion and reward must be projected");
    let vendor_potions_remaining = client
        .vendor_listings(vendor_id)
        .expect("vendor listing must be projected")
        .iter()
        .find(|listing| listing.item_id == ItemId::MINOR_HEALING_POTION)
        .expect("starter vendor must sell potions")
        .remaining_quantity;

    ReplayReport {
        player_id,
        vendor_id,
        enemy_ids,
        applied_events,
        projected_position: (player.position.x, player.position.y),
        projected_area: player.area,
        projected_gold: player.gold,
        projected_pelts: player.inventory.quantity(ItemId::FIELD_WOLF_PELT),
        projected_rations: player.inventory.quantity(ItemId::TOWN_RATION),
        projected_potions: player.inventory.quantity(ItemId::MINOR_HEALING_POTION),
        defeated_enemies: client
            .entities()
            .filter(|entity| {
                matches!(
                    entity,
                    ClientEntity::Npc(npc) if npc.kind == NpcKind::Enemy && npc.defeated
                )
            })
            .count(),
        completed_quest,
        vendor_potions_remaining,
    }
}

fn apply_step<I>(world: &mut World, client: &mut ClientWorld, commands: I) -> usize
where
    I: IntoIterator<Item = Command>,
{
    let events = world.step(commands);
    apply_events(client, &events)
}

fn apply_events(client: &mut ClientWorld, events: &[Event]) -> usize {
    for event in events {
        assert_eq!(
            client.apply_event(event),
            mmorpg_client_model::ApplyEventResult::Applied,
            "replay event must be projectable: {event:?}"
        );
    }
    events.len()
}

fn joined_player_id(events: &[Event]) -> EntityId {
    let [Event::PlayerJoined { player }] = events else {
        panic!("expected one player join event, got {events:?}");
    };
    player.id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_replay_projects_all_vertical_slice_state() {
        let report = run_replay();

        assert_eq!(report.enemy_ids.len(), 3);
        assert_eq!(report.applied_events, 52);
        assert_eq!(report.projected_position, (0.0, 0.0));
        assert_eq!(report.projected_area, mmorpg_core::ZoneArea::Town);
        assert_eq!(report.projected_pelts, 3);
        assert_eq!(report.projected_rations, 5);
        assert_eq!(report.projected_potions, 1);
        assert_eq!(report.projected_gold, 25);
        assert_eq!(report.defeated_enemies, 3);
        assert_eq!(report.completed_quest, QuestId::CLEAR_THE_FIELD);
        assert_eq!(report.vendor_potions_remaining, 49);
    }

    #[test]
    fn replay_is_deterministic() {
        assert_eq!(run_replay(), run_replay());
    }
}
