# EXP-002: Replication workload model

**Status:** Complete as a workload model; real implementation benchmark still required

**Date:** 2026-09-04

**Owner:** Networking/replication validation worker

## Objective

Model the outbound replication pressure created by approximately 200 players
participating in one shared overworld activity. The model compares full state
updates, changed-only deltas, and prioritized updates under a per-client
bandwidth budget. It also exposes update frequencies so that the effect of
throttling background entities can be explored reproducibly.

This experiment intentionally does not implement a network transport, Rust
region worker, production serializer, client, or server. It is a compact
workload calculator intended to establish useful counts and identify the next
measurements that a real benchmark must make.

## Hypothesis

1. Sending compact deltas for changed entities should reduce application
   payload substantially compared with sending full state for every visible
   entity.
2. Spatially scoped nearby sets and update frequencies should keep a dense
   encounter within a configurable per-client budget.
3. A simple priority scheduler should preserve high-value combat state, but
   strict priority alone may starve lower-priority visual state and therefore
   needs fairness or reserved budgets in a real implementation.

## Workload and assumptions

The modeled activity has 200 recipients. Every recipient has the following
nearby entity set:

| Entity set | Entities per recipient | Priority | Full bytes | Delta bytes | Dirty rate | Default Hz |
|---|---:|---:|---:|---:|---:|---:|
| Other players | 199 | 100 | 96 | 28 | 65% | 20 |
| Boss | 1 | 110 | 96 | 32 | 95% | 20 |
| Encounter adds | 40 | 90 | 80 | 24 | 75% | 20 |
| Area effects | 80 | 75 | 32 | 12 | 80% | 20 |
| Dynamic objects | 25 | 50 | 48 | 14 | 50% | 10 |
| Ambient NPCs | 20 | 20 | 64 | 18 | 15% | 5 |
| **Total** | **365** |  |  |  |  |  |

The set is deliberately dense: every player can see the other 199 players,
the boss, its adds, area effects, dynamic objects, and a small ambient set.
It represents one encounter interest set, not an entire zone broadcast to all
players.

The model uses these additional assumptions:

- 20 Hz is the baseline simulation and all-visible comparison rate.
- Full and delta comparison profiles give every set a 20 Hz opportunity to
  isolate encoding and dirty-state effects.
- The prioritized profile uses the configurable default rates in the table.
- A delta is emitted only when the entity is modeled as changed during its
  update opportunity.
- Dirty rates are deterministic workload assumptions. Fractional expected
  counts are distributed reproducibly across clients and ticks; individual
  entity dirty state is not simulated.
- A prioritized profile has a per-client aggregate byte budget. Its current
  implementation divides that budget evenly across ticks and admits higher
  priority buckets first. There is no token-bucket carryover or fairness
  reservation.
- Payload sizes are application-state payload estimates. They exclude packet
  headers, entity IDs unless included in the assumed size, framing, sequence
  numbers, encryption overhead, compression, ACKs, retransmission, spawn and
  removal messages, and transport padding.
- The model counts server-to-client payload only. Client-to-server input,
  chat, durable commands, and database traffic are out of scope.

## Implementation

The reproducible model is in
[`experiments/replication-model/model.py`](../../experiments/replication-model/model.py).
It uses only the Python standard library and a deterministic integer mixer.
The experiment does not modify production code.

The model reports:

- Candidate checks: visible-entity candidates considered for each recipient
  on each model tick.
- Due opportunities: entity slots whose configured frequency schedules an
  update opportunity.
- Changed updates: due opportunities that produce a full state or delta under
  the selected mode.
- Admitted and dropped updates: changed updates retained or rejected by the
  optional per-client budget.
- Payload bytes: the modeled payload for admitted updates.
- Estimated CPU work units: a normalized proxy composed of candidate checks,
  dirty checks, admitted-update work, payload-size work, and priority-bucket
  scheduling work. It is not CPU time and cannot be compared to a production
  server's milliseconds.
- Model runtime: wall-clock time for this Python calculator on the local host;
  it is reported only to expose model cost and is not a server-performance
  result.

## Commands run

Environment:

```text
Linux lewis-desktop 7.2.0-1 SMP PREEMPT_DYNAMIC x86_64
CachyOS
Python 3.14.7
```

Syntax validation and the main run:

```bash
python3 -m py_compile experiments/replication-model/model.py
python3 experiments/replication-model/model.py
```

The main run uses the defaults: 200 players, 10 seconds, 20 Hz, deterministic
seed 7, and a 64 KiB/s/client budget for the prioritized profile.

Additional sensitivity runs:

```bash
python3 experiments/replication-model/model.py --tick-hz 30 --seconds 10 --budget-kib 64
python3 experiments/replication-model/model.py --seconds 5 --budget-kib 96
python3 experiments/replication-model/model.py --seconds 5 --budget-kib 128
python3 experiments/replication-model/model.py \
  --players-hz 10 --combat-hz 20 --effects-hz 10 \
  --dynamic-hz 5 --ambient-hz 2.5 --seconds 10 --budget-kib 64
python3 experiments/replication-model/model.py --seconds 3 --budget-kib 32
```

Reproducibility was checked by running the JSON output twice and comparing all
counts and configuration fields after removing the intentionally variable
`model_runtime_ms` field. The comparison returned:

```text
reproducible_counts= True
```

## Main results: 200 players, 20 Hz, 10 seconds

These are modeled application payloads for one 200-recipient activity.

| Profile | Changed updates/s | Admitted updates/s | Dropped updates/s | Drop rate | Payload/client/s | Aggregate payload/s |
|---|---:|---:|---:|---:|---:|---:|
| Full state, all visible at 20 Hz | 1,460,000 | 1,460,000 | 0 | 0.0% | 535.9 KiB | 104.68 MiB |
| Delta, all visible at 20 Hz | 959,192 | 959,192 | 0 | 0.0% | 104.9 KiB | 20.48 MiB |
| Prioritized delta, default rates, 64 KiB/s/client | 925,186 | 468,000 | 457,186 | 49.4% | 64.0 KiB | 12.50 MiB |

The delta profile reduces the modeled payload by approximately 80.4% versus
the full-state baseline (20.48 MiB/s versus 104.68 MiB/s). This is a model
result based on the assumed field sizes and dirty rates, not a measured result
from a wire encoder.

The 64 KiB/s prioritized profile reaches its configured client budget, but it
drops nearly half of changed updates. With the current strict priority order,
the boss is retained while lower-priority entity sets are heavily or entirely
starved:

| Entity set | Changed/s | Admitted/s | Dropped/s | Admitted MiB/s |
|---|---:|---:|---:|---:|
| Other players | 517,390 | 460,416 | 56,975 | 12.294 |
| Boss | 3,792 | 3,792 | 0 | 0.116 |
| Encounter adds | 120,000 | 3,792 | 116,208 | 0.087 |
| Area effects | 256,000 | 0 | 256,000 | 0.000 |
| Dynamic objects | 25,004 | 0 | 25,004 | 0.000 |
| Ambient NPCs | 3,000 | 0 | 3,000 | 0.000 |

This is useful evidence against using a single strict priority queue as the
final scheduler. The real design needs at least reserved capacity, weighted
fairness, age-based promotion, or a policy that treats combat-critical state
as events rather than repeatedly competing snapshots.

The full baseline considers 365 candidates per recipient per tick:

```text
200 recipients × 365 candidates × 20 ticks/s = 1,460,000 candidate checks/s
```

Across the ten-second run, the model performed 14,600,000 candidate checks.
That is a workload count, not evidence that an optimized spatial index would
perform that many equivalent operations: a production replication graph should
avoid rebuilding the same candidate set unnecessarily.

## Sensitivity results

### 30 Hz model tick

The 30 Hz run kept the entity update frequencies at their configured values,
so the full and unbudgeted delta payloads remained approximately 104.68 MiB/s
and 20.48 MiB/s. Candidate checks rose from 14.6 million to 21.9 million in
the ten-second run because the model reevaluates the visible set every tick.

The prioritized 64 KiB/s/client profile admitted 312,000 updates/s and emitted
8.33 MiB/s aggregate, or 42.7 KiB/s/client. This is below the nominal budget
because the model divides the budget among 30 ticks while update opportunities
occur at 20 Hz; unused budget on non-opportunity ticks is discarded. A real
scheduler should use a token bucket or another smoothing mechanism rather than
assuming one independent hard budget per simulation tick.

### 96 and 128 KiB/s/client budgets

At 20 Hz and the default frequency profile:

| Budget | Admitted updates/s | Dropped updates/s | Drop rate | Payload/client/s | Aggregate payload/s |
|---:|---:|---:|---:|---:|---:|
| 96 KiB/s/client | 819,026 | 106,163 | 11.5% | 95.8 KiB | 18.71 MiB |
| 128 KiB/s/client | 925,189 | 0 | 0.0% | 102.4 KiB | 19.99 MiB |

The 128 KiB/s budget does not produce 128 KiB/s because the modeled changed
workload is only approximately 104.9 KiB/s/client. The 96 KiB budget still
starves dynamic objects and ambient state after higher-priority buckets are
served.

### Reduced background frequencies

The run with player updates at 10 Hz, combat at 20 Hz, area effects at 10 Hz,
dynamic objects at 5 Hz, and ambient NPCs at 2.5 Hz reduced the prioritized
changed workload to 524,488 updates/s. Under the same 64 KiB/s/client strict
budget it admitted 295,897 updates/s, dropped 228,591 updates/s, and emitted
7.68 MiB/s aggregate (39.3 KiB/s/client).

This shows that frequency throttling reduces offered work, but the current
priority policy can still waste budget or starve classes because of per-tick
granularity and item-size/order effects. Frequency selection must be evaluated
together with a fair scheduler and a real client presentation policy.

## Findings

1. **Delta replication is the strongest immediate lever.** Under the assumed
   payloads and dirty rates, changed-only deltas reduce application payload by
   about 80% without dropping modeled updates.
2. **The dense activity is large even before transport overhead.** A naive
   full-state approach would emit approximately 104.68 MiB/s of application
   payload to the 200 recipients, while an unbudgeted delta approach would
   emit approximately 20.48 MiB/s.
3. **A per-client budget is necessary but strict priority is insufficient.**
   The 64 KiB/s profile preserves the boss but drops most adds and all area
   effects, dynamic objects, and ambient NPC updates.
4. **Update budgets need smoothing.** The 30 Hz sensitivity run underused its
   nominal budget because 20 Hz update opportunities do not occur on every 30
   Hz tick. Token-bucket accounting or a rolling budget is a better next
   policy to test.
5. **Candidate generation is a distinct CPU problem.** The model performs
   14.6 million candidate checks in ten seconds at 20 Hz, before actual
   serialization. A real benchmark must compare a spatial index, cached
   interest sets, and naive scanning under movement churn.
6. **This does not validate 5,000 connections.** The model validates only one
   200-recipient dense activity. It has no sockets, connection memory, idle
   heartbeat, gateway, packet loss, retransmission, or instance workload.

## Limitations

- No real network transport or packet loss.
- No Rust implementation, allocator behavior, async scheduling, or thread
  contention.
- No actual binary serialization, compression, encryption, MTU handling, or
  packet coalescing.
- No client decode cost, frame time, interpolation, or rendering cost.
- No entity spawn/removal, baseline management, acknowledgement, or recovery
  traffic.
- No visibility changes caused by movement; each recipient keeps the same 365
  candidate set for the whole run.
- Dirty-state sampling is aggregate and deterministic rather than per-entity.
- CPU work units are a relative proxy with arbitrary weights.
- The model does not include the 5,000 mostly idle connected clients or the
  traffic and memory cost of dungeon, raid, and battleground instances.
- The model does not decide whether a world boss is canonical or replicated
  across transparent layers.

## Result

The hypothesis is partially supported. Delta state and frequency tiers look
promising as requirements for the eventual protocol, but a basic priority
budget is not an acceptable final replication policy. The results justify a
real Rust benchmark with a dense 200-player activity and a packetized
replication path.

This experiment does not accept a transport, tick rate, or production byte
budget. It narrows the next implementation questions:

- How much wire overhead does the actual schema add?
- Can a region worker generate and serialize the dense interest set within the
  tick budget?
- What scheduler preserves boss, player, add, and area-effect state under
  congestion?
- What does the client do when low-priority visual state is intentionally
  stale?

## Recommended next benchmark

Implement a headless Rust benchmark, without connecting it to production game
code yet, with these stages:

1. Generate 200 simulated players, one boss, 40 adds, 80 area effects, 25
   dynamic objects, and 20 ambient NPCs.
2. Implement the same entity fields and full/delta sizes as this model, then
   measure actual serialization and allocation behavior.
3. Compare a naive scan with a spatially indexed interest set under movement
   churn.
4. Add a token-bucket or rolling per-client budget and compare it with strict
   priority, weighted fairness, and reserved combat-critical capacity.
5. Measure 20 and 30 Hz region ticks, per-client bytes, aggregate egress,
   p50/p95/p99 tick time, serialization time, allocation rate, and memory.
6. Add packet loss and delayed delivery to test state freshness and whether
   replacement snapshots avoid head-of-line blocking.
7. Add a separate gateway load test for 5,000 mostly idle connections; do not
   infer this capacity from the 200-player encounter result.

The benchmark should report both offered bytes and admitted wire bytes, and it
should retain the workload seed and configuration in every result so that
changes can be compared across implementations.

## Final closure run

To finish this experiment without expanding scope, the smallest configured
scenario that still exercises the required 200-player activity was run: 200
players, one second, 20 Hz, seed 7, with the default entity sets and a 64
KiB/s/client prioritized budget.

Command:

```bash
python3 -m py_compile experiments/replication-model/model.py
python3 experiments/replication-model/model.py \
  --players 200 --seconds 1 --tick-hz 20 --budget-kib 64
```

The run completed successfully. It performed 1,460,000 candidate checks and
produced:

| Profile | Changed updates/s | Admitted updates/s | Dropped updates/s | Drop rate | Payload/client/s | Aggregate payload/s |
|---|---:|---:|---:|---:|---:|---:|
| Full state, all visible at 20 Hz | 1,460,000 | 1,460,000 | 0 | 0.0% | 535.9 KiB | 104.68 MiB |
| Delta, all visible at 20 Hz | 959,265 | 959,265 | 0 | 0.0% | 104.9 KiB | 20.48 MiB |
| Prioritized delta, default rates, 64 KiB/s/client | 925,231 | 468,000 | 457,231 | 49.4% | 64.0 KiB | 12.50 MiB |

These final values reinforce the earlier model findings: changed-only deltas
reduce the modeled payload substantially, while strict priority under a hard
budget preserves the boss but starves lower-priority encounter state. They are
still estimates from the workload model, not measurements of a real network or
Rust server.
