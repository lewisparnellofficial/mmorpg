# Authoritative server tick benchmark

This standalone benchmark measures the authoritative `mmorpg-core::World`
under a configurable role workload using the same 20 Hz, two-tick cast, and
two-tick cooldown configuration used by the development server. The default
profile has three players and four NPCs; pass `--players 200 --npcs 1000` for
the dense Milestone 6 workload. It includes target selection, deferred damage,
and tank threat commands over empty and active ticks.

It measures simulation-core work only. Socket polling, wire encoding,
replication, persistence, allocation outside the core, and operating-system
scheduling are excluded. The result is evidence for the local simulation
boundary, not a capacity claim for the complete server or for 200 players.

Run from the repository root:

```bash
cargo fmt --manifest-path experiments/server-tick-bench/Cargo.toml -- --check
cargo test --manifest-path experiments/server-tick-bench/Cargo.toml
cargo run --release --manifest-path experiments/server-tick-bench/Cargo.toml
cargo run --release --manifest-path experiments/server-tick-bench/Cargo.toml -- --players 200 --npcs 1000
```

The benchmark prints p50, p95, p99, and maximum tick duration in microseconds
alongside the fixed 12.5 ms three-client budget from `PLAN.md`.
