//! Authoritative, engine-independent MMORPG simulation primitives.
//!
//! This crate intentionally contains no sockets, database access, timers, or
//! rendering code. A region owner feeds commands into [`World::step`] and
//! consumes the resulting authoritative events.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

const MAX_MOVE_PER_COMMAND: f32 = 10.0;
const ATTACK_RANGE: f32 = 32.0;

/// Stable identifier for any live world entity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(pub u64);

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
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
}

/// Commands accepted by the authoritative world owner.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    JoinPlayer {
        name: String,
        role: Role,
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
    EnemyDefeated {
        enemy_id: EntityId,
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

/// Single-owner authoritative starter-zone simulation.
pub struct World {
    tick: u64,
    next_entity_id: u64,
    bounds: Bounds,
    players: BTreeMap<EntityId, Player>,
    npcs: BTreeMap<EntityId, Npc>,
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
        };

        world.spawn_npc(
            "Mira the Merchant",
            NpcKind::Vendor,
            Position::new(0.0, 0.0),
            1,
        );
        world.spawn_npc("Field Wolf", NpcKind::Enemy, Position::new(24.0, 0.0), 100);
        world.spawn_npc("Field Wolf", NpcKind::Enemy, Position::new(30.0, 6.0), 100);
        world.spawn_npc("Field Wolf", NpcKind::Enemy, Position::new(30.0, -6.0), 100);
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
    /// events. Commands are processed in the supplied order.
    pub fn step<I>(&mut self, commands: I) -> Vec<Event>
    where
        I: IntoIterator<Item = Command>,
    {
        let mut events = Vec::new();
        for command in commands {
            self.apply(command, &mut events);
        }
        self.tick = self.tick.saturating_add(1);
        events
    }

    fn spawn_npc(
        &mut self,
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
                name: name.to_owned(),
                kind,
                position,
                health,
                max_health: health,
            },
        );
        id
    }

    fn allocate_id(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id = self.next_entity_id.saturating_add(1);
        id
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
                };
                events.push(Event::PlayerJoined {
                    player: player.snapshot(),
                });
                self.players.insert(id, player);
            }
            Command::LeavePlayer { player_id } => {
                if self.players.remove(&player_id).is_some() {
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
                let Some(target) = self.npcs.get_mut(&target_id) else {
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
                target.health = target.health.saturating_sub(damage);
                events.push(Event::AttackResolved {
                    player_id,
                    target_id,
                    damage,
                    target_health: target.health,
                });
                if target.health == 0 {
                    events.push(Event::EnemyDefeated {
                        enemy_id: target_id,
                    });
                }
            }
        }
    }

    fn reject(events: &mut Vec<Event>, reason: impl Into<String>) {
        events.push(Event::CommandRejected {
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
}
