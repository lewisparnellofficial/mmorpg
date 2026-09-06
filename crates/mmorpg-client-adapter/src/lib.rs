//! Adapter from decoded development-protocol values to client presentation.
//!
//! This crate owns no network, renderer, or gameplay authority. It translates
//! values that have already passed the protocol decoder into the
//! renderer-independent `mmorpg-client-model`.

use mmorpg_client_model::{ApplyEventResult, ClientWorld};
use mmorpg_client_protocol::{
    EntityId, ItemId, NpcState, QuestOfferState, ServerEvent, Snapshot, VendorListingState,
};
use mmorpg_content::{NpcTemplateId, item_definition, starter_catalog};
use mmorpg_core::{
    EntityId as CoreEntityId, Event, Inventory, Npc, PlayerSnapshot, QuestId, QuestOffer,
    VendorListing,
};
use std::fmt;

const TEMPORARY_BOOTSTRAP_INVENTORY_CAPACITY: usize = 16;

/// Failure to translate an otherwise syntactically valid protocol value into
/// content-backed presentation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    MissingNpcTemplate { npc_id: EntityId },
    NpcTemplateOutOfRange { npc_id: EntityId, template_id: u64 },
    UnknownItem { item_id: ItemId },
    UnknownQuest { quest_id: QuestId },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNpcTemplate { npc_id } => {
                write!(formatter, "NPC {npc_id} has no content template ID")
            }
            Self::NpcTemplateOutOfRange {
                npc_id,
                template_id,
            } => write!(
                formatter,
                "NPC {npc_id} template ID {template_id} does not fit the content ID type"
            ),
            Self::UnknownItem { item_id } => {
                write!(
                    formatter,
                    "item {item_id} is not present in the content catalog"
                )
            }
            Self::UnknownQuest { quest_id } => {
                write!(
                    formatter,
                    "quest {quest_id} is not present in the content catalog"
                )
            }
        }
    }
}

impl std::error::Error for AdapterError {}

/// Applies a complete decoded snapshot to a presentation model.
///
/// The model is replaced only after every NPC and player has been translated
/// successfully. A bad content reference therefore cannot leave a partially
/// replaced presentation world.
pub fn apply_snapshot(model: &mut ClientWorld, snapshot: &Snapshot) -> Result<(), AdapterError> {
    let players = snapshot
        .players
        .values()
        .map(protocol_player_snapshot)
        .collect::<Vec<_>>();
    let npcs = snapshot
        .npcs
        .values()
        .map(protocol_npc)
        .collect::<Result<Vec<_>, _>>()?;

    model.replace_from_snapshot(snapshot.world.tick, players, npcs);
    Ok(())
}

/// Projects one decoded authoritative event into the presentation model.
pub fn apply_event(
    model: &mut ClientWorld,
    event: &ServerEvent,
) -> Result<ApplyEventResult, AdapterError> {
    let event = core_event(event)?;
    Ok(model.apply_event(&event))
}

fn protocol_player_snapshot(player: &mmorpg_client_protocol::PlayerState) -> PlayerSnapshot {
    PlayerSnapshot {
        id: core_entity_id(player.id),
        name: player.name.clone(),
        role: player.role,
        position: player.position,
        health: player.health,
        max_health: player.max_health,
        target: player.target.map(core_entity_id),
        gold: player.gold,
        inventory: Inventory::new(TEMPORARY_BOOTSTRAP_INVENTORY_CAPACITY),
        quests: Vec::new(),
    }
}

fn protocol_npc(npc: &NpcState) -> Result<Npc, AdapterError> {
    let Some(template_id) = npc.template_id else {
        return Err(AdapterError::MissingNpcTemplate { npc_id: npc.id });
    };
    let Ok(template_id) = u32::try_from(template_id) else {
        return Err(AdapterError::NpcTemplateOutOfRange {
            npc_id: npc.id,
            template_id,
        });
    };
    Ok(Npc {
        id: core_entity_id(npc.id),
        template_id: NpcTemplateId(template_id),
        name: npc.name.clone(),
        kind: npc.kind,
        position: npc.position,
        health: npc.health,
        max_health: npc.max_health,
    })
}

fn core_event(event: &ServerEvent) -> Result<Event, AdapterError> {
    let event = match event {
        ServerEvent::PlayerJoined {
            id,
            name,
            role,
            position,
        } => Event::PlayerJoined {
            player: PlayerSnapshot {
                id: core_entity_id(*id),
                name: name.clone(),
                role: *role,
                position: *position,
                health: 100,
                max_health: 100,
                target: None,
                gold: 20,
                inventory: Inventory::new(TEMPORARY_BOOTSTRAP_INVENTORY_CAPACITY),
                quests: Vec::new(),
            },
        },
        ServerEvent::PlayerLeft { player_id } => Event::PlayerLeft {
            player_id: core_entity_id(*player_id),
        },
        ServerEvent::PlayerMoved { id, position, area } => Event::PlayerMoved {
            player_id: core_entity_id(*id),
            position: *position,
            area: *area,
        },
        ServerEvent::TargetSelected {
            player_id,
            target_id,
        } => Event::TargetSelected {
            player_id: core_entity_id(*player_id),
            target_id: core_entity_id(*target_id),
        },
        ServerEvent::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => Event::AttackResolved {
            player_id: core_entity_id(*player_id),
            target_id: core_entity_id(*target_id),
            damage: *damage,
            target_health: *target_health,
        },
        ServerEvent::EnemyDefeated { enemy_id } => Event::EnemyDefeated {
            enemy_id: core_entity_id(*enemy_id),
        },
        ServerEvent::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => Event::VendorListed {
            player_id: core_entity_id(*player_id),
            vendor_id: core_entity_id(*vendor_id),
            listings: listings
                .iter()
                .map(core_vendor_listing)
                .collect::<Result<Vec<_>, _>>()?,
        },
        ServerEvent::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => Event::ItemPurchased {
            player_id: core_entity_id(*player_id),
            vendor_id: core_entity_id(*vendor_id),
            item_id: *item_id,
            quantity: *quantity,
            total_price: *total_price,
            gold_remaining: *gold_remaining,
        },
        ServerEvent::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => Event::LootRewarded {
            player_id: core_entity_id(*player_id),
            enemy_id: core_entity_id(*enemy_id),
            item_id: *item_id,
            quantity: *quantity,
        },
        ServerEvent::TransactionRejected { player_id, reason } => Event::TransactionRejected {
            player_id: core_entity_id(*player_id),
            reason: reason.clone(),
        },
        ServerEvent::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => Event::QuestOffersListed {
            player_id: core_entity_id(*player_id),
            npc_id: core_entity_id(*npc_id),
            quests: quests
                .iter()
                .map(core_quest_offer)
                .collect::<Result<Vec<_>, _>>()?,
        },
        ServerEvent::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => Event::QuestAccepted {
            player_id: core_entity_id(*player_id),
            npc_id: core_entity_id(*npc_id),
            quest_id: *quest_id,
        },
        ServerEvent::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => Event::QuestProgressed {
            player_id: core_entity_id(*player_id),
            quest_id: *quest_id,
            progress: *progress,
            required_count: *required_count,
        },
        ServerEvent::QuestCompleted {
            player_id,
            quest_id,
        } => Event::QuestCompleted {
            player_id: core_entity_id(*player_id),
            quest_id: *quest_id,
        },
        ServerEvent::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => Event::QuestRewarded {
            player_id: core_entity_id(*player_id),
            quest_id: *quest_id,
            gold: *gold,
            item_id: *item_id,
            item_quantity: *item_quantity,
            gold_remaining: *gold_remaining,
        },
        ServerEvent::QuestRejected { player_id, reason } => Event::QuestRejected {
            player_id: core_entity_id(*player_id),
            reason: reason.clone(),
        },
        ServerEvent::CommandRejected { reason } => Event::CommandRejected {
            reason: reason.clone(),
        },
    };
    Ok(event)
}

fn core_vendor_listing(listing: &VendorListingState) -> Result<VendorListing, AdapterError> {
    let Some(definition) = item_definition(listing.item_id) else {
        return Err(AdapterError::UnknownItem {
            item_id: listing.item_id,
        });
    };
    Ok(VendorListing {
        item_id: listing.item_id,
        name: definition.name,
        unit_price: listing.unit_price,
        remaining_quantity: listing.remaining_quantity,
        max_stack: listing.max_stack,
    })
}

fn core_quest_offer(offer: &QuestOfferState) -> Result<QuestOffer, AdapterError> {
    let Some(definition) = starter_catalog()
        .quests
        .iter()
        .find(|definition| definition.id == offer.quest_id)
    else {
        return Err(AdapterError::UnknownQuest {
            quest_id: offer.quest_id,
        });
    };
    Ok(QuestOffer {
        quest_id: definition.id,
        name: definition.name,
        description: definition.description,
    })
}

fn core_entity_id(id: EntityId) -> CoreEntityId {
    CoreEntityId(id.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_client_protocol::{
        NpcKind, PlayerState, Position, QuestId, QuestOfferState, Role, ServerEvent, Snapshot,
        WorldState,
    };

    fn snapshot() -> Snapshot {
        Snapshot {
            version: 1,
            world: WorldState {
                tick: 7,
                players: 1,
                npcs: 1,
                enemies: 1,
                vendors: 0,
            },
            players: [(
                EntityId(5),
                PlayerState {
                    id: EntityId(5),
                    name: "Aria".to_owned(),
                    role: Role::DamageDealer,
                    position: mmorpg_client_protocol::Position::new(2.0, 3.0),
                    health: 88,
                    max_health: 100,
                    gold: 20,
                    target: Some(EntityId(2)),
                },
            )]
            .into_iter()
            .collect(),
            npcs: [(
                EntityId(2),
                NpcState {
                    id: EntityId(2),
                    template_id: Some(2),
                    name: "Field Wolf".to_owned(),
                    kind: NpcKind::Enemy,
                    position: Position::new(24.0, 0.0),
                    health: 100,
                    max_health: 100,
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn atomically_projects_snapshot_and_uses_authoritative_content_ids() {
        let mut model = ClientWorld::default();
        apply_snapshot(&mut model, &snapshot()).expect("valid snapshot");

        assert_eq!(model.world_tick(), Some(7));
        assert_eq!(model.player(EntityId(5)).unwrap().gold, 20);
        assert_eq!(
            model.npc(EntityId(2)).unwrap().template_id,
            NpcTemplateId(2)
        );
        assert_eq!(model.npc(EntityId(2)).unwrap().name, "Field Wolf");
        assert_eq!(model.player(EntityId(5)).unwrap().inventory.capacity(), 16);
    }

    #[test]
    fn rejects_snapshot_npc_without_template_without_mutating_existing_model() {
        let mut model = ClientWorld::default();
        apply_snapshot(&mut model, &snapshot()).expect("valid snapshot");
        let mut invalid = snapshot();
        invalid.npcs.get_mut(&EntityId(2)).unwrap().template_id = None;

        assert_eq!(
            apply_snapshot(&mut model, &invalid),
            Err(AdapterError::MissingNpcTemplate {
                npc_id: EntityId(2)
            })
        );
        assert!(model.npc(EntityId(2)).is_some());
        assert_eq!(model.world_tick(), Some(7));
    }

    #[test]
    fn resolves_catalog_metadata_and_projects_events() {
        let mut model = ClientWorld::default();
        apply_snapshot(&mut model, &snapshot()).expect("valid snapshot");
        assert_eq!(
            apply_event(
                &mut model,
                &ServerEvent::QuestOffersListed {
                    player_id: EntityId(5),
                    npc_id: EntityId(1),
                    quests: vec![QuestOfferState {
                        quest_id: QuestId(1),
                        name: "Clear the Field".to_owned(),
                    }],
                },
            ),
            Ok(ApplyEventResult::Applied)
        );
        assert_eq!(
            model.quest_offers(EntityId(1)).unwrap()[0].name,
            "Clear the Field"
        );

        assert_eq!(
            apply_event(
                &mut model,
                &ServerEvent::VendorListed {
                    player_id: EntityId(5),
                    vendor_id: EntityId(1),
                    listings: vec![VendorListingState {
                        item_id: mmorpg_client_protocol::ItemId(2),
                        name: "untrusted server text".to_owned(),
                        unit_price: 2,
                        remaining_quantity: 100,
                        max_stack: 20,
                    }],
                },
            ),
            Ok(ApplyEventResult::Applied)
        );
        assert_eq!(
            model.vendor_listings(EntityId(1)).unwrap()[0].name,
            "Town Ration"
        );
    }
}
