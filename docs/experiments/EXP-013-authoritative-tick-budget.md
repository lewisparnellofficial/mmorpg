# EXP-013: authoritative three-client tick budget

**Status:** Local simulation-core measurement; full-server capacity remains
unproven

**Date:** 2026-09-09

## Objective

Measure the authoritative simulation work for the first three-role shared-zone
slice under the fixed-tick combat policy from Milestone 6. The fixed gate is a
p99 simulation-work duration no greater than 12.5 ms for a 20 Hz tick.

## Workload and method

The standalone `experiments/server-tick-bench` executable constructs one
`mmorpg-core::World` with three players (tank, healer, and damage dealer) and
the four starter-zone NPCs. Each tick exercises target selection for all three
players, periodic damage with the two-tick cast/two-tick cooldown policy, and
periodic tank taunt. It runs 1,000 warmup ticks followed by 10,000 measured
ticks and reports p50, p95, p99, and maximum durations.

The benchmark measures the authoritative core step only. It excludes socket
polling, command decoding, wire encoding, outbound replication, persistence,
checkpoint workers, and operating-system scheduling. It is therefore a
simulation-boundary measurement, not evidence for 200-player or 5,000-client
capacity.

## Reproduction

```bash
cargo fmt --manifest-path experiments/server-tick-bench/Cargo.toml -- --check
cargo test --manifest-path experiments/server-tick-bench/Cargo.toml
cargo run --quiet --release --manifest-path experiments/server-tick-bench/Cargo.toml
```

## Environment

- Host: `lewis-desktop`
- Kernel: `Linux 7.2.2-1-cachyos x86_64`, PREEMPT_DYNAMIC
- CPU: AMD Ryzen 7 2700X Eight-Core Processor, 16 logical CPUs
- Rust release profile: one benchmark binary, thin LTO, one codegen unit,
  stripped symbols
- Commit: `23c6be7`

## Measured result

Run output:

```text
configuration.players=3
configuration.npcs=4
configuration.tick_hz=20
configuration.cast_time_ticks=2
configuration.cooldown_ticks=2
configuration.warmup_ticks=1000
configuration.measured_ticks=10000
measurement.tick_avg_us=0.343
measurement.tick_p50_us=0.321
measurement.tick_p95_us=0.461
measurement.tick_p99_us=0.541
measurement.tick_max_us=17.303
measurement.tick_budget_us=12500.000
```

The p99 result is `0.541 µs`, below the fixed `12,500 µs` budget. The maximum
observed sample was `17.303 µs`. This local run passes the stated simulation
core gate by a wide margin. Repeated local runs can vary slightly because the
benchmark does not pin a CPU or suppress operating-system scheduling.

## Interpretation and limitations

- **Measured:** authoritative core step durations and the fixed workload's
  event checksum (`60067484`).
- **Project-specific inference:** the current three-role core workload has
  substantial headroom before the 20 Hz simulation budget on this host.
- **Not measured:** complete server tick duration with network intake,
  serialization, delivery, checkpoint enqueueing, or 200 active players.
- The benchmark does not establish a production capacity claim or validate
  multi-region scheduling, replication budgets, or database behavior.

The next capacity experiment should expand this same authoritative benchmark
to the representative 200-player encounter and include the complete server
owner boundary before using the result to accept a production scheduling ADR.
