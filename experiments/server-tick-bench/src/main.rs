use mmorpg_core::{CombatTiming, Command, EntityId, Role, World};
use std::env;
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
struct Config {
    players: usize,
    ticks: usize,
    warmup: usize,
}

impl Config {
    fn parse() -> Self {
        let mut config = Self {
            players: 3,
            ticks: 10_000,
            warmup: 1_000,
        };
        let mut arguments = env::args().skip(1);
        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .unwrap_or_else(|| panic!("{argument} requires a value"));
            match argument.as_str() {
                "--players" => config.players = parse_positive(&argument, &value),
                "--ticks" => config.ticks = parse_positive(&argument, &value),
                "--warmup" => {
                    config.warmup = value
                        .parse()
                        .unwrap_or_else(|_| panic!("{argument} requires a nonnegative integer"))
                }
                _ => panic!("unknown argument '{argument}'"),
            }
        }
        if config.ticks == 0 {
            panic!("--ticks requires a positive integer");
        }
        if config.players < 3 {
            panic!("--players requires at least three role players");
        }
        config
    }
}

fn parse_positive(argument: &str, value: &str) -> usize {
    value
        .parse()
        .ok()
        .filter(|value: &usize| *value > 0)
        .unwrap_or_else(|| panic!("{argument} requires a positive integer"))
}

fn main() {
    let config = Config::parse();
    let timing = CombatTiming::new(20, 2, 2).expect("benchmark timing is valid");
    let mut world = World::new_starter_zone();
    let tank_id = join(&mut world, "Bench Tank", Role::Tank);
    let healer_id = join(&mut world, "Bench Healer", Role::Healer);
    let damage_id = join(&mut world, "Bench Damage", Role::DamageDealer);
    let mut player_ids = vec![tank_id, healer_id, damage_id];
    for index in 3..config.players {
        let role = match index % 3 {
            0 => Role::Tank,
            1 => Role::Healer,
            _ => Role::DamageDealer,
        };
        player_ids.push(join(&mut world, &format!("Bench Player {index}"), role));
    }
    let enemy_id = EntityId(2);

    for tick in 0..config.warmup {
        let _ = world.step_with_combat_timing(
            commands(tick, &player_ids, tank_id, damage_id, enemy_id),
            timing,
        );
    }

    let mut durations = Vec::with_capacity(config.ticks);
    let mut checksum = 0_u64;
    for tick in 0..config.ticks {
        let started = Instant::now();
        let events = world.step_with_combat_timing(
            commands(
                tick + config.warmup,
                &player_ids,
                tank_id,
                damage_id,
                enemy_id,
            ),
            timing,
        );
        durations.push(started.elapsed().as_nanos());
        checksum = checksum.wrapping_add(events.len() as u64);
        checksum = checksum.wrapping_add(world.tick());
    }

    durations.sort_unstable();
    let total: u128 = durations.iter().sum();
    let average = total / durations.len() as u128;
    println!("configuration.players={}", config.players);
    println!("configuration.npcs=4");
    println!("configuration.tick_hz={}", timing.tick_hz());
    println!("configuration.cast_time_ticks={}", timing.cast_time_ticks());
    println!("configuration.cooldown_ticks={}", timing.cooldown_ticks());
    println!("configuration.warmup_ticks={}", config.warmup);
    println!("configuration.measured_ticks={}", config.ticks);
    println!("measurement.tick_avg_us={:.3}", average as f64 / 1_000.0);
    println!(
        "measurement.tick_p50_us={:.3}",
        percentile(&durations, 50) as f64 / 1_000.0
    );
    println!(
        "measurement.tick_p95_us={:.3}",
        percentile(&durations, 95) as f64 / 1_000.0
    );
    println!(
        "measurement.tick_p99_us={:.3}",
        percentile(&durations, 99) as f64 / 1_000.0
    );
    println!(
        "measurement.tick_max_us={:.3}",
        *durations.last().unwrap() as f64 / 1_000.0
    );
    println!("measurement.tick_budget_us=12500.000");
    println!("measurement.checksum={}", black_box(checksum));
}

fn join(world: &mut World, name: &str, role: Role) -> EntityId {
    let events = world.step([Command::JoinPlayer {
        name: name.to_owned(),
        role,
    }]);
    events
        .into_iter()
        .find_map(|event| match event {
            mmorpg_core::Event::PlayerJoined { player } => Some(player.id),
            _ => None,
        })
        .expect("benchmark player should join")
}

fn commands(
    tick: usize,
    player_ids: &[EntityId],
    tank_id: EntityId,
    damage_id: EntityId,
    enemy_id: EntityId,
) -> Vec<Command> {
    let mut commands = player_ids
        .iter()
        .enumerate()
        .map(|(index, &player_id)| Command::SelectTarget {
            player_id,
            target_id: if index == 1 { tank_id } else { enemy_id },
        })
        .collect::<Vec<_>>();
    if tick.is_multiple_of(8) {
        commands.push(Command::BasicAttack {
            player_id: damage_id,
        });
    }
    if tick.is_multiple_of(20) {
        commands.push(Command::Taunt { player_id: tank_id });
    }
    commands
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    let index = ((sorted.len() - 1) * percentile) / 100;
    sorted[index]
}
