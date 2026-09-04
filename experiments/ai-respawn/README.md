# Starter-zone AI and respawn experiment

This standalone Rust experiment models the fixed-tick behavior of starter-zone
enemies. It is deliberately independent of the production workspace so that
the behavior can be measured and changed without coupling the experiment to
the authoritative server implementation.

The model covers:

- deterministic waypoint patrol;
- nearest-player aggro with deterministic player-ID tie breaking;
- target pursuit while the enemy remains inside its home leash;
- leash/evade movement back to the spawn point;
- damage, death, and a scheduled respawn; and
- a configurable simulation tick rate and respawn delay.

The model has no networking, persistence, collision system, navigation mesh,
combat resolution, timers, threads, or random number generator. A player
observation is supplied directly to the simulation for each tick. Consequently
this is a behavior and scheduling model, not production server code and not a
capacity result for the MMORPG.

## Run

From this directory:

```bash
cargo fmt -- --check
cargo test
cargo run --quiet
```

The report prints the fixed-tick event sequence for one enemy as it patrols,
acquires a target, leashes, dies, and respawns.

## Model notes

The simulation consumes observations in ascending player-ID order and always
selects the closest eligible player, breaking equal-distance ties with the
lowest ID. There is no random patrol choice or random respawn variance.

Respawn delays are converted to whole ticks by rounding up, so an enemy never
respawns before the configured wall-clock delay. `Position` uses integer world
units, and movement is a fixed number of units per simulation tick to avoid
introducing a frame-time dependency into this experiment.
