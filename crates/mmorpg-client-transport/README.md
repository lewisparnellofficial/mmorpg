# `mmorpg-client-transport`

This crate is a small blocking transport adapter for the temporary
line-oriented development server. It connects a TCP stream, sends the typed
`ProtocolLine` values produced by `mmorpg-client-protocol`, and reads bounded
diagnostic lines. The configured maximum line size applies to both outgoing
command payloads and incoming response payloads; the newline terminator is
not counted.

It intentionally does not parse `WELCOME`, `EVENT`, `WORLD`, `PLAYER`, or
other human-readable server output into authoritative state. That output is a
development diagnostic surface. A production client needs a versioned,
machine-readable wire protocol and a decoder that feeds authoritative results
into `mmorpg-client-model`.

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
