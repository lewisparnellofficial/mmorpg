# `mmorpg-client-transport`

This crate contains blocking TCP adapters for local tools. `WireConnection`
uses the versioned `mmorpg-wire` envelope and provides typed command and
server-message operations, including compatibility decoding for the retained
fixture profile. The live server's primary listener is typed wire; it no
longer exposes the former line gameplay listener.

`DevelopmentConnection`, line-command helpers, and `read_event_payload` are
retained only as compatibility APIs for fixture consumers. They do not
represent a live gameplay path and cannot mutate the server's authoritative
world.

The line bridge intentionally does not parse `WELCOME`, `EVENT`, `WORLD`,
`PLAYER`, or other server output into authoritative state. The wire bridge
decodes stable IDs and typed fields, but the payload is still only a transport
result; callers must apply it through their own presentation or
authoritative-state boundary.

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

The current wire bridge is still a transport prototype. Production
authentication, encryption, session resume, interest-managed replication, and
production observability remain future work; typed development authentication,
bounded queues, and reconnect-by-new-session are implemented local behavior.
