# `mmorpg-client-adapter`

This standalone crate translates the decoded temporary development protocol
and typed `mmorpg-wire` server messages into the renderer-independent
`mmorpg-client-model`. It is the seam between
transport/schema code and Bevy (or a future renderer); it does not open
sockets, render frames, or make gameplay decisions.

Snapshots from either protocol path are applied as complete replacements. The
temporary snapshot schema
contains player identity/combat/economy scalars, NPC identity/combat state,
inventory stack records, and quest state records. The adapter translates those
records into the model so reconnect and reconciliation preserve the current
starter inventory and quest state. Snapshot schema version 2 carries the
authoritative inventory capacity per player; a missing capacity is rejected
without replacing the existing presentation model. The typed wire snapshot
uses the same explicit field.

NPC and item/quest display metadata are resolved by stable IDs from the
validated starter content catalog. Unknown IDs and out-of-range template IDs
are rejected rather than allowing untrusted protocol text to become static
content.

Scoped validation from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client-adapter/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-adapter/Cargo.toml
```
