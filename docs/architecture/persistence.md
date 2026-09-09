# Persistence and Recovery

**Status:** Proposed (with a local development checkpoint prototype)

## State categories

### Static content

- Terrain.
- NPC templates.
- Spells and abilities.
- Items.
- Quests and dialogue.
- Loot tables.
- Spawn definitions.
- Navigation data.
- Visual and audio references.

Static content should be versioned, validated, packaged, and loaded before the relevant simulation becomes active.

### Durable player and realm state

- Accounts and authentication metadata.
- Characters.
- Position checkpoints.
- Experience and level.
- Inventory and equipment.
- Currency.
- Quest progress.
- Reputation and social state.
- Guild state.
- Mail and marketplace records.
- Durable world-event state.

### Transient simulation state

- Movement between checkpoints.
- Current casts.
- Temporary auras.
- AI decisions.
- Local combat state.
- Nearby visibility.
- Temporary visual objects.

Transient state can be recovered through a safe reset or limited rollback, provided that durable operations are not duplicated or lost.

## Proposed storage model

Use a relational database such as PostgreSQL for durable state. Keep active simulation state in memory and use a controlled persistence pipeline.

The pipeline may combine:

- Transactional database writes.
- An append-only journal of important operations.
- Periodic snapshots of active region or instance state.
- Backups and restore verification.

The database should not be used as the per-frame simulation data structure.

## Current development identity and checkpoint boundary

The typed development server currently resolves a local token and its available
characters through an `AccountCharacterRepository` boundary. The active
implementation is an in-memory development catalog, not durable storage. This
separates socket/session handling from account and character lookup now, so a
future database-backed repository can be introduced without placing database
access in the simulation tick.

The current development server now has an opt-in, single-character local
checkpoint prototype behind `--character-store <path>`. It uses the repository
boundary to load validated player state when the selected character enters the
world and atomically replace a small versioned checkpoint file at a bounded
20-tick interval at the default 20 Hz, plus at safe logout/disconnect. Checkpoint
writes are handed to a bounded writer thread; the simulation loop only enqueues
an immutable job and polls completion results. Version-2 records carry a
monotonic checkpoint revision and operation ID. A retry at an older revision is
accepted as a no-op, so a delayed writer cannot overwrite newer state. The core
validates the state before restoring it; runtime-only target, health, and
combat timing state is reset.

This is deliberately **not** production persistence: it has no database
transaction, concurrent external writers, multi-character account store,
migration system, credential persistence, or durable transactional
economy/reward operation log with complete commit-before-live-apply semantics.
The
server now accepts an additive retryable command wrapper for purchase, loot,
and quest turn-in and keeps a bounded result cache keyed by account,
character, and operation ID. With `--character-store`, completed result
payloads are appended by a bounded off-thread operation journal and loaded at
the next process start; the typed intent is journaled before it enters the
authoritative queue, and completed records include the staged world revision.
Legacy version-1 records remain readable with revision zero. Without it, the
cache is process-local. The journal,
bounded writer, and revision fence demonstrate ownership and retry boundaries,
but completion still follows live mutation and does not reconcile a crash
between live mutation and result publication. A journal that ends in a
prepared record is recovered as an explicit interrupted-operation failure
before that key can be retried; because live mutation is staged until
completion acknowledgement, this recovery policy does not replay the command.
Completed records retain their original durable command payload. On world
entry, the server compares the record revision with the selected character's
checkpoint revision and replays an older completed operation once through the
staged path; an equal-or-newer checkpoint keeps the cached result without
replay. Legacy version-2 records without a command payload remain
deduplication-only.
A successful completion
acknowledgement gates success-event publication; store failure currently falls
back to a discarded staged batch and remains a crash-recovery gap. The
completion queue reserves capacity for an entire staged batch, and failed
attempts have an explicit journal state. The
reader discards only a torn, non-newline-terminated final record after an
abrupt process exit while preserving earlier complete records. The
repeatable evidence is recorded in
[`EXP-005`](../experiments/EXP-005-durable-character-checkpoint.md).

The development server also exposes `--shutdown-after-ticks <n>` for a
deterministic shutdown proof. Once the tick is reached, it stops accepting
new connections, drains prepared and staged operations within a bounded
deadline, applies queued commands, enqueues final checkpoints, applies final
leave events, and then lets the bounded persistence workers drain during
shutdown. This is a local ordering proof, not a process-signal coordinator or
production failover protocol.

## Transactional operations

The following must have clear atomicity and retry behavior:

- Loot generation and ownership.
- Quest rewards.
- Vendor purchases.
- Currency transfer.
- Player trades.
- Mail delivery.
- Marketplace operations.
- Equipment changes.

Operations should have unique IDs or revisions so that retrying a request cannot duplicate a reward or transaction.

## Content versioning

Persistent records should reference stable content IDs. The server must know which content version produced or modified important state.

Changing a definition should not silently rewrite historical player state unless an explicit migration says that it should.

## Recovery requirements

The design must define behavior for:

- Client disconnect.
- Gateway failure.
- Region-worker failure.
- Instance-worker failure.
- Coordinator failure.
- Temporary database unavailability.
- Process termination during a transaction.
- Interrupted layer handoff.
- Partial content deployment.

The first slice should prioritize never duplicating durable rewards and never corrupting inventory or currency.

## Provisional durability policy

- Durable economy, inventory, and rewards are written or journaled immediately.
- Ordinary movement is checkpointed periodically and on safe logout.
- Active combat may be safely terminated or rolled back after a simulation-worker failure.
- The acceptable movement/combat rollback window must be chosen before production operation.

## Required experiments

- Kill a worker during loot and reward processing.
- Retry vendor and inventory commands after simulated packet loss.
- Kill a process during a trade.
- Restore a region from its snapshot and journal.
- Run a content migration against representative character data.
