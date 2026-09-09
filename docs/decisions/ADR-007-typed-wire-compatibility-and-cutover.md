# ADR-007: typed wire compatibility and gameplay cutover

- Status: Accepted for the current development slice
- Date: 2026-09-09
- Scope: `mmorpg-wire`, typed TCP server/client, compatibility fixtures, and
  renderer-independent session handling

## Context

The first client/server implementation used a temporary line-oriented
development protocol. The project now has a bounded binary envelope, typed
commands and server messages, content-digest negotiation, explicit character
selection, reconnect-by-new-session behavior, sequenced bootstrap/event
delivery, and a renderer-independent client session. Keeping the line
listener active would leave an unauthenticated gameplay mutation surface and
would allow the graphical client and diagnostic tools to exercise different
authority paths.

The transition must not silently strand an older client. A peer that cannot
decode the gameplay version needs a small version-independent rejection
profile, and well-formed additive server events must be skippable without
making a session fail. Malformed lengths must remain fatal because accepting
them would make stream boundaries ambiguous.

## Decision

The versioned typed wire protocol is the only runtime gameplay/session path.
The primary server listener and graphical client both use the typed MMOW
envelope at `127.0.0.1:4000` by default. The optional `--wire-address` flag
opens a second typed listener for staged local tooling; it is not a line
listener.

The gameplay envelope remains at `PROTOCOL_VERSION = 1` until a future
deliberate protocol migration. Unsupported gameplay versions receive the
version-independent `COMPATIBILITY_PROFILE_VERSION = 0` control frame with
`VersionRejected { supported_min, supported_max }`. The compatibility profile
contains no gameplay state or command semantics.

Typed server delivery has two independent wrappers:

- `SequencedServerMessage` carries a nonzero per-session delivery sequence.
  Complete snapshots establish a baseline; the client session applies only
  contiguous later events, ignores duplicates/older messages, and requests a
  fresh bootstrap on a gap or bounded-buffer overflow.
- `ServerMessage::Response` and `ClientCommand::Request` carry a nonzero
  session-local request ID for immediate session/bootstrap exchanges. This
  correlation is separate from delivery ordering and from durable retryable
  operation IDs. Asynchronous gameplay events remain identified by their
  authoritative event fields and delivery sequence.

Server-message/event variants are length-delimited where additive skipping is
needed. An unknown but well-formed event becomes `SkippedEvent`; an invalid
declared length is fatal. Snapshot schemas retain explicit versions and
bounded collections, with older supported snapshot versions decoded through
the compatibility reader.

The legacy line parser, snapshot assembler, and line fixtures remain only as
test/fixture compatibility data. They have no runtime listener, socket
ownership, authoritative command queue, or mutation path. A fixed starter
transcript is checked through both the retained parser and typed adapter and
must produce identical normalized authoritative commands before this cleanup.

All typed sessions authenticate with the loopback-only development token,
list and explicitly select a character, exchange the shared content digest,
and enter the world only after the session checks succeed. A reconnect starts
the handshake again and cannot replay deferred gameplay commands.

## Consequences

- There is one runtime authority path for graphical, diagnostic, and typed
  smoke clients.
- A previous client can receive a machine-readable version rejection without
  interpreting a gameplay-version payload.
- Additive event variants can be introduced without disconnecting clients
  that do not understand them, provided their length framing is valid.
- Session/bootstrap response correlation is available without changing the
  delivery sequencing or durable operation-id semantics.
- The line parser no longer receives network input or mutates server state.
- The current implementation remains a local development protocol. It does
  not provide public authentication, encryption, production session resume,
  production backpressure, or production interest management.
- Retained line fixtures are not an interoperability guarantee for an old
  production client; they are bounded migration evidence and test data.

## Validation evidence

The decision is accepted for the current development slice based on:

- `mmorpg-wire` codec tests for compatibility rejection, frozen previous-
  client fixtures, truncated/malformed frame handling, unknown event skipping,
  snapshot compatibility, sequence wrappers, and request/response wrappers.
- The server request-correlation integration test
  `request_wrapper_correlates_an_immediate_session_response`.
- The client-session test
  `correlated_server_responses_feed_the_same_handshake_state_machine`.
- The server equivalence test
  `line_and_typed_adapters_preserve_the_starter_transcript`.
- Typed diagnostic, three-role, reconnect, restart-persistence, graphical,
  slow-client, privacy, and content-digest smoke tests.
- Aggregate validation via `./scripts/validate-all.sh`, including the
  typed-only server startup and client checks, passing on 2026-09-09.

## Conditions for revisiting

Revisit this decision before a public deployment, a gameplay protocol-version
bump, or a transport change. A revisit is also required if compatibility
fixtures reveal a semantic mismatch, if sequence recovery cannot establish an
atomic bootstrap baseline, or if request correlation is needed for
asynchronous gameplay results rather than only immediate session/bootstrap
responses. Any protocol-version bump must first add retained fixtures and an
explicit compatibility migration decision.
