//! Presentation state projected from authoritative `mmorpg-core` events.
//!
//! This crate intentionally does not expose a command application API. A
//! client may create commands elsewhere, but this model only changes when a
//! server event or an authoritative snapshot is applied.

use std::collections::{BTreeMap, BTreeSet};

use mmorpg_content::NpcTemplateId;
use mmorpg_core::{
    EntityId, Event, Inventory, ItemId, Npc, NpcKind, PlayerSnapshot, Position, QuestId,
    QuestOffer, QuestProgress, QuestStatus, Role, ZoneArea, item_definition,
};

/// Result of projecting one server event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyEventResult {
    /// The event changed presentation state, or was recorded as a notification.
    Applied,
    /// The event could not be safely projected because required authoritative
    /// state was not present or the event payload was inconsistent.
    Ignored,
}

/// A player entity as currently known to the presentation model.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientPlayer {
    pub id: EntityId,
    pub name: String,
    pub role: Role,
    pub position: Position,
    pub area: ZoneArea,
    pub health: u32,
    pub max_health: u32,
    pub target: Option<EntityId>,
    pub gold: u32,
    pub inventory: ClientInventory,
    pub quests: Vec<ClientQuestState>,
}

/// An NPC entity as currently known to the presentation model.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientNpc {
    pub id: EntityId,
    pub template_id: NpcTemplateId,
    pub name: String,
    pub kind: NpcKind,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
    pub defeated: bool,
}

/// A presentation entity. Rendering code can match on this without depending
/// on the authoritative `World` or on a renderer implementation.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientEntity {
    Player(ClientPlayer),
    Npc(ClientNpc),
}

impl ClientEntity {
    pub fn id(&self) -> EntityId {
        match self {
            Self::Player(player) => player.id,
            Self::Npc(npc) => npc.id,
        }
    }

    pub fn position(&self) -> Position {
        match self {
            Self::Player(player) => player.position,
            Self::Npc(npc) => npc.position,
        }
    }
}

/// A client-side copy of an authoritative inventory stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientItemStack {
    pub item_id: ItemId,
    pub quantity: u32,
}

/// Slot and stack information needed to render a player's inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientInventory {
    capacity: usize,
    stacks: Vec<ClientItemStack>,
}

impl ClientInventory {
    fn from_authoritative(inventory: &Inventory) -> Self {
        Self {
            capacity: inventory.capacity(),
            stacks: inventory
                .stacks()
                .map(|stack| ClientItemStack {
                    item_id: stack.item_id,
                    quantity: stack.quantity,
                })
                .collect(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used_slots(&self) -> usize {
        self.stacks.len()
    }

    pub fn stacks(&self) -> impl Iterator<Item = &ClientItemStack> {
        self.stacks.iter()
    }

    pub fn quantity(&self, item_id: ItemId) -> u32 {
        self.stacks
            .iter()
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| stack.quantity)
            .sum()
    }

    fn can_add(&self, item_id: ItemId, quantity: u32) -> bool {
        let Some(definition) = item_definition(item_id) else {
            return false;
        };
        if quantity == 0 {
            return false;
        }
        let existing_capacity: u64 = self
            .stacks
            .iter()
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| definition.max_stack.saturating_sub(stack.quantity) as u64)
            .sum();
        let empty_slots = self.capacity.saturating_sub(self.stacks.len()) as u64;
        existing_capacity.saturating_add(empty_slots * u64::from(definition.max_stack))
            >= u64::from(quantity)
    }

    fn add(&mut self, item_id: ItemId, mut quantity: u32) {
        let definition = item_definition(item_id).expect("can_add validates item IDs");
        for stack in &mut self.stacks {
            if stack.item_id != item_id || stack.quantity >= definition.max_stack {
                continue;
            }
            let amount = quantity.min(definition.max_stack - stack.quantity);
            stack.quantity += amount;
            quantity -= amount;
            if quantity == 0 {
                return;
            }
        }

        while quantity > 0 {
            let amount = quantity.min(definition.max_stack);
            self.stacks.push(ClientItemStack {
                item_id,
                quantity: amount,
            });
            quantity -= amount;
        }
    }
}

/// Presentation copy of one player's quest state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientQuestState {
    pub quest_id: QuestId,
    pub progress: u32,
    /// `None` until a progress event or a player snapshot supplies the count.
    pub required_count: Option<u32>,
    pub status: QuestStatus,
}

impl From<&QuestProgress> for ClientQuestState {
    fn from(progress: &QuestProgress) -> Self {
        Self {
            quest_id: progress.quest_id,
            progress: progress.progress,
            required_count: Some(progress.required_count),
            status: progress.status,
        }
    }
}

/// A vendor listing suitable for UI presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientVendorListing {
    pub item_id: ItemId,
    pub name: &'static str,
    pub unit_price: u32,
    pub remaining_quantity: u32,
    pub max_stack: u32,
}

/// A server rejection or other authoritative notification for UI display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientNotification {
    CommandRejected { reason: String },
    TransactionRejected { player_id: EntityId, reason: String },
    QuestRejected { player_id: EntityId, reason: String },
}

/// Presentation state for the currently known portion of the authoritative
/// world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientWorld {
    entities: BTreeMap<EntityId, ClientEntity>,
    defeated_enemies: BTreeSet<EntityId>,
    vendor_listings: BTreeMap<EntityId, Vec<ClientVendorListing>>,
    quest_offers: BTreeMap<EntityId, Vec<QuestOffer>>,
    last_notification: Option<ClientNotification>,
    world_tick: Option<u64>,
}

impl ClientWorld {
    pub fn entity(&self, entity_id: EntityId) -> Option<&ClientEntity> {
        self.entities.get(&entity_id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &ClientEntity> {
        self.entities.values()
    }

    pub fn player(&self, player_id: EntityId) -> Option<&ClientPlayer> {
        match self.entities.get(&player_id) {
            Some(ClientEntity::Player(player)) => Some(player),
            _ => None,
        }
    }

    pub fn npc(&self, npc_id: EntityId) -> Option<&ClientNpc> {
        match self.entities.get(&npc_id) {
            Some(ClientEntity::Npc(npc)) => Some(npc),
            _ => None,
        }
    }

    pub fn vendor_listings(&self, vendor_id: EntityId) -> Option<&[ClientVendorListing]> {
        self.vendor_listings.get(&vendor_id).map(Vec::as_slice)
    }

    pub fn quest_offers(&self, npc_id: EntityId) -> Option<&[QuestOffer]> {
        self.quest_offers.get(&npc_id).map(Vec::as_slice)
    }

    pub fn last_notification(&self) -> Option<&ClientNotification> {
        self.last_notification.as_ref()
    }

    /// Returns the tick associated with the most recently applied complete
    /// world snapshot, when the transport supplies one.
    pub fn world_tick(&self) -> Option<u64> {
        self.world_tick
    }

    /// Replaces the entity projection with one complete authoritative
    /// snapshot.
    ///
    /// The replacement is intentionally whole-world: stale entities and
    /// transient vendor/quest query results cannot survive a successful
    /// bootstrap or reconciliation snapshot. Inventory and quest state are
    /// carried by each [`PlayerSnapshot`], so this API does not invent or
    /// preserve mutable player data across a replacement.
    pub fn replace_from_snapshot(
        &mut self,
        world_tick: u64,
        players: impl IntoIterator<Item = PlayerSnapshot>,
        npcs: impl IntoIterator<Item = Npc>,
    ) {
        self.entities.clear();
        self.defeated_enemies.clear();
        self.vendor_listings.clear();
        self.quest_offers.clear();
        self.last_notification = None;
        self.world_tick = Some(world_tick);

        for player in players {
            self.entities.insert(
                player.id,
                ClientEntity::Player(ClientPlayer::from_snapshot(&player)),
            );
        }
        for npc in npcs {
            self.apply_npc_snapshot(&npc);
        }
    }

    /// Applies one authoritative server snapshot for an NPC.
    ///
    /// The current core event stream has no NPC snapshot event, so a client
    /// bootstrap/reconciliation adapter must call this method directly with
    /// data obtained from an authoritative snapshot channel.
    pub fn apply_npc_snapshot(&mut self, npc: &Npc) {
        let defeated = self.defeated_enemies.contains(&npc.id) || npc.health == 0;
        if defeated {
            self.defeated_enemies.insert(npc.id);
        } else {
            self.defeated_enemies.remove(&npc.id);
        }
        self.entities.insert(
            npc.id,
            ClientEntity::Npc(ClientNpc {
                id: npc.id,
                template_id: npc.template_id,
                name: npc.name.clone(),
                kind: npc.kind,
                position: npc.position,
                health: if defeated { 0 } else { npc.health },
                max_health: npc.max_health,
                defeated,
            }),
        );
    }

    /// Applies an authoritative event. No client command or locally computed
    /// gameplay result can enter this model through this API.
    pub fn apply_event(&mut self, event: &Event) -> ApplyEventResult {
        match event {
            Event::PlayerJoined { player } => {
                self.entities.insert(
                    player.id,
                    ClientEntity::Player(ClientPlayer::from_snapshot(player)),
                );
                ApplyEventResult::Applied
            }
            Event::PlayerLeft { player_id } => {
                let removed = matches!(
                    self.entities.remove(player_id),
                    Some(ClientEntity::Player(_))
                );
                if removed {
                    for entity in self.entities.values_mut() {
                        if let ClientEntity::Player(player) = entity
                            && player.target == Some(*player_id)
                        {
                            player.target = None;
                        }
                    }
                    ApplyEventResult::Applied
                } else {
                    ApplyEventResult::Ignored
                }
            }
            Event::PlayerMoved {
                player_id,
                position,
                area,
            } => {
                let Some(ClientEntity::Player(player)) = self.entities.get_mut(player_id) else {
                    return ApplyEventResult::Ignored;
                };
                player.position = *position;
                player.area = *area;
                ApplyEventResult::Applied
            }
            Event::TargetSelected {
                player_id,
                target_id,
            } => {
                let Some(ClientEntity::Player(player)) = self.entities.get_mut(player_id) else {
                    return ApplyEventResult::Ignored;
                };
                player.target = Some(*target_id);
                ApplyEventResult::Applied
            }
            Event::AttackResolved {
                player_id,
                target_id,
                target_health,
                ..
            } => {
                if !self.entities.contains_key(target_id) {
                    return ApplyEventResult::Ignored;
                }
                let Some(attacker) = self.entities.get_mut(player_id) else {
                    return ApplyEventResult::Ignored;
                };
                let ClientEntity::Player(attacker) = attacker else {
                    return ApplyEventResult::Ignored;
                };
                attacker.target = Some(*target_id);
                let Some(target) = self.entities.get_mut(target_id) else {
                    return ApplyEventResult::Ignored;
                };
                match target {
                    ClientEntity::Player(player) => player.health = *target_health,
                    ClientEntity::Npc(npc) => {
                        npc.health = *target_health;
                        npc.defeated = *target_health == 0;
                    }
                }
                ApplyEventResult::Applied
            }
            Event::EnemyDefeated { enemy_id } => {
                let Some(ClientEntity::Npc(npc)) = self.entities.get_mut(enemy_id) else {
                    return ApplyEventResult::Ignored;
                };
                self.defeated_enemies.insert(*enemy_id);
                npc.health = 0;
                npc.defeated = true;
                ApplyEventResult::Applied
            }
            Event::VendorListed {
                vendor_id,
                listings,
                ..
            } => {
                self.vendor_listings.insert(
                    *vendor_id,
                    listings.iter().map(ClientVendorListing::from).collect(),
                );
                ApplyEventResult::Applied
            }
            Event::ItemPurchased {
                player_id,
                vendor_id,
                item_id,
                quantity,
                gold_remaining,
                ..
            } => {
                if !self.apply_item_and_gold(*player_id, *item_id, *quantity, *gold_remaining) {
                    return ApplyEventResult::Ignored;
                }
                if let Some(listings) = self.vendor_listings.get_mut(vendor_id)
                    && let Some(listing) = listings
                        .iter_mut()
                        .find(|listing| listing.item_id == *item_id)
                {
                    listing.remaining_quantity =
                        listing.remaining_quantity.saturating_sub(*quantity);
                }
                ApplyEventResult::Applied
            }
            Event::LootRewarded {
                player_id,
                item_id,
                quantity,
                ..
            } => {
                if self.apply_item(*player_id, *item_id, *quantity) {
                    ApplyEventResult::Applied
                } else {
                    ApplyEventResult::Ignored
                }
            }
            Event::TransactionRejected { player_id, reason } => {
                self.last_notification = Some(ClientNotification::TransactionRejected {
                    player_id: *player_id,
                    reason: reason.clone(),
                });
                ApplyEventResult::Applied
            }
            Event::QuestOffersListed { npc_id, quests, .. } => {
                self.quest_offers.insert(*npc_id, quests.clone());
                ApplyEventResult::Applied
            }
            Event::QuestAccepted {
                player_id,
                quest_id,
                ..
            } => {
                let Some(player) = self.player_mut(*player_id) else {
                    return ApplyEventResult::Ignored;
                };
                upsert_quest(
                    &mut player.quests,
                    ClientQuestState {
                        quest_id: *quest_id,
                        progress: 0,
                        required_count: None,
                        status: QuestStatus::Accepted,
                    },
                );
                ApplyEventResult::Applied
            }
            Event::QuestProgressed {
                player_id,
                quest_id,
                progress,
                required_count,
            } => {
                let Some(player) = self.player_mut(*player_id) else {
                    return ApplyEventResult::Ignored;
                };
                let state = player
                    .quests
                    .iter_mut()
                    .find(|state| state.quest_id == *quest_id);
                if let Some(state) = state {
                    state.progress = *progress;
                    state.required_count = Some(*required_count);
                } else {
                    player.quests.push(ClientQuestState {
                        quest_id: *quest_id,
                        progress: *progress,
                        required_count: Some(*required_count),
                        status: QuestStatus::Accepted,
                    });
                }
                ApplyEventResult::Applied
            }
            Event::QuestCompleted {
                player_id,
                quest_id,
            } => {
                let Some(player) = self.player_mut(*player_id) else {
                    return ApplyEventResult::Ignored;
                };
                if let Some(state) = player
                    .quests
                    .iter_mut()
                    .find(|state| state.quest_id == *quest_id)
                {
                    state.status = QuestStatus::Completed;
                    ApplyEventResult::Applied
                } else {
                    player.quests.push(ClientQuestState {
                        quest_id: *quest_id,
                        progress: 0,
                        required_count: None,
                        status: QuestStatus::Completed,
                    });
                    ApplyEventResult::Applied
                }
            }
            Event::QuestRewarded {
                player_id,
                quest_id,
                gold_remaining,
                item_id,
                item_quantity,
                ..
            } => {
                if let Some(item_id) = item_id
                    && !self.apply_item_and_gold(
                        *player_id,
                        *item_id,
                        *item_quantity,
                        *gold_remaining,
                    )
                {
                    return ApplyEventResult::Ignored;
                }
                if item_id.is_none() {
                    let Some(player) = self.player_mut(*player_id) else {
                        return ApplyEventResult::Ignored;
                    };
                    player.gold = *gold_remaining;
                }
                let Some(player) = self.player_mut(*player_id) else {
                    return ApplyEventResult::Ignored;
                };
                if let Some(state) = player
                    .quests
                    .iter_mut()
                    .find(|state| state.quest_id == *quest_id)
                {
                    state.status = QuestStatus::Rewarded;
                } else {
                    player.quests.push(ClientQuestState {
                        quest_id: *quest_id,
                        progress: 0,
                        required_count: None,
                        status: QuestStatus::Rewarded,
                    });
                }
                ApplyEventResult::Applied
            }
            Event::QuestRejected { player_id, reason } => {
                self.last_notification = Some(ClientNotification::QuestRejected {
                    player_id: *player_id,
                    reason: reason.clone(),
                });
                ApplyEventResult::Applied
            }
            Event::CommandRejected { reason } => {
                self.last_notification = Some(ClientNotification::CommandRejected {
                    reason: reason.clone(),
                });
                ApplyEventResult::Applied
            }
        }
    }

    fn player_mut(&mut self, player_id: EntityId) -> Option<&mut ClientPlayer> {
        match self.entities.get_mut(&player_id) {
            Some(ClientEntity::Player(player)) => Some(player),
            _ => None,
        }
    }

    fn apply_item(&mut self, player_id: EntityId, item_id: ItemId, quantity: u32) -> bool {
        let Some(player) = self.player_mut(player_id) else {
            return false;
        };
        if !player.inventory.can_add(item_id, quantity) {
            return false;
        }
        player.inventory.add(item_id, quantity);
        true
    }

    fn apply_item_and_gold(
        &mut self,
        player_id: EntityId,
        item_id: ItemId,
        quantity: u32,
        gold_remaining: u32,
    ) -> bool {
        let Some(player) = self.player_mut(player_id) else {
            return false;
        };
        let mut inventory = player.inventory.clone();
        if !inventory.can_add(item_id, quantity) {
            return false;
        }
        inventory.add(item_id, quantity);
        player.inventory = inventory;
        player.gold = gold_remaining;
        true
    }
}

impl ClientPlayer {
    fn from_snapshot(snapshot: &PlayerSnapshot) -> Self {
        Self {
            id: snapshot.id,
            name: snapshot.name.clone(),
            role: snapshot.role,
            position: snapshot.position,
            area: ZoneArea::from_position_for_client(snapshot.position),
            health: snapshot.health,
            max_health: snapshot.max_health,
            target: snapshot.target,
            gold: snapshot.gold,
            inventory: ClientInventory::from_authoritative(&snapshot.inventory),
            quests: snapshot.quests.iter().map(ClientQuestState::from).collect(),
        }
    }
}

impl From<&mmorpg_core::VendorListing> for ClientVendorListing {
    fn from(listing: &mmorpg_core::VendorListing) -> Self {
        Self {
            item_id: listing.item_id,
            name: listing.name,
            unit_price: listing.unit_price,
            remaining_quantity: listing.remaining_quantity,
            max_stack: listing.max_stack,
        }
    }
}

// ZoneArea::from_position is intentionally private in mmorpg-core. The
// client receives the authoritative area on movement events; this fallback is
// only for the initial player snapshot, whose current API omits area.
trait ClientArea {
    fn from_position_for_client(position: Position) -> Self;
}

impl ClientArea for ZoneArea {
    fn from_position_for_client(position: Position) -> Self {
        if position.x.abs() <= 10.0 && position.y.abs() <= 10.0 {
            Self::Town
        } else {
            Self::Field
        }
    }
}

fn upsert_quest(quests: &mut Vec<ClientQuestState>, replacement: ClientQuestState) {
    if let Some(existing) = quests
        .iter_mut()
        .find(|state| state.quest_id == replacement.quest_id)
    {
        *existing = replacement;
    } else {
        quests.push(replacement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_core::{Command, World};

    fn join_events(world: &mut World) -> Vec<Event> {
        world.step([Command::JoinPlayer {
            name: "Aster".to_owned(),
            role: Role::DamageDealer,
        }])
    }

    fn apply_all(model: &mut ClientWorld, events: &[Event]) {
        for event in events {
            assert_eq!(model.apply_event(event), ApplyEventResult::Applied);
        }
    }

    #[test]
    fn projects_player_snapshot_movement_target_attack_defeat_and_leave() {
        let mut authoritative = World::new_starter_zone();
        let joined = join_events(&mut authoritative);
        let player_id = match &joined[0] {
            Event::PlayerJoined { player } => player.id,
            _ => panic!("expected player join"),
        };
        let wolf_id = authoritative
            .npcs()
            .find(|npc| npc.kind == NpcKind::Enemy)
            .expect("starter wolf")
            .id;
        let mut model = ClientWorld::default();
        apply_all(&mut model, &joined);
        for npc in authoritative.npcs() {
            model.apply_npc_snapshot(npc);
        }

        let events = authoritative.step([
            Command::Move {
                player_id,
                dx: 5.0,
                dy: 0.0,
            },
            Command::SelectTarget {
                player_id,
                target_id: wolf_id,
            },
            Command::BasicAttack { player_id },
        ]);
        apply_all(&mut model, &events);
        assert_eq!(
            model.player(player_id).unwrap().position,
            Position::new(5.0, 0.0)
        );
        assert_eq!(model.player(player_id).unwrap().target, Some(wolf_id));
        assert_eq!(model.npc(wolf_id).unwrap().health, 88);

        let defeated = authoritative.step((0..8).map(|_| Command::BasicAttack { player_id }));
        apply_all(&mut model, &defeated);
        assert!(model.npc(wolf_id).unwrap().defeated);

        apply_all(&mut model, &[Event::PlayerLeft { player_id }]);
        assert!(model.player(player_id).is_none());
        assert!(model.npc(wolf_id).is_some());
    }

    #[test]
    fn projects_vendor_purchase_loot_and_transaction_rejection() {
        let mut authoritative = World::new_starter_zone();
        let joined = join_events(&mut authoritative);
        let player_id = match &joined[0] {
            Event::PlayerJoined { player } => player.id,
            _ => unreachable!(),
        };
        let vendor_id = EntityId(1);
        let wolf_id = EntityId(2);
        let mut model = ClientWorld::default();
        apply_all(&mut model, &joined);
        for npc in authoritative.npcs() {
            model.apply_npc_snapshot(npc);
        }

        let events = authoritative.step([
            Command::ListVendor {
                player_id,
                vendor_id,
            },
            Command::BuyItem {
                player_id,
                vendor_id,
                item_id: ItemId::TOWN_RATION,
                quantity: 2,
            },
        ]);
        apply_all(&mut model, &events);
        let player = model.player(player_id).unwrap();
        assert_eq!(player.gold, 16);
        assert_eq!(player.inventory.quantity(ItemId::TOWN_RATION), 2);
        assert_eq!(
            model.vendor_listings(vendor_id).unwrap()[0].remaining_quantity,
            98
        );

        let events = authoritative.step([
            Command::SelectTarget {
                player_id,
                target_id: wolf_id,
            },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::BasicAttack { player_id },
            Command::LootEnemy {
                player_id,
                enemy_id: wolf_id,
            },
        ]);
        apply_all(&mut model, &events);
        assert_eq!(
            model
                .player(player_id)
                .unwrap()
                .inventory
                .quantity(ItemId::FIELD_WOLF_PELT),
            1
        );

        let rejection = Event::TransactionRejected {
            player_id,
            reason: "inventory is full".to_owned(),
        };
        assert_eq!(model.apply_event(&rejection), ApplyEventResult::Applied);
        assert!(matches!(
            model.last_notification(),
            Some(ClientNotification::TransactionRejected { player_id: id, .. }) if *id == player_id
        ));
    }

    #[test]
    fn projects_quest_lifecycle_and_reward() {
        let mut authoritative = World::new_starter_zone();
        let joined = join_events(&mut authoritative);
        let player_id = match &joined[0] {
            Event::PlayerJoined { player } => player.id,
            _ => unreachable!(),
        };
        let mut model = ClientWorld::default();
        apply_all(&mut model, &joined);
        for npc in authoritative.npcs() {
            model.apply_npc_snapshot(npc);
        }

        let offer_events = authoritative.step([Command::ListQuestOffers {
            player_id,
            npc_id: EntityId(1),
        }]);
        apply_all(&mut model, &offer_events);
        assert_eq!(model.quest_offers(EntityId(1)).unwrap().len(), 1);

        let accept_events = authoritative.step([Command::AcceptQuest {
            player_id,
            npc_id: EntityId(1),
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        apply_all(&mut model, &accept_events);
        assert_eq!(
            model.player(player_id).unwrap().quests[0].status,
            QuestStatus::Accepted
        );
        assert_eq!(
            model.player(player_id).unwrap().quests[0].required_count,
            None
        );

        let wolf_ids: Vec<_> = authoritative
            .npcs()
            .filter(|npc| npc.kind == NpcKind::Enemy)
            .map(|npc| npc.id)
            .collect();
        for npc_id in wolf_ids {
            let events = authoritative.step([
                Command::SelectTarget {
                    player_id,
                    target_id: npc_id,
                },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
                Command::BasicAttack { player_id },
            ]);
            apply_all(&mut model, &events);
        }
        let quest = &model.player(player_id).unwrap().quests[0];
        assert_eq!(quest.progress, 3);
        assert_eq!(quest.required_count, Some(3));
        assert_eq!(quest.status, QuestStatus::Completed);

        let reward_events = authoritative.step([Command::TurnInQuest {
            player_id,
            npc_id: EntityId(1),
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        apply_all(&mut model, &reward_events);
        let player = model.player(player_id).unwrap();
        assert_eq!(player.gold, 30);
        assert_eq!(player.inventory.quantity(ItemId::TOWN_RATION), 5);
        assert_eq!(player.quests[0].status, QuestStatus::Rewarded);
    }

    #[test]
    fn rejects_inconsistent_economy_event_without_partial_projection() {
        let mut authoritative = World::new_starter_zone();
        let joined = join_events(&mut authoritative);
        let player_id = match &joined[0] {
            Event::PlayerJoined { player } => player.id,
            _ => unreachable!(),
        };
        let mut model = ClientWorld::default();
        apply_all(&mut model, &joined);
        let event = Event::ItemPurchased {
            player_id,
            vendor_id: EntityId(1),
            item_id: ItemId(999),
            quantity: 1,
            total_price: 1,
            gold_remaining: 19,
        };
        assert_eq!(model.apply_event(&event), ApplyEventResult::Ignored);
        let player = model.player(player_id).unwrap();
        assert_eq!(player.gold, 20);
        assert_eq!(player.inventory.used_slots(), 0);
    }

    #[test]
    fn ignores_events_for_unknown_entities_without_partial_projection() {
        let mut authoritative = World::new_starter_zone();
        let joined = join_events(&mut authoritative);
        let player_id = match &joined[0] {
            Event::PlayerJoined { player } => player.id,
            _ => unreachable!(),
        };
        let mut model = ClientWorld::default();
        apply_all(&mut model, &joined);

        assert_eq!(
            model.apply_event(&Event::AttackResolved {
                player_id,
                target_id: EntityId(999),
                damage: 10,
                target_health: 90,
            }),
            ApplyEventResult::Ignored
        );
        assert_eq!(model.player(player_id).unwrap().target, None);

        assert_eq!(
            model.apply_event(&Event::EnemyDefeated {
                enemy_id: EntityId(999),
            }),
            ApplyEventResult::Ignored
        );
        assert!(model.entity(EntityId(999)).is_none());
    }
}
