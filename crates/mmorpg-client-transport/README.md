# `mmorpg-client-transport`

This crate contains two small blocking TCP adapters. `DevelopmentConnection`
connects to the temporary line-oriented development server, sends typed
`ProtocolLine` values produced by `mmorpg-client-protocol`, and reads bounded
diagnostic lines. `WireConnection` uses the versioned `mmorpg-wire` envelope
and currently carries those same validated command/event lines as opaque
payloads, allowing framing and socket behavior to be exercised before the
binary application schema is finalized.

The wire bridge intentionally does not parse `WELCOME`, `EVENT`, `WORLD`,
`PLAYER`, or other server output into authoritative state. The payload is
returned to a higher protocol layer, which must decode it and feed validated
results into `mmorpg-client-model`.

The transport is blocking and must not run on the simulation or render thread
in a future client. It is suitable for local development adapters and
loopback tests only. It provides connection, read, write, and line-size
timeouts, but it does not provide authentication, encryption, reconnection,
backpressure queues, or production observability.

Run its focused tests from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client-transport/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-transport/Cargo.toml
```

The current wire bridge is still a transport prototype: the server does not
yet expose a binary listener, and its payload is not the final structured
command/event schema. Authentication, encryption, reconnection, queues, and
production observability remain future work.
