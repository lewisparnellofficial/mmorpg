# EXP-004: typed wire gameplay smoke test

The wire boundary also has deterministic codec tests for the Milestone 2
compatibility gates: a version-independent `VersionRejected` control frame,
unknown length-delimited additive-event skipping, and malformed event-length
rejection. These tests are protocol evidence; the socket probe below adds
running server-side interoperability evidence for the compatibility response.

The server now also emits the compatibility frame on a real unsupported
version receive. A localhost socket probe consumed the initial sequenced
`Welcome`, sent a version-99 command envelope, and decoded the following
version-independent `VersionRejected { supported_min: 1, supported_max: 1 }`
frame before the server closed the incompatible session.

## Purpose

Verify that the versioned wire listener and the typed client-facing payloads
work together across a real TCP connection for the complete starter-zone
vertical slice.

## Procedure

1. Start `mmorpg-server`; its primary address is the typed listener. An
   optional `--wire-address` opens a second typed listener for staged runs.
2. Run `experiments/wire-gameplay-smoke` against the wire address.
3. The tool verifies that a legacy wire join is rejected before
   authentication, authenticates with the loopback-only development token,
   lists and explicitly selects the development character, requests a typed
   snapshot, buys one vendor item, accepts the starter quest, defeats and
   loots all three field wolves, and turns in the quest.

The companion `three-client-gate` binary runs the same boundary with three
simultaneous typed sessions. It verifies distinct tank, healer, and damage
characters in one shared zone, private snapshots and party summaries, a town
purchase and quest acceptance, tank taunt, authoritative healer recovery,
three quest kills with retryable loot, quest turn-in, and a second loot
generation after respawn.

## Evidence produced

The tool passes only when it decodes and validates:

- welcome, unauthenticated-command rejection, authentication, and connection
  messages;
- a snapshot containing one player, one vendor, and three enemies;
- vendor listings and a successful purchase;
- quest offers and acceptance;
- target selection, repeated authoritative attacks until defeat for each enemy;
- one server-derived loot reward for each enemy; and
- the final quest reward.

The three-client gate additionally passes only when it observes the shared
party/privacy, role authority, recovery, and repeated-generation checks across
all three sessions.

The server test `line_and_typed_adapters_preserve_the_starter_transcript`
drives a fixed starter command transcript through both the retained line
parser and the typed command adapter. Movement, targeting, combat, vendor,
loot, quest-offer, quest-acceptance, and quest-turn-in commands must produce
identical authoritative `Command` values for the same bound player. This is
the equivalence evidence for retaining line data as inert test-only
compatibility coverage while removing its runtime mutation path; it does not
treat line output as a production protocol or claim byte-identical wire
transcripts.

The server's listener admission is independently bounded at 256 simultaneous
wire clients; the focused server test fills that bound and confirms the next
accepted socket is rejected before a `WireClient` session is allocated. This
is a local resource-bound test, not evidence for the 5,000-client target.

## Classification

- Directly measured: the local command completed over TCP without a protocol,
  framing, or server-session failure.
- Project-specific inference: the current starter loop is reachable through
  the same typed intent and event boundary that the client uses in wire mode.
- Measured contract test: the retained line and typed adapters produce the
  same normalized authoritative commands for the fixed starter transcript.
- Not demonstrated: thousands of connected clients, 200-player activity,
  production authentication, encryption, interest management, or production
  backpressure. The development authentication path is covered only as a
  loopback smoke-test boundary.

## Reproduction

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4000 \
  --wire-address 127.0.0.1:4001
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml
```
