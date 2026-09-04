# Research: Durable Persistence and Recovery

**Status:** Preliminary findings and proposed design; implementation not yet
accepted

**Started:** 2026-09-04

## Question

How should the project persist player and realm state, recover from process or
machine failure, and preserve exactly-once behavior for rewards and economy
operations while keeping the real-time simulation off the database path?

## Project context

The accepted requirements target approximately 5,000 connected clients and
approximately 200 players participating in one shared overworld activity on a
single layer. The first vertical slice is a single town-and-field zone with
three roles, vendors, enemies, loot, quests, and a meaningful return-to-town
loop. Dungeons, raids, and battlegrounds are intended to be explicit
instances.

The current repository has deliberately not implemented durable persistence:

- `mmorpg-core::World` is a dependency-free, synchronous, single-owner
  in-memory simulation.
- `Player` contains mutable position, health, target, gold, inventory, and
  quest progress.
- `Npc` contains live entity state; vendor stock and enemy reward ownership
  are held in separate in-memory maps.
- `Event` values are authoritative in-process results, not a durable event
  log.
- `EntityId` identifies a live entity but is not yet a durable account or
  character identity.
- Economy and quest ADRs explicitly defer restart survival, durable
  idempotency, cross-worker concurrency, and crash-recovery commit semantics.
- The development server removes a player when its TCP connection closes. It
  has no authentication, session lease, reconnect token, database adapter, or
  persistence queue.

These facts mean that persistence cannot be added safely by serializing the
whole `World` from an arbitrary thread. The simulation ownership boundary and
the durable-operation commit boundary need to be designed together.

## Evidence classification

This record uses the following labels:

- **Sourced fact** — behavior documented by the linked project or standard.
- **Project inference** — a conclusion drawn from those facts and this
  project's requirements.
- **Recommendation** — a proposed project choice, not an accepted ADR.
- **Modeled or measured result** — a number produced by a model or local test.
- **Unresolved risk** — a question requiring a prototype, failure test, or
  explicit owner decision.

No capacity result in this document demonstrates support for 5,000 clients or
the 200-player encounter. Those remain workload-validation requirements.

## Durability vocabulary

The project should use precise terms because “saved” can mean several
different things:

- **Authoritative in memory:** the current simulation owner has accepted a
  command and changed live state.
- **Durably committed:** the required durable store has acknowledged the
  operation under the chosen durability policy.
- **Published:** a committed result has been made available to the network,
  client projection, or another service.
- **Checkpointed:** a snapshot represents a known simulation tick and a
  durable-operation watermark.
- **Recoverable:** the system can reconstruct a valid state after a specified
  failure using its snapshot and journal/operation records.
- **RPO:** maximum acceptable amount of acknowledged work that can be lost
  after a disaster.
- **RTO:** maximum acceptable time to make the service usable again after a
  failure.

The client must never be told that a reward or purchase succeeded merely
because the simulation tentatively changed an in-memory object. For durable
operations, the externally visible success point must be defined explicitly.

## Sourced database findings

### PostgreSQL

**Sourced facts:**

- PostgreSQL uses write-ahead logging: data-file changes are written only
  after the corresponding WAL records have been flushed to permanent storage.
  WAL replay can redo changes that were not yet applied to data pages after a
  crash. [PostgreSQL WAL documentation](https://www.postgresql.org/docs/current/wal-intro.html)
- Normal synchronous commit waits for transaction WAL to be flushed before
  returning success. Asynchronous commit can report success earlier, leaving a
  window in which recent acknowledged transactions may be lost; PostgreSQL
  describes that as data loss rather than database inconsistency when the
  normal safety settings are retained. [PostgreSQL asynchronous commit](https://www.postgresql.org/docs/current/wal-async-commit.html)
- PostgreSQL's `fsync` setting is intended to ensure recovery to a consistent
  state after an operating-system or hardware crash. The documentation warns
  that disabling it can cause unrecoverable corruption. [PostgreSQL WAL
  configuration](https://www.postgresql.org/docs/current/runtime-config-wal.html)
- PostgreSQL supports Read Committed, Repeatable Read, and Serializable
  isolation behaviors. Serializable transactions emulate some serial order,
  but applications must be prepared to retry transactions after serialization
  failures. [PostgreSQL transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- Constraints can reject invalid rows, and primary keys, unique constraints,
  foreign keys, and checks can encode invariants in the database. [PostgreSQL
  constraints](https://www.postgresql.org/docs/current/ddl-constraints.html)
- `INSERT ... ON CONFLICT` provides an atomic alternative to a unique-constraint
  violation. This is useful for idempotency records and deduplicating an
  operation key. [PostgreSQL INSERT](https://www.postgresql.org/docs/current/sql-insert.html)
- PostgreSQL offers SQL dumps, filesystem-level backups, and continuous
  archiving as distinct backup approaches. [PostgreSQL backup and restore](https://www.postgresql.org/docs/current/backup.html)
- Continuous archiving combines a base backup with archived WAL. WAL can be
  replayed to a selected point in time, and the base backup need not be an
  instantaneous filesystem image because WAL replay repairs the internal
  inconsistencies introduced while the backup was taken. [PostgreSQL
  continuous archiving and PITR](https://www.postgresql.org/docs/current/continuous-archiving.html)
- PostgreSQL streaming replication is asynchronous by default. Its synchronous
  commit modes can wait for a standby to receive, flush, or apply the commit,
  with corresponding latency and durability trade-offs. [PostgreSQL
  standby servers](https://www.postgresql.org/docs/current/warm-standby.html)
- `ALTER TABLE` forms can require an `ACCESS EXCLUSIVE` lock, and some schema
  changes scan or rewrite data. This is evidence that migrations need an
  operational plan rather than being treated as harmless startup code.
  [PostgreSQL `ALTER TABLE`](https://www.postgresql.org/docs/current/sql-altertable.html)
- PostgreSQL supports major-version upgrades through dump/restore, `pg_upgrade`,
  or logical-replication-based approaches. The upgrade documentation warns
  that some logical-replication upgrade steps are not transactional and should
  be backed up. [PostgreSQL upgrading](https://www.postgresql.org/docs/current/upgrading.html)

**Project inference:** PostgreSQL supplies the transaction, constraint, WAL,
backup, and replication primitives needed for the durable account/economy
boundary without requiring the simulation to use SQL as its frame-by-frame
state structure. It does not automatically solve application-level
idempotency, event publication, operation ordering, ownership fencing, or
recovery of transient NPC and combat state.

### SQLite

**Sourced facts:**

- SQLite transactions are atomic, consistent, isolated, and durable under the
  documented assumptions, including interruption by a program crash,
  operating-system crash, or power failure. SQLite says its regression suite
  tests these properties using crash and power-failure simulation.
  [SQLite is transactional](https://www.sqlite.org/transactional.html)
- SQLite WAL mode permits readers and a writer to proceed concurrently, but
  all processes must be on the same host and WAL does not work over a network
  filesystem. [SQLite WAL](https://www.sqlite.org/wal.html)
- SQLite supports multiple simultaneous readers but only one simultaneous
  write transaction. [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- SQLite's online backup API can copy a live database incrementally so the
  source need not remain locked for the entire copy. [SQLite Online Backup
  API](https://www.sqlite.org/backup.html)

**Project inference:** SQLite is excellent for local tools, a single-process
development server, deterministic tests, and offline content or replay
artifacts. Its one-writer model and same-host WAL limitation make it a poor
canonical store for a future multi-process realm with several thousand
connected clients. It could remain a useful test backend if the persistence
interface is database-neutral.

### RocksDB

**Sourced facts:**

- RocksDB is an embedded key-value engine whose basic interface operates on
  arbitrary byte-stream keys and values. Its architecture contains memtables,
  SST files, and log files. [RocksDB overview](https://github.com/facebook/rocksdb/wiki/RocksDB-Overview)
- RocksDB writes updates to an in-memory memtable and, by default, to a WAL.
  Its documentation describes WAL replay as the mechanism for recovering
  memtable state after failure. [RocksDB WAL](https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-%28WAL%29)
- RocksDB checkpoints provide a consistent snapshot in another directory. On
  the same filesystem, SST files may be hard-linked; the checkpoint process
  also handles WAL files needed across the checkpoint interval.
  [RocksDB checkpoints](https://github.com/facebook/rocksdb/wiki/Checkpoints)
- A non-synchronous RocksDB write can lose recent updates after a machine
  crash, while synchronous writes wait for the underlying storage flush. The
  application chooses this trade-off per write. [RocksDB basic operations](https://github.com/facebook/rocksdb/wiki/Basic-Operations)

**Project inference:** RocksDB is a plausible implementation for a local
region snapshot store or an embedded single-owner service. It leaves the
project responsible for relational constraints, indexes, migrations,
cross-player transactions, replication, backup shipping, and operational
inspection. That application burden is not justified for the first canonical
player/economy store, although it may become attractive for high-volume
region-local state after measurements.

### Distributed PostgreSQL-compatible alternatives

CockroachDB is a representative alternative if the project eventually needs
database-level multi-node distribution rather than a PostgreSQL primary with a
standby. Its official documentation states that committed transactions are
atomic and serializable by default, and that contention can produce retry
errors that application code must handle. [CockroachDB developer basics](https://www.cockroachlabs.com/docs/stable/developer-basics.html)

**Project inference:** A distributed SQL database may simplify some
multi-machine failover problems, but it adds operational, latency, and
transaction-retry complexity before this project has measured a need for it.
The realm's simulation ownership model should prevent most hot writes from
requiring global database transactions. A PostgreSQL primary plus tested
backups and an optional standby is the smaller first deployment.

## Database comparison

| Candidate | Strengths for this project | Costs and limitations | Proposed use |
| --- | --- | --- | --- |
| PostgreSQL | Mature client/server database; relational constraints; transactions; WAL; PITR; standby replication; good inspection and migration ecosystem | Requires a separate service; schema and query operations must be designed; hot rows and long transactions can create contention; application still owns idempotency and recovery protocol | **Canonical durable store** for account, character, economy, quest, social, and durable realm state |
| SQLite | Very easy Linux deployment; transactional; excellent local test and tool database; online backup API | One writer; WAL is same-host only; embedded process lifecycle; weaker fit for multiple persistence workers or independent realm services | Local development backend, editor metadata, fixtures, and deterministic integration tests |
| RocksDB | Embedded, fast key-value writes; WAL and consistent checkpoints; natural fit for owner-local data | Application must build schema, queries, constraints, migrations, backup shipping, and multi-key semantics; inspection is less convenient | Possible future region-local snapshot/cache store after a benchmark |
| Distributed SQL such as CockroachDB | Distributed replicas, serializable transactions, PostgreSQL wire compatibility, and automatic retry mechanisms | More operational complexity and latency; retry semantics still affect application code; likely overkill for the first realm | Future failover or sharded-database experiment, not the initial default |

## Recommendation

Use PostgreSQL as the initial canonical durable store, behind a Rust
persistence adapter that exposes domain operations rather than SQL to
`mmorpg-core`.

The first production-shaped deployment should be:

```text
simulation owner(s)
       |
       | domain commands / durable intents
       v
persistence adapter + bounded queue
       |
       v
PostgreSQL primary ---- optional standby
       |
       +---- WAL archive / base backups ---- separate backup storage
```

The development server can initially run the adapter and simulation in one
process, but the queue, operation IDs, and ownership metadata should exist as
logical boundaries before they are moved to separate processes.

Do not make PostgreSQL the per-frame source of truth for movement, casts, AI,
nearby visibility, or animation. Persist meaningful durable transitions and
periodic safe checkpoints. Treat combat and other transient state as
reconstructible unless a later game rule explicitly makes it durable.

## Durable state model

### Identity and versioning

The current `EntityId` is a live-world ID. Add separate stable identifiers
before persistence is implemented:

- `AccountId` — owner of authentication and account-level state.
- `CharacterId` — durable player identity; foreign key target for inventory,
  gold, quests, equipment, and social records.
- `LiveEntityId` or the existing `EntityId` — current process/world instance
  identity; safe to discard and reallocate after recovery.
- `InstanceId` and `LayerId` — durable references only where game rules require
  them; ordinary transient placement should not accidentally become permanent
  canonical state.
- `ContentRevision` — the validated static content package that defines the
  meaning of an operation or saved record.
- `SchemaVersion` — persistence representation version, distinct from content
  revision.

Stable IDs must be generated by the server or migration, never accepted as
authoritative values from an untrusted client.

### Suggested relational records

The following is a proposed shape, not a finalized schema:

| Record | Important fields | Purpose |
| --- | --- | --- |
| `accounts` | `account_id`, credential/provider reference, status, timestamps | Authentication ownership; keep secrets out of gameplay tables |
| `characters` | `character_id`, `account_id`, name, role/class, level, experience, revision, content revision | Durable identity and optimistic-concurrency version |
| `character_position` | character ID, zone, x/y/z, layer/instance hint, checkpoint tick, saved timestamp | Safe reconnect location; not every movement sample |
| `character_currency` | character ID, currency kind, non-negative amount, revision | Gold and future currencies; constrain uniqueness and non-negative values |
| `character_inventory` | character ID, slot, item definition ID, quantity, item-instance ID if needed, revision | Inventory state; retain item-instance path for future equipment |
| `character_quests` | character ID, quest ID, status, objective counters, content revision, completion operation ID | Quest progress and terminal reward identity |
| `vendor_stock` | vendor/content ID, item ID, quantity, revision | Only persist stock when design says it is shared or survives restart |
| `world_state` | realm/world key, event key, state, owner epoch, revision, content revision | Durable world events and canonical objects, not every NPC AI decision |
| `operation_log` | operation ID, semantic key, type, character/realm scope, request hash, status, result, owner epoch, timestamps | Idempotency, audit, recovery, and result replay |
| `outbox` | event ID, operation ID, event type, payload/version, publication state | Publish committed results without a database-to-network dual write |
| `region_checkpoints` | region/layer/instance, snapshot ID, tick, operation watermark, content revision, checksum, location | Recovery metadata for simulation snapshots |

Use foreign keys, unique constraints, check constraints, and indexes for
invariants that must hold even if an application bug bypasses a normal code
path. The database is a backstop, not a replacement for authoritative
validation in the simulation.

## Operation journal and idempotency

### Why a journal is needed

The current in-process `Event` stream is useful for presentation and testing,
but replaying every combat event as a durable event-sourced history would
create unnecessary volume and couple recovery to transient simulation details.
The project needs a smaller journal of operations whose effects must never be
duplicated or silently lost.

Journal these categories first:

- Vendor purchases and sales.
- Loot grants.
- Quest reward grants and quest terminal transitions.
- Player-to-player trades and currency transfers.
- Mail delivery and marketplace settlement when those systems exist.
- Equipment changes when item instances or progression depend on them.
- Durable world-event transitions.
- Character creation, deletion, rename, and other account lifecycle changes.

Do not journal every movement sample, target selection, AI decision, or visual
effect. Save position checkpoints and reconstruct transient activity instead.

### Two identities for a retried operation

Each request should have two distinct IDs:

1. **Request ID:** generated by the client/session or gateway to correlate a
   retry of one user action. It is not trusted for authorization.
2. **Semantic operation key:** generated or derived by the authoritative
   server, and stable across retries of the same game transition.

Examples:

- Purchase: `purchase:<character_id>:<client_request_id>` after validating
  that the request ID cannot be reused with a different payload.
- Enemy loot: `loot:<defeated_enemy_instance_id>:<loot_slot>` or a server
  generated reward claim ID. The client cannot select the item or quantity.
- Quest reward: `quest-reward:<character_id>:<quest_id>:<completion_generation>`.
- Trade: `trade:<trade_id>:<commit_generation>`.

The semantic key prevents a retry from creating a second effect even if the
client, gateway, or persistence worker loses the response.

### Proposed idempotent transaction pattern

For one durable operation:

1. Authenticate the session and resolve the durable `CharacterId`.
2. Validate that the command is legal for the current authoritative world
   state and derive all prices, rewards, item IDs, and quantities server-side.
3. Compute a canonical request hash. If the same request ID was previously
   used with another hash, reject it as a protocol error.
4. Start one short database transaction.
5. Insert the operation row with a unique semantic key. If it already exists,
   lock/read its stored result and return that result without applying the
   effect again.
6. Lock or conditionally update all affected durable rows in a deterministic
   order: character(s), stock/claim row, then related quest or world rows.
7. Recheck balances, capacity, revisions, ownership, and availability inside
   the transaction. Do not rely on a read performed before the transaction.
8. Apply every mutation: inventory, currency, stock, quest terminal state,
   reward claim, or trade legs.
9. Insert the durable result in `operation_log` and an outbox message in the
   same transaction.
10. Commit with the configured durability policy.
11. Only after the commit point, publish the authoritative success event to
    the client and update the simulation projection. A duplicate delivery of
    the result is acceptable; a duplicate state mutation is not.

The operation row should store enough result data to replay the same outcome
after a timeout, such as resulting gold, granted item IDs and quantities,
remaining stock, and a reason for rejection. Avoid recomputing a historical
result against changed content or current prices.

`ON CONFLICT` or an equivalent locked lookup can implement the unique-key
step, but the exact SQL must be tested under concurrent retries. A successful
database commit does not guarantee that the network response reached the
client; this is the reason the stored result and idempotent retry path are
required.

### Exactly-once wording

The project should say **exactly-once effect**, not exactly-once message
delivery. Networks and queues may deliver a result multiple times or lose a
response. The invariant is:

> For a valid semantic operation key, the durable state transition occurs zero
> or one time, and every later retry returns the same committed result or a
> definitive failure.

If a database commit succeeded but the process crashed before publishing, the
operation-log recovery scan republishes the stored result. If the database
transaction never committed, recovery must not grant the effect from an
uncommitted in-memory event.

## Transaction boundaries

### Economy and rewards

One purchase, loot claim, quest reward, or trade commit should be one atomic
transaction covering every related mutation. For the current slice this means:

- Purchase: character gold, inventory, vendor stock, operation log, and
  outbox result.
- Loot: reward claim, inventory, operation log, and outbox result.
- Quest turn-in: quest terminal/rewarded status, gold, inventory, operation
  log, and outbox result.
- Trade: both characters' offered items/currency, trade state, operation log,
  and outbox result.

Do not split “remove gold” and “add item” into separate commits. Do not mark a
quest rewarded before its reward rows are durable. The accepted economy and
quest ADRs already require pre-validation and no partial in-memory mutation;
the database transaction must preserve the same property across process
failure.

### Character checkpoint

A safe logout or periodic checkpoint may update position, health/resource
policy, and other explicitly checkpointed fields in one short transaction.
Use an optimistic `revision` or owner/session generation so an old worker
cannot overwrite a newer checkpoint.

### Group and cross-owner operations

An operation involving two characters should use one database transaction if
both durable rows are in the same canonical PostgreSQL database. Acquire rows
in deterministic ID order to reduce deadlock risk. A future cross-database or
cross-service operation should use a coordinator/reservation protocol, not
pretend that two independent commits are atomic.

### Batch writes

Batch independent checkpoint updates to amortize I/O, but keep the unit of
failure and retry explicit. Do not put unrelated purchases, quest rewards, or
trades in a large batch transaction: one conflict or failure should not widen
the rollback scope or create a long lock interval.

## Simulation-to-persistence commit protocol

The hardest design issue is the gap between an in-memory simulation mutation
and a database commit. A queue alone is not durable: a process can die after
mutating memory but before enqueueing, or after enqueueing but before the
worker records the operation.

### Recommended first protocol

Use a persistence service as the commit authority for durable commands:

```text
client intent
    |
    v
simulation validates and derives operation
    |
    v
persistence transaction: operation + domain rows + outbox
    |
    +-- commit succeeds --> durable acknowledgement
    |                            |
    |                            v
    |                    simulation applies/publishes result
    |
    +-- commit fails ------> simulation keeps prior durable state
```

For a responsive real-time loop, the simulation may reserve a pending
operation and continue unrelated ticks, but it must not publish a durable
success or make the pending state observable as final until the commit
acknowledgement arrives. The pending operation needs a timeout and a recovery
path; the simulation can reject or retry it without changing the semantic
operation key.

An alternative is a local durable ingress journal before the in-memory change.
That can be useful for a standalone server, but it introduces a second
durability system and a journal-to-PostgreSQL reconciliation problem. It is
not recommended before the PostgreSQL operation/outbox path has been tested.

### Outbox publication

Write the outbox row in the same transaction as the durable mutation. A
publisher reads committed outbox rows and sends them to the session, replay,
or other service. Marking a row published can race with a crash, so the
consumer must tolerate duplicate event IDs and the publisher must retry safely.
The client presentation model should also tolerate duplicate authoritative
events where the protocol guarantees an event ID and sequence.

The outbox is for committed facts, not a substitute for the operation log.
The operation log answers “what did this retry do?”; the outbox answers “what
committed result still needs publication?”

## Snapshots and journals

### Player persistence

Canonical player state should live in normalized PostgreSQL rows and be
reconstructed by loading the character record plus its inventory, quest, and
other durable components. A denormalized serialized character blob may be a
cache or migration aid, but it should not be the only representation of gold,
items, or quest completion if operators need to inspect and repair them.

### Region and instance snapshots

An active region or instance snapshot should include:

- Region/layer/instance identity.
- Content revision and schema version.
- Simulation tick and monotonic snapshot generation.
- Ownership epoch and worker identity at capture.
- Entity records that are intentionally recoverable.
- NPC health, respawn timers, persistent object state, and world-event state
  where the game design requires continuity.
- A durable-operation watermark or list of committed operation IDs included in
  the snapshot.
- Checksum and byte length.
- Creation time and parent snapshot ID.

Do not snapshot an arbitrary live map while another thread mutates it. The
region owner must reach a tick boundary, either pause mutation briefly or
serialize a stable copy, and record the exact tick/watermark relationship.

For a file-based snapshot format, write to a new temporary file, flush and
sync it according to the Linux durability policy, atomically rename it into a
versioned directory, and update a small manifest only after the payload is
complete. The implementation must test the exact filesystem assumptions; a
successful `write` alone is not a durable commit.

### Recovery order

Recovery should be deterministic:

1. Validate schema version, content revision availability, checksum, and
   snapshot metadata.
2. Load the last valid region/instance snapshot.
3. Reapply only committed durable operations after its watermark.
4. Rebuild transient state: casts, AI decisions, visibility, animation, and
   temporary effects.
5. Reconcile player attachments and session ownership through the coordinator.
6. Mark the new worker epoch and accept new commands only after validation.
7. Re-publish any committed outbox results not yet acknowledged to the
   appropriate sessions.

If the snapshot is missing, corrupt, from an unavailable content revision, or
has an invalid ownership epoch, refuse to activate it and fall back to a
known-good snapshot or safe region reset. Never silently interpret a newer
schema as an older one.

### What can be lost

The initial policy should explicitly allow rollback of transient state:

- Movement since the last safe checkpoint.
- Current casts and channels.
- Temporary auras and AI decisions.
- Nearby visibility and presentation-only objects.

It should not allow loss or duplication of:

- A committed purchase, trade, loot claim, or quest reward.
- Committed currency or inventory changes.
- A completed character lifecycle operation.
- A durable world-event transition once the event promises persistence.

The project owner still needs to choose concrete checkpoint intervals and RPO
and RTO targets. Those values should be measured against storage latency,
worker load, reconnect behavior, and backup restoration time rather than
chosen solely from a round number.

## Disconnect, reconnect, and session recovery

### Recommended behavior

Separate the network session from the durable character:

- A TCP/QUIC connection owns a session, not the character record.
- A session gets a server-issued session ID, authentication context, and
  monotonically increasing session generation or fencing token.
- The character remains durable when the connection disappears.
- The gateway marks the session disconnected and gives the simulation a
  bounded grace period if the game wants combat/logout grace behavior.
- Critical durable operations already in progress continue or become
  retryable by operation ID; they are not silently discarded.
- A safe logout checkpoint persists the approved position and any other
  checkpoint fields. It must not overwrite a newer revision.
- On reconnect, authenticate the account, verify the session generation,
  load the latest durable character state, attach the character to a valid
  region/layer/instance, and send a full authoritative baseline before deltas.
- If the old session reconnects after a newer session has taken ownership,
  reject or revoke the old session rather than allowing two active writers.

Do not use client-supplied position, inventory, quest progress, or gold as
reconnect state. Reconstruct them from PostgreSQL plus the valid region
snapshot and committed operation records.

### Grace-period trade-off

A short disconnect grace period can preserve a player's combat context and
reduce relog friction, but it consumes entity/session memory and complicates
ownership. A longer grace period increases resource use and stale-session
risk. This should be configurable and measured; it is not a persistence
guarantee.

## Region ownership handoff and fencing

The architecture requires one owner to mutate an entity at a time, while
future region, layer, and instance workers must be able to move ownership.
Persistence must prevent a stopped or partitioned worker from writing after a
new owner takes over.

### Proposed ownership record

Maintain a coordinator-owned record keyed by region/layer/instance:

```text
scope_id
owner_worker_id
owner_epoch
lease_expiry
last_checkpoint_id
last_checkpoint_tick
last_durable_operation_watermark
```

Every durable write from a worker carries `owner_epoch`. The persistence
adapter updates only when the epoch is current, for example through a
conditional update or stored procedure. A new owner increments the epoch;
old workers then receive a fencing failure even if they continue running
briefly after a network partition.

### Handoff sequence

1. Coordinator chooses a target worker and reserves a new owner epoch.
2. Source stops accepting new cross-owner transfers and reaches a safe tick
   boundary.
3. Source drains or commits pending durable operations, then records a
   checkpoint with tick, content revision, and operation watermark.
4. Source marks the handoff prepared with the checkpoint ID and epoch.
5. Target validates and loads the snapshot, then acquires the new epoch/lease.
6. Target replays committed operations after the snapshot watermark and
   announces ready.
7. Coordinator routes new commands to the target and retires the source.

If the source dies before step 3, the coordinator recovers from the prior
checkpoint and durable operation log. If it dies after the database commit but
before the acknowledgment, the idempotent operation row decides whether the
effect already happened. If two workers attempt to write, the epoch condition
must reject the stale owner.

Avoid live combat handoff in the first implementation. Keep players in their
current owner during combat, trading, looting, quest turn-in, and other
unsafe transitions, matching the existing layering proposal.

## Crash consistency and failure semantics

The project should document failure behavior by layer rather than claim a
single generic “recovery” guarantee:

| Failure | Expected durable result | Expected transient result | Recovery action |
| --- | --- | --- | --- |
| Client disconnect | Already committed operations remain; uncommitted operation is retried or rejected | Movement/cast state may be checkpointed or reset | Mark session offline, fence it, reconnect from durable baseline |
| Gateway crash | Character/economy state unaffected | Session lost; client reconnects | Re-authenticate and attach a new session generation |
| Simulation worker crash | Committed operation rows and outbox survive | In-flight casts, AI, visibility, and post-checkpoint movement may roll back | Activate last valid snapshot, replay committed operations, rebuild transients |
| Instance worker crash | Player durable state survives; instance result depends on its commit point | Instance combat may reset or roll back | Restore instance snapshot or reset instance under explicit policy |
| Coordinator crash | PostgreSQL and checkpoints remain | Routing/leases pause | Recover coordinator state and renew/acquire epochs safely |
| PostgreSQL temporary outage | No new durable success is acknowledged; queued operations remain bounded | Simulation may continue only for operations explicitly allowed to be provisional | Apply backpressure, reject durable commands safely, alert operators |
| PostgreSQL primary failure with standby | Depends on synchronous/asynchronous replication and last acknowledged commit | Simulation pauses or enters controlled read-only mode | Promote only under a fencing procedure; reconcile RPO |
| Process kill during DB transaction | Transaction commits atomically or is rolled back by PostgreSQL | In-memory tentative result is discarded | Retry with same semantic operation key; return stored result if commit won |
| Process kill during snapshot write | Prior complete snapshot remains authoritative | Recent transient state may be lost | Ignore incomplete file; load prior snapshot and journal watermark |
| Interrupted layer handoff | Prior or new owner is selected by epoch/lease state | Some transient state replays from checkpoint | Reject stale owner writes; resume or abort handoff |
| Partial content deployment | Existing state remains tied to prior content revision | New content is not activated in affected scope | Validate package, activate atomically at a boundary, run explicit migrations |
| Backup corruption or restore error | Live primary is unchanged | Service remains unavailable until alternate restore | Fail restore validation; use another backup set and alert |

The exact RPO after a primary-and-standby disaster remains unresolved. The
project should distinguish a process crash, host crash, storage loss, and
operator error because PITR and replicas address them differently.

## Backups and restore operations

### Recommended initial backup policy

Use PostgreSQL's base-backup plus continuous WAL-archive path as the primary
disaster-recovery mechanism. Keep at least:

- A recent base backup.
- All WAL needed from that base backup through the desired recovery point.
- More than one independent backup set.
- A separate copy with access controls distinct from the live database host.
- The schema/content revision metadata needed to interpret restored rows.
- Region/instance snapshots if those scopes promise continuity beyond player
  state.

Use SQL dumps for small development fixtures, schema inspection, and
human-readable migration checks; do not mistake a periodic dump for a low-RPO
production backup.

PostgreSQL's documentation notes that the interval between base backups should
consider both archive storage cost and recovery replay time, and recommends
keeping several backup sets for confidence. [PostgreSQL PITR guidance](https://www.postgresql.org/docs/current/continuous-archiving.html)

### Restore verification

An untested backup is not an operational capability. Automate a restore drill
that:

1. Creates an isolated PostgreSQL instance.
2. Restores the selected base backup and WAL to a named target time.
3. Runs schema and invariant checks.
4. Loads representative characters, inventories, quests, operation results,
   world-event state, and region checkpoint metadata.
5. Verifies operation-key uniqueness and no negative currency or impossible
   inventory quantities.
6. Boots a read-only or recovery server against the restored data.
7. Records restore duration, WAL replay duration, data age, and failures.

The test must include a target before and after a purchase, loot claim, quest
reward, trade, and content migration. Do not restore a live backup directly
over the primary during a routine drill.

## Migrations and content revisions

### Database schema migrations

Use numbered, source-controlled migrations with a database migration metadata
table. The application should refuse to start if the database is newer than
the binary or if a required migration is missing. Each migration should state
whether it is transactional, estimated lock impact, expected duration, and
whether it supports rollback or requires a forward repair migration.

Prefer an expand/ migrate/ contract sequence:

1. Add nullable or compatibility columns and indexes.
2. Deploy code that writes both old and new representations.
3. Backfill in bounded batches with progress and resumability.
4. Validate counts, constraints, and semantic equivalence.
5. Switch readers at a controlled version boundary.
6. Remove old columns only after no older process can write them.

Do not assume every `ALTER TABLE` is a cheap metadata change; PostgreSQL
documents lock and scan behavior per subform. Test migrations against a
production-shaped dataset and record lock duration.

### Content migrations

Static content revisions are separate from schema versions. A character quest
row should retain the content revision that gave it its meaning. When an item,
quest, NPC template, reward, or objective changes:

- Keep old IDs or compatibility mappings when historical state needs them.
- Do not silently recompute completed rewards from current definitions.
- Store enough immutable result data in the operation log to explain a
  historical reward.
- Run explicit migration code for objective changes, deleted items, or altered
  stack semantics.
- Activate a validated content package atomically for a region or realm
  boundary.
- Keep a prior content package available for rollback and replay.

Content tools should validate references before a package can be activated.
This matches the existing `mmorpg-content` catalog validator, but the future
package format must include a stable revision and compatibility metadata.

## Test and failure matrix

The following tests should be implemented before calling the first persistence
slice reliable.

| Test | Fault injection point | Invariants to assert | Evidence produced |
| --- | --- | --- | --- |
| Purchase retry | Drop response after commit; resend same request | Gold, inventory, stock, and operation effect change once; retry returns identical result | Operation row and before/after ledger |
| Purchase crash | Kill worker before SQL, during transaction, after commit | No partial gold/item state; committed operation can be replayed | Recovery classification and result |
| Loot retry | Duplicate packet and concurrent duplicate commands | One reward claim and one item grant | Unique semantic key plus inventory count |
| Quest reward crash | Kill between reward-row update and response | Quest cannot be completed twice; item/gold matches one committed reward | `character_quests`, operation, outbox |
| Trade crash | Kill at each leg of a two-character transaction | Neither, or both, sides of the trade commit | Transaction result and account balances |
| DB outage | Refuse connections or pause commits | No durable success is reported; bounded queue/backpressure; no unbounded memory growth | Queue depth, errors, client result |
| Serialization conflict | Concurrent updates to the same character/vendor | Retry whole operation or reject safely; no lost update | SQLSTATE/retry metrics and final rows |
| Stale owner write | Pause old worker, promote new epoch, resume old worker | Old write is rejected; new owner remains authoritative | Epoch rejection audit |
| Handoff crash | Kill source before/after checkpoint and target activation | One owner epoch wins; replay does not duplicate rewards | Coordinator state and snapshot watermark |
| Snapshot truncation | Truncate or corrupt temporary/final snapshot | Invalid snapshot is ignored; prior valid snapshot loads | Checksum failure and selected snapshot |
| Journal gap | Remove or reorder post-snapshot operation records in a test copy | Recovery detects a gap rather than silently producing state | Watermark continuity error |
| Backup restore | Restore base backup plus WAL to several target times | All durable invariants hold at each target | Restore logs and invariant report |
| Migration interruption | Kill during backfill/index/constraint phases | Migration resumes or fails closed; no mixed semantic state | Migration ledger and validation report |
| Content revision mismatch | Remove package required by a snapshot or historical operation | Activation fails closed or uses explicit compatibility mapping | Revision-resolution error |
| Reconnect race | Connect two sessions for one character with reordered messages | One current fencing token; stale session cannot mutate | Session generation audit |
| Process restart | Stop and restart the development server during active slice | Characters reload; transient combat reset is explicit; committed rewards survive | Replay transcript and state comparison |

Property-based or model-based tests should eventually generate arbitrary
interleavings of retries, disconnects, worker failures, and concurrent economy
commands. The core's existing pure command/event tests remain valuable, but
they cannot prove cross-process crash semantics.

## Implementation path

1. Add stable `AccountId`/`CharacterId` concepts to the server-facing domain
   without putting PostgreSQL dependencies in `mmorpg-core`.
2. Define persistence DTOs and a trait for loading a character baseline,
   checkpointing safe state, and committing typed durable operations.
3. Build a PostgreSQL adapter and migrations for characters, inventory,
   currency, quests, operation log, outbox, and schema metadata.
4. Add idempotent purchase, loot, and quest-reward integration using semantic
   operation keys and stored results.
5. Add reconnect/session fencing and safe logout checkpoints.
6. Add region checkpoint files with version, tick, operation watermark,
   content revision, checksum, and atomic publication.
7. Add coordinator ownership epochs and a controlled region handoff test.
8. Add backup/restore automation and fault injection before enabling a
   persistent external test community.

The first implementation should keep the PostgreSQL adapter behind the
server/application boundary. `mmorpg-core` should continue to be testable
without a running database, and its current in-memory economy/quest behavior
should remain a useful reference model for adapter tests.

## Unresolved risks and decisions

- What RPO and RTO are acceptable for ordinary movement, player progression,
  economy, and the whole realm after host loss?
- Should the first durable success wait for local WAL flush only, or for a
  synchronous standby as well?
- Which Rust PostgreSQL driver and migration tool fit the project's Linux and
  async-runtime boundaries?
- Should operation results be stored as versioned binary payloads, JSON, or
  normalized result rows?
- How much operation-log history is retained online, and where is archived
  audit history stored?
- Which world objects are canonical across transparent overworld layers, and
  which can reset independently?
- How are world-boss rewards represented when hundreds of players participate
  and multiple region workers contribute events?
- How are instance completion and rollback handled when a player disconnects
  during a final encounter or reward commit?
- What checkpoint interval keeps movement rollback acceptable without causing
  write amplification?
- Which Linux filesystem and storage configuration will be treated as the
  supported durability baseline for region snapshot files?
- Can a worker be fenced strongly enough in the chosen deployment model, or
  is an external coordinator/lease service required?
- What is the operational policy for manual repair of a bad inventory, quest,
  or currency row while preserving the operation audit trail?
- Which schema/content migrations are allowed while players are online?
- What is the maximum recovery replay time before the realm must use more
  frequent snapshots or a standby worker?

## Confidence

**High** that the simulation must not use PostgreSQL as a per-frame state
container, and that economy/reward effects need durable idempotency records.

**Moderate to high** that PostgreSQL is the best initial canonical store for
this project's scale and multi-process path, with SQLite and RocksDB retained
as useful local/test technologies.

**Moderate** for the proposed operation/outbox and region-checkpoint protocol.
It needs an implementation spike with PostgreSQL, forced process termination,
duplicate requests, reconnect races, and ownership handoff before it becomes
an accepted architecture decision.

## Primary sources

- [PostgreSQL 18 documentation](https://www.postgresql.org/docs/current/)
- [PostgreSQL WAL](https://www.postgresql.org/docs/current/wal-intro.html)
- [PostgreSQL transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- [PostgreSQL constraints](https://www.postgresql.org/docs/current/ddl-constraints.html)
- [PostgreSQL `INSERT ... ON CONFLICT`](https://www.postgresql.org/docs/current/sql-insert.html)
- [PostgreSQL backup and restore](https://www.postgresql.org/docs/current/backup.html)
- [PostgreSQL continuous archiving and PITR](https://www.postgresql.org/docs/current/continuous-archiving.html)
- [PostgreSQL standby servers](https://www.postgresql.org/docs/current/warm-standby.html)
- [PostgreSQL WAL configuration](https://www.postgresql.org/docs/current/runtime-config-wal.html)
- [PostgreSQL `ALTER TABLE`](https://www.postgresql.org/docs/current/sql-altertable.html)
- [PostgreSQL upgrading](https://www.postgresql.org/docs/current/upgrading.html)
- [SQLite transactional guarantees](https://www.sqlite.org/transactional.html)
- [SQLite WAL](https://www.sqlite.org/wal.html)
- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite Online Backup API](https://www.sqlite.org/backup.html)
- [RocksDB overview](https://github.com/facebook/rocksdb/wiki/RocksDB-Overview)
- [RocksDB WAL](https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-%28WAL%29)
- [RocksDB checkpoints](https://github.com/facebook/rocksdb/wiki/Checkpoints)
- [RocksDB basic operations and sync writes](https://github.com/facebook/rocksdb/wiki/Basic-Operations)
- [CockroachDB transaction behavior](https://www.cockroachlabs.com/docs/stable/developer-basics.html)
