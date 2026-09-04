# Replication workload model

`model.py` is a deterministic, dependency-free workload model for the dense
200-player activity requirement. It is intentionally not a network stack and
does not open sockets, encode a production protocol, or claim server
performance.

Run the default ten-second model from the repository root:

```bash
python3 experiments/replication-model/model.py
```

Useful parameters include:

```bash
python3 experiments/replication-model/model.py \
  --tick-hz 30 \
  --budget-kib 96 \
  --players-hz 10 \
  --combat-hz 20 \
  --effects-hz 10 \
  --dynamic-hz 5 \
  --ambient-hz 2.5
```

The output compares three profiles:

- `full_all_visible_20hz`: every visible entity set sends a full state at
  20 Hz; this is a deliberately naive upper-bound baseline.
- `delta_all_visible_20hz`: every visible entity set has a 20 Hz send
  opportunity, but only changed entities send compact deltas; this isolates
  dirty-state and delta-payload effects.
- `prioritized_delta_*`: configurable update frequencies, dirty deltas, and a
  per-client per-second byte budget. Higher-priority entity sets are admitted
  first using compact priority buckets.

The model uses deterministic aggregate dirty sampling. It reports modeled
candidate checks, dirty checks, changed/admitted/dropped updates, payload
bytes, and a normalized CPU-work proxy. The reported `model_runtime_ms` is
only the local Python model's wall time; it is not an estimate of a Rust server
tick time.
