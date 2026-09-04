use std::collections::{HashMap, VecDeque};
use std::env;
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy)]
struct Config {
    players: usize,
    npcs: usize,
    ticks: usize,
    warmup: usize,
    commands_per_tick: usize,
    command_budget: usize,
    initial_queued_commands: usize,
    seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: 200,
            npcs: 400,
            ticks: 1_000,
            warmup: 100,
            commands_per_tick: 400,
            command_budget: 400,
            initial_queued_commands: 0,
            seed: 0x5eed_1234_5678_9abc,
        }
    }
}

impl Config {
    fn parse() -> Self {
        let mut config = Self::default();
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                print_help();
                std::process::exit(0);
            }
            let mut value = || {
                args.next().unwrap_or_else(|| {
                    eprintln!("missing value for {arg}");
                    std::process::exit(2);
                })
            };
            match arg.as_str() {
                "--players" => config.players = parse_usize(&arg, &value()),
                "--npcs" => config.npcs = parse_usize(&arg, &value()),
                "--ticks" => config.ticks = parse_usize(&arg, &value()),
                "--warmup" => config.warmup = parse_usize(&arg, &value()),
                "--commands-per-tick" => config.commands_per_tick = parse_usize(&arg, &value()),
                "--command-budget" => config.command_budget = parse_usize(&arg, &value()),
                "--initial-queued-commands" => {
                    config.initial_queued_commands = parse_usize(&arg, &value())
                }
                "--seed" => config.seed = parse_u64(&arg, &value()),
                _ => {
                    eprintln!("unknown argument: {arg}\n");
                    print_help();
                    std::process::exit(2);
                }
            }
        }
        if config.ticks == 0 {
            eprintln!("--ticks must be greater than zero");
            std::process::exit(2);
        }
        config
    }
}

fn parse_usize(name: &str, value: &str) -> usize {
    value.parse().unwrap_or_else(|_| {
        eprintln!("{name} requires a non-negative integer, got {value}");
        std::process::exit(2);
    })
}

fn parse_u64(name: &str, value: &str) -> u64 {
    value.parse().unwrap_or_else(|_| {
        eprintln!("{name} requires an unsigned integer, got {value}");
        std::process::exit(2);
    })
}

fn print_help() {
    println!(
        "rust-region-bench\n\n\
         Models one fixed-tick region worker using only the Rust standard library.\n\n\
         Options (defaults):\n\
           --players N                    200\n\
           --npcs N                       400\n\
           --ticks N                      1000 measured ticks\n\
           --warmup N                     100 ticks excluded from timing\n\
           --commands-per-tick N          400 commands enqueued per tick\n\
           --command-budget N             400 commands processed per tick\n\
           --initial-queued-commands N    commands queued before the first tick\n\
           --seed N                       deterministic workload seed\n"
    );
}

#[derive(Clone, Copy)]
enum CommandKind {
    Move,
    Ability,
}

#[derive(Clone, Copy)]
struct Command {
    player: usize,
    target: usize,
    kind: CommandKind,
    sequence: u64,
}

struct Entity {
    x: f32,
    y: f32,
    hp: i32,
    energy: i32,
    is_player: bool,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn next_u32(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 7;
        value ^= value >> 9;
        value ^= value << 8;
        self.0 = value;
        value as u32
    }

    fn signed_step(&mut self) -> f32 {
        match self.next_u32() % 3 {
            0 => -0.25,
            1 => 0.0,
            _ => 0.25,
        }
    }
}

#[derive(Default)]
struct Totals {
    commands_processed: u64,
    movement_commands: u64,
    ability_commands: u64,
    npc_ai_operations: u64,
    combat_hits: u64,
    queue_sum: u128,
    max_queue_depth: usize,
    checksum: u64,
}

struct RegionWorker {
    entities: Vec<Entity>,
    commands: VecDeque<Command>,
    spatial: HashMap<(i32, i32), Vec<usize>>,
    rng: Rng,
    next_sequence: u64,
    tick: u64,
    cell_size: f32,
}

struct TickResult {
    elapsed_ns: u128,
    queue_depth: usize,
    checksum: u64,
}

impl RegionWorker {
    fn new(config: Config) -> Self {
        let mut entities = Vec::with_capacity(config.players + config.npcs);
        let mut rng = Rng::new(config.seed);
        for index in 0..config.players {
            entities.push(Entity {
                x: (index % 20) as f32 * 3.0,
                y: (index / 20) as f32 * 3.0,
                hp: 100,
                energy: 100,
                is_player: true,
            });
        }
        for index in 0..config.npcs {
            entities.push(Entity {
                x: ((index * 7) % 80) as f32 - 120.0 + rng.signed_step(),
                y: ((index * 11) % 80) as f32 - 120.0 + rng.signed_step(),
                hp: 100,
                energy: 0,
                is_player: false,
            });
        }
        Self {
            entities,
            commands: VecDeque::with_capacity(
                config.initial_queued_commands.max(config.commands_per_tick),
            ),
            spatial: HashMap::new(),
            rng,
            next_sequence: 0,
            tick: 0,
            cell_size: 16.0,
        }
    }

    fn enqueue_work(&mut self, command_count: usize) {
        if self.entities.is_empty() {
            return;
        }
        let player_count = self
            .entities
            .iter()
            .filter(|entity| entity.is_player)
            .count();
        if player_count == 0 {
            return;
        }
        for _ in 0..command_count {
            let sequence = self.next_sequence;
            self.next_sequence += 1;
            let player = (sequence as usize) % player_count;
            let target = (sequence as usize * 17 + 3) % self.entities.len();
            let kind = if sequence % 3 == 0 {
                CommandKind::Move
            } else {
                CommandKind::Ability
            };
            self.commands.push_back(Command {
                player,
                target,
                kind,
                sequence,
            });
        }
    }

    fn run_tick(&mut self, command_budget: usize, commands_per_tick: usize) -> TickResult {
        let started = Instant::now();
        self.tick += 1;
        self.enqueue_work(commands_per_tick);
        self.rebuild_spatial_index();

        let mut checksum = self.tick;
        for _ in 0..command_budget {
            let Some(command) = self.commands.pop_front() else {
                break;
            };
            if command.player >= self.entities.len() || !self.entities[command.player].is_player {
                continue;
            }
            match command.kind {
                CommandKind::Move => {
                    let entity = &mut self.entities[command.player];
                    entity.x = (entity.x + self.rng.signed_step()).clamp(-500.0, 500.0);
                    entity.y = (entity.y + self.rng.signed_step()).clamp(-500.0, 500.0);
                    entity.energy = (entity.energy + 1).min(100);
                    checksum = checksum.wrapping_add(entity.x.to_bits() as u64);
                }
                CommandKind::Ability => {
                    checksum = checksum.wrapping_add(self.resolve_ability(command));
                }
            }
        }

        self.rebuild_spatial_index();
        checksum = checksum.wrapping_add(self.run_npc_ai());
        let queue_depth = self.commands.len();
        TickResult {
            elapsed_ns: started.elapsed().as_nanos(),
            queue_depth,
            checksum,
        }
    }

    fn resolve_ability(&mut self, command: Command) -> u64 {
        let source = &self.entities[command.player];
        let source_cell = self.cell(source.x, source.y);
        let mut nearest: Option<(usize, f32)> = None;
        let mut work = 0u64;
        for cell_y in (source_cell.1 - 1)..=(source_cell.1 + 1) {
            for cell_x in (source_cell.0 - 1)..=(source_cell.0 + 1) {
                if let Some(candidates) = self.spatial.get(&(cell_x, cell_y)) {
                    for &candidate in candidates {
                        if self.entities[candidate].is_player {
                            continue;
                        }
                        let target = &self.entities[candidate];
                        let dx = source.x - target.x;
                        let dy = source.y - target.y;
                        let distance_squared = dx * dx + dy * dy;
                        work = work.wrapping_add(distance_squared.to_bits() as u64);
                        if nearest.map_or(true, |(_, distance)| distance_squared < distance) {
                            nearest = Some((candidate, distance_squared));
                        }
                    }
                }
            }
        }
        if let Some((target, _)) = nearest {
            let damage = 1 + (self.rng.next_u32() % 5) as i32;
            let target_entity = &mut self.entities[target];
            target_entity.hp -= damage;
            if target_entity.hp <= 0 {
                target_entity.hp = 100;
                work = work.wrapping_add(1);
            }
            work = work.wrapping_add(command.target as u64);
        }
        work ^ command.sequence
    }

    fn rebuild_spatial_index(&mut self) {
        self.spatial.clear();
        for (index, entity) in self.entities.iter().enumerate() {
            self.spatial
                .entry(self.cell(entity.x, entity.y))
                .or_default()
                .push(index);
        }
    }

    fn run_npc_ai(&mut self) -> u64 {
        let mut checksum = 0u64;
        let npc_indices: Vec<usize> = self
            .entities
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (!entity.is_player).then_some(index))
            .collect();
        for npc_index in npc_indices {
            let npc = &self.entities[npc_index];
            let npc_cell = self.cell(npc.x, npc.y);
            let mut nearest: Option<(usize, f32)> = None;
            for cell_y in (npc_cell.1 - 1)..=(npc_cell.1 + 1) {
                for cell_x in (npc_cell.0 - 1)..=(npc_cell.0 + 1) {
                    if let Some(candidates) = self.spatial.get(&(cell_x, cell_y)) {
                        for &candidate in candidates {
                            if !self.entities[candidate].is_player {
                                continue;
                            }
                            let player = &self.entities[candidate];
                            let dx = npc.x - player.x;
                            let dy = npc.y - player.y;
                            let distance_squared = dx * dx + dy * dy;
                            checksum = checksum.wrapping_add(distance_squared.to_bits() as u64);
                            if nearest.map_or(true, |(_, distance)| distance_squared < distance) {
                                nearest = Some((candidate, distance_squared));
                            }
                        }
                    }
                }
            }
            if let Some((player, distance)) = nearest {
                let player_x = self.entities[player].x;
                let player_y = self.entities[player].y;
                let npc_entity = &mut self.entities[npc_index];
                let direction = if distance > 9.0 { 0.01 } else { 0.0 };
                npc_entity.x += (player_x - npc_entity.x) * direction;
                npc_entity.y += (player_y - npc_entity.y) * direction;
                checksum = checksum.wrapping_add(player as u64);
            }
        }
        checksum
    }

    fn cell(&self, x: f32, y: f32) -> (i32, i32) {
        (
            (x / self.cell_size).floor() as i32,
            (y / self.cell_size).floor() as i32,
        )
    }
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) * percentile) / 100;
    sorted[index]
}

fn main() {
    let config = Config::parse();
    let mut worker = RegionWorker::new(config);
    worker.enqueue_work(config.initial_queued_commands);

    for _ in 0..config.warmup {
        let _ = worker.run_tick(config.command_budget, config.commands_per_tick);
    }

    let mut durations = Vec::with_capacity(config.ticks);
    let mut totals = Totals::default();
    for _ in 0..config.ticks {
        let result = worker.run_tick(config.command_budget, config.commands_per_tick);
        durations.push(result.elapsed_ns);
        totals.queue_sum += result.queue_depth as u128;
        totals.max_queue_depth = totals.max_queue_depth.max(result.queue_depth);
        totals.checksum = totals.checksum.wrapping_add(result.checksum);
    }

    let commands_processed = (config.ticks * config.command_budget)
        .min(config.ticks * config.commands_per_tick + config.initial_queued_commands);
    totals.commands_processed = commands_processed as u64;
    totals.movement_commands = (0..commands_processed)
        .filter(|sequence| sequence % 3 == 0)
        .count() as u64;
    totals.ability_commands = totals.commands_processed - totals.movement_commands;
    totals.npc_ai_operations = (config.ticks * config.npcs) as u64;
    totals.combat_hits = totals.ability_commands;

    let mut sorted = durations.clone();
    sorted.sort_unstable();
    let total_ns: u128 = durations.iter().sum();
    let average_ns = total_ns / durations.len() as u128;
    let elapsed_seconds = total_ns as f64 / 1_000_000_000.0;
    let ticks_per_second = config.ticks as f64 / elapsed_seconds.max(f64::MIN_POSITIVE);

    println!("configuration.players={}", config.players);
    println!("configuration.npcs={}", config.npcs);
    println!("configuration.entities={}", config.players + config.npcs);
    println!("configuration.measured_ticks={}", config.ticks);
    println!("configuration.warmup_ticks={}", config.warmup);
    println!(
        "configuration.commands_per_tick={}",
        config.commands_per_tick
    );
    println!("configuration.command_budget={}", config.command_budget);
    println!(
        "configuration.initial_queued_commands={}",
        config.initial_queued_commands
    );
    println!(
        "measurement.total_tick_time_ms={:.3}",
        elapsed_seconds * 1000.0
    );
    println!("measurement.tick_rate_equivalent={:.1}", ticks_per_second);
    println!("measurement.tick_avg_us={:.3}", average_ns as f64 / 1000.0);
    println!(
        "measurement.tick_p50_us={:.3}",
        percentile(&sorted, 50) as f64 / 1000.0
    );
    println!(
        "measurement.tick_p95_us={:.3}",
        percentile(&sorted, 95) as f64 / 1000.0
    );
    println!(
        "measurement.tick_p99_us={:.3}",
        percentile(&sorted, 99) as f64 / 1000.0
    );
    println!(
        "measurement.tick_max_us={:.3}",
        *sorted.last().unwrap() as f64 / 1000.0
    );
    println!(
        "measurement.average_queue_depth={:.3}",
        totals.queue_sum as f64 / config.ticks as f64
    );
    println!("measurement.max_queue_depth={}", totals.max_queue_depth);
    println!("work.commands_processed={}", totals.commands_processed);
    println!("work.movement_commands={}", totals.movement_commands);
    println!("work.ability_commands={}", totals.ability_commands);
    println!("work.npc_ai_operations={}", totals.npc_ai_operations);
    println!("work.combat_hits={}", totals.combat_hits);
    println!("measurement.checksum={}", black_box(totals.checksum));
}
