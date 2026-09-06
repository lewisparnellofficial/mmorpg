# `mmorpg-wire`

This standalone crate is a prototype for the production protocol boundary. It
defines a small, transport-independent envelope for versioned commands and
events. The envelope remains independent of sockets and the simulation, while
the first typed client-command payload schema now lives alongside it.

## Frame format

Each frame is encoded in network byte order as:

```text
u32 body_length
u8[4] magic             "MMOW"
u16 protocol_version   1
u8 message_kind        1 = command, 2 = event
u8 flags               reserved; must be 0
u32 payload_length
u8[payload_length] payload
```

`body_length` includes the 12-byte body header and must be no larger than
64 KiB. `payload_length` must exactly equal the number of bytes remaining in
the body. The length prefix allows a receiver to distinguish frame boundaries
when multiple frames share a stream and to identify a partial frame without
guessing where a message ends. `decode_one` returns the number of consumed
bytes, leaving any following frame available to the caller.

The current implementation accepts only protocol version 1, command/event
message kinds 1 and 2, and zero flags. These explicit rejection rules are
intentional: a future protocol can add negotiated versions or flags behind a
deliberate compatibility decision instead of silently interpreting unknown
bytes.

## Typed client commands

`ClientCommand::encode_payload` and `ClientCommand::decode_payload` define the
first structured application payload above the envelope. The schema currently
covers join, movement, target selection, attack, vendor listing and purchase,
loot, quest offers/acceptance/turn-in, and snapshot request. Numeric IDs are
big-endian, movement values are IEEE-754 `f32` bit patterns, names are bounded
UTF-8 strings, and zero IDs/quantities or non-finite movement values are
rejected. The opcode table is intentionally private to the Rust API until the
server session adapter is ready to publish a compatibility contract.

## Why the current TCP output is not production protocol

The development server currently writes human-readable lines such as
`EVENT ...`, `WORLD ...`, and `PLAYER ...`. That output is useful while
debugging, but it is not a safe or stable production protocol:

- Text lines do not provide a versioned schema or an explicit payload boundary
  for structured values.
- Formatting changes, spaces, names, and diagnostic messages can accidentally
  become client-visible API changes.
- A line-oriented parser has to define escaping and limits for every field,
  while partial reads and multiple messages still need careful buffering.
- Human-readable output does not provide a clear distinction between
  authoritative events, snapshots, errors, and diagnostics.
- There is no negotiated compatibility policy, message type registry, or
  machine-checkable maximum frame size.
- Parsing server display text would make client state dependent on wording
  rather than stable IDs and typed fields.

This prototype addresses framing, envelope metadata, and the first typed
command payload schema. It does not yet define typed event/snapshot payloads,
authentication, encryption, compression,
capability negotiation, replay protection, sequencing, acknowledgements,
interest-managed replication, or socket ownership. A future network adapter
should own buffering and I/O, call `decode_one` only after receiving bytes,
and apply the resulting typed payload through the authoritative protocol
boundary.

## Local validation

Run from this directory or from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-wire/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-wire/Cargo.toml
```

The tests cover command and event round trips, concatenated frames, every
truncated prefix, exact maximum-size acceptance, oversized-frame rejection,
and malformed header/payload boundaries.
