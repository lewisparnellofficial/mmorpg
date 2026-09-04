# `mmorpg-client-protocol`

This crate is a small, typed client-side adapter for the temporary
line-oriented development protocol exposed by `mmorpg-server`.

It constructs validated command lines for the current development slice:

```text
connect <name> <tank|healer|damage>
move <dx> <dy>
target <entity-id>
attack
vendor <vendor-id>
buy <vendor-id> <item-id> <quantity>
loot <enemy-id>
quest-offers <npc-id>
accept-quest <npc-id> <quest-id>
turn-in-quest <npc-id> <quest-id>
state
help
inventory
quit
```

`CommandLine` takes typed `EntityId`, `ItemId`, `QuestId`, and `Role` values.
It rejects zero IDs, zero purchase quantities, unsafe player-name characters,
non-finite movement, and movement outside the server's per-command limit,
including the server's combined vector-magnitude limit.
`ProtocolLine` contains no trailing newline; a transport adapter may append one
when writing the line to a socket.

The adapter intentionally has no networking dependency and does not parse
server output. The server's `EVENT`, `WORLD`, `PLAYER`, and related responses
are human-readable diagnostics, not a stable client protocol. In particular,
do not build a client by parsing authoritative results from those lines. A
future production protocol needs versioned, machine-readable result messages
and a separate result decoder.

The crate is a standalone nested Cargo workspace so it can be checked before
the temporary development adapter is promoted into the root workspace.

Scoped validation from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client-protocol/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-protocol/Cargo.toml
```
