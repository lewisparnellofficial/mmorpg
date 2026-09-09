//! Adapter from decoded development-protocol values to client presentation.
//!
//! This crate owns no network, renderer, or gameplay authority. It translates
//! values that have already passed the protocol decoder into the
//! renderer-independent `mmorpg-client-model`.

use mmorpg_client_model::{ApplyEventResult, ClientItemStack, ClientQuestState, ClientWorld};
use mmorpg_client_protocol::{
    EntityId, ItemId, ItemState, NpcKind, NpcState, QuestOfferState, QuestState, QuestStatus, Role,
    ServerEvent, Snapshot, VendorListingState,
};
use mmorpg_content::{NpcTemplateId, item_definition, starter_catalog};
use mmorpg_core::{
    EntityId as CoreEntityId, Event, Inventory, Npc, PlayerSnapshot, QuestId, QuestOffer,
    VendorListing,
};
use std::collections::BTreeSet;
use std::fmt;

use mmorpg_wire::{
    NpcKindCode, PlayerState as WirePlayerState, QuestStatusCode, ServerEvent as WireServerEvent,
    ServerMessage, WorldSnapshot,
};

/// Capacity used only for event projections whose payload is not a complete
/// player snapshot. Complete snapshots carry their own explicit capacity.
const EVENT_INVENTORY_CAPACITY_FALLBACK: usize = 16;

/// Failure to translate an otherwise syntactically valid protocol value into
/// content-backed presentation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterError {
    MissingNpcTemplate { npc_id: EntityId },
    NpcTemplateOutOfRange { npc_id: EntityId, template_id: u64 },
    UnknownItem { item_id: ItemId },
    UnknownQuest { quest_id: QuestId },
    UnsupportedSnapshotVersion { version: u32, supported: u32 },
    UnknownPlayer { player_id: EntityId },
    InvalidInventory { player_id: EntityId },
    InvalidQuestState { player_id: EntityId },
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
            Self::UnsupportedSnapshotVersion { version, supported } => write!(
                formatter,
                "snapshot schema version {version} is unsupported; expected {supported}"
            ),
            Self::UnknownPlayer { player_id } => {
                write!(
                    formatter,
                    "snapshot record refers to unknown player {player_id}"
                )
            }
            Self::InvalidInventory { player_id } => {
                write!(
                    formatter,
                    "inventory snapshot for player {player_id} is invalid"
                )
            }
            Self::InvalidQuestState { player_id } => {
                write!(
                    formatter,
                    "quest snapshot for player {player_id} is invalid"
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
    if snapshot.version != mmorpg_client_protocol::TEMP_SNAPSHOT_VERSION {
        return Err(AdapterError::UnsupportedSnapshotVersion {
            version: snapshot.version,
            supported: mmorpg_client_protocol::TEMP_SNAPSHOT_VERSION,
        });
    }
    let players = snapshot
        .players
        .values()
        .map(protocol_player_snapshot)
        .collect::<Result<Vec<_>, _>>()?;
    let npcs = snapshot
        .npcs
        .values()
        .map(protocol_npc)
        .collect::<Result<Vec<_>, _>>()?;
    let inventories = snapshot
        .items
        .iter()
        .map(|(player_id, stacks)| {
            if !snapshot.players.contains_key(player_id) {
                return Err(AdapterError::UnknownPlayer {
                    player_id: *player_id,
                });
            }
            let capacity = snapshot
                .players
                .get(player_id)
                .and_then(|player| player.inventory_capacity)
                .ok_or(AdapterError::InvalidInventory {
                    player_id: *player_id,
                })?;
            let stacks = stacks
                .iter()
                .map(|stack| {
                    if stack.player_id != *player_id {
                        return Err(AdapterError::InvalidInventory {
                            player_id: *player_id,
                        });
                    }
                    protocol_item_stack(stack)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if stacks.len() > capacity {
                return Err(AdapterError::InvalidInventory {
                    player_id: *player_id,
                });
            }
            let mut item_ids = BTreeSet::new();
            if stacks.iter().any(|stack| !item_ids.insert(stack.item_id)) {
                return Err(AdapterError::InvalidInventory {
                    player_id: *player_id,
                });
            }
            Ok((*player_id, stacks))
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    let quests = snapshot
        .quests
        .iter()
        .map(|(player_id, quests)| {
            if !snapshot.players.contains_key(player_id) {
                return Err(AdapterError::UnknownPlayer {
                    player_id: *player_id,
                });
            }
            let mut quest_ids = BTreeSet::new();
            for quest in quests {
                if quest.player_id != *player_id {
                    return Err(AdapterError::InvalidQuestState {
                        player_id: *player_id,
                    });
                }
                if !starter_catalog()
                    .quests
                    .iter()
                    .any(|definition| definition.id == quest.quest_id)
                {
                    return Err(AdapterError::UnknownQuest {
                        quest_id: quest.quest_id,
                    });
                }
                if quest.required_count == 0 || quest.progress > quest.required_count {
                    return Err(AdapterError::InvalidQuestState {
                        player_id: *player_id,
                    });
                }
                if !quest_ids.insert(quest.quest_id) {
                    return Err(AdapterError::InvalidQuestState {
                        player_id: *player_id,
                    });
                }
            }
            Ok((
                *player_id,
                quests.iter().map(protocol_quest_state).collect::<Vec<_>>(),
            ))
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;

    let mut projected = model.clone();
    projected.replace_from_snapshot(snapshot.world.tick, players, npcs);
    for (player_id, stacks) in inventories {
        if !projected.replace_inventory_snapshot(
            player_id,
            snapshot
                .players
                .get(&player_id)
                .and_then(|player| player.inventory_capacity)
                .ok_or(AdapterError::InvalidInventory { player_id })?,
            stacks,
        ) {
            return Err(AdapterError::InvalidInventory { player_id });
        }
    }
    for (player_id, quests) in quests {
        if !projected.replace_quest_snapshot(player_id, quests) {
            return Err(AdapterError::InvalidQuestState { player_id });
        }
    }
    *model = projected;
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

/// Applies one typed wire message to the renderer-independent presentation
/// model. Session-only messages are intentionally no-ops here; the client
/// transport owns connection status while this adapter owns authoritative
/// world projection.
pub fn apply_wire_message(
    model: &mut ClientWorld,
    message: &ServerMessage,
) -> Result<(), AdapterError> {
    match message {
        ServerMessage::Snapshot(snapshot) => apply_wire_snapshot(model, snapshot),
        ServerMessage::Event(event) => {
            let event = wire_event(event)?;
            let _ = model.apply_event(&event);
            Ok(())
        }
        ServerMessage::SkippedEvent { .. } => Ok(()),
        ServerMessage::Welcome { .. }
        | ServerMessage::Authenticated { .. }
        | ServerMessage::CharacterList { .. }
        | ServerMessage::CharacterSelected { .. }
        | ServerMessage::Connected { .. }
        | ServerMessage::ContentAccepted { .. }
        | ServerMessage::ContentMismatch { .. }
        | ServerMessage::Error { .. } => Ok(()),
    }
}

fn apply_wire_snapshot(
    model: &mut ClientWorld,
    snapshot: &WorldSnapshot,
) -> Result<(), AdapterError> {
    if snapshot.version != mmorpg_wire::SNAPSHOT_SCHEMA_VERSION {
        return Err(AdapterError::UnsupportedSnapshotVersion {
            version: u32::from(snapshot.version),
            supported: u32::from(mmorpg_wire::SNAPSHOT_SCHEMA_VERSION),
        });
    }
    let protocol = mmorpg_client_protocol::Snapshot {
        version: u32::from(snapshot.version),
        world: mmorpg_client_protocol::WorldState {
            tick: snapshot.tick,
            players: u64::from(snapshot.player_count),
            npcs: u64::from(snapshot.npc_count),
            enemies: u64::from(snapshot.enemy_count),
            vendors: u64::from(snapshot.vendor_count),
        },
        players: snapshot
            .players
            .iter()
            .map(wire_player)
            .map(|player| (player.id, player))
            .collect(),
        npcs: snapshot
            .npcs
            .iter()
            .map(wire_npc)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|npc| (npc.id, npc))
            .collect(),
        items: snapshot
            .players
            .iter()
            .map(|player| {
                (
                    EntityId(player.player_id),
                    player
                        .inventory
                        .iter()
                        .map(|stack| ItemState {
                            player_id: EntityId(player.player_id),
                            item_id: ItemId(stack.item_id),
                            quantity: stack.quantity,
                        })
                        .collect(),
                )
            })
            .collect(),
        quests: snapshot
            .players
            .iter()
            .map(|player| {
                (
                    EntityId(player.player_id),
                    player
                        .quests
                        .iter()
                        .map(|quest| QuestState {
                            player_id: EntityId(player.player_id),
                            quest_id: QuestId(quest.quest_id),
                            progress: quest.progress,
                            required_count: quest.required_count,
                            status: wire_quest_status(quest.status),
                        })
                        .collect(),
                )
            })
            .collect(),
    };
    apply_snapshot(model, &protocol)
}

fn wire_player(player: &WirePlayerState) -> mmorpg_client_protocol::PlayerState {
    mmorpg_client_protocol::PlayerState {
        id: EntityId(player.player_id),
        name: player.name.clone(),
        role: wire_role(player.role),
        position: mmorpg_client_protocol::Position::new(player.position.x, player.position.y),
        health: player.health,
        max_health: player.max_health,
        gold: player.gold,
        inventory_capacity: Some(player.inventory_capacity as usize),
        target: player.target_id.map(EntityId),
    }
}

fn wire_npc(npc: &mmorpg_wire::NpcState) -> Result<mmorpg_client_protocol::NpcState, AdapterError> {
    Ok(mmorpg_client_protocol::NpcState {
        id: EntityId(npc.entity_id),
        template_id: Some(u64::from(npc.template_id)),
        name: npc.name.clone(),
        kind: match npc.kind {
            NpcKindCode::Vendor => NpcKind::Vendor,
            NpcKindCode::Enemy => NpcKind::Enemy,
        },
        position: mmorpg_client_protocol::Position::new(npc.position.x, npc.position.y),
        health: npc.health,
        max_health: npc.max_health,
    })
}

fn wire_role(role: mmorpg_wire::RoleCode) -> Role {
    match role {
        mmorpg_wire::RoleCode::Tank => Role::Tank,
        mmorpg_wire::RoleCode::Healer => Role::Healer,
        mmorpg_wire::RoleCode::DamageDealer => Role::DamageDealer,
    }
}

fn wire_quest_status(status: QuestStatusCode) -> QuestStatus {
    match status {
        QuestStatusCode::Accepted => QuestStatus::Accepted,
        QuestStatusCode::Completed => QuestStatus::Completed,
        QuestStatusCode::Rewarded => QuestStatus::Rewarded,
    }
}

fn wire_event(event: &WireServerEvent) -> Result<Event, AdapterError> {
    let event = match event {
        WireServerEvent::PlayerJoined { player } => Event::PlayerJoined {
            player: PlayerSnapshot {
                id: CoreEntityId(player.player_id),
                name: player.name.clone(),
                role: wire_role(player.role),
                position: mmorpg_core::Position::new(player.position.x, player.position.y),
                health: player.health,
                max_health: player.max_health,
                target: player.target_id.map(CoreEntityId),
                gold: player.gold,
                inventory: Inventory::new(EVENT_INVENTORY_CAPACITY_FALLBACK),
                quests: Vec::new(),
            },
        },
        WireServerEvent::PlayerLeft { player_id } => Event::PlayerLeft {
            player_id: CoreEntityId(*player_id),
        },
        WireServerEvent::PlayerMoved {
            player_id,
            position,
            area,
        } => Event::PlayerMoved {
            player_id: CoreEntityId(*player_id),
            position: mmorpg_core::Position::new(position.x, position.y),
            area: match area {
                mmorpg_wire::ZoneAreaCode::Town => mmorpg_core::ZoneArea::Town,
                mmorpg_wire::ZoneAreaCode::Field => mmorpg_core::ZoneArea::Field,
            },
        },
        WireServerEvent::TargetSelected {
            player_id,
            target_id,
        } => Event::TargetSelected {
            player_id: CoreEntityId(*player_id),
            target_id: CoreEntityId(*target_id),
        },
        WireServerEvent::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => Event::AttackResolved {
            player_id: CoreEntityId(*player_id),
            target_id: CoreEntityId(*target_id),
            damage: *damage,
            target_health: *target_health,
        },
        WireServerEvent::HealResolved {
            player_id,
            target_id,
            amount,
            target_health,
        } => Event::HealResolved {
            player_id: CoreEntityId(*player_id),
            target_id: CoreEntityId(*target_id),
            amount: *amount,
            target_health: *target_health,
        },
        WireServerEvent::EnemyDefeated { enemy_id } => Event::EnemyDefeated {
            enemy_id: CoreEntityId(*enemy_id),
        },
        WireServerEvent::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => Event::VendorListed {
            player_id: CoreEntityId(*player_id),
            vendor_id: CoreEntityId(*vendor_id),
            listings: listings
                .iter()
                .map(wire_vendor_listing)
                .collect::<Result<Vec<_>, _>>()?,
        },
        WireServerEvent::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => Event::ItemPurchased {
            player_id: CoreEntityId(*player_id),
            vendor_id: CoreEntityId(*vendor_id),
            item_id: ItemId(*item_id),
            quantity: *quantity,
            total_price: *total_price,
            gold_remaining: *gold_remaining,
        },
        WireServerEvent::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => Event::LootRewarded {
            player_id: CoreEntityId(*player_id),
            enemy_id: CoreEntityId(*enemy_id),
            item_id: ItemId(*item_id),
            quantity: *quantity,
        },
        WireServerEvent::TransactionRejected { player_id, reason } => Event::TransactionRejected {
            player_id: CoreEntityId(*player_id),
            reason: reason.clone(),
        },
        WireServerEvent::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => Event::QuestOffersListed {
            player_id: CoreEntityId(*player_id),
            npc_id: CoreEntityId(*npc_id),
            quests: quests
                .iter()
                .map(wire_quest_offer)
                .collect::<Result<Vec<_>, _>>()?,
        },
        WireServerEvent::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => Event::QuestAccepted {
            player_id: CoreEntityId(*player_id),
            npc_id: CoreEntityId(*npc_id),
            quest_id: QuestId(*quest_id),
        },
        WireServerEvent::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => Event::QuestProgressed {
            player_id: CoreEntityId(*player_id),
            quest_id: QuestId(*quest_id),
            progress: *progress,
            required_count: *required_count,
        },
        WireServerEvent::QuestCompleted {
            player_id,
            quest_id,
        } => Event::QuestCompleted {
            player_id: CoreEntityId(*player_id),
            quest_id: QuestId(*quest_id),
        },
        WireServerEvent::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => Event::QuestRewarded {
            player_id: CoreEntityId(*player_id),
            quest_id: QuestId(*quest_id),
            gold: *gold,
            item_id: item_id.map(ItemId),
            item_quantity: *item_quantity,
            gold_remaining: *gold_remaining,
        },
        WireServerEvent::QuestRejected { player_id, reason } => Event::QuestRejected {
            player_id: CoreEntityId(*player_id),
            reason: reason.clone(),
        },
        WireServerEvent::CommandRejected { reason } => Event::CommandRejected {
            reason: reason.clone(),
        },
    };
    Ok(event)
}

fn wire_vendor_listing(
    listing: &mmorpg_wire::VendorListingState,
) -> Result<VendorListing, AdapterError> {
    let Some(definition) = item_definition(ItemId(listing.item_id)) else {
        return Err(AdapterError::UnknownItem {
            item_id: ItemId(listing.item_id),
        });
    };
    Ok(VendorListing {
        item_id: ItemId(listing.item_id),
        name: definition.name,
        unit_price: listing.unit_price,
        remaining_quantity: listing.remaining_quantity,
        max_stack: listing.max_stack,
    })
}

fn wire_quest_offer(offer: &mmorpg_wire::QuestOfferState) -> Result<QuestOffer, AdapterError> {
    let Some(definition) = starter_catalog()
        .quests
        .iter()
        .find(|definition| definition.id == QuestId(offer.quest_id))
    else {
        return Err(AdapterError::UnknownQuest {
            quest_id: QuestId(offer.quest_id),
        });
    };
    Ok(QuestOffer {
        quest_id: definition.id,
        name: definition.name,
        description: definition.description,
    })
}

fn protocol_player_snapshot(
    player: &mmorpg_client_protocol::PlayerState,
) -> Result<PlayerSnapshot, AdapterError> {
    let Some(capacity) = player.inventory_capacity else {
        return Err(AdapterError::InvalidInventory {
            player_id: player.id,
        });
    };
    Ok(PlayerSnapshot {
        id: core_entity_id(player.id),
        name: player.name.clone(),
        role: player.role,
        position: player.position,
        health: player.health,
        max_health: player.max_health,
        target: player.target.map(core_entity_id),
        gold: player.gold,
        inventory: Inventory::new(capacity),
        quests: Vec::new(),
    })
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

fn protocol_item_stack(stack: &ItemState) -> Result<ClientItemStack, AdapterError> {
    let Some(definition) = item_definition(stack.item_id) else {
        return Err(AdapterError::UnknownItem {
            item_id: stack.item_id,
        });
    };
    if stack.quantity == 0 || stack.quantity > definition.max_stack {
        return Err(AdapterError::InvalidInventory {
            player_id: stack.player_id,
        });
    }
    Ok(ClientItemStack {
        item_id: stack.item_id,
        quantity: stack.quantity,
    })
}

fn protocol_quest_state(state: &QuestState) -> ClientQuestState {
    ClientQuestState {
        quest_id: state.quest_id,
        progress: state.progress,
        required_count: Some(state.required_count),
        status: state.status,
    }
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
                inventory: Inventory::new(EVENT_INVENTORY_CAPACITY_FALLBACK),
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
        ItemState, NpcKind, PlayerState, Position, QuestId, QuestOfferState, QuestState,
        QuestStatus, Role, ServerEvent, Snapshot, WorldState,
    };

    fn snapshot() -> Snapshot {
        Snapshot {
            version: mmorpg_client_protocol::TEMP_SNAPSHOT_VERSION,
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
                    inventory_capacity: Some(24),
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
            items: [(
                EntityId(5),
                vec![ItemState {
                    player_id: EntityId(5),
                    item_id: ItemId(2),
                    quantity: 2,
                }],
            )]
            .into_iter()
            .collect(),
            quests: [(
                EntityId(5),
                vec![QuestState {
                    player_id: EntityId(5),
                    quest_id: QuestId(1),
                    progress: 1,
                    required_count: 3,
                    status: QuestStatus::Accepted,
                }],
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
            model
                .player(EntityId(5))
                .unwrap()
                .inventory
                .quantity(ItemId(2)),
            2
        );
        assert_eq!(model.player(EntityId(5)).unwrap().quests[0].progress, 1);
        assert_eq!(
            model.npc(EntityId(2)).unwrap().template_id,
            NpcTemplateId(2)
        );
        assert_eq!(model.npc(EntityId(2)).unwrap().name, "Field Wolf");
        assert_eq!(model.player(EntityId(5)).unwrap().inventory.capacity(), 24);
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

    #[test]
    fn applies_typed_wire_snapshot_atomically_with_inventory_and_quests() {
        let mut model = ClientWorld::default();
        let message = ServerMessage::Snapshot(WorldSnapshot {
            version: mmorpg_wire::SNAPSHOT_SCHEMA_VERSION,
            tick: 12,
            player_count: 1,
            npc_count: 1,
            enemy_count: 1,
            vendor_count: 0,
            players: vec![WirePlayerState {
                player_id: 5,
                name: "Aria".to_owned(),
                role: mmorpg_wire::RoleCode::DamageDealer,
                position: mmorpg_wire::PositionState { x: 2.0, y: 3.0 },
                health: 88,
                max_health: 100,
                target_id: Some(2),
                gold: 20,
                inventory_capacity: 24,
                inventory: vec![mmorpg_wire::ItemStackState {
                    item_id: 2,
                    quantity: 2,
                }],
                quests: vec![mmorpg_wire::QuestState {
                    quest_id: 1,
                    progress: 1,
                    required_count: 3,
                    status: QuestStatusCode::Accepted,
                }],
            }],
            npcs: vec![mmorpg_wire::NpcState {
                entity_id: 2,
                template_id: 2,
                name: "Field Wolf".to_owned(),
                kind: NpcKindCode::Enemy,
                position: mmorpg_wire::PositionState { x: 24.0, y: 0.0 },
                health: 100,
                max_health: 100,
            }],
        });

        apply_wire_message(&mut model, &message).expect("valid typed snapshot");

        assert_eq!(model.world_tick(), Some(12));
        assert_eq!(model.player(EntityId(5)).unwrap().gold, 20);
        assert_eq!(model.player(EntityId(5)).unwrap().inventory.capacity(), 24);
        assert_eq!(
            model
                .player(EntityId(5))
                .unwrap()
                .inventory
                .quantity(ItemId(2)),
            2
        );
        assert_eq!(model.player(EntityId(5)).unwrap().quests[0].progress, 1);
        assert_eq!(model.npc(EntityId(2)).unwrap().name, "Field Wolf");
    }

    #[test]
    fn applies_typed_wire_event_through_the_same_authoritative_projection() {
        let mut model = ClientWorld::default();
        apply_snapshot(&mut model, &snapshot()).expect("valid snapshot");
        let message = ServerMessage::Event(WireServerEvent::VendorListed {
            player_id: 5,
            vendor_id: 1,
            listings: vec![mmorpg_wire::VendorListingState {
                item_id: 2,
                name: "untrusted display name".to_owned(),
                unit_price: 2,
                remaining_quantity: 99,
                max_stack: 20,
            }],
        });

        apply_wire_message(&mut model, &message).expect("valid typed event");

        assert_eq!(
            model.vendor_listings(EntityId(1)).unwrap()[0].name,
            "Town Ration"
        );
        assert_eq!(
            model.vendor_listings(EntityId(1)).unwrap()[0].remaining_quantity,
            99
        );
    }

    #[test]
    fn rejects_a_snapshot_without_explicit_inventory_capacity_atomically() {
        let mut model = ClientWorld::default();
        let mut snapshot = snapshot();
        snapshot
            .players
            .get_mut(&EntityId(5))
            .expect("fixture player")
            .inventory_capacity = None;

        assert_eq!(
            apply_snapshot(&mut model, &snapshot),
            Err(AdapterError::InvalidInventory {
                player_id: EntityId(5)
            })
        );
        assert_eq!(model, ClientWorld::default());
    }

    #[test]
    fn rejects_an_unsupported_snapshot_version_atomically() {
        let mut model = ClientWorld::default();
        let mut snapshot = snapshot();
        snapshot.version = 1;

        assert_eq!(
            apply_snapshot(&mut model, &snapshot),
            Err(AdapterError::UnsupportedSnapshotVersion {
                version: 1,
                supported: mmorpg_client_protocol::TEMP_SNAPSHOT_VERSION,
            })
        );
        assert_eq!(model, ClientWorld::default());
    }
}
