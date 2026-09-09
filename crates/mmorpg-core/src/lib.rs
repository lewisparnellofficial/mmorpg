//! Authoritative, engine-independent MMORPG simulation primitives.
//!
//! This crate intentionally contains no sockets, database access, timers, or
//! rendering code. A region owner feeds commands into [`World::step`] and
//! consumes the resulting authoritative events.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

pub use mmorpg_content::{
    ContentCatalog, ItemDefinition, ItemId, QuestDefinition, QuestId, item_definition,
};
use mmorpg_content::{NpcTemplateId, ObjectiveDefinition, starter_catalog};

const MAX_MOVE_PER_COMMAND: f32 = 10.0;
const ATTACK_RANGE: f32 = 32.0;
const HEAL_RANGE: f32 = 32.0;
const HEAL_AMOUNT: u32 = 30;
const VENDOR_INTERACTION_RANGE: f32 = 12.0;
const STARTER_GOLD: u32 = 20;
const STARTER_INVENTORY_CAPACITY: usize = 16;

/// Server-owned timing parameters for the explicit timed-combat path.
///
/// Durations are represented in simulation ticks rather than wall-clock time.
/// This keeps combat decisions deterministic and leaves scheduling of the
/// fixed-rate simulation loop to the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatTiming {
    tick_hz: u32,
    cast_time_ticks: u64,
    cooldown_ticks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidCombatTiming;

impl CombatTiming {
    pub fn new(
        tick_hz: u32,
        cast_time_ticks: u64,
        cooldown_ticks: u64,
    ) -> Result<Self, InvalidCombatTiming> {
        if tick_hz == 0 {
            return Err(InvalidCombatTiming);
        }
        Ok(Self {
            tick_hz,
            cast_time_ticks,
            cooldown_ticks,
        })
    }

    pub const fn tick_hz(self) -> u32 {
        self.tick_hz
    }

    pub const fn cast_time_ticks(self) -> u64 {
        self.cast_time_ticks
    }

    pub const fn cooldown_ticks(self) -> u64 {
        self.cooldown_ticks
    }
}

impl fmt::Display for InvalidCombatTiming {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("combat tick rate must be greater than zero")
    }
}

impl std::error::Error for InvalidCombatTiming {}

/// Stable identifier for any live world entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(pub u64);

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// One non-empty inventory stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub quantity: u32,
}

/// Slot-limited, stack-aware player inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    capacity: usize,
    stacks: Vec<ItemStack>,
}

impl Inventory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            stacks: Vec::new(),
        }
    }

    /// Reconstructs inventory data captured by a persistence adapter.
    /// Validation of content IDs, stack bounds, and capacity belongs to the
    /// authoritative restore path before it becomes a live player.
    pub fn from_stacks(capacity: usize, stacks: Vec<ItemStack>) -> Self {
        Self { capacity, stacks }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used_slots(&self) -> usize {
        self.stacks.len()
    }

    pub fn stacks(&self) -> impl Iterator<Item = &ItemStack> {
        self.stacks.iter()
    }

    pub fn quantity(&self, item_id: ItemId) -> u32 {
        self.stacks
            .iter()
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| stack.quantity)
            .sum()
    }

    fn can_add(&self, definition: ItemDefinition, quantity: u32) -> bool {
        if quantity == 0 {
            return false;
        }
        let existing_capacity: u64 = self
            .stacks
            .iter()
            .filter(|stack| stack.item_id == definition.id)
            .map(|stack| (definition.max_stack - stack.quantity) as u64)
            .sum();
        let empty_slots = self.capacity.saturating_sub(self.stacks.len()) as u64;
        existing_capacity.saturating_add(empty_slots * definition.max_stack as u64)
            >= quantity as u64
    }

    fn add(&mut self, definition: ItemDefinition, mut quantity: u32) {
        for stack in &mut self.stacks {
            if stack.item_id != definition.id || stack.quantity == definition.max_stack {
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
            self.stacks.push(ItemStack {
                item_id: definition.id,
                quantity: amount,
            });
            quantity -= amount;
        }
    }
}

/// The three roles in the initial vertical slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Tank,
    Healer,
    DamageDealer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tank => "tank",
            Self::Healer => "healer",
            Self::DamageDealer => "damage",
        }
    }
}

impl FromStr for Role {
    type Err = InvalidRole;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "tank" => Ok(Self::Tank),
            "healer" | "heal" => Ok(Self::Healer),
            "damage" | "dps" | "damage-dealer" => Ok(Self::DamageDealer),
            _ => Err(InvalidRole(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidRole(pub String);

impl fmt::Display for InvalidRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown role '{}'; expected tank, healer, or damage",
            self.0
        )
    }
}

impl std::error::Error for InvalidRole {}

/// Two-dimensional starter-zone position. The client is responsible for
/// rendering; the server owns the authoritative value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    fn translated(self, dx: f32, dy: f32, bounds: Bounds) -> Self {
        Self {
            x: (self.x + dx).clamp(bounds.min_x, bounds.max_x),
            y: (self.y + dy).clamp(bounds.min_y, bounds.max_y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

impl Bounds {
    pub const fn starter_zone() -> Self {
        Self {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneArea {
    Town,
    Field,
}

impl ZoneArea {
    fn from_position(position: Position) -> Self {
        if position.x.abs() <= 10.0 && position.y.abs() <= 10.0 {
            Self::Town
        } else {
            Self::Field
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: EntityId,
    pub name: String,
    pub role: Role,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
    pub target: Option<EntityId>,
    pub gold: u32,
    pub inventory: Inventory,
    pub quests: Vec<QuestProgress>,
}

impl Player {
    fn snapshot(&self) -> PlayerSnapshot {
        PlayerSnapshot {
            id: self.id,
            name: self.name.clone(),
            role: self.role,
            position: self.position,
            health: self.health,
            max_health: self.max_health,
            target: self.target,
            gold: self.gold,
            inventory: self.inventory.clone(),
            quests: self.quests.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcKind {
    Vendor,
    Enemy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Npc {
    pub id: EntityId,
    pub template_id: NpcTemplateId,
    pub name: String,
    pub kind: NpcKind,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnapshot {
    pub id: EntityId,
    pub name: String,
    pub role: Role,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
    pub target: Option<EntityId>,
    pub gold: u32,
    pub inventory: Inventory,
    pub quests: Vec<QuestProgress>,
}

/// Character state retained across a safe logout or server restart.
///
/// Live entity IDs, health, targets, casts, and cooldowns deliberately remain
/// transient and are reset when this state becomes a live player again.
#[derive(Clone, Debug, PartialEq)]
pub struct DurablePlayerState {
    pub name: String,
    pub role: Role,
    pub position: Position,
    pub gold: u32,
    pub inventory: Inventory,
    pub quests: Vec<QuestProgress>,
}

impl Player {
    pub fn durable_state(&self) -> DurablePlayerState {
        DurablePlayerState {
            name: self.name.clone(),
            role: self.role,
            position: self.position,
            gold: self.gold,
            inventory: self.inventory.clone(),
            quests: self.quests.clone(),
        }
    }
}

/// State for one quest accepted by a player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestProgress {
    pub quest_id: QuestId,
    pub progress: u32,
    pub required_count: u32,
    pub status: QuestStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestStatus {
    Accepted,
    Completed,
    Rewarded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestOffer {
    pub quest_id: QuestId,
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VendorListing {
    pub item_id: ItemId,
    pub name: &'static str,
    pub unit_price: u32,
    pub remaining_quantity: u32,
    pub max_stack: u32,
}

/// Commands accepted by the authoritative world owner.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    JoinPlayer {
        name: String,
        role: Role,
    },
    RestorePlayer {
        state: DurablePlayerState,
    },
    LeavePlayer {
        player_id: EntityId,
    },
    Move {
        player_id: EntityId,
        dx: f32,
        dy: f32,
    },
    SelectTarget {
        player_id: EntityId,
        target_id: EntityId,
    },
    BasicAttack {
        player_id: EntityId,
    },
    Heal {
        player_id: EntityId,
        target_id: EntityId,
    },
    ListVendor {
        player_id: EntityId,
        vendor_id: EntityId,
    },
    BuyItem {
        player_id: EntityId,
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    },
    LootEnemy {
        player_id: EntityId,
        enemy_id: EntityId,
    },
    ListQuestOffers {
        player_id: EntityId,
        npc_id: EntityId,
    },
    AcceptQuest {
        player_id: EntityId,
        npc_id: EntityId,
        quest_id: QuestId,
    },
    TurnInQuest {
        player_id: EntityId,
        npc_id: EntityId,
        quest_id: QuestId,
    },
}

/// Authoritative results emitted by [`World::step`]. A network layer can map
/// these events to a versioned wire protocol later.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    PlayerJoined {
        player: PlayerSnapshot,
    },
    PlayerLeft {
        player_id: EntityId,
    },
    PlayerMoved {
        player_id: EntityId,
        position: Position,
        area: ZoneArea,
    },
    TargetSelected {
        player_id: EntityId,
        target_id: EntityId,
    },
    AttackResolved {
        player_id: EntityId,
        target_id: EntityId,
        damage: u32,
        target_health: u32,
    },
    HealResolved {
        player_id: EntityId,
        target_id: EntityId,
        amount: u32,
        target_health: u32,
    },
    EnemyDefeated {
        enemy_id: EntityId,
    },
    VendorListed {
        player_id: EntityId,
        vendor_id: EntityId,
        listings: Vec<VendorListing>,
    },
    ItemPurchased {
        player_id: EntityId,
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
        total_price: u32,
        gold_remaining: u32,
    },
    LootRewarded {
        player_id: EntityId,
        enemy_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    },
    TransactionRejected {
        player_id: EntityId,
        reason: String,
    },
    QuestOffersListed {
        player_id: EntityId,
        npc_id: EntityId,
        quests: Vec<QuestOffer>,
    },
    QuestAccepted {
        player_id: EntityId,
        npc_id: EntityId,
        quest_id: QuestId,
    },
    QuestProgressed {
        player_id: EntityId,
        quest_id: QuestId,
        progress: u32,
        required_count: u32,
    },
    QuestCompleted {
        player_id: EntityId,
        quest_id: QuestId,
    },
    QuestRewarded {
        player_id: EntityId,
        quest_id: QuestId,
        gold: u32,
        item_id: Option<ItemId>,
        item_quantity: u32,
        gold_remaining: u32,
    },
    QuestRejected {
        player_id: EntityId,
        reason: String,
    },
    CommandRejected {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldSummary {
    pub tick: u64,
    pub player_count: usize,
    pub npc_count: usize,
    pub enemy_count: usize,
    pub vendor_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VendorStock {
    unit_price: u32,
    remaining_quantity: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EnemyReward {
    owner: Option<EntityId>,
    claimed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingAttack {
    target_id: EntityId,
    damage: u32,
    resolve_tick: u64,
}

/// Single-owner authoritative starter-zone simulation.
pub struct World {
    tick: u64,
    next_entity_id: u64,
    bounds: Bounds,
    players: BTreeMap<EntityId, Player>,
    npcs: BTreeMap<EntityId, Npc>,
    vendor_stock: BTreeMap<(EntityId, ItemId), VendorStock>,
    enemy_rewards: BTreeMap<EntityId, EnemyReward>,
    combat_cooldowns: BTreeMap<EntityId, u64>,
    pending_attacks: BTreeMap<EntityId, PendingAttack>,
}

impl World {
    /// Creates the initial town-and-field vertical slice.
    pub fn new_starter_zone() -> Self {
        let mut world = Self {
            tick: 0,
            next_entity_id: 1,
            bounds: Bounds::starter_zone(),
            players: BTreeMap::new(),
            npcs: BTreeMap::new(),
            vendor_stock: BTreeMap::new(),
            enemy_rewards: BTreeMap::new(),
            combat_cooldowns: BTreeMap::new(),
            pending_attacks: BTreeMap::new(),
        };

        let vendor_id = world.spawn_npc(
            NpcTemplateId::MIRA_THE_MERCHANT,
            "Mira the Merchant",
            NpcKind::Vendor,
            Position::new(0.0, 0.0),
            1,
        );
        for listing in starter_catalog().vendor_listings {
            world.vendor_stock.insert(
                (vendor_id, listing.item_id),
                VendorStock {
                    unit_price: listing.unit_price,
                    remaining_quantity: listing.initial_quantity,
                },
            );
        }
        world.spawn_npc(
            NpcTemplateId::FIELD_WOLF,
            "Field Wolf",
            NpcKind::Enemy,
            Position::new(24.0, 0.0),
            100,
        );
        world.spawn_npc(
            NpcTemplateId::FIELD_WOLF,
            "Field Wolf",
            NpcKind::Enemy,
            Position::new(30.0, 6.0),
            100,
        );
        world.spawn_npc(
            NpcTemplateId::FIELD_WOLF,
            "Field Wolf",
            NpcKind::Enemy,
            Position::new(30.0, -6.0),
            100,
        );
        world
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn player(&self, player_id: EntityId) -> Option<&Player> {
        self.players.get(&player_id)
    }

    pub fn npc(&self, npc_id: EntityId) -> Option<&Npc> {
        self.npcs.get(&npc_id)
    }

    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.values()
    }

    pub fn npcs(&self) -> impl Iterator<Item = &Npc> {
        self.npcs.values()
    }

    pub fn summary(&self) -> WorldSummary {
        let enemy_count = self
            .npcs
            .values()
            .filter(|npc| npc.kind == NpcKind::Enemy && npc.health > 0)
            .count();
        let vendor_count = self
            .npcs
            .values()
            .filter(|npc| npc.kind == NpcKind::Vendor)
            .count();
        WorldSummary {
            tick: self.tick,
            player_count: self.players.len(),
            npc_count: self.npcs.len(),
            enemy_count,
            vendor_count,
        }
    }

    /// Applies all commands for one simulation tick and returns authoritative
    /// events. Commands are processed in the supplied order. This is the
    /// compatibility entry point; it uses the same fixed-tick machinery as
    /// [`World::step_with_combat_timing`] with zero cast/cooldown defaults.
    pub fn step<I>(&mut self, commands: I) -> Vec<Event>
    where
        I: IntoIterator<Item = Command>,
    {
        self.step_with_combat_timing(
            commands,
            CombatTiming::new(20, 0, 0).expect("constant compatibility timing is valid"),
        )
    }

    /// Applies commands using server-owned fixed-tick combat timing.
    ///
    /// [`Command::BasicAttack`] validates its cooldown and cast state at the
    /// current server tick, then resolves after `cast_time_ticks`. Empty
    /// command batches still advance the tick and resolve due work.
    pub fn step_with_combat_timing<I>(&mut self, commands: I, timing: CombatTiming) -> Vec<Event>
    where
        I: IntoIterator<Item = Command>,
    {
        let mut events = Vec::new();
        for command in commands {
            match command {
                Command::BasicAttack { player_id } => {
                    self.apply_timed_basic_attack(player_id, timing, &mut events);
                }
                command => self.apply(command, &mut events),
            }
        }
        self.tick = self.tick.saturating_add(1);
        self.resolve_pending_attacks(&mut events);
        events
    }

    fn apply_timed_basic_attack(
        &mut self,
        player_id: EntityId,
        timing: CombatTiming,
        events: &mut Vec<Event>,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            Self::reject(events, format!("unknown player {player_id}"));
            return;
        };
        if self.pending_attacks.contains_key(&player_id) {
            Self::reject(events, "combat cast is already in progress");
            return;
        }
        if let Some(&ready_tick) = self.combat_cooldowns.get(&player_id)
            && self.tick < ready_tick
        {
            Self::reject(
                events,
                format!("combat cooldown is not ready until tick {ready_tick}"),
            );
            return;
        }
        let Some(target_id) = player.target else {
            Self::reject(events, "player has no target");
            return;
        };
        let player_position = player.position;
        let damage = match player.role {
            Role::Tank => 8,
            Role::Healer => 4,
            Role::DamageDealer => 12,
        };
        let Some(target) = self.npcs.get(&target_id) else {
            Self::reject(events, "target no longer exists");
            return;
        };
        if target.kind != NpcKind::Enemy || target.health == 0 {
            Self::reject(events, "target is not a living enemy");
            return;
        }
        if player_position.distance_squared(target.position) > ATTACK_RANGE * ATTACK_RANGE {
            Self::reject(events, "target is out of attack range");
            return;
        }

        let resolve_tick = self.tick.saturating_add(timing.cast_time_ticks);
        let ready_tick = resolve_tick.saturating_add(timing.cooldown_ticks);
        self.combat_cooldowns.insert(player_id, ready_tick);
        if timing.cast_time_ticks == 0 {
            self.resolve_attack(player_id, target_id, damage, events);
        } else {
            self.pending_attacks.insert(
                player_id,
                PendingAttack {
                    target_id,
                    damage,
                    resolve_tick,
                },
            );
        }
    }

    fn resolve_pending_attacks(&mut self, events: &mut Vec<Event>) {
        let due_attacks: Vec<_> = self
            .pending_attacks
            .iter()
            .filter_map(|(&player_id, attack)| {
                (attack.resolve_tick <= self.tick).then_some((player_id, *attack))
            })
            .collect();
        for (player_id, attack) in due_attacks {
            self.pending_attacks.remove(&player_id);
            self.resolve_attack(player_id, attack.target_id, attack.damage, events);
        }
    }

    fn resolve_attack(
        &mut self,
        player_id: EntityId,
        target_id: EntityId,
        damage: u32,
        events: &mut Vec<Event>,
    ) {
        if !self.players.contains_key(&player_id) {
            Self::reject(events, format!("unknown player {player_id}"));
            return;
        }
        let (target_template_id, target_health, defeated) = {
            let Some(target) = self.npcs.get_mut(&target_id) else {
                Self::reject(events, "target no longer exists");
                return;
            };
            if target.kind != NpcKind::Enemy || target.health == 0 {
                Self::reject(events, "target is not a living enemy");
                return;
            }
            target.health = target.health.saturating_sub(damage);
            (target.template_id, target.health, target.health == 0)
        };
        events.push(Event::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        });
        if defeated {
            events.push(Event::EnemyDefeated {
                enemy_id: target_id,
            });
        }
        if let Some(reward) = self.enemy_rewards.get_mut(&target_id)
            && reward.owner.is_none()
        {
            reward.owner = Some(player_id);
        }
        if defeated {
            self.advance_kill_quests(player_id, target_template_id, events);
        }
    }

    fn spawn_npc(
        &mut self,
        template_id: NpcTemplateId,
        name: &str,
        kind: NpcKind,
        position: Position,
        health: u32,
    ) -> EntityId {
        let id = self.allocate_id();
        self.npcs.insert(
            id,
            Npc {
                id,
                template_id,
                name: name.to_owned(),
                kind,
                position,
                health,
                max_health: health,
            },
        );
        if kind == NpcKind::Enemy {
            self.enemy_rewards.insert(
                id,
                EnemyReward {
                    owner: None,
                    claimed: false,
                },
            );
        }
        id
    }

    fn allocate_id(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.saturating_add(1);
        id
    }

    fn restore_player(&mut self, state: DurablePlayerState, events: &mut Vec<Event>) {
        let name = state.name.trim();
        if name.is_empty() || name.len() > 24 {
            Self::reject(events, "restored player name is invalid");
            return;
        }
        if !state.position.x.is_finite()
            || !state.position.y.is_finite()
            || state.position.x < self.bounds.min_x
            || state.position.x > self.bounds.max_x
            || state.position.y < self.bounds.min_y
            || state.position.y > self.bounds.max_y
        {
            Self::reject(events, "restored player position is invalid");
            return;
        }
        if state.inventory.capacity > STARTER_INVENTORY_CAPACITY
            || state.inventory.stacks.len() > state.inventory.capacity
        {
            Self::reject(events, "restored inventory capacity is invalid");
            return;
        }
        for stack in &state.inventory.stacks {
            let Some(definition) = item_definition(stack.item_id) else {
                Self::reject(events, "restored inventory item is unknown");
                return;
            };
            if stack.quantity == 0 || stack.quantity > definition.max_stack {
                Self::reject(events, "restored inventory stack is invalid");
                return;
            }
        }
        for (index, quest) in state.quests.iter().enumerate() {
            if state.quests[..index]
                .iter()
                .any(|previous| previous.quest_id == quest.quest_id)
            {
                Self::reject(events, "restored quest is duplicated");
                return;
            }
            let Some(definition) = starter_catalog()
                .quests
                .iter()
                .find(|item| item.id == quest.quest_id)
            else {
                Self::reject(events, "restored quest is unknown");
                return;
            };
            let Some(ObjectiveDefinition::KillNpc { required_count, .. }) =
                definition.objectives.first()
            else {
                Self::reject(events, "restored quest objective is invalid");
                return;
            };
            if quest.required_count == 0
                || quest.progress > quest.required_count
                || quest.required_count != *required_count
            {
                Self::reject(events, "restored quest state is invalid");
                return;
            }
        }
        let id = self.allocate_id();
        let player = Player {
            id,
            name: name.to_owned(),
            role: state.role,
            position: state.position,
            health: 100,
            max_health: 100,
            target: None,
            gold: state.gold,
            inventory: state.inventory,
            quests: state.quests,
        };
        events.push(Event::PlayerJoined {
            player: player.snapshot(),
        });
        self.players.insert(id, player);
    }

    fn apply(&mut self, command: Command, events: &mut Vec<Event>) {
        match command {
            Command::JoinPlayer { name, role } => {
                let name = name.trim();
                if name.is_empty() {
                    Self::reject(events, "player name cannot be empty");
                    return;
                }
                if name.len() > 24 {
                    Self::reject(events, "player name cannot exceed 24 characters");
                    return;
                }
                let id = self.allocate_id();
                let player = Player {
                    id,
                    name: name.to_owned(),
                    role,
                    position: Position::new(0.0, 0.0),
                    health: 100,
                    max_health: 100,
                    target: None,
                    gold: STARTER_GOLD,
                    inventory: Inventory::new(STARTER_INVENTORY_CAPACITY),
                    quests: Vec::new(),
                };
                events.push(Event::PlayerJoined {
                    player: player.snapshot(),
                });
                self.players.insert(id, player);
            }
            Command::RestorePlayer { state } => self.restore_player(state, events),
            Command::LeavePlayer { player_id } => {
                if self.players.remove(&player_id).is_some() {
                    self.combat_cooldowns.remove(&player_id);
                    self.pending_attacks.remove(&player_id);
                    events.push(Event::PlayerLeft { player_id });
                } else {
                    Self::reject(events, format!("unknown player {player_id}"));
                }
            }
            Command::Move { player_id, dx, dy } => {
                if !dx.is_finite() || !dy.is_finite() {
                    Self::reject(events, "movement must be finite");
                    return;
                }
                if dx.hypot(dy) > MAX_MOVE_PER_COMMAND {
                    Self::reject(
                        events,
                        format!("movement exceeds {} units", MAX_MOVE_PER_COMMAND),
                    );
                    return;
                }
                let Some(player) = self.players.get_mut(&player_id) else {
                    Self::reject(events, format!("unknown player {player_id}"));
                    return;
                };
                player.position = player.position.translated(dx, dy, self.bounds);
                events.push(Event::PlayerMoved {
                    player_id,
                    position: player.position,
                    area: ZoneArea::from_position(player.position),
                });
            }
            Command::SelectTarget {
                player_id,
                target_id,
            } => {
                if !self.players.contains_key(&player_id) {
                    Self::reject(events, format!("unknown player {player_id}"));
                    return;
                }
                let Some(target) = self.npcs.get(&target_id) else {
                    Self::reject(events, format!("unknown target {target_id}"));
                    return;
                };
                if target.kind != NpcKind::Enemy || target.health == 0 {
                    Self::reject(events, "target is not a living enemy");
                    return;
                }
                self.players
                    .get_mut(&player_id)
                    .expect("player was checked above")
                    .target = Some(target_id);
                events.push(Event::TargetSelected {
                    player_id,
                    target_id,
                });
            }
            Command::BasicAttack { player_id } => {
                let Some(player) = self.players.get(&player_id) else {
                    Self::reject(events, format!("unknown player {player_id}"));
                    return;
                };
                let Some(target_id) = player.target else {
                    Self::reject(events, "player has no target");
                    return;
                };
                let player_position = player.position;
                let damage = match player.role {
                    Role::Tank => 8,
                    Role::Healer => 4,
                    Role::DamageDealer => 12,
                };
                let (target_template_id, target_health, defeated) = {
                    let Some(target) = self.npcs.get_mut(&target_id) else {
                        Self::reject(events, "target no longer exists");
                        return;
                    };
                    if target.kind != NpcKind::Enemy || target.health == 0 {
                        Self::reject(events, "target is not a living enemy");
                        return;
                    }
                    if player_position.distance_squared(target.position)
                        > ATTACK_RANGE * ATTACK_RANGE
                    {
                        Self::reject(events, "target is out of attack range");
                        return;
                    }
                    target.health = target.health.saturating_sub(damage);
                    (target.template_id, target.health, target.health == 0)
                };
                events.push(Event::AttackResolved {
                    player_id,
                    target_id,
                    damage,
                    target_health,
                });
                if defeated {
                    events.push(Event::EnemyDefeated {
                        enemy_id: target_id,
                    });
                }
                if let Some(reward) = self.enemy_rewards.get_mut(&target_id) {
                    if reward.owner.is_none() {
                        reward.owner = Some(player_id);
                    }
                }
                if defeated {
                    self.advance_kill_quests(player_id, target_template_id, events);
                }
            }
            Command::Heal {
                player_id,
                target_id,
            } => {
                let Some(healer) = self.players.get(&player_id) else {
                    Self::reject(events, format!("unknown player {player_id}"));
                    return;
                };
                if healer.role != Role::Healer {
                    Self::reject(events, "only healers can use heal");
                    return;
                }
                if healer.health == 0 {
                    Self::reject(events, "dead players cannot heal");
                    return;
                }
                let healer_position = healer.position;
                let Some(target) = self.players.get(&target_id) else {
                    Self::reject(events, "heal target is not a player");
                    return;
                };
                if target.health == 0 {
                    Self::reject(events, "cannot heal a defeated player");
                    return;
                }
                if healer_position.distance_squared(target.position) > HEAL_RANGE * HEAL_RANGE {
                    Self::reject(events, "heal target is out of range");
                    return;
                }
                if target.health >= target.max_health {
                    Self::reject(events, "heal target is already at full health");
                    return;
                }
                let target_health = {
                    let target = self
                        .players
                        .get_mut(&target_id)
                        .expect("heal target was checked above");
                    let before = target.health;
                    target.health = target
                        .health
                        .saturating_add(HEAL_AMOUNT)
                        .min(target.max_health);
                    target.health - before
                };
                events.push(Event::HealResolved {
                    player_id,
                    target_id,
                    amount: target_health,
                    target_health: self.players[&target_id].health,
                });
            }
            Command::ListVendor {
                player_id,
                vendor_id,
            } => {
                if !self.can_use_vendor(player_id, vendor_id, events) {
                    return;
                }
                let listings = self
                    .vendor_stock
                    .iter()
                    .filter_map(|((stock_vendor_id, item_id), stock)| {
                        if *stock_vendor_id != vendor_id || stock.remaining_quantity == 0 {
                            return None;
                        }
                        let definition = item_definition(*item_id)?;
                        Some(VendorListing {
                            item_id: *item_id,
                            name: definition.name,
                            unit_price: stock.unit_price,
                            remaining_quantity: stock.remaining_quantity,
                            max_stack: definition.max_stack,
                        })
                    })
                    .collect();
                events.push(Event::VendorListed {
                    player_id,
                    vendor_id,
                    listings,
                });
            }
            Command::BuyItem {
                player_id,
                vendor_id,
                item_id,
                quantity,
            } => {
                if !self.can_use_vendor(player_id, vendor_id, events) {
                    return;
                }
                let Some(definition) = item_definition(item_id) else {
                    Self::reject_transaction(events, player_id, format!("unknown item {item_id}"));
                    return;
                };
                if quantity == 0 {
                    Self::reject_transaction(
                        events,
                        player_id,
                        "quantity must be greater than zero",
                    );
                    return;
                }
                let Some(stock) = self.vendor_stock.get(&(vendor_id, item_id)).copied() else {
                    Self::reject_transaction(
                        events,
                        player_id,
                        format!("vendor does not sell item {item_id}"),
                    );
                    return;
                };
                if quantity > stock.remaining_quantity {
                    Self::reject_transaction(
                        events,
                        player_id,
                        "vendor does not have enough stock",
                    );
                    return;
                }
                let Some(total_price) = stock.unit_price.checked_mul(quantity) else {
                    Self::reject_transaction(events, player_id, "purchase price is too large");
                    return;
                };
                let Some(player) = self.players.get(&player_id) else {
                    Self::reject_transaction(
                        events,
                        player_id,
                        format!("unknown player {player_id}"),
                    );
                    return;
                };
                if player.gold < total_price {
                    Self::reject_transaction(events, player_id, "player cannot afford purchase");
                    return;
                }
                if !player.inventory.can_add(definition, quantity) {
                    Self::reject_transaction(
                        events,
                        player_id,
                        "inventory has insufficient capacity",
                    );
                    return;
                }

                let player = self
                    .players
                    .get_mut(&player_id)
                    .expect("player was checked above");
                player.gold -= total_price;
                player.inventory.add(definition, quantity);
                self.vendor_stock
                    .get_mut(&(vendor_id, item_id))
                    .expect("stock was checked above")
                    .remaining_quantity -= quantity;
                events.push(Event::ItemPurchased {
                    player_id,
                    vendor_id,
                    item_id,
                    quantity,
                    total_price,
                    gold_remaining: player.gold,
                });
            }
            Command::LootEnemy {
                player_id,
                enemy_id,
            } => {
                let Some(player) = self.players.get(&player_id) else {
                    Self::reject_transaction(
                        events,
                        player_id,
                        format!("unknown player {player_id}"),
                    );
                    return;
                };
                let Some(enemy) = self.npcs.get(&enemy_id) else {
                    Self::reject_transaction(
                        events,
                        player_id,
                        format!("unknown enemy {enemy_id}"),
                    );
                    return;
                };
                if enemy.kind != NpcKind::Enemy {
                    Self::reject_transaction(events, player_id, "entity is not an enemy");
                    return;
                }
                if enemy.health > 0 {
                    Self::reject_transaction(events, player_id, "enemy is not defeated");
                    return;
                }
                let Some(reward) = self.enemy_rewards.get(&enemy_id).copied() else {
                    Self::reject_transaction(events, player_id, "enemy has no reward");
                    return;
                };
                if reward.owner != Some(player_id) {
                    Self::reject_transaction(events, player_id, "player does not own enemy reward");
                    return;
                }
                if reward.claimed {
                    Self::reject_transaction(events, player_id, "enemy reward was already claimed");
                    return;
                }
                let definition = item_definition(ItemId::FIELD_WOLF_PELT)
                    .expect("starter loot item must have a definition");
                if !player.inventory.can_add(definition, 1) {
                    Self::reject_transaction(
                        events,
                        player_id,
                        "inventory has insufficient capacity",
                    );
                    return;
                }
                self.players
                    .get_mut(&player_id)
                    .expect("player was checked above")
                    .inventory
                    .add(definition, 1);
                self.enemy_rewards
                    .get_mut(&enemy_id)
                    .expect("reward was checked above")
                    .claimed = true;
                events.push(Event::LootRewarded {
                    player_id,
                    enemy_id,
                    item_id: ItemId::FIELD_WOLF_PELT,
                    quantity: 1,
                });
            }
            Command::ListQuestOffers { player_id, npc_id } => {
                let Some(npc_template_id) = self.quest_giver_template(player_id, npc_id, events)
                else {
                    return;
                };
                let quests = starter_catalog()
                    .quests
                    .iter()
                    .filter(|quest| quest.giver == npc_template_id)
                    .map(|quest| QuestOffer {
                        quest_id: quest.id,
                        name: quest.name,
                        description: quest.description,
                    })
                    .collect();
                events.push(Event::QuestOffersListed {
                    player_id,
                    npc_id,
                    quests,
                });
            }
            Command::AcceptQuest {
                player_id,
                npc_id,
                quest_id,
            } => {
                let Some(npc_template_id) = self.quest_giver_template(player_id, npc_id, events)
                else {
                    return;
                };
                let Some(quest) = starter_catalog()
                    .quests
                    .iter()
                    .find(|quest| quest.id == quest_id && quest.giver == npc_template_id)
                else {
                    Self::reject_quest(events, player_id, "quest is not offered by this NPC");
                    return;
                };
                let Some(ObjectiveDefinition::KillNpc {
                    npc_template_id: objective_template_id,
                    required_count,
                }) = quest.objectives.first()
                else {
                    Self::reject_quest(events, player_id, "quest has no supported objective");
                    return;
                };
                if *objective_template_id == NpcTemplateId(0) || *required_count == 0 {
                    Self::reject_quest(events, player_id, "quest has invalid objective data");
                    return;
                }
                let Some(player) = self.players.get_mut(&player_id) else {
                    Self::reject_quest(events, player_id, format!("unknown player {player_id}"));
                    return;
                };
                if player.quests.iter().any(|state| state.quest_id == quest_id) {
                    Self::reject_quest(events, player_id, "quest was already accepted");
                    return;
                }
                player.quests.push(QuestProgress {
                    quest_id,
                    progress: 0,
                    required_count: *required_count,
                    status: QuestStatus::Accepted,
                });
                events.push(Event::QuestAccepted {
                    player_id,
                    npc_id,
                    quest_id,
                });
            }
            Command::TurnInQuest {
                player_id,
                npc_id,
                quest_id,
            } => {
                let Some(npc_template_id) = self.quest_giver_template(player_id, npc_id, events)
                else {
                    return;
                };
                let Some(quest) = starter_catalog()
                    .quests
                    .iter()
                    .find(|quest| quest.id == quest_id && quest.giver == npc_template_id)
                else {
                    Self::reject_quest(events, player_id, "quest is not turned in to this NPC");
                    return;
                };
                let reward = quest.reward;
                let Some(player) = self.players.get(&player_id) else {
                    Self::reject_quest(events, player_id, format!("unknown player {player_id}"));
                    return;
                };
                let Some(state) = player
                    .quests
                    .iter()
                    .find(|state| state.quest_id == quest_id)
                else {
                    Self::reject_quest(events, player_id, "quest was not accepted");
                    return;
                };
                if state.status != QuestStatus::Completed {
                    let reason = match state.status {
                        QuestStatus::Accepted => "quest objectives are not complete",
                        QuestStatus::Completed => unreachable!(),
                        QuestStatus::Rewarded => "quest reward was already claimed",
                    };
                    Self::reject_quest(events, player_id, reason);
                    return;
                }
                let item_definition = reward.item_id.and_then(item_definition);
                if reward.item_id.is_some() && item_definition.is_none() {
                    Self::reject_quest(events, player_id, "quest reward item is unknown");
                    return;
                }
                if let Some(definition) = item_definition
                    && !player.inventory.can_add(definition, reward.item_quantity)
                {
                    Self::reject_quest(events, player_id, "inventory has insufficient capacity");
                    return;
                }
                let Some(gold_remaining) = player.gold.checked_add(reward.gold) else {
                    Self::reject_quest(events, player_id, "quest reward gold is too large");
                    return;
                };
                let player = self
                    .players
                    .get_mut(&player_id)
                    .expect("player was checked above");
                player.gold = gold_remaining;
                if let Some(definition) = item_definition {
                    player.inventory.add(definition, reward.item_quantity);
                }
                player
                    .quests
                    .iter_mut()
                    .find(|state| state.quest_id == quest_id)
                    .expect("quest state was checked above")
                    .status = QuestStatus::Rewarded;
                events.push(Event::QuestRewarded {
                    player_id,
                    quest_id,
                    gold: reward.gold,
                    item_id: reward.item_id,
                    item_quantity: reward.item_quantity,
                    gold_remaining,
                });
            }
        }
    }

    fn advance_kill_quests(
        &mut self,
        player_id: EntityId,
        defeated_template_id: NpcTemplateId,
        events: &mut Vec<Event>,
    ) {
        let updates: Vec<_> = self
            .players
            .get(&player_id)
            .into_iter()
            .flat_map(|player| player.quests.iter())
            .filter_map(|state| {
                if state.status != QuestStatus::Accepted {
                    return None;
                }
                let quest = starter_catalog()
                    .quests
                    .iter()
                    .find(|quest| quest.id == state.quest_id)?;
                let Some(ObjectiveDefinition::KillNpc {
                    npc_template_id,
                    required_count,
                }) = quest.objectives.first()
                else {
                    return None;
                };
                (*npc_template_id == defeated_template_id).then_some((
                    state.quest_id,
                    state.progress,
                    *required_count,
                ))
            })
            .collect();

        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for (quest_id, old_progress, required_count) in updates {
            let Some(state) = player
                .quests
                .iter_mut()
                .find(|state| state.quest_id == quest_id)
            else {
                continue;
            };
            state.progress = old_progress.saturating_add(1).min(required_count);
            events.push(Event::QuestProgressed {
                player_id,
                quest_id,
                progress: state.progress,
                required_count,
            });
            if state.progress == required_count {
                state.status = QuestStatus::Completed;
                events.push(Event::QuestCompleted {
                    player_id,
                    quest_id,
                });
            }
        }
    }

    fn quest_giver_template(
        &self,
        player_id: EntityId,
        npc_id: EntityId,
        events: &mut Vec<Event>,
    ) -> Option<NpcTemplateId> {
        let Some(player) = self.players.get(&player_id) else {
            Self::reject_quest(events, player_id, format!("unknown player {player_id}"));
            return None;
        };
        let Some(npc) = self.npcs.get(&npc_id) else {
            Self::reject_quest(events, player_id, format!("unknown NPC {npc_id}"));
            return None;
        };
        if ZoneArea::from_position(player.position) != ZoneArea::Town {
            Self::reject_quest(events, player_id, "quest giver is only available in town");
            return None;
        }
        if player.position.distance_squared(npc.position)
            > VENDOR_INTERACTION_RANGE * VENDOR_INTERACTION_RANGE
        {
            Self::reject_quest(events, player_id, "player is too far from quest giver");
            return None;
        }
        Some(npc.template_id)
    }

    fn can_use_vendor(
        &self,
        player_id: EntityId,
        vendor_id: EntityId,
        events: &mut Vec<Event>,
    ) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            Self::reject_transaction(events, player_id, format!("unknown player {player_id}"));
            return false;
        };
        let Some(vendor) = self.npcs.get(&vendor_id) else {
            Self::reject_transaction(events, player_id, format!("unknown vendor {vendor_id}"));
            return false;
        };
        if vendor.kind != NpcKind::Vendor {
            Self::reject_transaction(events, player_id, "entity is not a vendor");
            return false;
        }
        if ZoneArea::from_position(player.position) != ZoneArea::Town {
            Self::reject_transaction(events, player_id, "vendor is only available in town");
            return false;
        }
        if player.position.distance_squared(vendor.position)
            > VENDOR_INTERACTION_RANGE * VENDOR_INTERACTION_RANGE
        {
            Self::reject_transaction(events, player_id, "player is too far from vendor");
            return false;
        }
        true
    }

    fn reject(events: &mut Vec<Event>, reason: impl Into<String>) {
        events.push(Event::CommandRejected {
            reason: reason.into(),
        });
    }

    fn reject_transaction(events: &mut Vec<Event>, player_id: EntityId, reason: impl Into<String>) {
        events.push(Event::TransactionRejected {
            player_id,
            reason: reason.into(),
        });
    }

    fn reject_quest(events: &mut Vec<Event>, player_id: EntityId, reason: impl Into<String>) {
        events.push(Event::QuestRejected {
            player_id,
            reason: reason.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join(world: &mut World, name: &str, role: Role) -> EntityId {
        let events = world.step([Command::JoinPlayer {
            name: name.to_owned(),
            role,
        }]);
        let Event::PlayerJoined { player } = &events[0] else {
            panic!("expected player join event: {events:?}");
        };
        player.id
    }

    fn first_enemy(world: &World) -> EntityId {
        world
            .npcs()
            .find(|npc| npc.kind == NpcKind::Enemy)
            .expect("starter zone should contain an enemy")
            .id
    }

    fn vendor(world: &World) -> EntityId {
        world
            .npcs()
            .find(|npc| npc.kind == NpcKind::Vendor)
            .expect("starter zone should contain a vendor")
            .id
    }

    fn defeat_enemy(world: &mut World, player_id: EntityId, enemy_id: EntityId) {
        world.step([
            Command::Move {
                player_id,
                dx: 10.0,
                dy: 0.0,
            },
            Command::Move {
                player_id,
                dx: 10.0,
                dy: 0.0,
            },
            Command::SelectTarget {
                player_id,
                target_id: enemy_id,
            },
        ]);
        for _ in 0..9 {
            world.step([Command::BasicAttack { player_id }]);
        }
        assert_eq!(world.npc(enemy_id).unwrap().health, 0);
    }

    #[test]
    fn starter_zone_contains_town_vendor_and_field_enemies() {
        let world = World::new_starter_zone();
        let summary = world.summary();
        assert_eq!(summary.npc_count, 4);
        assert_eq!(summary.vendor_count, 1);
        assert_eq!(summary.enemy_count, 3);
        assert_eq!(
            ZoneArea::from_position(Position::new(0.0, 0.0)),
            ZoneArea::Town
        );
        assert_eq!(
            ZoneArea::from_position(Position::new(20.0, 0.0)),
            ZoneArea::Field
        );
    }

    #[test]
    fn restored_player_rehydrates_durable_state_and_resets_transient_state() {
        let mut source = World::new_starter_zone();
        source.step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }]);
        let player_id = source.players().next().expect("player joined").id;
        source.step([Command::Move {
            player_id,
            dx: 8.0,
            dy: 0.0,
        }]);
        let state = source
            .player(player_id)
            .expect("player exists")
            .durable_state();

        let mut restored = World::new_starter_zone();
        let events = restored.step([Command::RestorePlayer { state }]);
        let player = match events.as_slice() {
            [Event::PlayerJoined { player }] => player,
            other => panic!("expected restored join event, got {other:?}"),
        };
        assert_eq!(player.name, "Aria");
        assert_eq!(player.position, Position::new(8.0, 0.0));
        assert_eq!(player.health, 100);
        assert_eq!(player.target, None);
    }

    #[test]
    fn restored_player_rejects_invalid_checkpoint_position() {
        let mut world = World::new_starter_zone();
        let events = world.step([Command::RestorePlayer {
            state: DurablePlayerState {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
                position: Position::new(f32::NAN, 0.0),
                gold: 20,
                inventory: Inventory::new(16),
                quests: Vec::new(),
            },
        }]);
        assert!(matches!(events.as_slice(), [Event::CommandRejected { .. }]));
        assert_eq!(world.players().count(), 0);
    }

    #[test]
    fn restored_player_rejects_duplicate_quest_state() {
        let quest = QuestProgress {
            quest_id: QuestId::CLEAR_THE_FIELD,
            progress: 0,
            required_count: 3,
            status: QuestStatus::Accepted,
        };
        let mut world = World::new_starter_zone();
        let events = world.step([Command::RestorePlayer {
            state: DurablePlayerState {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
                position: Position::new(0.0, 0.0),
                gold: 20,
                inventory: Inventory::new(16),
                quests: vec![quest.clone(), quest],
            },
        }]);
        assert!(matches!(events.as_slice(), [Event::CommandRejected { .. }]));
        assert_eq!(world.players().count(), 0);
    }

    #[test]
    fn movement_is_bounded_and_emits_authoritative_position() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Mover", Role::Tank);

        let events = world.step([Command::Move {
            player_id,
            dx: 3.0,
            dy: 4.0,
        }]);
        assert!(matches!(
            events[0],
            Event::PlayerMoved {
                area: ZoneArea::Town,
                ..
            }
        ));
        assert_eq!(
            world.player(player_id).unwrap().position,
            Position::new(3.0, 4.0)
        );

        let events = world.step([Command::Move {
            player_id,
            dx: 11.0,
            dy: 0.0,
        }]);
        assert!(matches!(events[0], Event::CommandRejected { .. }));
        assert_eq!(
            world.player(player_id).unwrap().position,
            Position::new(3.0, 4.0)
        );
    }

    #[test]
    fn targeting_and_basic_attack_are_server_authoritative() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Fighter", Role::DamageDealer);
        let enemy_id = first_enemy(&world);

        let events = world.step([Command::SelectTarget {
            player_id,
            target_id: enemy_id,
        }]);
        assert_eq!(
            events,
            vec![Event::TargetSelected {
                player_id,
                target_id: enemy_id
            }]
        );

        let events = world.step([
            Command::Move {
                player_id,
                dx: 10.0,
                dy: 0.0,
            },
            Command::Move {
                player_id,
                dx: 10.0,
                dy: 0.0,
            },
            Command::BasicAttack { player_id },
        ]);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::AttackResolved { damage: 12, .. }))
        );
        assert_eq!(world.npc(enemy_id).unwrap().health, 88);
    }

    #[test]
    fn invalid_target_and_leave_are_handled_without_panicking() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Leaver", Role::Healer);
        let vendor_id = world
            .npcs()
            .find(|npc| npc.kind == NpcKind::Vendor)
            .unwrap()
            .id;

        let events = world.step([Command::SelectTarget {
            player_id,
            target_id: vendor_id,
        }]);
        assert!(matches!(events[0], Event::CommandRejected { .. }));

        let events = world.step([Command::LeavePlayer { player_id }]);
        assert_eq!(events, vec![Event::PlayerLeft { player_id }]);
        assert!(world.player(player_id).is_none());
    }

    #[test]
    fn roles_parse_for_the_development_protocol() {
        assert_eq!("tank".parse::<Role>().unwrap(), Role::Tank);
        assert_eq!("heal".parse::<Role>().unwrap(), Role::Healer);
        assert_eq!("dps".parse::<Role>().unwrap(), Role::DamageDealer);
        assert!("mage".parse::<Role>().is_err());
    }

    #[test]
    fn quest_progress_is_derived_from_enemy_defeats_and_rewarded_once() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Questing", Role::DamageDealer);
        let npc_id = vendor(&world);

        let events = world.step([Command::ListQuestOffers { player_id, npc_id }]);
        assert_eq!(
            events,
            vec![Event::QuestOffersListed {
                player_id,
                npc_id,
                quests: vec![QuestOffer {
                    quest_id: QuestId::CLEAR_THE_FIELD,
                    name: "Clear the Field",
                    description: "Thin the wolves threatening travelers outside town.",
                }],
            }]
        );

        let events = world.step([Command::AcceptQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        assert_eq!(
            events,
            vec![Event::QuestAccepted {
                player_id,
                npc_id,
                quest_id: QuestId::CLEAR_THE_FIELD,
            }]
        );

        let enemies: Vec<_> = world
            .npcs()
            .filter(|npc| npc.kind == NpcKind::Enemy)
            .map(|npc| npc.id)
            .collect();
        for enemy_id in enemies {
            defeat_enemy(&mut world, player_id, enemy_id);
        }
        assert_eq!(
            world.player(player_id).unwrap().quests,
            vec![QuestProgress {
                quest_id: QuestId::CLEAR_THE_FIELD,
                progress: 3,
                required_count: 3,
                status: QuestStatus::Completed,
            }]
        );

        for _ in 0..5 {
            world.step([Command::Move {
                player_id,
                dx: -10.0,
                dy: 0.0,
            }]);
        }
        let events = world.step([Command::TurnInQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        assert_eq!(
            events,
            vec![Event::QuestRewarded {
                player_id,
                quest_id: QuestId::CLEAR_THE_FIELD,
                gold: 10,
                item_id: Some(ItemId::TOWN_RATION),
                item_quantity: 5,
                gold_remaining: 30,
            }]
        );
        assert_eq!(world.player(player_id).unwrap().gold, 30);
        assert_eq!(
            world
                .player(player_id)
                .unwrap()
                .inventory
                .quantity(ItemId::TOWN_RATION),
            5
        );

        let events = world.step([Command::TurnInQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        assert_eq!(
            events,
            vec![Event::QuestRejected {
                player_id,
                reason: "quest reward was already claimed".to_owned(),
            }]
        );
    }

    #[test]
    fn incomplete_or_duplicate_quest_operations_are_rejected_without_mutation() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Questing", Role::Tank);
        let npc_id = vendor(&world);

        let events = world.step([Command::TurnInQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        assert!(matches!(events[0], Event::QuestRejected { .. }));

        world.step([Command::AcceptQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        let events = world.step([Command::AcceptQuest {
            player_id,
            npc_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);
        assert_eq!(
            events,
            vec![Event::QuestRejected {
                player_id,
                reason: "quest was already accepted".to_owned(),
            }]
        );
        assert_eq!(world.player(player_id).unwrap().gold, 20);
        assert!(
            world
                .player(player_id)
                .unwrap()
                .inventory
                .stacks()
                .next()
                .is_none()
        );
    }

    #[test]
    fn vendor_purchase_succeeds_and_merges_same_item_into_a_stack() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Buyer", Role::Tank);
        let vendor_id = vendor(&world);

        let events = world.step([Command::ListVendor {
            player_id,
            vendor_id,
        }]);
        let Event::VendorListed { listings, .. } = &events[0] else {
            panic!("expected vendor listing: {events:?}");
        };
        assert_eq!(listings.len(), 2);
        assert!(
            listings.iter().any(|listing| {
                listing.item_id == ItemId::TOWN_RATION && listing.unit_price == 2
            })
        );

        let events = world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::TOWN_RATION,
            quantity: 2,
        }]);
        assert_eq!(
            events,
            vec![Event::ItemPurchased {
                player_id,
                vendor_id,
                item_id: ItemId::TOWN_RATION,
                quantity: 2,
                total_price: 4,
                gold_remaining: 16,
            }]
        );

        world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::TOWN_RATION,
            quantity: 3,
        }]);
        let player = world.player(player_id).unwrap();
        assert_eq!(player.gold, 10);
        assert_eq!(player.inventory.quantity(ItemId::TOWN_RATION), 5);
        assert_eq!(player.inventory.used_slots(), 1);
        assert_eq!(
            world.vendor_stock[&(vendor_id, ItemId::TOWN_RATION)].remaining_quantity,
            95
        );
    }

    #[test]
    fn insufficient_gold_rejects_purchase_without_mutating_state() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Poor Buyer", Role::Healer);
        let vendor_id = vendor(&world);

        let events = world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::MINOR_HEALING_POTION,
            quantity: 5,
        }]);
        assert_eq!(
            events,
            vec![Event::TransactionRejected {
                player_id,
                reason: "player cannot afford purchase".to_owned(),
            }]
        );
        assert_eq!(world.player(player_id).unwrap().gold, STARTER_GOLD);
        assert_eq!(
            world
                .player(player_id)
                .unwrap()
                .inventory
                .quantity(ItemId::MINOR_HEALING_POTION),
            0
        );
        assert_eq!(
            world.vendor_stock[&(vendor_id, ItemId::MINOR_HEALING_POTION)].remaining_quantity,
            50
        );
    }

    #[test]
    fn invalid_vendor_and_item_are_rejected() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Validator", Role::DamageDealer);
        let enemy_id = first_enemy(&world);
        let vendor_id = vendor(&world);

        let events = world.step([Command::ListVendor {
            player_id,
            vendor_id: enemy_id,
        }]);
        assert!(matches!(
            events.as_slice(),
            [Event::TransactionRejected { player_id: rejected_id, .. }] if *rejected_id == player_id
        ));

        let events = world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId(999),
            quantity: 1,
        }]);
        assert_eq!(
            events,
            vec![Event::TransactionRejected {
                player_id,
                reason: "unknown item 999".to_owned(),
            }]
        );
    }

    #[test]
    fn inventory_capacity_rejects_purchase_without_spending_gold() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Full Pack", Role::Tank);
        let vendor_id = vendor(&world);
        let player = world.players.get_mut(&player_id).unwrap();
        for _ in 0..player.inventory.capacity() {
            player.inventory.stacks.push(ItemStack {
                item_id: ItemId::TOWN_RATION,
                quantity: 20,
            });
        }

        let events = world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::MINOR_HEALING_POTION,
            quantity: 1,
        }]);
        assert_eq!(
            events,
            vec![Event::TransactionRejected {
                player_id,
                reason: "inventory has insufficient capacity".to_owned(),
            }]
        );
        assert_eq!(world.player(player_id).unwrap().gold, STARTER_GOLD);
        assert_eq!(world.player(player_id).unwrap().inventory.used_slots(), 16);
    }

    #[test]
    fn defeated_enemy_rewards_are_owned_once_by_the_first_attacker() {
        let mut world = World::new_starter_zone();
        let owner_id = join(&mut world, "Hunter", Role::DamageDealer);
        let other_player_id = join(&mut world, "Observer", Role::Tank);
        let enemy_id = first_enemy(&world);
        defeat_enemy(&mut world, owner_id, enemy_id);

        let events = world.step([Command::LootEnemy {
            player_id: other_player_id,
            enemy_id,
        }]);
        assert_eq!(
            events,
            vec![Event::TransactionRejected {
                player_id: other_player_id,
                reason: "player does not own enemy reward".to_owned(),
            }]
        );

        let events = world.step([Command::LootEnemy {
            player_id: owner_id,
            enemy_id,
        }]);
        assert_eq!(
            events,
            vec![Event::LootRewarded {
                player_id: owner_id,
                enemy_id,
                item_id: ItemId::FIELD_WOLF_PELT,
                quantity: 1,
            }]
        );
        assert_eq!(
            world
                .player(owner_id)
                .unwrap()
                .inventory
                .quantity(ItemId::FIELD_WOLF_PELT),
            1
        );

        let events = world.step([Command::LootEnemy {
            player_id: owner_id,
            enemy_id,
        }]);
        assert_eq!(
            events,
            vec![Event::TransactionRejected {
                player_id: owner_id,
                reason: "enemy reward was already claimed".to_owned(),
            }]
        );
    }

    #[test]
    fn healer_restores_a_nearby_player_and_caps_at_max_health() {
        let mut world = World::new_starter_zone();
        let healer_id = join(&mut world, "Healer", Role::Healer);
        let target_id = join(&mut world, "Tank", Role::Tank);
        world.players.get_mut(&target_id).unwrap().health = 20;

        let events = world.step([Command::Heal {
            player_id: healer_id,
            target_id,
        }]);
        assert_eq!(
            events,
            vec![Event::HealResolved {
                player_id: healer_id,
                target_id,
                amount: 30,
                target_health: 50,
            }]
        );

        world.players.get_mut(&target_id).unwrap().health = 90;
        let events = world.step([Command::Heal {
            player_id: healer_id,
            target_id,
        }]);
        assert_eq!(
            events,
            vec![Event::HealResolved {
                player_id: healer_id,
                target_id,
                amount: 10,
                target_health: 100,
            }]
        );
    }

    #[test]
    fn non_healers_and_invalid_heal_targets_are_rejected_without_mutation() {
        let mut world = World::new_starter_zone();
        let tank_id = join(&mut world, "Tank", Role::Tank);
        let target_id = join(&mut world, "Target", Role::DamageDealer);
        world.players.get_mut(&target_id).unwrap().health = 40;

        assert_eq!(
            world.step([Command::Heal {
                player_id: tank_id,
                target_id,
            }]),
            vec![Event::CommandRejected {
                reason: "only healers can use heal".to_owned(),
            }]
        );
        assert_eq!(world.player(target_id).unwrap().health, 40);

        let healer_id = join(&mut world, "Healer", Role::Healer);
        assert_eq!(
            world.step([Command::Heal {
                player_id: healer_id,
                target_id: first_enemy(&world),
            }]),
            vec![Event::CommandRejected {
                reason: "heal target is not a player".to_owned(),
            }]
        );
    }

    #[test]
    fn out_of_range_or_full_health_heals_are_rejected() {
        let mut world = World::new_starter_zone();
        let healer_id = join(&mut world, "Healer", Role::Healer);
        let target_id = join(&mut world, "Target", Role::Tank);
        assert_eq!(
            world.step([Command::Heal {
                player_id: healer_id,
                target_id,
            }]),
            vec![Event::CommandRejected {
                reason: "heal target is already at full health".to_owned(),
            }]
        );

        world.players.get_mut(&target_id).unwrap().health = 40;
        for _ in 0..4 {
            world.step([Command::Move {
                player_id: target_id,
                dx: 10.0,
                dy: 0.0,
            }]);
        }
        assert_eq!(
            world.step([Command::Heal {
                player_id: healer_id,
                target_id,
            }]),
            vec![Event::CommandRejected {
                reason: "heal target is out of range".to_owned(),
            }]
        );
        assert_eq!(world.player(target_id).unwrap().health, 40);
    }

    #[test]
    fn timed_combat_resolves_after_fixed_tick_cast_time() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Caster", Role::DamageDealer);
        let enemy_id = first_enemy(&world);
        let timing = CombatTiming::new(20, 2, 0).unwrap();

        assert_eq!(timing.tick_hz(), 20);
        assert_eq!(world.tick(), 1);
        world.step_with_combat_timing(
            [
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::SelectTarget {
                    player_id,
                    target_id: enemy_id,
                },
            ],
            timing,
        );
        assert_eq!(world.tick(), 2);

        let events = world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        assert_eq!(world.tick(), 3);
        assert!(events.is_empty());
        assert_eq!(world.npc(enemy_id).unwrap().health, 100);

        let events = world.step_with_combat_timing([], timing);
        assert_eq!(world.tick(), 4);
        assert!(matches!(
            events.as_slice(),
            [Event::AttackResolved {
                player_id: resolved_player,
                target_id: resolved_target,
                damage: 12,
                target_health: 88,
            }] if *resolved_player == player_id && *resolved_target == enemy_id
        ));
        assert_eq!(world.npc(enemy_id).unwrap().health, 88);
    }

    #[test]
    fn empty_fixed_ticks_advance_and_resolve_deferred_combat() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Empty Tick", Role::DamageDealer);
        let enemy_id = first_enemy(&world);
        let timing = CombatTiming::new(20, 2, 0).unwrap();
        world.step_with_combat_timing(
            [Command::SelectTarget {
                player_id,
                target_id: enemy_id,
            }],
            timing,
        );
        world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        assert_eq!(world.npc(enemy_id).unwrap().health, 100);
        let events = world.step_with_combat_timing([], timing);
        assert!(matches!(
            events.as_slice(),
            [Event::AttackResolved {
                target_id,
                target_health: 88,
                ..
            }] if *target_id == enemy_id
        ));
        assert_eq!(world.tick(), 4);
    }

    #[test]
    fn timed_combat_cooldown_rejection_does_not_mutate_state() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Cooldown", Role::DamageDealer);
        let enemy_id = first_enemy(&world);
        let timing = CombatTiming::new(20, 0, 2).unwrap();
        world.step_with_combat_timing(
            [
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::SelectTarget {
                    player_id,
                    target_id: enemy_id,
                },
            ],
            timing,
        );
        world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        let health_before = world.npc(enemy_id).unwrap().health;
        let cooldown_before = world.combat_cooldowns[&player_id];

        let events = world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        assert_eq!(
            events,
            vec![Event::CommandRejected {
                reason: "combat cooldown is not ready until tick 4".to_owned(),
            }]
        );
        assert_eq!(world.npc(enemy_id).unwrap().health, health_before);
        assert_eq!(world.combat_cooldowns[&player_id], cooldown_before);
        assert!(world.pending_attacks.is_empty());
    }

    #[test]
    fn timed_combat_becomes_ready_at_the_server_tick_boundary() {
        let mut world = World::new_starter_zone();
        let player_id = join(&mut world, "Ready", Role::DamageDealer);
        let enemy_id = first_enemy(&world);
        let timing = CombatTiming::new(20, 0, 2).unwrap();
        world.step_with_combat_timing(
            [
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::Move {
                    player_id,
                    dx: 10.0,
                    dy: 0.0,
                },
                Command::SelectTarget {
                    player_id,
                    target_id: enemy_id,
                },
            ],
            timing,
        );
        world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        world.step_with_combat_timing([], timing);
        assert_eq!(world.tick(), 4);

        let events = world.step_with_combat_timing([Command::BasicAttack { player_id }], timing);
        assert!(matches!(
            events.as_slice(),
            [Event::AttackResolved {
                player_id: resolved_player,
                target_id: resolved_target,
                damage: 12,
                target_health: 76,
            }] if *resolved_player == player_id && *resolved_target == enemy_id
        ));
        assert_eq!(world.npc(enemy_id).unwrap().health, 76);
    }
}
