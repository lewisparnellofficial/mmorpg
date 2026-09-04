# EXP-003: Transparent overworld layer manager

**Status:** Complete as a local behavioral simulation; not a production implementation

**Date:** 2026-09-04

## Objective

Validate the behavioral seams of transparent dynamic overworld layering before implementing server code. The simulation models players arriving at a hotspot, atomic party placement, soft and hard layer capacities, safe-point migration, migration cooldowns, creation of new layers, and retirement of empty non-default layers.

This experiment is intentionally a layer-manager model, not a networked server or a performance benchmark.

## Requirements exercised

- The overworld remains represented as a logical set of layers rather than explicit dungeon instances.
- Players are assigned automatically; no player-facing layer selection is modeled.
- Groups remain atomic during both admission and migration.
- A soft capacity begins pressure-based rebalancing.
- A hard capacity prevents admission into an overloaded layer.
- Migration is allowed only when every member of a party is at a safe point.
- Recently migrated players receive a cooldown to reduce immediate layer churn.
- Empty non-default layers retire only after a grace period.
- Layer identity is carried on every simulated player.

## Hypothesis

A small deterministic manager can keep parties together and maintain the hard limit while absorbing a hotspot surge, provided that assignment and migration use explicit soft/hard budgets and safe-point rules. Safe-point restrictions should produce observable blocked migration rather than silently splitting groups or moving active players.

## Model and design

The simulation uses one logical realm and starts with default layer `0`. The manager creates additional overworld layers only when a complete arriving party would exceed the hard capacity of every active layer.

Admission policy:

1. Prefer the least-populated active layer where the whole party fits at or below the soft capacity (`160`).
2. If that is impossible, prefer the least-populated active layer where the whole party fits at or below the hard capacity (`200`). This intentionally permits temporary soft overflow.
3. If no active layer can accept the whole party under the hard capacity, create a new layer and assign the party atomically.

Rebalance policy:

1. Find layers above the soft capacity.
2. Enumerate distinct parties in deterministic arrival order.
3. Reject a party if any member is busy or still inside migration cooldown.
4. Select another active layer with room, preferring a destination that remains at or below soft capacity.
5. Move every member of a party in one ownership handoff.
6. Stop when the source reaches soft capacity or no eligible party can be moved.

Migration cooldown is `30` logical ticks in this experiment. It is a hysteresis mechanism to prevent a player that was just moved from being immediately moved back by the next balancing pass. A real implementation should use time-based and activity-aware thresholds rather than these arbitrary tick values.

Retirement policy:

- The default layer `0` is never retired.
- Any other active layer that becomes empty begins an empty grace timer.
- An empty layer retires after `3` logical ticks.

The manager asserts after every scenario tick that:

- No active player exists in more than one layer.
- Every active player is present in its pointed-to layer.
- No party has active members split between layers.
- Retired layers contain no players.
- No layer exceeds the hard capacity.

## Reproducible scenario

Logical ticks represent ordered simulation steps, not wall-clock seconds.

| Tick | Event |
|---:|---|
| 0 | Admit 15 parties of 8 and 30 solo players into the default layer. |
| 5 | Admit 12 parties of 5. The default layer reaches its hard capacity; a new layer is created for the remaining complete party groups. |
| 8 | Admit 20 solo players. |
| 10 | Mark parties 1 and 2, plus solo players 121–130, unsafe until tick 15. Rebalance the overloaded first layer. Safe parties move as units; unsafe parties are blocked. |
| 20 | Admit 40 solo players. |
| 30 | Admit 180 solo players to simulate a world-event surge. A third layer is created after the existing layers fill, then overloaded layers are rebalanced toward soft capacity. Recently migrated parties are held by cooldown. |
| 50 | All players in layer 2 leave. |
| 55 | All players in layer 1 leave. |
| 53 / 58 | Empty layers 2 and 1 retire after the three-tick grace period. |

The source is in [`experiments/layer-manager/src/main.rs`](../../experiments/layer-manager/src/main.rs), with a dependency-free Cargo manifest in the same directory.

## Commands run

```text
CARGO_TARGET_DIR=/home/lewis/Projects/mmorpg/experiments/layer-manager/target \
  cargo run --release --manifest-path experiments/layer-manager/Cargo.toml
```

The command was run locally on Linux with Rust/Cargo. The program emits deterministic event lines followed by a summary. The build completed successfully.

Final verification also ran:

```text
cargo fmt --manifest-path experiments/layer-manager/Cargo.toml -- --check

CARGO_TARGET_DIR=/home/lewis/Projects/mmorpg/experiments/layer-manager/target \
  cargo test --manifest-path experiments/layer-manager/Cargo.toml
```

Formatting passed. The test target passed with zero tests defined.

## Results

The run produced these summary metrics:

```text
config soft_capacity=160
config hard_capacity=200
config retirement_grace_ticks=3
config migration_cooldown_ticks=30
arrivals players=450 groups=297
peak total_players=450
peak active_layers=3
layers created=2
layers retired=2
migration group_attempts=212
migration group_completions=42
migration players=120
migration blocked_busy_groups=2
migration blocked_cooldown_groups=5
capacity soft_overflow_layer_ticks=5
capacity hard_overflow_layer_ticks=0
invariant_failures=0
layer id=0 active=true created_tick=0 final_population=160 max_population=200 arrivals=240 migrations_in=0 migrations_out=10
layer id=1 active=false created_tick=5 final_population=0 max_population=200 arrivals=160 migrations_in=5 migrations_out=32
layer id=2 active=false created_tick=30 final_population=0 max_population=130 arrivals=50 migrations_in=37 migrations_out=0
```

Interpretation:

- The scenario admitted all `450` players without exceeding the `200` hard capacity.
- Two additional layers were created during surges, for a peak of three active layers.
- The default layer temporarily reached the hard boundary, but no layer tick exceeded it. Five layer-ticks were above the soft threshold while a rebalance was pending.
- Forty-two complete party migrations moved `120` players; no party was split.
- Two party migration attempts were blocked by active/busy state.
- Five migration attempts were blocked by the cooldown, demonstrating hysteresis and the cost of delaying churn.
- Both dynamic layers retired after their populations drained and their grace timers elapsed.
- The invariant checks reported zero failures.

The full event trace is intentionally generated by the executable rather than duplicated here, so the metrics can be regenerated after changing the scenario.

## Simulated behavior

The experiment simulates:

- Automatic admission into the least-loaded eligible layer.
- Whole-party admission, including a party that triggers new-layer creation.
- Soft capacity (`160`) and hard capacity (`200`).
- Safe-point eligibility through a synthetic busy-until tick.
- Atomic whole-party migration.
- Migration cooldown/hysteresis.
- Destination selection and layer creation when migration needs room.
- Layer population and peak metrics.
- Layer creation and empty-layer retirement.
- Deterministic ownership and group-cohesion invariants.
- Departure-driven draining of layers.

## Omitted behavior

This is not evidence that the eventual Rust server can support the target load. It omits:

- Network connections, packet ordering, loss, retransmission, or bandwidth.
- Client visibility and interest management.
- Region/cell ownership inside a layer.
- Real movement, combat, casts, threat, area effects, NPC AI, or world simulation.
- Database persistence, journaling, snapshots, reward idempotency, and crash recovery.
- Worker health, process placement, worker failure, and handoff recovery.
- Cross-layer chat, friends, group search, trading, or social visibility.
- Instance creation and instance-specific lifecycle.
- Loading screens, asset streaming, client presentation, and player perception.
- Real wall-clock safe points and cancellation of gameplay operations.
- Dynamic layer splitting and merging based on CPU, AI, or replication pressure.
- Multiple simultaneous hotspots.
- A canonical world-boss coordinator or reward service.
- Adversarial clients and exploit attempts.

## World-boss policy implications

The experiment validates the mechanics needed to make a second overworld layer available, but it does not choose what happens when more than `200` players want the same world-boss.

The current scenario's layer manager could technically place additional arrivals in another layer, but that would create multiple encounter contexts if the boss is instantiated independently per layer. That is not automatically acceptable for a persistent world event. A future world-boss implementation must choose and enforce one of these policies:

1. **Canonical encounter:** one layer owns the boss and admits at most 200 participants; excess players wait, remain outside, or are assigned according to an explicit queue/admission rule.
2. **Event replicas:** multiple layer-local encounters share one realm-wide event ID, with centrally enforced once-per-event reward eligibility and an explicit decision about shared versus independent progress.
3. **Hybrid event policy:** each event definition chooses canonical or replicated behavior based on its intended social and technical properties.

The present experiment recommends not silently treating a crowded world-boss as an ordinary layer-placement problem. The world-event coordinator, encounter membership, reward ledger, and failure recovery need a separate design and experiment.

## Findings

1. A whole-party admission unit is sufficient to avoid group splitting in the modeled case.
2. A soft/hard distinction permits controlled temporary overflow while retaining a strict safety boundary.
3. Safe-point migration creates real pressure: the manager must either wait, find other eligible groups, or create capacity elsewhere. It cannot promise immediate rebalance.
4. Migration cooldown is valuable. Without a hysteresis mechanism, a later balancing pass could immediately move recently migrated players back and create visible churn.
5. Empty-layer retirement is simple only after all layer-local participants have drained. A production system needs a drain state and must account for reconnects, group invitations, and delayed worker messages.
6. The experiment is small enough to run in a single process, which supports a one-machine development mode while preserving future process boundaries.

## Recommended follow-up

- Add `EXP-004` for a networked or in-process simulated 200-player world-boss workload; measure tick time, replication volume, and client update pressure.
- Define a production `LayerAssignment`/`LayerMigration` state machine, including source fencing, destination acknowledgement, duplicate suppression, and rollback after worker failure.
- Decide whether the layer manager may migrate players who are merely nearby versus players participating in an encounter. Encounter membership should probably pin players until the activity ends.
- Add a layer drain state, admission freeze, and reconnect handling before implementing retirement in the server.
- Specify group and raid cohesion rules for groups larger than one party.
- Design the canonical world-event and reward ledger before allowing replicated world-boss encounters.
- Revisit the `160` soft threshold, `200` hard threshold, and `30`-tick cooldown after measuring real simulation and replication costs.
