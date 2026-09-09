# ADR-009: bounded durable-operation boundary

- Status: Accepted for the current development slice
- Date: 2026-09-09

## Context

The first vertical slice needs retryable vendor, loot, and quest operations
without applying a reward or currency mutation twice after reconnect or a
server restart. The authoritative world remains an in-memory simulation owner;
blocking storage work must not run inside its tick. A complete production
transaction boundary is outside the current development server because the
repository, credentials, database schema, migrations, and failover procedure
do not yet exist.

The local prototype nevertheless needs an explicit ordering rule. A journal
record that only follows live mutation can lose the duplicate-suppression
record at the exact point where a client retries. A record published as
successful before the authoritative mutation can acknowledge an operation
that never happened.

## Decision

For the current development slice, retryable operations use a bounded,
off-thread local operation journal and a staged world batch:

1. Validate the scoped operation key and serialize the original command before
   entering the authoritative command queue.
2. Persist a `prepared` record before simulation admission.
3. Apply the command to a cloned/staged world, collecting the authoritative
   result and the resulting checkpoint revision.
4. Persist a completion record containing the operation key, original command,
   result payload, and staged revision.
5. Publish the staged world and success events only after completion storage is
   acknowledged.
6. On restart, load bounded journal records before accepting gameplay. A
   completed record whose checkpoint revision is newer than the selected
   character checkpoint is replayed once through the staged path; an equal or
   newer checkpoint serves the cached result without replay. An interrupted
   `prepared` record becomes an explicit failed operation and is not replayed.

The journal, checkpoint writer, and character ownership markers are bounded
local development mechanisms. They do not claim database transaction
semantics, strong distributed fencing, multi-writer safety, or production
failover guarantees.

## Consequences

- Duplicate retry keys are scoped by account and character and are retained in
  a bounded in-memory cache, with durable completion/failure records when the
  optional character store is enabled.
- Completion-store failure discards the staged batch and leaves the live world
  and success cache unchanged.
- The simulation owner performs no journal or checkpoint file I/O; it submits
  immutable jobs and polls bounded results.
- Journal and payload limits provide resource safety but can reject work under
  pressure; callers receive an explicit error rather than unbounded growth.
- A crash boundary still exists around process failure, filesystem semantics,
  and recovery coordination. The local prototype is not sufficient for public
  or persistent operation.

## Alternatives considered

### Mutate the live world before journal completion

Rejected for the development slice. A completion-store failure could leave a
live economy mutation without a durable duplicate-suppression record.

### Persist every simulation tick

Rejected. It would place blocking durability pressure on the simulation path
and would incorrectly treat transient combat state as durable business state.

### Add PostgreSQL immediately

Deferred. Production storage requires a schema, transaction and idempotency
design, credentials, migrations, backup/restore verification, and a tested
deployment fencing procedure. The repository boundary keeps that replacement
possible without coupling the core to a database.

## Validation evidence

- `cargo test -p mmorpg-server` covers commit-before-publish, staged-world
  failure atomicity, retry deduplication, revision-aware restart recovery,
  interrupted-prepare rejection, bounded journal input, and shutdown drain.
- The typed gameplay and restart smokes pass with purchase, loot, and quest
  completion surviving the documented local restart path.
- `./scripts/validate-all.sh` passes with `aggregate validation: PASS`.
- EXP-012 records the measured retryable-command prototype and its limits;
  EXP-005 records checkpoint persistence and local fencing evidence.

## Conditions for revisiting

Revisit this decision before public or persistent operation, or when adding
multiple simulation workers. Replace the local journal with a durable
transaction/idempotency design, define recovery for every crash boundary,
coordinate ownership with strong fencing, and verify backup, restore, and
failover behavior under representative workloads.
