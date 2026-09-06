# Typed wire gameplay smoke test

This standalone tool validates the current typed client/server protocol over
real TCP. It connects to an already-running development server, verifies that
an unauthenticated legacy join is rejected, authenticates with the loopback-only
development token, verifies the character list, explicitly selects the
development character, verifies the typed bootstrap snapshot, buys from the
vendor, accepts the starter quest, defeats and loots all three field wolves,
and turns the quest in.

It is an integration smoke test, not a benchmark. It uses one client and one
development server process and does not measure capacity, latency, persistence,
production authentication, or production backpressure. The development auth
token is intentionally not a deployable credential mechanism.

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
