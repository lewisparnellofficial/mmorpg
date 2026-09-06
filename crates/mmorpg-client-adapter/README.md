# `mmorpg-client-adapter`

This standalone crate translates the decoded temporary development protocol
into the renderer-independent `mmorpg-client-model`. It is the seam between
transport/schema code and Bevy (or a future renderer); it does not open
sockets, render frames, or make gameplay decisions.

Snapshots are applied as complete replacements. The current temporary
snapshot schema contains player identity/combat/economy scalars and NPC
identity/combat state, but does not yet contain inventory stacks or quest
progress. The adapter therefore creates a newly joined presentation player
with the authoritative scalar values and an empty default inventory for that
temporary bootstrap path. Live economy and quest events subsequently populate
the model. The production snapshot schema must carry the full player state
before reconnect/reconciliation can preserve those fields.

NPC and item/quest display metadata are resolved by stable IDs from the
validated starter content catalog. Unknown IDs and out-of-range template IDs
are rejected rather than allowing untrusted protocol text to become static
content.

Scoped validation from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client-adapter/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-adapter/Cargo.toml
```
