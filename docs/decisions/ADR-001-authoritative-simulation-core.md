# ADR-001: Authoritative engine-independent simulation core

**Status:** Accepted for the initial server slice

**Date:** 2026-09-04

## Context

The project needs a Linux server written in Rust that can eventually support a
persistent realm, transparent overworld layers, explicit instances, and a
200-player shared activity. The first vertical slice needs a town, a field,
NPCs, vendors, player roles, movement, targeting, and basic combat.

The simulation must remain usable by future network, persistence, and tooling
systems without coupling gameplay rules to sockets, databases, or rendering.

## Decision

Implement the first server around a dependency-free, engine-independent
authoritative simulation crate in `crates/mmorpg-core`.

The core exposes:

- Stable entity IDs.
- Tank, healer, and damage-dealer roles.
- Player and NPC entities.
- Starter-zone town and field data.
- Commands for joining, leaving, movement, targeting, and basic attack.
- A fixed-step `World::step` operation.
- Authoritative events for accepted actions and rejected commands.

The simulation owns mutable world state and processes commands in a supplied
order. It does not perform socket I/O, database access, timers, rendering, or
async operations.

The initial `crates/mmorpg-server` binary wraps this core in a Linux headless
development process. It currently uses a nonblocking TCP listener and a
temporary line-oriented protocol so the world can be exercised manually and
by smoke tests.

## Consequences

Positive:

- Gameplay rules can be tested without a client or database.
- The server remains authoritative by construction.
- The core can be reused by overworld regions and future instances.
- Rust's type system makes entity ownership and command/event APIs explicit.
- The initial slice is easy to run on one Linux machine.

Negative:

- The current server protocol is not suitable for internet deployment.
- The core does not yet include persistence, interest management, real combat
  timing, AI, or multi-worker ownership.
- The first server process is intentionally single-world and single-worker.
- The event API will need versioning or an adapter before external clients are
  supported.

## Alternatives considered

### Put gameplay logic directly in the network server

Rejected for the initial architecture because it would make deterministic unit
testing and future instance reuse harder.

### Use the client engine's world model on the server

Rejected because the server should remain Linux-headless, authoritative, and
independent from client rendering and asset dependencies.

### Introduce a distributed actor framework immediately

Deferred. The current API preserves single-owner region boundaries while the
first implementation stays small enough to benchmark and debug locally.

## Validation

- Core unit tests pass for starter-zone construction, movement validation,
  target validation, combat resolution, player removal, and role parsing.
- Server protocol tests pass for connection binding and command ownership.
- A local TCP smoke test passes through connect, state, movement, targeting,
  and attack commands.
- The earlier fixed-tick region benchmark is recorded in
  `docs/experiments/EXP-001-rust-region-worker.md`.

## Revisit conditions

Revisit this decision if:

- The core API prevents region and instance simulations from sharing gameplay
  rules cleanly.
- Profiling shows the ownership model cannot meet the 200-player encounter
  target after replication and persistence are included.
- A required gameplay feature needs a different state or scheduling model.
- Cross-process entity handoff cannot be implemented without changing the
  ownership boundary.
