# ADR-005: loopback-only development authentication boundary

- Status: Accepted for the current development slice
- Date: 2026-09-06
- Scope: typed wire listener and local client smoke path

## Context

The client is moving from the temporary line protocol to the typed wire
listener. Allowing a wire client to submit `Join` directly would leave the
session boundary implicit and would make later account/character integration
more difficult to validate. The project also needs a repeatable local path for
testing an authenticated session before production identity infrastructure is
designed.

The current server is not an internet deployment. A hard-coded development
token must therefore not become an accidentally exposed credential when the
listener is bound to a non-loopback interface.

## Decision

The typed wire listener uses this temporary state transition:

```text
connected -> authenticated -> character-selected -> entered-world -> bound-player
```

The client sends `Authenticate { token: "dev-local" }`. The server validates
the exact token only if the wire listener is bound to loopback, assigns a
monotonically increasing development session ID, and returns
`Authenticated { account_id, session_id }`. The client then requests
`ListCharacters`, receives the account's character summaries, sends
`SelectCharacter`, and only then sends `EnterWorld`. The development account
currently exposes one static `Aria` damage-dealer character and returns the
normal `Connected` response after that explicit selection.

Before authentication, all commands except `Authenticate` are rejected. After
authentication, the legacy wire `Join` command is rejected; world entry is the
only path that binds a wire session to a player. The existing line listener is
unchanged because it remains a local debugging protocol.

## Consequences

- The client, server, and TCP smoke test exercise an explicit authenticated
  session boundary now.
- The development token cannot be used through a wire listener bound to a
  non-loopback address.
- Multiple local wire clients currently receive the same account and static
  character catalog; this is acceptable only for the development slice.
- The typed client may reconnect after a socket loss by executing this full
  development transition again. Each connection obtains a new development
  session and an authoritative bootstrap; no session-resume guarantee is
  implied.
- The protocol retains the legacy `Join` codec variant temporarily so older
  payload round-trip tests and compatibility tooling can still decode it, but
  the server does not accept it on the typed wire path.
- No claim is made about production identity, credential storage, transport
  encryption, authorization, revocation, reconnect policy, or multi-character
  selection UX.

## Follow-up

Production authentication should be designed before any externally reachable
server deployment. It should replace the shared token with an account/session
service, use a secure credential exchange, bind an authenticated account to an
explicit character selection flow, support revocation and reconnect policy,
and keep authorization decisions server-side.
