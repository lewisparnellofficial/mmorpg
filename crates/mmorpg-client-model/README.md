# mmorpg-client-model

`mmorpg-client-model` is a dependency-light presentation model for the
authoritative events emitted by `mmorpg-core`. It contains no networking,
rendering, windowing, input, or UI-scripting dependencies. A future client
adapter can feed decoded server events into [`ClientWorld::apply_event`] and
let rendering/UI code read the resulting state.

The model deliberately has no command API. It does not accept movement,
combat, inventory, quest, or other client-authored results. All mutable state
changes enter through `Event` values or authoritative snapshots supplied by a
server adapter.

`ClientWorld::replace_from_snapshot` performs a whole-projection replacement
and records the snapshot tick. It clears stale entities and transient vendor
or quest query results, which gives reconnect and reconciliation code an
explicit atomic replacement boundary. Inventory and quest snapshot records
are applied through the corresponding replacement methods only after the
adapter has translated and validated the complete frame.

## Current projection

The model projects:

- Player joins, leaves, snapshots, and movement.
- NPC snapshots, target selection, attack results, and enemy defeat.
- Vendor listings, purchases, loot, and transaction rejections.
- Quest offers, acceptance, progress, completion, rewards, and rejections.

`ClientWorld::apply_npc_snapshot` exists because the current `mmorpg-core`
event API has `PlayerJoined` snapshots but no world/NPC snapshot event. A
client bootstrapper must therefore provide NPC snapshots through a separate
authoritative snapshot channel. The model does not invent NPCs from IDs in
combat events; an event referring to an entity that has not been snapshotted
is safely ignored.

The current event API also does not include a simulation tick, event sequence,
zone/layer/instance identity, or remaining vendor stock in purchase events.
Consequently this crate cannot detect out-of-order or cross-world events, and
it can only decrement a previously displayed vendor listing optimistically
after a purchase. A future versioned network protocol should add an event
sequence and world identity, and should include a server-authoritative stock
value in purchase results.

## Validation

The crate is a member of the root workspace. Run its focused tests directly
when iterating on the presentation model, or use the workspace commands from
the repository guide for the complete runtime validation:

```bash
cargo fmt --manifest-path crates/mmorpg-client-model/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-model/Cargo.toml
```
