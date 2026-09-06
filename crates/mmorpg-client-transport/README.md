# `mmorpg-client-transport`

This crate contains two small blocking TCP adapters. `DevelopmentConnection`
connects to the temporary line-oriented development server, sends typed
`ProtocolLine` values produced by `mmorpg-client-protocol`, and reads bounded
diagnostic lines. `WireConnection` uses the versioned `mmorpg-wire` envelope
and provides both a compatibility method for carrying those validated command
lines and `send_typed_command` for the first structured client-command schema.
`read_server_message` now decodes the structured server-event and snapshot
schema. `read_event_payload` remains as a compatibility method for temporary
diagnostic text.

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

The current wire bridge is still a transport prototype. Authentication,
encryption, reconnection, queues, interest-managed replication, and production
observability remain future work.
