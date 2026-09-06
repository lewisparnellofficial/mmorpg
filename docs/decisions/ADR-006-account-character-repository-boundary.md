# ADR-006: account and character repository boundary

- Status: Accepted for the current development slice
- Date: 2026-09-06
- Scope: typed wire authentication and character-selection path

## Context

The typed wire client now authenticates, lists characters, selects one, and
enters the world. The first implementation held the development token and the
single character directly in the socket server. That made the identity boundary
implicit and would force a future durable account system to modify transport
and session code alongside storage code.

The project requires eventual persistent characters and server-authoritative
durable state, but the current starter simulation is intentionally in-memory.
Introducing a production database before the transaction, recovery, and
ownership model is ready would add operational complexity without proving the
right persistence behavior.

## Decision

The server owns an `AccountCharacterRepository` interface. It is responsible
for:

- validating the local development token;
- resolving the authenticated account ID;
- listing characters owned by that account; and
- resolving an account-scoped selected character before world entry.

`DevelopmentAccountRepository` is the initial implementation. It is a
loopback-gated, in-memory catalog containing the current development account
and `Aria` character. The server socket loop calls the repository; it does not
hold the catalog constants or implement character ownership checks itself.

The repository returns core-role character records and projects them to typed
wire summaries at the network boundary. The authoritative simulation receives
a `JoinPlayer` command only after account-scoped character lookup succeeds.

## Consequences

- Session and transport code no longer own development account/character data.
- The character-selection flow has a testable replacement seam for durable
  storage and account policy.
- The development repository remains intentionally non-durable; restarting the
  server loses all active world state and does not represent a persistent MMO.
- Future storage work must preserve account ownership checks and must not put
  blocking database operations in the simulation-critical path.

## Follow-up

Before public or persistent operation, replace the development repository with
a durable account and character implementation. Design its schema, credential
handling, transaction/idempotency rules, content-version references, session
revocation, and recovery behavior alongside the persistence experiments in the
architecture dossier.
