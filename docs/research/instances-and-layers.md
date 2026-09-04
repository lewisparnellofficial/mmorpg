# Research: Explicit Instances and Transparent Overworld Layers

**Status:** Preliminary findings recorded; prototype and design review required

**Started:** 2026-09-04

## Question

How should the project implement explicit dungeon, raid, and battleground
instances alongside transparent, dynamically created overworld layers while
preserving one authoritative persistent realm?

The investigation covers:

- Instance and layer ownership.
- Placement and admission.
- Party and raid cohesion.
- Transfers and safe points.
- The 200-player world-boss requirement.
- Hot-spot admission and overload behavior.
- Layer draining and retirement.
- Worker, coordinator, and handoff failure recovery.
- Persistent identity and durable reward safety.
- Operational observability.

This is a research record, not an accepted architecture decision. The
recommendations below remain provisional until a local prototype and the
200-player encounter benchmark validate them.

## Scope and non-goals

This document addresses the placement and lifecycle of runtime spaces. It does
not select a final transport, database, container orchestrator, matchmaking
product, or client engine. Those choices remain covered by the related
architecture and research documents.

It also does not define encounter mechanics, dungeon layouts, battleground
rules, or a final player-facing queue UX. It defines the state and ownership
boundaries those systems will need.

## Current project constraints

The accepted requirements are:

- One logical persistent realm should support approximately 5,000 connected
  clients.
- Approximately 200 players must be able to participate in one shared
  overworld activity on one layer. A second layer must not be used to evade
  that test.
- The ordinary overworld should not normally be instanced.
- Multiple transparent overworld layers are allowed as a capacity mechanism.
- Players should not normally select or manage a layer themselves.
- Dungeons, raids, and battlegrounds should be explicit instances.
- Parties and raids should remain coherent when assigned to a space.
- The server is authoritative for movement, combat, rewards, progression,
  inventory, and persistence.
- The server is written in Rust and must retain a path from one Linux process
  to multi-process or multi-machine deployment.

The relevant project documents are:

- [Requirements baseline](../architecture/requirements.md).
- [System overview](../architecture/system-overview.md).
- [World, layers, regions, and instances](../architecture/world-and-layering.md).
- [Networking and replication](../architecture/networking.md).
- [Persistence and recovery](../architecture/persistence.md).
- [Networking and replication research](networking-and-replication.md).
- [Transparent overworld layering research](overworld-layering.md).

## Current implementation state

The current Rust implementation does not yet model instances or layers in
production code.

- `mmorpg-core::World` is a single-owner starter-zone simulation containing a
  town, a field, one vendor, and three enemies.
- Live IDs are currently `EntityId(u64)` values allocated by that one `World`.
- The core has no realm ID, space ID, layer ID, instance ID, worker epoch,
  party, raid, membership, transfer, lease, or persistence record.
- `mmorpg-server` runs one in-memory world behind a temporary line-oriented
  TCP protocol.
- `experiments/layer-manager` is a deterministic simulation of party-preserving
  arrivals, migrations, soft and hard population limits, and empty-layer
  retirement. It has no real entity serialization, network handoff, combat,
  persistence, or failure injection.

Therefore, every production rule in this document is a proposed boundary, not
an existing guarantee.

## Terminology

### Realm

The durable social and economic universe. Characters, inventory, currency,
quest progress, guilds, durable event state, content version, and reward
claims belong to the realm even when a character is currently hosted by a
particular worker.

### Space

The generic runtime term for an authoritative simulation context. A space has
one lifecycle, one current owner, a content/build identity, a membership
policy, and an addressable simulation state.

### Overworld layer

An invisible, runtime-created partition of the ordinary overworld. It uses the
same geographic content and coordinate system as other layers. It exists to
reduce local simulation and replication pressure, not to create a separate
dungeon-like game mode.

### Explicit instance

A deliberately created dungeon, raid, or battleground simulation with explicit
membership, lifecycle, content version, entry/exit rules, and reward policy.

### Region

A spatial simulation and replication unit inside an overworld layer or
instance. A space can initially be owned by one worker while its internal
regions remain logical boundaries for later partitioning.

### Hot spot

A geographic cell or event context whose local work, replication pressure, or
population is approaching a configured budget. Population alone is not enough
to determine hot-spot pressure.

## Evidence and source-backed findings

### Placement is a separate concern from simulation

Amazon GameLift Servers documents game-session placement as finding an
available server resource and starting a game session there. Its placement API
can include a group of desired player sessions and returns connection
information after the session is ready. The queue can prioritize location,
cost, and player latency, and placement requests are retried until their queue
timeout.

- [Amazon GameLift Servers: configure game session placement](https://docs.aws.amazon.com/gameliftservers/latest/developerguide/queues-intro.html)
- [Amazon GameLift Servers: `StartGameSessionPlacement`](https://docs.aws.amazon.com/gameliftservers/latest/apireference/API_StartGameSessionPlacement.html)
- [Amazon GameLift Servers: placement priority](https://docs.aws.amazon.com/gameliftservers/latest/developerguide/queues-design-priority.html)

**Directly sourced fact:** A placement service can choose a hosting resource,
carry group/player data into the session, expose the resulting connection
information, and retry until timeout.

**Project inference:** The realm coordinator should decide *where* a player or
group will go, while a space worker remains responsible for *how* the space
simulates the accepted members. Placement should not be implemented as a
client-side teleport or as a direct mutation by the matchmaking layer.

### Capacity admission can be expressed as atomic allocation

Agones documents `GameServerAllocation` as an atomic allocation of one
GameServer from a selected set. Its selectors can filter on state and
capacity-related counters or lists, and allocation can update counters or
lists as part of the operation. Agones also documents that its query cache is
eventually consistent for performance, while the allocation operation itself
retains its atomicity properties.

- [Agones: `GameServerAllocation` specification](https://agones.dev/site/docs/reference/gameserverallocation/)
- [Agones: counters and lists](https://agones.dev/site/docs/guides/counters-and-lists/)

**Directly sourced fact:** Capacity selection and admission need not be based
on a plain population number; they can use typed capacity dimensions and an
atomic reservation step.

**Project inference:** A layer or instance admission decision should reserve
capacity at the authoritative coordinator before issuing a connection
assignment. Cached metrics may guide candidate selection, but a stale cache
must not be the final authority for a hard-capacity decision.

### Matchmaking and backfill support long-lived social spaces

Open Match describes a ticket as a matchmaking entity representing a player or
group requesting a match. Its backfill documentation explicitly includes
long-lived social or world spaces where existing servers are preferred and
new players can be added as other players leave. It also describes carrying a
backfill identifier through game-server allocation and acknowledging it from
the running server.

- [Open Match: getting started and tickets](https://open-match.dev/site/docs/getting-started/)
- [Open Match: backfill](https://open-match.dev/site/docs/guides/backfill/)

**Directly sourced fact:** A matchmaking system can model both group requests
and filling available capacity in a long-lived server.

**Project inference:** Dungeon and battleground entry may use an explicit
matchmaking ticket, while ordinary overworld arrival should use a lightweight
layer admission request. The two paths can share placement primitives without
giving the overworld the semantics of an instance.

### Runtime spaces need an observable lifecycle

Agones documents a game-server lifecycle including readiness, allocation, and
shutdown. Its SDK supports health checks, state watching, and a graceful
shutdown request; state changes are asynchronously queued and should be
verified through observed state rather than assumed to be immediate.

- [Agones: GameServer lifecycle and specification](https://agones.dev/site/docs/reference/gameserver/)
- [Agones: client SDK lifecycle functions](https://agones.dev/site/docs/guides/client-sdks/)
- [Agones: Rust SDK](https://agones.dev/site/docs/guides/client-sdks/rust/)

**Directly sourced fact:** A hosting process can expose readiness, health,
allocation, and graceful shutdown as separate lifecycle concepts, and callers
should observe state transitions rather than assuming that a state-changing
request has taken effect synchronously.

**Project inference:** Both instances and layers should have an explicit state
machine. “Retired” must be a state transition with a drain protocol, not an
immediate deletion from a coordinator map.

### Dynamic geographic ownership has established precedent

The Hydra paper describes partitioning a large virtual world into regions,
using locality of interest, and dynamically reorganizing regions when static
partitions are insufficient. The existing project layering research also uses
this as precedent for treating migration as an ownership transfer between
simulation servers.

- [A Distributed Architecture for MMORPG](https://www.comp.nus.edu.sg/~bleong/hydra/related/assiotis06mmorpg.pdf)
- [Project research: transparent overworld layering](overworld-layering.md)

**Directly sourced fact:** Dynamic repartitioning and transfer of world objects
are established subjects in distributed virtual-world architecture research.

**Project inference:** The project should make ownership explicit before it has
multiple processes. A future worker handoff should be an explicit protocol
between owners, not an incidental consequence of moving a Rust value between
threads.

### Durable identity should be independent of a current host

RFC 9562 defines UUIDs as 128-bit identifiers intended to provide uniqueness
across space and time. It defines UUIDv7 with a Unix-epoch millisecond field
and random or monotonicity-supporting remaining bits. PostgreSQL has a native
UUID type and documents that UUIDs provide a distributed uniqueness guarantee
that database-local sequence generators do not provide.

- [RFC 9562: UUIDs](https://www.rfc-editor.org/rfc/rfc9562.html)
- [PostgreSQL: UUID type](https://www.postgresql.org/docs/current/datatype-uuid.html)

**Directly sourced fact:** A durable UUID can provide an identity namespace
that is not tied to one database sequence or one process.

**Project inference:** Durable realm, character, instance, layer, transfer,
and reward-operation identities should not be derived from a worker-local
counter. The existing compact `EntityId(u64)` can remain an in-memory live
entity ID, but it should eventually be paired with durable space and character
identities.

### Transactions and serialization failures must be handled explicitly

PostgreSQL documents that serializable transactions can be rolled back with a
serialization failure when concurrent transactions would otherwise produce a
non-serial execution. That behavior is a correctness tool, not an automatic
retry policy.

- [PostgreSQL: transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- [PostgreSQL: `SET TRANSACTION`](https://www.postgresql.org/docs/current/sql-set-transaction.html)

**Directly sourced fact:** A serializable transaction may fail and must be
retried or surfaced to the caller.

**Project inference:** Reward claims, membership commits, and transfer fences
need explicit operation IDs, unique constraints, and retry-safe state machines.
Choosing serializable isolation alone does not prove exactly-once rewards.

### Observability should correlate metrics, logs, and traces

OpenTelemetry defines traces, metrics, logs, and baggage as related telemetry
signals. Its context-propagation documentation describes carrying trace and
span context across process boundaries so that a request can be correlated
across services. Its logging specification recommends recording trace context
in logs for cross-component correlation.

- [OpenTelemetry: signals](https://opentelemetry.io/docs/concepts/signals/)
- [OpenTelemetry: context propagation](https://opentelemetry.io/docs/concepts/context-propagation/)
- [OpenTelemetry: logs](https://opentelemetry.io/docs/specs/otel/logs/)

**Directly sourced fact:** Telemetry context can connect activity across
processes and signal types, but propagated baggage may expose sensitive data
and should be treated carefully.

**Project inference:** A layer transfer and an instance placement should carry
a server-generated operation or trace ID through coordinator, source worker,
destination worker, gateway, and persistence logs. Character IDs and other
private data should not be indiscriminately placed in public telemetry labels.

## Proposed common space model

Use one generic runtime record with explicit kind-specific policy:

```text
SpaceKey
  realm_id
  space_id
  kind = overworld_layer | dungeon | raid | battleground
  geography_id              # overworld map/zone or instance template
  layer_id                  # present for overworld layers
  instance_id               # present for explicit instances
  content_version

SpaceRuntime
  owner_worker_id
  owner_epoch
  lifecycle_state
  membership_policy
  capacity_policy
  created_at / last_activity_at
  snapshot_revision
  active_encounter_ids
```

The exact Rust representation is open, but these concepts should not be
collapsed into a single integer called `layer_id`. An overworld layer and a
raid instance may share worker infrastructure while remaining distinguishable
in permissions, persistence, admission, and player-facing behavior.

### Required lifecycle states

The minimum useful state machine is:

```text
Provisioning -> Ready -> Active -> Draining -> Retired
       |          |        |          |
       +------> Failed <---+----------+
```

Suggested meaning:

- `Provisioning`: space identity exists, but the worker has not acknowledged
  content and runtime readiness.
- `Ready`: the space can accept admission reservations.
- `Active`: one or more members or active world state exist.
- `Draining`: no new admissions; existing members are being moved or allowed
  to leave.
- `Retired`: no simulation ownership remains. The identity remains resolvable
  in history and audit data.
- `Failed`: the coordinator has fenced the owner and recovery or safe reset is
  in progress.

An instance may go directly from `Ready` to `Draining` when a creation request
is cancelled. An overworld layer should not be retired merely because its
population is temporarily low if an encounter or durable world object still
requires it.

## Ownership and single-writer rules

### Recommended ownership tuple

Every active space should have an ownership lease:

```text
Owner = (worker_id, owner_epoch, lease_expiry)
```

`owner_epoch` must increase whenever ownership changes. Commands and durable
handoff records should include the epoch they target. A worker must reject
commands for a stale epoch, and the coordinator must reject a completion from
an old owner after a new owner has been committed.

This is a project recommendation based on the single-owner principle already
recorded in [the system overview](../architecture/system-overview.md), not a
claim that a particular lease technology has been selected.

### Ownership responsibilities

The coordinator owns:

- Space identity and lifecycle state.
- Admission reservations.
- Current owner and epoch.
- Membership intent and assignment records.
- Transfer operation state.
- Recovery decisions and fencing.
- Realm-wide event policy.

The space worker owns:

- Live entity mutation for its space.
- Simulation ticks and local timers.
- Local combat, AI, and transient objects.
- Local spatial indexes.
- Authoritative event emission for entities it currently owns.
- Snapshot and journal checkpoints requested by the coordinator.

The persistence worker owns database and journal I/O, but it must not become a
second simulation owner. A persistence acknowledgment confirms durable state;
it does not grant permission to mutate live combat state.

### No dual ownership during transfer

The source remains the sole live owner until the destination has validated and
durably acknowledged the transfer payload. After the coordinator commits the
new epoch, the source is fenced and releases its live entities. There must not
be a period where both workers accept commands for the same character or
entity.

For ordinary overworld movement, the atomic unit should usually be a player
party rather than an individual player. For a raid or battleground, the atomic
unit is the instance membership record and its reconnecting session, not a
single client socket.

## Explicit instance design

### Placement record

An instance placement should create a durable or journaled record before
connection details are delivered:

```text
PlacementRequest
  placement_id
  realm_id
  requester / party_id / raid_id / matchmaking_ticket_id
  instance_template_id
  instance_kind
  content_version
  requested_capacity
  member_character_ids
  created_at
  expires_at

PlacementResult
  placement_id
  instance_id
  owner_worker_id
  owner_epoch
  connection_assignment_token
  assigned_member_ids
  expires_at
```

The coordinator must atomically decide whether the complete requested group
fits the instance's admission policy. It must not create a five-player dungeon
and then silently strand the fifth member because only four slots were
reserved.

### Dungeon and raid placement

Recommended initial behavior:

1. Verify the requester is authorized to enter the content and has a valid
   party/raid snapshot.
2. Resolve the content version and instance template.
3. Reserve the full group capacity with one placement ID.
4. Create the instance and wait for worker readiness.
5. Commit membership records for all accepted characters.
6. Return an assignment that includes the instance identity and an expiring
   connection token.
7. Keep the source space and group membership visible until each member either
   acknowledges entry or is handled by a timeout policy.

The group should not be split across two dungeon instances unless the game
explicitly defines that behavior. A partial placement should fail or remain
queued as one unit.

For raids, the placement record should preserve raid identity, subgroup
membership, leadership, role metadata, and lockout/reward policy. A raid
member who disconnects should reconnect to the same instance if that instance
is still active and the membership record is valid.

### Battleground placement

Battlegrounds have two-sided admission rather than one cooperative group. The
matchmaker should form the teams and freeze the roster before the instance
starts, subject to the battleground's fill/backfill rules. Team membership,
side, rating or bracket, and reconnect deadline belong in the instance
membership record.

Recommended behavior:

- Matchmaking tickets may represent parties, but a party remains an atomic
  ticket for team assignment.
- The coordinator reserves capacity for both sides before activating the
  battleground.
- Backfill is permitted only if the battleground rules allow it and the new
  player is assigned a new membership operation ID.
- A player reconnecting within the allowed deadline resumes the same
  membership; it does not create a second slot.
- When the match ends, reward eligibility is committed once against the
  membership and match IDs.

### Instance exit and expiration

An instance should not be destroyed at the instant the last client socket
closes. Use a grace period so that reconnecting members can return and so that
pending durable operations can settle. Expiration requires:

- No active members or reconnect reservations.
- No in-flight transfer.
- No pending reward or lockout commit.
- A current snapshot or a deliberate “completed and discardable” marker.
- A durable final lifecycle record.

An explicit instance ID should remain queryable after expiration for support,
replay, reward investigation, and audit purposes even though its simulation
state is no longer resident.

## Transparent overworld layer design

### Layer semantics

An overworld layer is an invisible capacity partition, not a player-selected
realm and not a dungeon instance. It should preserve:

- The same zone and coordinate-space identity.
- The same static NPC, terrain, and placement content.
- Realm-wide social services.
- Realm-wide character progression, inventory, currency, and quest state.
- A clearly defined policy for which transient world objects are layer-local.

It may vary in:

- Nearby players.
- Ordinary enemy lifecycle and respawn state.
- Temporary objects and effects.
- Local encounter participation, except where an event is canonical and
  explicitly bound to one layer.

Layer IDs should be carried in server messages and diagnostics but should not
normally be shown as a choice or a gameplay objective to players.

### Arrival admission

When a player enters an overworld zone or reconnects, the coordinator should:

1. Preserve the current layer when it remains healthy and within policy.
2. Prefer the player's current party or raid layer.
3. Prefer a layer with the same active event context when the player is
   eligible for that context.
4. Consider local population, active combat, AI work, area effects, tick
   duration, replication pressure, and worker health.
5. Reserve one slot for a solo player or the complete group for a party/raid.
6. Admit only when the reservation fits the hard capacity policy.
7. Return the assignment through the gateway, without asking the client to
   choose a layer.

The selection should use hysteresis: a layer that is just below a threshold
should not oscillate members in and out every tick. Soft thresholds can steer
new arrivals; hard thresholds must reject or queue arrivals when admitting them
would violate the service target.

### Group cohesion

Party and raid cohesion is a placement invariant:

- A new group is admitted as one unit.
- A group is never split merely because a preferred layer has room for only
  some members.
- A group transfer is one coordinator operation with one transfer ID.
- If no layer can admit the complete group, the group waits, chooses another
  eligible layer, or receives a clear failure according to policy.
- A player who forms or joins a group should not be moved mid-combat solely to
  make the group co-located; the group operation should wait for safe points.

Cross-layer party chat and social presence should work at realm scope. Direct
world interaction, buffs, trading, and shared combat across layers should be
explicitly denied or modeled through a controlled cross-layer operation; it
should never happen accidentally because two records share a coordinate.

### World-boss policy

The initial world-boss policy should be deliberately strict:

- One canonical world-boss event ID.
- One canonical encounter context.
- One overworld layer for the event.
- An admission target of up to 200 active participants on that layer.
- No automatic layer split to make the benchmark appear to pass.
- A queue or admission rejection above the tested capacity.

This is the only policy that directly satisfies the stated requirement that
200 players participate in one shared activity on one layer. It turns the
200-player encounter into a capacity test rather than a layer-management
problem.

For demand above 200, the project may later choose one of:

1. **Canonical admission ceiling:** keep one event and queue or reject new
   participants.
2. **Event replicas:** create separate encounter contexts tied to a global
   event ID.
3. **Hybrid:** keep a canonical layer for the main event and create explicitly
   defined replicas only when the event definition allows it.

Replicas should not be introduced until the project defines all of the
following:

- Whether boss health and progress are shared or independent.
- Whether contribution is per replica or global.
- Whether a character can appear in more than one replica.
- Whether rewards are per character, per replica, or per global event.
- How a replica failure affects eligibility.
- How realm-wide announcements describe multiple outcomes.

Layering ordinary players around a world-boss is different from instancing the
boss. The event policy must bind the boss to its encounter context so that an
ordinary layer manager cannot accidentally put participants into incompatible
copies.

## Transfers and safe points

### Safe-point definition

A safe point is a server-confirmed state in which the player and all related
durable operations can be serialized and resumed without duplicating or
silently losing game state.

Initial safe-point eligibility should require:

- Not in combat.
- Not casting or being interrupted.
- No pending trade, vendor purchase, loot claim, or quest reward transaction.
- No active vehicle or movement operation that the current slice cannot
  serialize.
- No uncommitted group or battleground roster change.
- A valid position or a selected fallback point such as a town entrance,
  instance entrance, graveyard, or reconnect checkpoint.

The initial system should reject or defer layer migration during combat. It can
later support combat migration only after the snapshot includes all threat,
cast, aura, cooldown, target, encounter, and ownership relationships needed to
resume deterministically.

### Recommended handoff protocol

```text
Coordinator -> source: PrepareTransfer(transfer_id, source_epoch, members)
Source: freeze admission-sensitive mutations at a tick boundary
Source: serialize an explicit transfer bundle and snapshot revision
Source -> coordinator: Prepared(transfer_id, bundle_digest, source_epoch)
Coordinator -> destination: AdmitTransfer(transfer_id, bundle, expected_epoch)
Destination: validate content, identity, capacity, and bundle digest
Destination -> coordinator: Accepted(transfer_id, destination_epoch)
Coordinator: commit assignment and fence source_epoch
Coordinator -> source: CommitTransfer(transfer_id)
Coordinator -> gateway/client: new assignment and snapshot boundary
Destination: resume simulation and rebuild interest state
```

Important properties:

- `transfer_id` is unique and retryable.
- The source does not delete the bundle before commit.
- The destination does not expose the player before admission commit.
- The old epoch cannot accept commands after fencing.
- A repeated `Prepare`, `Admit`, or `Commit` returns the existing operation
  result rather than creating a second copy.
- A destination timeout leaves the source as owner until recovery decides
  otherwise.

For a group, the bundle contains every member and the group relationship. The
coordinator must commit the whole group or leave the source assignment intact.

### Entry and exit transitions

The client should see a connection or space transition, not a local mutation
of its authoritative world identity. A useful sequence is:

```text
current space remains authoritative
  -> gateway receives transfer assignment
  -> client acknowledges assignment/build compatibility
  -> destination sends authoritative initial snapshot
  -> client switches presentation context at snapshot boundary
  -> old visibility is discarded
```

If the client fails during the transition, reconnect should use the durable
assignment record and the last committed safe point. It should not infer the
destination from a stale local layer ID.

## Hot-spot admission and overload policy

### Capacity dimensions

The coordinator should maintain separate soft and hard budgets for at least:

- Connected players.
- Players in combat.
- Participants in a canonical event.
- NPC count and AI work.
- Active casts, area effects, and projectiles.
- Replication candidate count.
- Bytes and messages per second.
- Simulation tick duration and deadline misses.
- Memory and worker health.

The hard limit for the world-boss is not interchangeable with the soft limit
for ordinary zone population. A layer that can host 180 idle players may not be
able to host 180 players with active area effects.

### Admission algorithm

Provisional algorithm:

1. Sample short rolling windows rather than reacting to one noisy tick.
2. Compute a weighted pressure score per layer and hot-spot cell.
3. Mark a layer as `soft_full` when new arrivals would consume the safety
   reserve or the pressure score crosses the soft threshold.
4. Mark it as `hard_full` when a complete reservation would cross any hard
   dimension.
5. Prefer an existing eligible layer below soft limits.
6. Create a new overworld layer only when the hot spot is eligible for
   splitting and no existing layer can admit the reservation.
7. Route a group as one reservation.
8. Queue or reject when all valid destinations are hard full.
9. Emit an admission decision with reason and pressure snapshot for debugging.

Layer creation should be rate-limited and should have a minimum lifetime. This
prevents a burst of reconnects from creating many empty layers and prevents
oscillation while a hot spot is still settling.

### What may be split

Ordinary overworld traffic may be split by layer if:

- The event context is layer-local or there is no event context.
- Required social interactions remain valid.
- Static placements and content versions match.
- The split does not violate a canonical event policy.

The following should not be split automatically:

- A canonical 200-player world-boss encounter.
- A party or raid that has been promised one shared encounter context.
- An in-progress cross-layer transfer.
- A durable operation whose ownership is not yet committed.

## Layer retirement and merging

### Draining state

Retirement should begin by marking a layer `Draining`:

- No new player or group admission is allowed.
- Reconnects are still directed to the layer long enough to complete or cancel
  an in-flight handoff, unless a failure policy says otherwise.
- New canonical event participation is rejected.
- Existing players receive no player-facing layer-management requirement.
- The coordinator selects safe-point migrations to an eligible destination.

Do not merge layers merely because their population is low. First verify that
their local event, object, spawn, and reward semantics can be combined. For the
initial implementation, retirement should require an empty layer and no
pending local durable operation.

### Retirement conditions

A layer can become `Retired` only after:

- Population is zero.
- No active encounter context remains.
- No transfer is prepared or awaiting commit.
- No durable reward or inventory operation is awaiting ownership resolution.
- The latest snapshot/journal position is recorded.
- The coordinator has removed it from admission candidates.
- A grace period has elapsed to absorb disconnect/reconnect races.

Keep a tombstone containing the layer ID, final owner epoch, final snapshot
revision, retirement reason, and timestamps. This makes stale messages and
support investigations explainable without keeping the simulation alive.

### Merging policy

Merging should initially mean “drain one layer into another at safe points,”
not a live union of two simulations. A merge is safe only if the destination
has capacity for complete groups and the two spaces have compatible content
and event policies. Ordinary transient enemies can be reset or respawned
according to content rules; durable player state must never be merged by
copying two competing values.

## Failure recovery

### Worker failure

On missed health/lease deadlines, the coordinator should:

1. Fence the failed `(worker_id, owner_epoch)`.
2. Stop new assignments to that owner.
3. Mark the space `Failed`.
4. Determine the last durable snapshot and journal position.
5. Reconstruct the space on a replacement worker if the recovery policy
   supports it.
6. Otherwise move players to their last safe point in an available space,
   explicitly resetting transient combat state.
7. Reconcile pending reward operations through idempotent durable records.
8. Emit a recovery result for each affected membership and operation.

For an overworld layer, ordinary enemy combat and local effects may be reset
after a crash if the project chooses a safe-reset policy. Player inventory,
currency, progression, quest completion, and rewards must be recovered from
durable records and must not be reconstructed from an untrusted client.

For an instance, the policy should be explicit per content type:

- **Recoverable instance:** restore from snapshot plus journal, retaining the
  same instance ID and membership.
- **Abortable instance:** close the instance, return members to safe points,
  and mark rewards according to a documented compensation/lockout policy.

The first vertical slice can use the abortable policy because it has no real
instance content. The first production dungeon should not silently choose a
different policy at runtime.

### Coordinator failure

The coordinator is the authority for space assignment and ownership epochs. A
single in-process coordinator is acceptable for early development, but it is a
single point of failure and cannot be called production-capable. A future
deployment needs a recoverable coordinator state containing:

- Current space lifecycle and owner epochs.
- Membership and admission reservations.
- Transfer operations and their phases.
- Canonical event bindings.
- Durable operation IDs and outcomes.

On coordinator restart, workers should not accept new cross-space operations
until the coordinator has re-established the current epoch and membership
view. This prevents a worker with stale local knowledge from becoming an
accidental second owner.

### Handoff failure matrix

| Failure point | Safe result | Required recovery action |
|---|---|---|
| Before source prepare | No state change | Retry or cancel transfer |
| After source prepare, before destination admit | Source remains owner | Expire or retry the transfer ID |
| Destination admitted, before coordinator commit | Destination quarantines bundle | Abort destination copy or retry commit; no client exposure |
| Coordinator committed, source not released | New epoch fences source | Reconcile and delete stale source copy |
| Client disconnect after commit | Destination retains membership | Reconnect using durable assignment |
| Reward write timeout | Outcome unknown | Retry same operation ID; unique record prevents duplication |
| Worker crash after reward write | Durable result wins | Rebuild presentation from durable operation result |
| Worker crash before reward write | No committed reward | Revalidate eligibility and retry once through operation ID |

## Persistence identity and state ownership

### Identity vocabulary

The design should distinguish these identities:

| Identity | Scope | Durable? | Purpose |
|---|---|---:|---|
| `RealmId` | Realm | Yes | Selects the persistent universe |
| `CharacterId` | Realm | Yes | Stable player-character identity |
| `SpaceId` | Realm | Yes | Stable identity for an instance or layer lifetime |
| `InstanceId` | Realm | Yes | Explicit dungeon/raid/battleground history and membership |
| `LayerId` | Realm | Recommended | Overworld layer assignment and diagnostics |
| `EntityId` | Space/runtime | Usually no | Live simulation entity lookup |
| `WorkerId` | Deployment | Yes in logs | Host identity for ownership history |
| `OwnerEpoch` | Space | Yes/journaled | Fences stale owners |
| `TransferId` | Realm | Yes while active | Idempotent handoff operation |
| `PlacementId` | Realm | Yes while active | Idempotent admission operation |
| `RewardGrantId` | Realm | Yes | Exactly-once durable reward claim |
| `ContentVersion` | Content | Yes | Reconstructs the rules and definitions used |
| `WorldEventId` | Realm | Yes | Canonical world event identity |

`LayerId` must not be reused while stale messages, reconnect tokens, or audit
records could still refer to it. A retired layer may have a tombstone, but a
new layer should receive a new ID even if it uses the same zone coordinates.

### Durable versus transient records

Persist or journal at least:

- Character safe position and current `SpaceId` assignment.
- Explicit instance membership and reconnect deadline.
- Dungeon/raid lockout and battleground result state.
- Placement, transfer, and reward operation IDs and outcomes.
- Content version and instance template ID.
- Canonical world-event identity and reward eligibility.
- Layer retirement and recovery tombstones.

Do not persist every movement update or every local AI decision. Checkpoint
ordinary movement periodically and at safe logout; persist transactional
changes immediately or through a durable operation journal.

### Reward and membership idempotency

A durable reward claim should have a unique key based on the game rule, not on
the client retry count. For example:

```text
unique(realm_id, character_id, world_event_id, reward_rule_id)
unique(realm_id, character_id, instance_id, completion_id)
unique(realm_id, placement_id)
unique(realm_id, transfer_id)
```

The precise uniqueness key depends on the reward policy. The important rule is
that an old layer, a retrying client, a restored worker, and a duplicate event
replica cannot each grant the same durable reward.

## Observability requirements

### Structured identity fields

Every space lifecycle, admission, transfer, and recovery event should be
structured rather than only printed as a sentence. Recommended fields:

- `trace_id` or server-generated operation correlation ID.
- `realm_id`.
- `space_id`, `space_kind`, `zone_id`, and `layer_id` where applicable.
- `instance_id` or `world_event_id` where applicable.
- `worker_id` and `owner_epoch`.
- `character_count`, `party_id`, `raid_id`, or `match_id` as permitted.
- `placement_id` or `transfer_id`.
- `content_version`.
- Lifecycle transition, result, reason, and duration.

Avoid putting character IDs, names, or raw client addresses into high-cardinality
metrics. They may be present in access-controlled structured logs where needed
for support and audit.

### Metrics

Record at least:

**Space lifecycle and ownership**

- Active spaces by kind and lifecycle state.
- Space age, population, and last activity.
- Worker lease age, epoch changes, and fencing failures.
- Snapshot age, journal lag, and recovery duration.

**Admission and placement**

- Admission requests, accepts, rejects, and queue depth.
- Reject reasons: hard capacity, no compatible content, group too large,
  event-locked, worker unhealthy, or timeout.
- Group placement wait time and full-group reservation failures.
- Instance creation and readiness latency.
- Battleground fill/backfill latency.

**Hot spots and layers**

- Players, combatants, NPCs, effects, and replication candidates per space.
- Region tick duration, p95/p99 tick duration, and deadline misses.
- Soft-full and hard-full durations.
- Layer creation, migration, drain, and retirement counts.
- Group migration blocks by safe-point reason.

**Transfers and correctness**

- Transfer phase counts and duration by phase.
- Source/destination rejection and timeout counts.
- Stale-epoch command counts.
- Duplicate ownership invariant failures.
- Duplicate placement, transfer, and reward-operation attempts.

**Durability and recovery**

- Reward journal commit latency and retry count.
- Serialization failures and transaction retries.
- Worker failure count and affected memberships.
- Safe-reset and restored-instance counts.
- Reconnect success/failure after a space transition.

**Network and clients**

- Assignment acknowledgment latency.
- Initial snapshot size and delivery time.
- Per-space and per-client replication bytes/messages.
- Queue depth and backpressure.
- Disconnects during placement or transfer.

### Traces and logs

Trace or correlate the entire operation:

```text
client intent
  -> gateway/session
  -> placement or layer coordinator
  -> source worker
  -> destination worker
  -> persistence/journal
  -> assignment acknowledgment
```

Use span attributes for bounded dimensions such as space kind, zone, result,
and failure class. Use structured logs for the full operation and IDs. Keep
trace/baggage propagation server-generated or sanitized; do not trust client
supplied trace context as an authorization or ownership field.

### Operator views

The first useful local admin view should answer:

- Which worker owns each space and under which epoch?
- Which spaces are soft-full, hard-full, draining, or failed?
- Which groups are waiting for placement or blocked from migration?
- Which world event and encounter context bind a hot spot?
- Which transfers are stuck and at which phase?
- Which rewards have an unknown or retrying outcome?
- What are the p95/p99 tick durations and replication budgets for the 200-player
  encounter?

An event log without these query dimensions will make layer bugs extremely
hard to diagnose.

## Comparison of policy choices

| Concern | Conservative first policy | More ambitious future policy | Recommendation |
|---|---|---|---|
| Instance placement | One complete group per placement | Partial/backfill-aware placement | Start complete; add backfill per mode |
| Layer admission | Existing layer or new layer; queue above hard limit | Predictive load-aware admission | Start threshold + hysteresis |
| Party migration | Whole party at safe point | Combat migration with full snapshot | Defer combat migration |
| World boss | One canonical layer, ceiling at 200 | Replicas linked to global event | Benchmark canonical policy first |
| Layer retirement | Empty + grace period + no pending work | Live merge with object reconciliation | Start drain-to-empty |
| Worker failure | Safe reset or restore from snapshot | Deterministic continuation of combat | Start safe reset, then experiment |
| IDs | Durable space/operation IDs plus runtime IDs | Fully globally distributed ID service | Separate identity scopes now |
| Observability | Structured events and bounded metrics | Full distributed traces and replay | Add correlation fields before split |

## Recommended initial architecture

The current project should implement the following sequence:

1. Add a typed `SpaceId`/`SpaceRef` boundary to the simulation-facing API,
   without yet changing the starter-zone gameplay rules.
2. Add an in-process coordinator that owns one overworld layer and can create
   additional layer records without moving live entities.
3. Add explicit party-aware admission reservations and invariant tests.
4. Add a deterministic transfer state machine over serialized test bundles,
   with stale-epoch and retry tests.
5. Add a minimal explicit instance worker using the same space lifecycle but a
   separate instance identity and membership policy.
6. Run the 200-player world-boss simulation in one canonical layer before
   enabling automatic splitting for any event context.
7. Add durable operation IDs and failure-injection tests for placement,
   transfer, loot, and rewards.
8. Add structured metrics and logs before introducing process boundaries.

This keeps the early implementation runnable on one Linux machine while
preserving the boundaries needed for later worker and process separation.

## Prototype and benchmark plan

Create a standalone deterministic experiment before accepting the production
design. It should model:

### Instance cases

- One party requesting a dungeon.
- A raid requesting more members than capacity.
- Two simultaneous placement requests competing for the last slots.
- Battleground two-sided roster formation.
- Disconnect and reconnect during instance startup.
- Instance expiration with a pending reward retry.

### Layer cases

- Solo arrivals converging on one field hot spot.
- Parties arriving when only partial capacity remains.
- A world-boss event with 200 active players on one layer.
- A demand wave above the world-boss ceiling.
- Safe-point group migration after a layer becomes soft-full.
- Busy/combat groups blocking migration.
- Layer drain, reconnect, and grace-period retirement.

### Failure cases

- Destination failure before admission.
- Coordinator failure after destination admission but before commit.
- Source failure after commit.
- Duplicate transfer messages.
- Duplicate reward messages after worker restart.
- Stale owner commands after epoch fencing.

### Measurements

Record measured values separately from modeled estimates:

- Admission latency.
- Group queue latency.
- Transfer duration by phase.
- Snapshot serialization size/time.
- Duplicate-operation rejection rate.
- Tick duration and deadline misses.
- Memory per member, entity, and space.
- Replication candidate and bandwidth costs.
- Recovery duration and number of safe resets.

The existing [layer-manager experiment](../../experiments/layer-manager/src/main.rs)
is a useful starting point for population, group cohesion, migration cooldown,
and retirement scenarios, but it must be extended with ownership epochs,
transfer phases, event bindings, failure injection, and durable operation IDs
before it can validate this design.

## Risks and rejected alternatives

### Treat every hot spot as an instance

Rejected as the default because it would make ordinary overworld events
behave like dungeons and would violate the requirement that the overworld not
normally be instanced. It may still be appropriate for explicitly authored
scenarios whose game design calls for an instance.

### Split a world boss across ordinary layers

Rejected for the initial world-boss policy because it would fail the direct
200-player-on-one-layer requirement and would introduce reward and progress
semantics before the canonical encounter is benchmarked.

### Move individual players whenever a layer is full

Rejected because it splits groups, increases churn, and can intersect combat
or durable operations. Whole-group safe-point transfers are more conservative.

### Make the database the live simulation owner

Rejected because per-frame simulation state and database transactions have
different latency and consistency requirements. The database/journal should
own durable facts; a single space worker should own live simulation mutation.

### Use population as the only capacity metric

Rejected because 200 idle clients and 200 players running area-effect combat
do not represent the same simulation or replication load.

### Delete a layer as soon as it becomes empty

Rejected because reconnect races, delayed messages, pending rewards, and stale
ownership records need a drain and grace period.

## Confidence and assumptions

**Moderate confidence** in the separation between explicit instances and
transparent layers, single-writer ownership, group-atomic admission, safe-point
transfers, and drain-based retirement. These are consistent with the project’s
current architecture and with the cited hosting, matchmaking, distributed
simulation, persistence, and observability sources.

**Low-to-moderate confidence** in numerical thresholds, the correct worker
granularity, and the best recovery policy. Those depend on the actual Rust
simulation, network protocol, content density, and the 200-player encounter.

Assumptions that must be tested:

- One coordinator can make admission decisions quickly enough for the initial
  target.
- A serialized safe-point bundle is small enough for timely handoff.
- The initial encounter can be simulated on one layer without hiding players
  behind separate event contexts.
- Durable operation IDs and unique records can provide the required reward
  behavior under retries and worker recovery.

## Unresolved questions

1. Is `LayerId` durable for the life of a layer, or is a stable `SpaceId` the
   only persisted identity while layer labels remain ephemeral?
2. What exact instance capacity and roster rules apply to the first dungeon,
   raid, and battleground prototypes?
3. Are party members allowed to enter an instance while one member is
   disconnected, and how long is the reservation held?
4. Should a party reform or raid subgroup change force a safe-point migration,
   or only affect future placement?
5. Which overworld objects are layer-local, layer-reconciled, or realm-
   canonical? This is especially important for resource nodes, rare spawns,
   gates, and world-boss phases.
6. Does the first world boss queue players above 200, reject them, or expose a
   separate explicitly authored event replica later?
7. What happens to a world-boss when its canonical layer worker fails during
   combat: restore, reset, or abort with a durable event outcome?
8. Which safe points are available in each zone, and can the client load a new
   space without disconnecting its session?
9. What is the maximum acceptable transfer duration and snapshot size?
10. How are cross-layer whispers, party chat, friends, guilds, and group
    finding represented when the gameplay spaces differ?
11. Can the coordinator be single-threaded in the first multi-process
    deployment, or does admission need a replicated state machine earlier?
12. Which durable store and journal format will provide the required atomic
    operation records and recovery tooling on Linux?
13. What bounded metric dimensions are acceptable for per-layer dashboards
    without creating unmanageable telemetry cardinality?
14. Should instance IDs and operation IDs use UUIDv7, a Snowflake-like format,
    or another project-owned generator?
15. What anti-cheat and authorization checks are required when a reconnecting
    client presents an assignment token for an instance or layer?

## Sources

Primary and official sources used in this record:

- [RFC 9562: Universally Unique Identifiers](https://www.rfc-editor.org/rfc/rfc9562.html)
- [PostgreSQL: UUID type](https://www.postgresql.org/docs/current/datatype-uuid.html)
- [PostgreSQL: transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- [PostgreSQL: `SET TRANSACTION`](https://www.postgresql.org/docs/current/sql-set-transaction.html)
- [Amazon GameLift Servers: configure game session placement](https://docs.aws.amazon.com/gameliftservers/latest/developerguide/queues-intro.html)
- [Amazon GameLift Servers: `StartGameSessionPlacement`](https://docs.aws.amazon.com/gameliftservers/latest/apireference/API_StartGameSessionPlacement.html)
- [Amazon GameLift Servers: placement priority](https://docs.aws.amazon.com/gameliftservers/latest/developerguide/queues-design-priority.html)
- [Agones: GameServer specification and lifecycle](https://agones.dev/site/docs/reference/gameserver/)
- [Agones: GameServerAllocation specification](https://agones.dev/site/docs/reference/gameserverallocation/)
- [Agones: counters and lists](https://agones.dev/site/docs/guides/counters-and-lists/)
- [Agones: client SDK lifecycle](https://agones.dev/site/docs/guides/client-sdks/)
- [Open Match: getting started](https://open-match.dev/site/docs/getting-started/)
- [Open Match: backfill](https://open-match.dev/site/docs/guides/backfill/)
- [OpenTelemetry: signals](https://opentelemetry.io/docs/concepts/signals/)
- [OpenTelemetry: context propagation](https://opentelemetry.io/docs/concepts/context-propagation/)
- [OpenTelemetry: logs](https://opentelemetry.io/docs/specs/otel/logs/)
- [A Distributed Architecture for MMORPG](https://www.comp.nus.edu.sg/~bleong/hydra/related/assiotis06mmorpg.pdf)

## Recommendation summary

Adopt a common, lifecycle-managed space abstraction with two policy families:

- **Explicit instances** are created through complete-group or match roster
  placement, have explicit membership and expiration, and own local encounter
  state and reward eligibility.
- **Transparent overworld layers** are coordinator-assigned partitions of the
  ordinary persistent world, use the same geography and realm services, and
  are admitted, migrated, and retired without normal player-facing layer
  management.

Implement one authoritative owner per space with monotonically fenced epochs,
group-atomic reservations, safe-point transfers, idempotent operation IDs, and
drain-based retirement. Keep the first world boss canonical in one layer and
benchmark 200 active participants there. Add replicas, combat migration, live
merging, and deterministic instance continuation only after targeted failure
and load experiments demonstrate that their semantics are safe.
