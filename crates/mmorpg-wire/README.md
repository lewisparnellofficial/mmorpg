# `mmorpg-wire`

## Compatibility and additive events

The gameplay envelope remains protocol version `1`, while a frozen
version-independent control profile (`version = 0`, `MessageKind::Control`)
can carry `VersionRejected { supported_min, supported_max }`. Use
`encode_compatibility_control` and `decode_compatibility_control` for this
profile; the ordinary `decode_one` path intentionally continues to reject the
version-independent frame as a gameplay envelope.

Server-message event payloads use `encode_framed_server_event` and
`decode_framed_server_event`. The five-byte event prefix declares the opcode
and body length. A well-formed unknown opcode becomes
`ServerMessage::SkippedEvent`; a length mismatch is a fatal
`MalformedEventLength` error. The standalone helpers expose the same profile
for stream and fixture tests.

This crate is a prototype for the production protocol boundary. It
defines a small, transport-independent envelope for versioned commands and
events. The envelope remains independent of sockets and the simulation, while
typed client-command and server-message payload schemas live alongside it.

## Frame format

Each frame is encoded in network byte order as:

```text
u32 body_length
u8[4] magic             "MMOW"
u16 protocol_version   1
u8 message_kind        1 = command, 2 = event, 3 = control
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

The current gameplay implementation accepts protocol version 1, command,
event, or control message kinds, and zero flags. The compatibility profile
uses version 0 only for its separately decoded control frame. These explicit
rejection rules are
intentional: a future protocol can add negotiated versions or flags behind a
deliberate compatibility decision instead of silently interpreting unknown
bytes.

## Typed client commands

`ClientCommand::encode_payload` and `ClientCommand::decode_payload` define the
first structured application payload above the envelope. The schema currently
covers development authentication, character listing/selection, world entry,
join compatibility decoding, movement, target selection, attack, vendor
listing and purchase, loot, quest offers/acceptance/turn-in, party
invite/accept/decline, leave/remove/leader-transfer/disband, snapshot request,
and pre-entry content digest exchange. Numeric IDs are
big-endian, movement values are IEEE-754 `f32` bit patterns, names are bounded
UTF-8 strings, and zero IDs/quantities or non-finite movement values are
rejected. The server session adapter now consumes these commands on its
optional wire listener; the opcode table remains an explicitly versioned Rust
contract until a stable external compatibility document is accepted.

Typed server messages sent by the development listener are wrapped in
`SequencedServerMessage`, which carries a nonzero per-session `u64` sequence
before the existing message payload. `decode_payload` remains available for
legacy unsequenced fixtures; `SequencedServerMessage::decode_payload` is the
path used by sequence-aware clients.

## Typed server messages

`ServerMessage` encodes welcome, development-authentication,
character-list/selection, connect/error responses, every current authoritative
gameplay event, and a bounded bootstrap `WorldSnapshot`. The snapshot has its
own schema version (`SNAPSHOT_SCHEMA_VERSION`, currently 3) inside the
protocol envelope. Version 2 remains decodable; version 3 adds an optional
private party summary containing only party ID, leader ID, and member IDs.
Player state includes an explicit inventory capacity, inventory stacks, and
quest progress; NPCs, vendor listings, quest offers, and party member summaries
use explicit bounded collections. Additive party events carry invite,
membership, leadership, and disband transitions. IDs, enum
values, strings, floats,
collection counts, schema versions, and trailing bytes are validated during
both encode and decode. `WireConnection::read_server_message` consumes these
messages without interpreting diagnostic text. Additive event variants are
length-delimited; well-formed unknown variants are reported as `SkippedEvent`
and ignored by presentation/session adapters, while malformed declared
lengths are fatal.

The server currently places all server messages in the envelope's `Event`
message kind. This keeps the envelope small while the payload discriminator
distinguishes welcome, event, error, and snapshot records. A future protocol
version can separate channels if replication or reliability requirements make
that useful.

## Why the diagnostic TCP output is not production protocol

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

This prototype addresses framing, envelope metadata, typed command payloads,
and typed server event/snapshot payloads. It has only a loopback-only
development authentication message; it does not yet provide production
authentication, encryption, compression,
capability negotiation, replay protection, sequencing, acknowledgements,
interest-managed replication, acknowledgements, or socket ownership. The
development listener now wraps server messages with per-session sequences;
production replay protection and resumable sessions remain out of scope. A
network adapter should own buffering and I/O, call `decode_one` only after
receiving bytes, and apply the resulting typed payload through the
authoritative protocol boundary.

The frozen previous-client compatibility fixture and decoder harness live in
`tests/compatibility_fixtures.rs`. Compatibility profile changes must preserve
that fixture or add a new profile through an explicit migration decision.

## Local validation

Run from this directory or from the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-wire/Cargo.toml -- --check
cargo test --manifest-path crates/mmorpg-wire/Cargo.toml
```

The tests cover command and event round trips, concatenated frames, every
truncated prefix, exact maximum-size acceptance, oversized-frame rejection,
and malformed header/payload boundaries.
