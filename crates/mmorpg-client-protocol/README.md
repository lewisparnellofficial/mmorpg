# `mmorpg-client-protocol`

This crate is a small, typed client-side adapter for the temporary
line-oriented development protocol exposed by `mmorpg-server`.

It constructs validated command lines and decodes the bounded structured state
and event lines for the current development slice. Supported commands are:

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

The decoder is exposed as `decode_server_line` and
`ServerLine::decode`. It accepts only exact `WORLD`, `PLAYER`, `NPC`,
`CONNECTED`, and all current machine-readable `EVENT` prefixes with their
required key/value fields. It currently decodes:

```text
TEMP_SNAPSHOT_BEGIN version=1
TEMP_SNAPSHOT WORLD ...
TEMP_SNAPSHOT PLAYER ...
TEMP_SNAPSHOT NPC ...
TEMP_SNAPSHOT ITEM ...
TEMP_SNAPSHOT QUEST ...
TEMP_SNAPSHOT_END
WORLD tick=... players=... npcs=... enemies=... vendors=...
PLAYER id=... name=... role=... pos=... hp=... gold=... target=...
NPC id=... name=... kind=... pos=... hp=...
CONNECTED player_id=... role=...
EVENT player_joined ...
EVENT player_left ...
EVENT player_moved ...
EVENT target_selected ...
EVENT attack ...
EVENT enemy_defeated ...
EVENT vendor_listed ...
EVENT item_purchased ...
EVENT loot_rewarded ...
EVENT transaction_rejected ...
EVENT quest_offers ...
EVENT quest_accepted ...
EVENT quest_progressed ...
EVENT quest_completed ...
EVENT quest_rewarded ...
EVENT quest_rejected ...
EVENT rejected ...
```

The decoder rejects unknown prefixes and fields, duplicate fields, missing
fields, control characters, non-finite positions, invalid IDs/enums, and lines
larger than `MAX_SERVER_LINE_BYTES`. The schema-defined `name` and `reason`
values may contain spaces until the next `key=` field in legacy records. The
`TEMP_SNAPSHOT` records use the server's bounded percent encoding for names.
Compound vendor and quest event fields use semicolon-separated records, with
underscores representing spaces in display names. The protocol exposes owned
strings; the client may resolve richer descriptions and metadata from its
validated content package. No other prose or diagnostic line is interpreted.
This remains a temporary development adapter, not a versioned production
protocol.

The adapter has no networking dependency. A transport reads one complete line
and passes it to the decoder without its trailing newline.

## Atomic snapshot assembly

`SnapshotAssembler` is the stream-level boundary for the temporary snapshot
format. Feed it one complete line at a time with `push_line`:

```rust
let mut assembler = SnapshotAssembler::new();

for line in lines_from_the_transport {
    if let Some(snapshot) = assembler.push_line(line)? {
        // Replace the displayed client state in one operation.
        current_snapshot = snapshot;
    }
}
assembler.finish()?; // reports an incomplete frame at end-of-stream
```

Only a valid sequence beginning with `TEMP_SNAPSHOT_BEGIN version=1` and
ending with `TEMP_SNAPSHOT_END` produces a `Snapshot`. The assembler buffers
the world, player, NPC, item, and quest records and does not publish any of
them individually. Item and quest records must refer to a player present in
the same frame. It rejects records outside an active frame, duplicate begin or
record entries, missing world data, unsupported versions, malformed records,
inconsistent world counts, and truncated frames. A failed frame is discarded,
so a caller can retain its last completed snapshot and start assembling the
next one.

Player and NPC entity IDs must be unique across the whole frame. The default
limit is `MAX_SNAPSHOT_RECORDS` (4,096 records total);
`SnapshotAssembler::with_max_records` can select a smaller bound but cannot
raise the fixed maximum. Legacy `WORLD`, `PLAYER`,
and `NPC` diagnostics are ignored by the assembler. Their exact legacy
prefixes remain separate from the `TEMP_SNAPSHOT` prefixes, so they cannot be
mistaken for snapshot records.

The crate is a standalone nested Cargo workspace so it can be checked before
the temporary development adapter is promoted into the root workspace.

Scoped validation from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client-protocol/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-client-protocol/Cargo.toml
```
