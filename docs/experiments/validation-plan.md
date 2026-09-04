# Local Validation Plan: First Architecture Pass

**Status:** First pass complete; next pass defined

**Started:** 2026-09-04

## Purpose

Validate the riskiest assumptions in the proposed server architecture before selecting production dependencies or implementing the full game runtime.

The first pass focuses on three coupled concerns:

1. Fixed-tick Rust region simulation.
2. Replication workload for 200 players in one activity.
3. Transparent layer assignment during a hotspot.

These are microbenchmarks and behavioral models, not production server components. Their purpose is to expose scaling relationships, ownership mistakes, and missing requirements early.

## Environment baseline

The initial run environment is:

- Linux workspace.
- Rust `1.96.0`.
- Cargo `1.96.0`.
- Python `3.14.7`.
- Repository: `/home/lewis/Projects/mmorpg`.

Each experiment must record its actual environment when it runs. Results from a microbenchmark should not be interpreted as a capacity guarantee for the eventual server.

## Experiment matrix

| ID | Experiment | Primary question | Assigned output |
|---|---|---|---|
| [EXP-001](EXP-001-rust-region-worker.md) | Rust region worker | Can a fixed-tick, single-owner region process representative queued work with stable tick time? | `rust-region-bench/` and `EXP-001-rust-region-worker.md` |
| [EXP-002](EXP-002-replication-workload.md) | Replication workload | How do candidate counts, update rates, and prioritization affect a 200-player encounter? | `replication-model/` and `EXP-002-replication-workload.md` |
| [EXP-003](EXP-003-layer-manager.md) | Layer manager | Can transparent layers preserve group cohesion and safe migration during a hotspot? | `layer-manager/` and `EXP-003-layer-manager.md` |

## First-pass findings

### EXP-001: region simulation

The synthetic single-owner Rust region worker remained below 1 ms at p99 for the tested 200-player/1,000-NPC workload in release mode. The result supports keeping the ownership model as a viable direction, but it excludes network I/O, replication, persistence, real combat, and worker coordination.

Queue pressure produced unbounded backlog when commands arrived faster than the worker's processing budget. A production worker therefore needs admission control, input coalescing for stale movement, prioritization, and overload metrics.

### EXP-002: replication

The deterministic workload model estimated approximately 104.68 MiB/s of application payload for full-state updates and 20.48 MiB/s for changed-only deltas in the modeled 200-player encounter. These are model results based on assumed payloads and dirty rates, not wire measurements.

A strict 64 KiB/s/client priority budget preserved the boss but dropped approximately 49.4% of changed updates, starving lower-priority effects and ambient state. The next scheduler should test reserved combat capacity, weighted fairness, age-based promotion, and token-bucket smoothing.

### EXP-003: layering

The behavioral layer simulation admitted 450 players across three layers without exceeding the hard capacity of 200, preserved all modeled party cohesion, moved 120 players through safe migrations, retired two drained layers, and reported zero invariant failures.

This validates assignment and migration invariants only. It does not validate real worker fencing, network handoff, persistence, combat, or world-boss semantics.

## Next validation pass

The next experiment should combine the first two models in Rust:

- Actual binary serialization.
- A spatial interest index.
- A fair/token-bucket replication scheduler.
- A fixed-tick region worker.
- A 200-player world-boss workload.
- Separate measurement of simulation, candidate generation, serialization, and outbound queueing.

A separate gateway test should then address 5,000 mostly idle connections. The full 5,000-client requirement must not be inferred from the 200-player region result.

## Shared workload assumptions

The experiments should use explicit inputs rather than hidden constants. At minimum, support:

- 200 active players in one activity.
- Configurable NPC/add counts.
- Configurable update frequencies.
- Configurable party sizes.
- A hotspot arrival curve.
- Soft and hard layer capacity thresholds.
- Safe-point and in-combat player states.

The 5,000-client requirement is a separate connection-capacity scenario. It should not be inferred from the 200-player simulation result.

## Initial success criteria

The first pass succeeds if it produces:

- Reproducible commands.
- Measured output or an explicitly documented reason measurement was impossible.
- Clear separation of simulation, replication, and assignment work.
- No hidden assumption that turns multiple layers into a substitute for the 200-player single-activity test.
- A list of parameters that dominate cost.
- Follow-up experiments with concrete inputs.

The pass does not yet establish:

- Production hardware capacity.
- Final transport choice.
- Final server tick rate.
- Final layer policy above 200 participants.
- Final database schema.

## Review procedure

After the three workers finish:

1. Verify that each experiment is reproducible from a clean checkout.
2. Run the commands locally on the main branch.
3. Compare assumptions and units across the three records.
4. Identify contradictions or missing measurements.
5. Update `docs/open-questions.md`.
6. Create proposed ADRs only for decisions supported by evidence.
7. Design the next benchmark round around the largest remaining uncertainty.

## Result status vocabulary

- **Measured** — produced by a local run and includes environment/configuration.
- **Modeled** — produced by a simplified model; useful for relationships, not capacity claims.
- **Blocked** — could not run; the blocker is recorded.
- **Follow-up required** — result is informative but insufficient for a decision.
