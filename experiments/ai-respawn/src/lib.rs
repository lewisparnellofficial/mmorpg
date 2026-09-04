#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnemyId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlayerId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn distance_squared(self, other: Self) -> u64 {
        let dx = i64::from(self.x) - i64::from(other.x);
        let dy = i64::from(self.y) - i64::from(other.y);
        (dx * dx + dy * dy) as u64
    }

    fn step_toward(self, destination: Self, step: u32) -> Self {
        let dx = i64::from(destination.x) - i64::from(self.x);
        let dy = i64::from(destination.y) - i64::from(self.y);
        let distance = integer_distance(dx, dy);
        if distance <= u64::from(step) || distance == 0 {
            return destination;
        }

        // This is intentionally integer arithmetic. The small truncation is
        // stable across runs and prevents render-frame timing from affecting
        // the result of this fixed-tick model.
        let step = i64::from(step);
        Self::new(
            self.x + ((dx * step) / i64::try_from(distance).unwrap_or(i64::MAX)) as i32,
            self.y + ((dy * step) / i64::try_from(distance).unwrap_or(i64::MAX)) as i32,
        )
    }
}

fn integer_distance(dx: i64, dy: i64) -> u64 {
    // The experiment uses modest starter-zone coordinates. Saturation keeps
    // malformed extreme coordinates from panicking the report process.
    let squared = dx
        .checked_mul(dx)
        .and_then(|value| value.checked_add(dy.checked_mul(dy)?))
        .unwrap_or(i64::MAX);
    integer_sqrt(squared as u64)
}

fn integer_sqrt(value: u64) -> u64 {
    let mut low = 0;
    let mut high = 1;
    while high <= value / high {
        high = high.saturating_mul(2);
    }
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if middle <= value / middle.max(1) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldConfig {
    pub tick_rate_hz: u32,
    pub respawn_delay: Duration,
    pub respawn_delay_ticks: u64,
}

impl WorldConfig {
    pub fn try_new(tick_rate_hz: u32, respawn_delay: Duration) -> Result<Self, ConfigError> {
        if tick_rate_hz == 0 {
            return Err(ConfigError::ZeroTickRate);
        }

        let numerator = respawn_delay.as_nanos() * u128::from(tick_rate_hz);
        let respawn_delay_ticks = numerator
            .div_ceil(NANOS_PER_SECOND)
            .try_into()
            .map_err(|_| ConfigError::DelayTooLarge)?;

        Ok(Self {
            tick_rate_hz,
            respawn_delay,
            respawn_delay_ticks,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    ZeroTickRate,
    DelayTooLarge,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroTickRate => formatter.write_str("tick rate must be greater than zero"),
            Self::DelayTooLarge => formatter.write_str("respawn delay does not fit in tick count"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnemyState {
    Patrolling { waypoint_index: usize },
    Aggro { target: PlayerId },
    Leashing,
    Dead { respawn_at_tick: u64 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enemy {
    pub id: EnemyId,
    pub spawn_position: Position,
    pub position: Position,
    pub patrol_waypoints: Vec<Position>,
    pub movement_per_tick: u32,
    pub aggro_range: u32,
    pub leash_range: u32,
    pub max_health: u32,
    pub health: u32,
    pub state: EnemyState,
}

impl Enemy {
    pub fn new(
        id: EnemyId,
        spawn_position: Position,
        patrol_waypoints: Vec<Position>,
        movement_per_tick: u32,
        aggro_range: u32,
        leash_range: u32,
        max_health: u32,
    ) -> Result<Self, EnemyError> {
        if max_health == 0 {
            return Err(EnemyError::ZeroMaxHealth);
        }
        if leash_range < aggro_range {
            return Err(EnemyError::LeashShorterThanAggro);
        }

        let patrol_waypoints = if patrol_waypoints.is_empty() {
            vec![spawn_position]
        } else {
            patrol_waypoints
        };

        Ok(Self {
            id,
            spawn_position,
            position: spawn_position,
            patrol_waypoints,
            movement_per_tick,
            aggro_range,
            leash_range,
            max_health,
            health: max_health,
            state: EnemyState::Patrolling { waypoint_index: 0 },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnemyError {
    ZeroMaxHealth,
    LeashShorterThanAggro,
}

impl fmt::Display for EnemyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroMaxHealth => {
                formatter.write_str("enemy max health must be greater than zero")
            }
            Self::LeashShorterThanAggro => {
                formatter.write_str("enemy leash range must be at least its aggro range")
            }
        }
    }
}

impl std::error::Error for EnemyError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerObservation {
    pub id: PlayerId,
    pub position: Position,
    pub alive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateKind {
    Patrolling,
    Aggro,
    Leashing,
    Dead,
}

impl From<EnemyState> for StateKind {
    fn from(state: EnemyState) -> Self {
        match state {
            EnemyState::Patrolling { .. } => Self::Patrolling,
            EnemyState::Aggro { .. } => Self::Aggro,
            EnemyState::Leashing => Self::Leashing,
            EnemyState::Dead { .. } => Self::Dead,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiEvent {
    StateChanged {
        tick: u64,
        enemy: EnemyId,
        from: StateKind,
        to: StateKind,
    },
    TargetAcquired {
        tick: u64,
        enemy: EnemyId,
        target: PlayerId,
    },
    TargetLost {
        tick: u64,
        enemy: EnemyId,
        target: PlayerId,
    },
    Died {
        tick: u64,
        enemy: EnemyId,
        respawn_at_tick: u64,
    },
    Respawned {
        tick: u64,
        enemy: EnemyId,
    },
}

pub struct AiWorld {
    pub config: WorldConfig,
    pub tick: u64,
    pub enemies: BTreeMap<EnemyId, Enemy>,
    players: BTreeMap<PlayerId, PlayerObservation>,
    events: Vec<AiEvent>,
}

impl AiWorld {
    pub fn new(config: WorldConfig) -> Self {
        Self {
            config,
            tick: 0,
            enemies: BTreeMap::new(),
            players: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    pub fn add_enemy(&mut self, enemy: Enemy) -> Option<Enemy> {
        self.enemies.insert(enemy.id, enemy)
    }

    pub fn observe_player(&mut self, observation: PlayerObservation) {
        self.players.insert(observation.id, observation);
    }

    pub fn remove_player(&mut self, player: PlayerId) {
        self.players.remove(&player);
    }

    pub fn drain_events(&mut self) -> Vec<AiEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn damage_enemy(&mut self, enemy_id: EnemyId, damage: u32) -> bool {
        let Some(enemy) = self.enemies.get_mut(&enemy_id) else {
            return false;
        };
        if matches!(enemy.state, EnemyState::Dead { .. }) || damage == 0 {
            return false;
        }

        enemy.health = enemy.health.saturating_sub(damage);
        if enemy.health != 0 {
            return false;
        }

        let respawn_at_tick = self.tick.saturating_add(self.config.respawn_delay_ticks);
        let from = StateKind::from(enemy.state);
        enemy.state = EnemyState::Dead { respawn_at_tick };
        self.events.push(AiEvent::StateChanged {
            tick: self.tick,
            enemy: enemy_id,
            from,
            to: StateKind::Dead,
        });
        self.events.push(AiEvent::Died {
            tick: self.tick,
            enemy: enemy_id,
            respawn_at_tick,
        });
        true
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
        let enemy_ids: Vec<_> = self.enemies.keys().copied().collect();
        for enemy_id in enemy_ids {
            self.advance_enemy(enemy_id);
        }
    }

    fn advance_enemy(&mut self, enemy_id: EnemyId) {
        let Some(state) = self.enemies.get(&enemy_id).map(|enemy| enemy.state) else {
            return;
        };

        if let EnemyState::Dead { respawn_at_tick } = state {
            if self.tick >= respawn_at_tick {
                let enemy = self
                    .enemies
                    .get_mut(&enemy_id)
                    .expect("enemy ID came from map");
                enemy.health = enemy.max_health;
                enemy.position = enemy.spawn_position;
                enemy.state = EnemyState::Patrolling { waypoint_index: 0 };
                self.events.push(AiEvent::Respawned {
                    tick: self.tick,
                    enemy: enemy_id,
                });
            }
            return;
        }

        let (position, spawn_position, aggro_range, leash_range) = {
            let enemy = self.enemies.get(&enemy_id).expect("enemy ID came from map");
            (
                enemy.position,
                enemy.spawn_position,
                enemy.aggro_range,
                enemy.leash_range,
            )
        };

        match state {
            EnemyState::Patrolling { waypoint_index } => {
                if let Some(target) = self.nearest_player(position, aggro_range) {
                    self.transition(enemy_id, EnemyState::Aggro { target });
                    self.events.push(AiEvent::TargetAcquired {
                        tick: self.tick,
                        enemy: enemy_id,
                        target,
                    });
                } else {
                    self.patrol(enemy_id, waypoint_index);
                }
            }
            EnemyState::Aggro { target } => {
                let Some(player) = self.players.get(&target).copied() else {
                    self.begin_leash(enemy_id, Some(target));
                    return;
                };
                if !player.alive {
                    self.begin_leash(enemy_id, Some(target));
                    return;
                }
                let leash_squared = u64::from(leash_range) * u64::from(leash_range);
                if position.distance_squared(spawn_position) > leash_squared
                    || player.position.distance_squared(spawn_position) > leash_squared
                {
                    self.begin_leash(enemy_id, Some(target));
                } else {
                    self.move_enemy_toward(enemy_id, player.position);
                }
            }
            EnemyState::Leashing => {
                let at_spawn = position == spawn_position;
                if at_spawn {
                    self.transition(enemy_id, EnemyState::Patrolling { waypoint_index: 0 });
                } else {
                    self.move_enemy_toward(enemy_id, spawn_position);
                }
            }
            EnemyState::Dead { .. } => unreachable!("dead state handled above"),
        }
    }

    fn nearest_player(&self, position: Position, aggro_range: u32) -> Option<PlayerId> {
        let range_squared = u64::from(aggro_range) * u64::from(aggro_range);
        self.players
            .values()
            .filter(|player| player.alive)
            .filter(|player| position.distance_squared(player.position) <= range_squared)
            .min_by_key(|player| (position.distance_squared(player.position), player.id))
            .map(|player| player.id)
    }

    fn patrol(&mut self, enemy_id: EnemyId, waypoint_index: usize) {
        let (position, destination, movement_per_tick, waypoint_count) = {
            let enemy = self.enemies.get(&enemy_id).expect("enemy ID came from map");
            (
                enemy.position,
                enemy.patrol_waypoints[waypoint_index],
                enemy.movement_per_tick,
                enemy.patrol_waypoints.len(),
            )
        };
        let next_position = position.step_toward(destination, movement_per_tick);
        let reached = next_position == destination;
        let enemy = self
            .enemies
            .get_mut(&enemy_id)
            .expect("enemy ID came from map");
        enemy.position = next_position;
        if reached {
            enemy.state = EnemyState::Patrolling {
                waypoint_index: (waypoint_index + 1) % waypoint_count,
            };
        }
    }

    fn move_enemy_toward(&mut self, enemy_id: EnemyId, destination: Position) {
        let (position, movement_per_tick) = {
            let enemy = self.enemies.get(&enemy_id).expect("enemy ID came from map");
            (enemy.position, enemy.movement_per_tick)
        };
        let enemy = self
            .enemies
            .get_mut(&enemy_id)
            .expect("enemy ID came from map");
        enemy.position = position.step_toward(destination, movement_per_tick);
    }

    fn begin_leash(&mut self, enemy_id: EnemyId, target: Option<PlayerId>) {
        if let Some(target) = target {
            self.events.push(AiEvent::TargetLost {
                tick: self.tick,
                enemy: enemy_id,
                target,
            });
        }
        self.transition(enemy_id, EnemyState::Leashing);
    }

    fn transition(&mut self, enemy_id: EnemyId, next: EnemyState) {
        let enemy = self
            .enemies
            .get_mut(&enemy_id)
            .expect("enemy ID came from map");
        let from = StateKind::from(enemy.state);
        let to = StateKind::from(next);
        if from != to {
            self.events.push(AiEvent::StateChanged {
                tick: self.tick,
                enemy: enemy_id,
                from,
                to,
            });
        }
        enemy.state = next;
    }
}

impl fmt::Display for AiEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateChanged {
                tick,
                enemy,
                from,
                to,
            } => {
                write!(
                    formatter,
                    "tick {tick}: enemy {:?} {from:?} -> {to:?}",
                    enemy
                )
            }
            Self::TargetAcquired {
                tick,
                enemy,
                target,
            } => {
                write!(
                    formatter,
                    "tick {tick}: enemy {:?} acquired {:?}",
                    enemy, target
                )
            }
            Self::TargetLost {
                tick,
                enemy,
                target,
            } => {
                write!(
                    formatter,
                    "tick {tick}: enemy {:?} lost {:?}",
                    enemy, target
                )
            }
            Self::Died {
                tick,
                enemy,
                respawn_at_tick,
            } => write!(
                formatter,
                "tick {tick}: enemy {:?} died; respawn scheduled for tick {respawn_at_tick}",
                enemy
            ),
            Self::Respawned { tick, enemy } => {
                write!(formatter, "tick {tick}: enemy {:?} respawned", enemy)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> WorldConfig {
        WorldConfig::try_new(10, Duration::from_millis(500)).unwrap()
    }

    fn enemy() -> Enemy {
        Enemy::new(
            EnemyId(1),
            Position::new(0, 0),
            vec![Position::new(0, 0), Position::new(3, 0)],
            1,
            5,
            10,
            100,
        )
        .unwrap()
    }

    #[test]
    fn patrol_is_fixed_tick_and_deterministic() {
        let mut first = AiWorld::new(config());
        let mut second = AiWorld::new(config());
        first.add_enemy(enemy());
        second.add_enemy(enemy());
        for _ in 0..5 {
            first.advance_tick();
            second.advance_tick();
        }
        assert_eq!(first.enemies, second.enemies);
        assert_eq!(first.enemies[&EnemyId(1)].position, Position::new(2, 0));
        assert!(first.drain_events().is_empty());
    }

    #[test]
    fn nearest_target_wins_and_equal_distance_uses_lowest_id() {
        let mut world = AiWorld::new(config());
        world.add_enemy(enemy());
        world.observe_player(PlayerObservation {
            id: PlayerId(9),
            position: Position::new(2, 0),
            alive: true,
        });
        world.observe_player(PlayerObservation {
            id: PlayerId(3),
            position: Position::new(-2, 0),
            alive: true,
        });
        world.advance_tick();
        assert_eq!(
            world.enemies[&EnemyId(1)].state,
            EnemyState::Aggro {
                target: PlayerId(3)
            }
        );
        assert!(world.drain_events().contains(&AiEvent::TargetAcquired {
            tick: 1,
            enemy: EnemyId(1),
            target: PlayerId(3),
        }));
    }

    #[test]
    fn target_leashes_and_returns_to_patrol() {
        let mut world = AiWorld::new(config());
        world.add_enemy(enemy());
        world.observe_player(PlayerObservation {
            id: PlayerId(1),
            position: Position::new(2, 0),
            alive: true,
        });
        world.advance_tick();
        world.drain_events();
        world.observe_player(PlayerObservation {
            id: PlayerId(1),
            position: Position::new(20, 0),
            alive: true,
        });
        world.advance_tick();
        assert_eq!(world.enemies[&EnemyId(1)].state, EnemyState::Leashing);
        for _ in 0..10 {
            world.advance_tick();
            if matches!(
                world.enemies[&EnemyId(1)].state,
                EnemyState::Patrolling { .. }
            ) {
                break;
            }
        }
        assert_eq!(world.enemies[&EnemyId(1)].position, Position::new(0, 0));
        assert_eq!(
            world.enemies[&EnemyId(1)].state,
            EnemyState::Patrolling { waypoint_index: 0 }
        );
    }

    #[test]
    fn death_respawns_on_the_first_eligible_tick() {
        let mut world = AiWorld::new(config());
        world.add_enemy(enemy());
        assert!(world.damage_enemy(EnemyId(1), 100));
        assert_eq!(
            world.enemies[&EnemyId(1)].state,
            EnemyState::Dead { respawn_at_tick: 5 }
        );
        world.drain_events();
        for _ in 0..4 {
            world.advance_tick();
            assert!(world.drain_events().is_empty());
        }
        world.advance_tick();
        assert_eq!(world.enemies[&EnemyId(1)].health, 100);
        assert_eq!(
            world.drain_events(),
            vec![AiEvent::Respawned {
                tick: 5,
                enemy: EnemyId(1)
            }]
        );
    }

    #[test]
    fn respawn_delay_rounds_up_to_a_whole_tick() {
        let config = WorldConfig::try_new(20, Duration::from_millis(251)).unwrap();
        assert_eq!(config.respawn_delay_ticks, 6);
        assert!(WorldConfig::try_new(0, Duration::ZERO).is_err());
    }
}
