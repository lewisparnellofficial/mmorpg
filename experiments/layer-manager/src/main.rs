use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
struct Config {
    soft_capacity: usize,
    hard_capacity: usize,
    retirement_grace_ticks: u64,
    migration_cooldown_ticks: u64,
}

#[derive(Debug)]
struct Player {
    id: u32,
    party_id: u32,
    layer_id: u32,
    busy_until_tick: u64,
    migration_eligible_at: u64,
    active: bool,
}

#[derive(Debug)]
struct Party {
    id: u32,
    members: Vec<u32>,
    arrival_tick: u64,
}

#[derive(Debug)]
struct Layer {
    id: u32,
    created_tick: u64,
    active: bool,
    members: BTreeSet<u32>,
    max_population: usize,
    arrival_assignments: usize,
    migrations_in: usize,
    migrations_out: usize,
    empty_since_tick: Option<u64>,
}

impl Layer {
    fn new(id: u32, created_tick: u64) -> Self {
        Self {
            id,
            created_tick,
            active: true,
            members: BTreeSet::new(),
            max_population: 0,
            arrival_assignments: 0,
            migrations_in: 0,
            migrations_out: 0,
            empty_since_tick: None,
        }
    }

    fn population(&self) -> usize {
        self.members.len()
    }
}

#[derive(Default, Debug)]
struct Metrics {
    arrival_players: usize,
    arrival_groups: usize,
    layer_creations: usize,
    layer_retirements: usize,
    migration_group_attempts: usize,
    migration_group_completions: usize,
    migration_players: usize,
    blocked_busy_groups: usize,
    blocked_cooldown_groups: usize,
    soft_overflow_ticks: usize,
    hard_overflow_ticks: usize,
    invariant_failures: usize,
    peak_active_layers: usize,
    peak_total_players: usize,
}

struct LayerManager {
    config: Config,
    tick: u64,
    next_player_id: u32,
    next_party_id: u32,
    next_layer_id: u32,
    players: BTreeMap<u32, Player>,
    parties: BTreeMap<u32, Party>,
    layers: BTreeMap<u32, Layer>,
    metrics: Metrics,
}

impl LayerManager {
    fn new(config: Config) -> Self {
        let mut manager = Self {
            config,
            tick: 0,
            next_player_id: 1,
            next_party_id: 1,
            next_layer_id: 0,
            players: BTreeMap::new(),
            parties: BTreeMap::new(),
            layers: BTreeMap::new(),
            metrics: Metrics::default(),
        };
        manager.create_layer();
        manager
    }

    fn create_layer(&mut self) -> u32 {
        let id = self.next_layer_id;
        self.next_layer_id += 1;
        self.layers.insert(id, Layer::new(id, self.tick));
        self.metrics.layer_creations += 1;
        println!("EVENT tick={} layer_created id={}", self.tick, id);
        id
    }

    fn active_layer_ids(&self) -> Vec<u32> {
        self.layers
            .values()
            .filter(|layer| layer.active)
            .map(|layer| layer.id)
            .collect()
    }

    fn total_active_players(&self) -> usize {
        self.players.values().filter(|player| player.active).count()
    }

    fn arrive_party(&mut self, member_count: usize, label: &str) -> u32 {
        assert!(member_count > 0);

        let party_id = self.next_party_id;
        self.next_party_id += 1;
        let mut members = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            let player_id = self.next_player_id;
            self.next_player_id += 1;
            members.push(player_id);
        }

        let layer_id = self.choose_arrival_layer(member_count);
        let party = Party {
            id: party_id,
            members: members.clone(),
            arrival_tick: self.tick,
        };
        self.parties.insert(party_id, party);

        for player_id in members.iter().copied() {
            self.players.insert(
                player_id,
                Player {
                    id: player_id,
                    party_id,
                    layer_id,
                    busy_until_tick: 0,
                    migration_eligible_at: 0,
                    active: true,
                },
            );
        }

        let layer = self
            .layers
            .get_mut(&layer_id)
            .expect("arrival layer exists");
        layer.members.extend(members.iter().copied());
        layer.arrival_assignments += member_count;
        layer.max_population = layer.max_population.max(layer.population());
        layer.empty_since_tick = None;

        self.metrics.arrival_players += member_count;
        self.metrics.arrival_groups += 1;
        println!(
            "EVENT tick={} arrival label={} party={} players={} layer={} layer_population={}",
            self.tick,
            label,
            party_id,
            member_count,
            layer_id,
            layer.population()
        );
        layer_id
    }

    fn arrive_party_wave(&mut self, count: usize, member_count: usize, label: &str) {
        for index in 0..count {
            self.arrive_party(member_count, &format!("{}-{}", label, index + 1));
        }
    }

    fn choose_arrival_layer(&mut self, group_size: usize) -> u32 {
        let active_ids = self.active_layer_ids();

        let under_soft = active_ids
            .iter()
            .filter_map(|id| self.layers.get(id))
            .filter(|layer| layer.population() + group_size <= self.config.soft_capacity)
            .min_by_key(|layer| (layer.population(), layer.id))
            .map(|layer| layer.id);
        if let Some(id) = under_soft {
            return id;
        }

        let under_hard = active_ids
            .iter()
            .filter_map(|id| self.layers.get(id))
            .filter(|layer| layer.population() + group_size <= self.config.hard_capacity)
            .min_by_key(|layer| (layer.population(), layer.id))
            .map(|layer| layer.id);
        if let Some(id) = under_hard {
            return id;
        }

        self.create_layer()
    }

    fn mark_party_busy(&mut self, party_id: u32, busy_until_tick: u64) {
        let members = self
            .parties
            .get(&party_id)
            .map(|party| party.members.clone())
            .unwrap_or_default();
        for player_id in members {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.busy_until_tick = busy_until_tick;
            }
        }
        println!(
            "EVENT tick={} party_busy party={} busy_until={}",
            self.tick, party_id, busy_until_tick
        );
    }

    fn mark_player_range_busy(&mut self, first_id: u32, last_id: u32, busy_until_tick: u64) {
        let mut count = 0;
        for player_id in first_id..=last_id {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.busy_until_tick = busy_until_tick;
                count += 1;
            }
        }
        println!(
            "EVENT tick={} players_busy first={} last={} count={} busy_until={}",
            self.tick, first_id, last_id, count, busy_until_tick
        );
    }

    fn choose_migration_destination(&mut self, source_id: u32, group_size: usize) -> u32 {
        let destination = self
            .active_layer_ids()
            .into_iter()
            .filter(|id| *id != source_id)
            .filter_map(|id| self.layers.get(&id))
            .filter(|layer| layer.population() + group_size <= self.config.soft_capacity)
            .min_by_key(|layer| (layer.population(), layer.id))
            .map(|layer| layer.id);
        if let Some(id) = destination {
            return id;
        }

        let destination = self
            .active_layer_ids()
            .into_iter()
            .filter(|id| *id != source_id)
            .filter_map(|id| self.layers.get(&id))
            .filter(|layer| layer.population() + group_size <= self.config.hard_capacity)
            .min_by_key(|layer| (layer.population(), layer.id))
            .map(|layer| layer.id);
        destination.unwrap_or_else(|| self.create_layer())
    }

    fn source_party_ids(&self, source_id: u32) -> Vec<u32> {
        let members = &self.layers.get(&source_id).expect("source exists").members;
        let party_ids: BTreeSet<u32> = members
            .iter()
            .filter_map(|player_id| self.players.get(player_id))
            .filter(|player| player.active)
            .map(|player| player.party_id)
            .collect();

        let mut ids: Vec<u32> = party_ids.into_iter().collect();
        ids.sort_by_key(|party_id| {
            let party = self.parties.get(party_id).expect("party exists");
            (party.arrival_tick, party.id)
        });
        ids
    }

    fn party_migration_block(&self, party_id: u32) -> Option<&'static str> {
        let party = self.parties.get(&party_id).expect("party exists");
        for player_id in &party.members {
            let player = self.players.get(player_id).expect("player exists");
            if !player.active {
                return Some("inactive");
            }
            if player.busy_until_tick > self.tick {
                return Some("busy");
            }
            if player.migration_eligible_at > self.tick {
                return Some("cooldown");
            }
        }
        None
    }

    fn migrate_party(&mut self, party_id: u32, source_id: u32, destination_id: u32) {
        let member_ids = self
            .parties
            .get(&party_id)
            .expect("party exists")
            .members
            .clone();
        let source = self.layers.get_mut(&source_id).expect("source exists");
        for player_id in &member_ids {
            assert!(source.members.remove(player_id));
        }
        source.migrations_out += 1;

        let destination = self
            .layers
            .get_mut(&destination_id)
            .expect("destination exists");
        destination.members.extend(member_ids.iter().copied());
        destination.migrations_in += 1;
        destination.max_population = destination.max_population.max(destination.population());
        destination.empty_since_tick = None;

        for player_id in &member_ids {
            let player = self.players.get_mut(player_id).expect("player exists");
            assert_eq!(player.layer_id, source_id);
            player.layer_id = destination_id;
            player.migration_eligible_at = self.tick + self.config.migration_cooldown_ticks;
        }

        self.metrics.migration_group_completions += 1;
        self.metrics.migration_players += member_ids.len();
        println!(
            "EVENT tick={} migration party={} players={} from={} to={}",
            self.tick,
            party_id,
            member_ids.len(),
            source_id,
            destination_id
        );
    }

    fn rebalance_overloaded_layers(&mut self, label: &str) {
        let source_ids: Vec<u32> = self
            .active_layer_ids()
            .into_iter()
            .filter(|id| {
                self.layers.get(id).expect("layer exists").population() > self.config.soft_capacity
            })
            .collect();

        for source_id in source_ids {
            let mut blocked_this_pass = BTreeSet::new();
            loop {
                let source_population = self
                    .layers
                    .get(&source_id)
                    .expect("source exists")
                    .population();
                if source_population <= self.config.soft_capacity {
                    break;
                }

                let candidates = self.source_party_ids(source_id);
                let mut moved = false;
                for party_id in candidates {
                    self.metrics.migration_group_attempts += 1;
                    if let Some(reason) = self.party_migration_block(party_id) {
                        if blocked_this_pass.insert(party_id) {
                            match reason {
                                "busy" => self.metrics.blocked_busy_groups += 1,
                                "cooldown" => self.metrics.blocked_cooldown_groups += 1,
                                _ => {}
                            }
                            println!(
                                "EVENT tick={} migration_blocked label={} party={} reason={}",
                                self.tick, label, party_id, reason
                            );
                        }
                        continue;
                    }

                    let group_size = self
                        .parties
                        .get(&party_id)
                        .expect("party exists")
                        .members
                        .len();
                    let destination_id = self.choose_migration_destination(source_id, group_size);
                    self.migrate_party(party_id, source_id, destination_id);
                    moved = true;
                    break;
                }

                if !moved {
                    println!(
                        "EVENT tick={} rebalance_stalled label={} layer={} population={}",
                        self.tick, label, source_id, source_population
                    );
                    break;
                }
            }
        }
    }

    fn depart_all_from_layer(&mut self, layer_id: u32, label: &str) {
        let player_ids: Vec<u32> = self
            .layers
            .get(&layer_id)
            .expect("layer exists")
            .members
            .iter()
            .copied()
            .collect();
        for player_id in player_ids {
            let player = self.players.get_mut(&player_id).expect("player exists");
            player.active = false;
            self.layers
                .get_mut(&layer_id)
                .expect("layer exists")
                .members
                .remove(&player_id);
        }
        println!(
            "EVENT tick={} departures label={} layer={} players_remaining={}",
            self.tick,
            label,
            layer_id,
            self.layers
                .get(&layer_id)
                .expect("layer exists")
                .population()
        );
    }

    fn retire_empty_layers(&mut self) {
        let active_ids = self.active_layer_ids();
        for layer_id in active_ids {
            let population = self
                .layers
                .get(&layer_id)
                .expect("layer exists")
                .population();
            if population == 0 {
                let layer = self.layers.get_mut(&layer_id).expect("layer exists");
                if layer.empty_since_tick.is_none() {
                    layer.empty_since_tick = Some(self.tick);
                }
                let empty_since = layer.empty_since_tick.expect("set above");
                if layer_id != 0
                    && self.tick.saturating_sub(empty_since) >= self.config.retirement_grace_ticks
                {
                    layer.active = false;
                    self.metrics.layer_retirements += 1;
                    println!("EVENT tick={} layer_retired id={}", self.tick, layer_id);
                }
            } else {
                self.layers
                    .get_mut(&layer_id)
                    .expect("layer exists")
                    .empty_since_tick = None;
            }
        }
    }

    fn record_capacity_metrics(&mut self) {
        let active_ids = self.active_layer_ids();
        self.metrics.peak_active_layers = self.metrics.peak_active_layers.max(active_ids.len());
        self.metrics.peak_total_players = self
            .metrics
            .peak_total_players
            .max(self.total_active_players());
        for layer_id in active_ids {
            let population = self
                .layers
                .get(&layer_id)
                .expect("layer exists")
                .population();
            if population > self.config.soft_capacity {
                self.metrics.soft_overflow_ticks += 1;
            }
            if population > self.config.hard_capacity {
                self.metrics.hard_overflow_ticks += 1;
            }
        }
    }

    fn assert_invariants(&mut self) {
        let mut seen_players = BTreeSet::new();
        for (layer_id, layer) in &self.layers {
            if !layer.active {
                assert!(layer.members.is_empty(), "retired layer contains players");
            }
            assert!(
                layer.population() <= self.config.hard_capacity,
                "hard capacity exceeded"
            );
            for player_id in &layer.members {
                assert!(
                    seen_players.insert(*player_id),
                    "player appears in multiple layers"
                );
                let player = self.players.get(player_id).expect("layer member exists");
                assert!(player.active, "inactive player in active layer");
                assert_eq!(player.layer_id, *layer_id, "player layer pointer disagrees");
            }
        }

        for player in self.players.values().filter(|player| player.active) {
            assert!(
                seen_players.contains(&player.id),
                "active player missing from layer"
            );
            let party = self.parties.get(&player.party_id).expect("party exists");
            for member_id in &party.members {
                let member = self.players.get(member_id).expect("party member exists");
                if member.active {
                    assert_eq!(
                        member.layer_id, player.layer_id,
                        "party split across layers"
                    );
                }
            }
        }
    }

    fn print_summary(&self) {
        println!();
        println!("SUMMARY");
        println!("config soft_capacity={}", self.config.soft_capacity);
        println!("config hard_capacity={}", self.config.hard_capacity);
        println!(
            "config retirement_grace_ticks={}",
            self.config.retirement_grace_ticks
        );
        println!(
            "config migration_cooldown_ticks={}",
            self.config.migration_cooldown_ticks
        );
        println!(
            "arrivals players={} groups={}",
            self.metrics.arrival_players, self.metrics.arrival_groups
        );
        println!("peak total_players={}", self.metrics.peak_total_players);
        println!("peak active_layers={}", self.metrics.peak_active_layers);
        println!(
            "layers created={}",
            self.metrics.layer_creations.saturating_sub(1)
        );
        println!("layers retired={}", self.metrics.layer_retirements);
        println!(
            "migration group_attempts={}",
            self.metrics.migration_group_attempts
        );
        println!(
            "migration group_completions={}",
            self.metrics.migration_group_completions
        );
        println!("migration players={}", self.metrics.migration_players);
        println!(
            "migration blocked_busy_groups={}",
            self.metrics.blocked_busy_groups
        );
        println!(
            "migration blocked_cooldown_groups={}",
            self.metrics.blocked_cooldown_groups
        );
        println!(
            "capacity soft_overflow_layer_ticks={}",
            self.metrics.soft_overflow_ticks
        );
        println!(
            "capacity hard_overflow_layer_ticks={}",
            self.metrics.hard_overflow_ticks
        );
        println!("invariant_failures={}", self.metrics.invariant_failures);
        for layer in self.layers.values() {
            println!(
                "layer id={} active={} created_tick={} final_population={} max_population={} arrivals={} migrations_in={} migrations_out={}",
                layer.id,
                layer.active,
                layer.created_tick,
                layer.population(),
                layer.max_population,
                layer.arrival_assignments,
                layer.migrations_in,
                layer.migrations_out
            );
        }
    }
}

fn main() {
    let config = Config {
        soft_capacity: 160,
        hard_capacity: 200,
        retirement_grace_ticks: 3,
        migration_cooldown_ticks: 30,
    };
    let mut manager = LayerManager::new(config);

    println!("SCENARIO deterministic_hotspot_layering");
    println!("tick_duration=1 logical unit; no wall-clock timing");

    manager.arrive_party_wave(15, 8, "initial_party");
    manager.arrive_party_wave(30, 1, "initial_solo");
    manager.assert_invariants();

    for tick in 1..=60 {
        manager.tick = tick;
        match tick {
            5 => {
                manager.arrive_party_wave(12, 5, "hotspot_party");
                println!("EVENT tick=5 note=admission reaches hard capacity before rebalance");
            }
            8 => manager.arrive_party_wave(20, 1, "hotspot_solo"),
            10 => {
                manager.mark_party_busy(1, 15);
                manager.mark_party_busy(2, 15);
                manager.mark_player_range_busy(121, 130, 15);
                manager.rebalance_overloaded_layers("first_hotspot");
            }
            20 => manager.arrive_party_wave(40, 1, "second_wave"),
            30 => {
                manager.arrive_party_wave(180, 1, "world_event_surge");
                manager.rebalance_overloaded_layers("world_event_surge");
            }
            50 => manager.depart_all_from_layer(2, "event_ended_layer_2"),
            55 => manager.depart_all_from_layer(1, "event_ended_layer_1"),
            _ => {}
        }

        manager.retire_empty_layers();
        manager.record_capacity_metrics();
        manager.assert_invariants();
    }

    manager.print_summary();
}
