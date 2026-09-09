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
`mmorpg-core::World` with role players and the four starter-zone NPCs. Each
tick exercises target selection for every player, periodic damage with the
two-tick cast/two-tick cooldown policy, and periodic tank taunt. The default
three-player profile runs 1,000 warmup ticks followed by 10,000 measured
ticks. A second profile runs 200 players, 1,000 warmup ticks, and 10,000
measured ticks; both report p50, p95, p99, and maximum durations.

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
cargo run --quiet --release --manifest-path experiments/server-tick-bench/Cargo.toml -- --players 200
```

## Environment

- Host: `lewis-desktop`
- Kernel: `Linux 7.2.2-1-cachyos x86_64`, PREEMPT_DYNAMIC
- CPU: AMD Ryzen 7 2700X Eight-Core Processor, 16 logical CPUs
- Rust release profile: one benchmark binary, thin LTO, one codegen unit,
  stripped symbols
- Commit: `30462c0`

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

The 200-player profile produced this separate run:

```text
configuration.players=200
configuration.npcs=4
configuration.tick_hz=20
configuration.cast_time_ticks=2
configuration.cooldown_ticks=2
configuration.warmup_ticks=1000
configuration.measured_ticks=10000
measurement.tick_avg_us=11.591
measurement.tick_p50_us=12.123
measurement.tick_p95_us=13.255
measurement.tick_p99_us=15.720
measurement.tick_max_us=77.156
measurement.tick_budget_us=12500.000
measurement.checksum=64007484
```

The 200-player p99 also passes the fixed simulation-core budget. This profile
uses only four starter NPCs and does not model a world-boss encounter,
replication fanout, or complete server work; it narrows the uncertainty but
does not close the production capacity gate.

## Interpretation and limitations

- **Measured:** authoritative core step durations and the fixed workload's
  event checksum (`60067484`).
- **Project-specific inference:** the current three-role core workload has
  substantial headroom before the 20 Hz simulation budget on this host.
- **Not measured:** complete server tick duration with network intake,
  serialization, delivery, or checkpoint enqueueing. The 200-player profile
  is measured below, but it still excludes those complete-server concerns.
- The benchmark does not establish a production capacity claim or validate
  multi-region scheduling, replication budgets, or database behavior.

The next capacity experiment should expand this same authoritative benchmark
to the representative world-boss encounter and include the complete server
owner boundary before using the result to accept a production scheduling ADR.
