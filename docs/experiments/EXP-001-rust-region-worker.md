# EXP-001: Rust fixed-tick region worker

**Status:** Measured prototype

**Date:** 2026-09-04

## Objective

Validate the smallest useful version of the proposed Rust simulation model:

```text
queued commands -> one fixed-tick region owner -> movement/combat-like work
                  -> spatial index -> NPC work -> measurable tick duration
```

This experiment is intentionally not production server code. It is a local,
deterministic workload for comparing configurations and finding a first-order
tick budget. It does not implement sockets, serialization, persistence,
interest-management output, real combat rules, or multiple workers.

## Hypothesis

A single Rust region worker should be able to process a representative
200-player activity workload on this development machine with enough headroom
for a future network and persistence layer. The result must be treated as a
simulation-only measurement, not as evidence that a complete MMO server can
support the target.

## Workload model

The benchmark uses only the Rust standard library and models:

- Configurable player and NPC/entity counts.
- A `VecDeque` command queue owned by the region worker.
- Deterministically generated movement and ability commands.
- A configurable per-tick command budget, allowing queue buildup to be tested.
- Fixed-tick execution without sleeping; each measured iteration is one region
  tick and is timed with `std::time::Instant`.
- A spatial hash grid rebuilt each tick.
- Ability-like work that searches nearby cells for an NPC and applies simple
  deterministic damage.
- NPC AI-like work that searches nearby cells for players and performs simple
  steering toward the nearest player.
- Warmup ticks excluded from the reported timing.
- A checksum printed at the end to keep the workload observable and discourage
  dead-code elimination.

The implementation deliberately keeps all mutable entity state behind one
`RegionWorker`; there are no locks or cross-thread operations in this
experiment. That mirrors the proposed single-owner region boundary but does
not validate a particular production actor framework.

## Reproduction

From the repository root:

```bash
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 \
  --npcs 400 \
  --ticks 1000 \
  --warmup 100 \
  --commands-per-tick 400 \
  --command-budget 400
```

The default values are the same as the command above. Useful queue-pressure
variants are:

```bash
# Commands arrive faster than this worker is allowed to process them.
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 --npcs 400 --ticks 1000 --warmup 100 \
  --commands-per-tick 800 --command-budget 400

# More NPCs and commands approximate a denser encounter.
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 --npcs 1000 --ticks 1000 --warmup 100 \
  --commands-per-tick 800 --command-budget 800
```

The benchmark has no external crate dependencies. Cargo still generated a
minimal `Cargo.lock`, which is retained with the experiment for reproducible
local builds.

## Environment

Measured on 2026-09-04:

- OS: Linux, x86_64
- Kernel: `7.2.0-1-cachyos #1 SMP PREEMPT_DYNAMIC Thu Aug 20 2026`
- Host: `lewis-desktop`
- Logical CPUs reported by `nproc`: 16
- Memory: 31 GiB total; 18 GiB available at measurement time
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- Cargo: `cargo 1.96.0 (ac68faa20 2026-05-25)`
- Build: Cargo `--release`; benchmark profile uses thin LTO, one codegen
  unit, and symbol stripping
- Repository revision when recorded: see the Git history for this experiment

The machine was not isolated from other desktop work. Results should therefore
be considered local indicative measurements rather than a controlled capacity
claim.

## Measured results

The following results are recorded from actual local runs. Durations are the
benchmark's simulation loop only; they exclude process startup and compilation.

### Baseline: 200 players, 400 NPCs

Command:

```bash
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 --npcs 400 --ticks 1000 --warmup 100 \
  --commands-per-tick 400 --command-budget 400
```

| Metric | Result |
|---|---:|
| Measured ticks | 1,000 |
| Entities | 600 |
| Commands enqueued/tick | 400 |
| Commands processed | 400,000 |
| NPC AI operations | 400,000 |
| Equivalent tick rate | 4,684.2 Hz |
| Average tick | 213.483 µs |
| p50 tick | 211.078 µs |
| p95 tick | 225.596 µs |
| p99 tick | 230.414 µs |
| Maximum tick | 255.652 µs |
| Average queue depth | 0.000 |
| Maximum queue depth | 0 |

### Dense activity: 200 players, 1,000 NPCs

Command:

```bash
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 --npcs 1000 --ticks 1000 --warmup 100 \
  --commands-per-tick 800 --command-budget 800
```

| Metric | Result |
|---|---:|
| Measured ticks | 1,000 |
| Entities | 1,200 |
| Commands enqueued/tick | 800 |
| Commands processed | 800,000 |
| NPC AI operations | 1,000,000 |
| Equivalent tick rate | 1,657.4 Hz |
| Average tick | 603.350 µs |
| p50 tick | 572.881 µs |
| p95 tick | 858.570 µs |
| p99 tick | 872.226 µs |
| Maximum tick | 1,022.680 µs |
| Average queue depth | 0.000 |
| Maximum queue depth | 0 |

### Queue-pressure check: 200 players, 400 NPCs

Command:

```bash
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 --npcs 400 --ticks 1000 --warmup 100 \
  --commands-per-tick 800 --command-budget 400
```

| Metric | Result |
|---|---:|
| Commands enqueued/tick | 800 |
| Commands processed/tick | 400 |
| Equivalent tick rate | 4,379.4 Hz |
| Average tick | 228.341 µs |
| p95 tick | 281.972 µs |
| p99 tick | 336.946 µs |
| Maximum tick | 1,467.158 µs |
| Average queue depth | 240,200.000 |
| Maximum queue depth | 440,000 |

The queue-pressure run confirms that an undersized command budget produces
unbounded backlog in this synthetic model. A production worker needs admission
control, prioritization, overload metrics, and a policy for dropping or
coalescing stale input such as movement. Durable commands cannot be handled by
blindly dropping queue entries.

## Interpretation

Measured result: on this machine, the prototype's simulation-only loop remains
below 1 ms at p99 for both the 200-player/400-NPC baseline and the denser
200-player/1,000-NPC workload. Those numbers are useful as a relative baseline
for future changes to the prototype. The maximum samples were 0.256 ms and
1.023 ms respectively, but maximums should not be treated as stable capacity
limits from one run.

Not measured: a complete server's capacity. This benchmark omits network I/O,
packet parsing and serialization, per-client replication, visibility filtering
cost beyond the local spatial search, real combat rules, persistence, logging,
admin traffic, layer coordination, instance coordination, and client rendering.
It also runs a single worker on one thread and does not measure scheduler or
cross-process overhead.

The results therefore do not prove that one worker can support 200 real clients
or that one server can support 5,000 connected clients. They do support keeping
the single-owner fixed-tick region model as a viable prototype direction and
justify building the next benchmark around simulated network clients and
replication output.

### Estimates, clearly separated from measurements

The following are planning estimates, not measured results:

- A 20 Hz simulation tick has a 50 ms wall-clock budget; a 30 Hz tick has a
  33.3 ms budget. The measured prototype work is far below either budget, but
  real networking and replication could consume a substantial part of it.
- If a future replication implementation consumes more than the remaining
  tick budget, region work will need to be split, prioritized, or moved to
  separate workers. The current experiment cannot quantify that threshold.
- The 5,000-client requirement will likely need a connection/gateway benchmark
  separate from the region simulation benchmark because connected idle clients
  and active simulated entities have different costs.

## Limitations

- No real network connections or 5,000-client connection test.
- No outbound snapshots, delta compression, interest-management fanout, or
  bandwidth measurement.
- No concurrent persistence queue or database failure test.
- No cross-region, cross-layer, or instance handoff.
- No wall-clock sleeping or deadline-miss measurement.
- Simplified movement, combat, and NPC AI; the workload is not representative
  of final game rules.
- Single-threaded process with no contention or worker scheduling overhead.
- Desktop machine was not dedicated to the run.
- The benchmark reports one run per scenario; variance across repeated runs has
  not yet been characterized.

## Result

**Preliminary outcome: supports the direction, does not establish capacity.**

The experiment validates that Rust tooling is available and that a small,
standard-library-only fixed-tick region worker can process the intended
200-player scale with measurable headroom in this synthetic workload. It does
not validate the full realm target or the 200-player world-boss network
experience.

## Follow-up work

1. Add a separate simulated-client/gateway benchmark for 5,000 mostly idle
   connections.
2. Add replication preparation and serialized message-size measurements for
   200 players, NPCs, effects, and combat events.
3. Add fixed-tick deadline reporting at explicit 20 Hz and 30 Hz budgets.
4. Repeat each scenario multiple times and report variance on a quieter host.
5. Prototype a layer manager with group-cohesion and safe-point migration.
6. Add a persistence queue model that can be slowed or stopped without blocking
   the simulation owner.
