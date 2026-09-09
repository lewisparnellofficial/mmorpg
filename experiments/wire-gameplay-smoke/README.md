# Typed wire gameplay smoke test

This standalone tool validates the current typed client/server protocol over
real TCP. It connects to an already-running development server, verifies that
an unauthenticated legacy join is rejected, authenticates with the loopback-only
development token, verifies the character list, explicitly selects the
development character, verifies the typed bootstrap snapshot, buys from the
vendor, accepts the starter quest, defeats and loots all three field wolves,
and turns the quest in.

Before entering the world it computes the canonical compiled-catalog digest,
sends `ContentDigest`, and requires `ContentAccepted`; a mismatched catalog is
therefore rejected before the character is bound to the simulation.

It is an integration smoke test, not a benchmark. It uses one client and one
development server process and does not measure capacity, latency, persistence,
production authentication, or production backpressure. The development auth
token is intentionally not a deployable credential mechanism.

The purchase, loot, and quest-turn-in intents use the additive retryable
command wrapper with explicit operation IDs. The development server caches up
to 256 completed results per process and returns a cached result instead of
reapplying a duplicate operation. The cache is intentionally not a durable
cross-restart journal yet.

The crate also provides a deterministic three-role shared-zone gate. It opens
three typed connections for the tank, healer, and damage characters, verifies
that each receives only its own detailed player snapshot, forms a two-member
party, and verifies that only the party members receive the private party
summary:

```bash
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml \
  --bin three-client-gate -- 127.0.0.1:4001
```

The three-client gate is a headless protocol/authority check, not the graphical
Milestone 13 acceptance run. It now exercises the town purchase and quest
acceptance path, tank target/taunt authority, healer recovery, damage-role
defeat and retryable loot, quest completion/turn-in, and two respawn
generations. It still does not replace the graphical run or cover process
restart, slow-client network behavior, or repeated healer recovery across
multiple encounter generations.

The bounded slow-client gate uses two real TCP sessions. One client floods
snapshot requests without reading responses while the other must remain able
to receive a snapshot; the wrapper also requires the server's structured
output-saturation record:

```bash
./scripts/smoke-slow-client.sh
```

This proves local queue bounding and failure isolation, not production network
backpressure or capacity.

## Run

Start the server with its optional wire listener:

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4000 \
  --wire-address 127.0.0.1:4001
```

Then run the smoke test from the repository root:

```bash
cargo fmt --manifest-path experiments/wire-gameplay-smoke/Cargo.toml -- --check
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml
```

Pass another wire address as the first argument when needed:

```bash
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml -- \
  127.0.0.1:4401
```

Expected output is similar to:

```text
wire gameplay smoke: success (player=5, enemies=3, vendor_purchase=1, quest=1)
```

## Restart persistence smoke

Run the normal gameplay smoke against a server started with `--character-store`,
stop that server, start a fresh server with the same checkpoint path, and run:

```bash
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml -- \
  127.0.0.1:4401 --expect-restored
```

This second mode only enters the selected character and verifies that the
starter quest reward, purchased ration, and rewarded quest state survived the
restart.

The repository-root wrapper repeats both halves using an isolated temporary
checkpoint directory, then removes that directory:

```bash
./scripts/smoke-restart-persistence.sh
```

It accepts optional line and wire addresses as its first and second arguments.
