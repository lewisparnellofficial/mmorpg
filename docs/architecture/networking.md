# Networking and Replication

**Status:** Proposed; experiment required

The current wire crate now has a compatibility-only control profile for
version rejection, a length-delimited additive-event helper, and an additive
request/response correlation wrapper. These are validated codec boundaries;
server/client cutover, retained previous-client fixtures, content-digest
negotiation, and sequence-aware resync are still required before the gameplay
protocol version can change.

## Core model

The server is authoritative. Clients send input or intent, and the server sends authoritative state and event results.

The protocol should support independent delivery classes rather than treating every packet identically.

## Message classes

### Reliable ordered

- Login and session state.
- Character selection.
- Chat messages.
- Inventory and quest updates.
- Group and guild operations.
- Important world events.

### Reliable retryable commands

- Loot requests.
- Vendor purchases.
- Trades.
- Quest reward claims.
- Other operations with unique idempotency keys.

### Unreliable state updates

- Position snapshots.
- Facing direction.
- Movement state.
- Animation state.
- Nonessential nearby-object updates.

New state must not be blocked behind stale movement or animation state.

## Simulation timing

A provisional model is:

- Fixed server simulation tick, likely in the 20–30 Hz range.
- Client rendering at its own frame rate.
- Client interpolation for remote entities.
- Limited client prediction for local movement.
- Server reconciliation when prediction diverges.
- Tick numbers or server timestamps on authoritative events.

The development server now routes every world step, including empty ticks,
through one server-owned fixed-tick timing path. The current default is 20 Hz;
the live server uses a two-tick cast time and two-tick basic-combat cooldown,
while the compatibility `World::step` API retains zero cast/cooldown timing for
older core callers. Empty server ticks still advance deferred work and enemy
lifecycle state through the same owner.
This is an implementation boundary, not a capacity result. The exact tick
budget must still be established through profiling, especially for the
200-player encounter. Each live server tick records elapsed simulation work,
the maximum observed duration, and deadline misses; deterministic shutdown
prints those counters for experiment capture. A deadline miss is telemetry,
not proof that the server can safely absorb the workload.

Typed intake is bounded per session and globally, and the server interleaves
decoded per-session queues before handing commands to the single world owner.
Each fixed-tick world step also admits at most 256 commands; overflow is
rejected (including a durable failure result where applicable) rather than
carried as hidden simulation debt.
The development listener also has a 256-client admission bound and rejects an
over-capacity connection before allocating a session. These are fairness and
resource guards for the development path, not a production admission or
backpressure policy.

## Interest management

The server must avoid broadcasting the entire world to every connected client. It should use:

- Spatial grids, cells, or another spatial index.
- Visibility and perception rules.
- Delta snapshots.
- Entity update priorities.
- Different update frequencies by distance and activity.
- Bandwidth budgets per client.
- Backpressure handling.

Players should not receive information about entities they cannot legitimately perceive. This is both a scalability and gameplay-security requirement.

The current development boundary has a first privacy floor: typed bootstrap
snapshots contain only the bound player's player record, and player-private
vendor, loot, quest, and transaction events are addressed to that session.
The authoritative core owns a bounded party registry and the development
server filters invite and membership events to current/former party members.
Public combat and movement events now receive a 45-unit Nearby filter before
enqueueing; player movement is replaceable and coalesced per recipient/entity,
while transactions, party events, and other authoritative results remain
reliable ordered messages. Version-3 private bootstrap snapshots now carry a
separate party summary of membership and leadership IDs, while nearby detail
remains subject to the existing interest filter. Production spatial indexing,
delta snapshots, and bandwidth budgets remain future work. A disconnected authenticated character is now retained as a
bounded server-side detached binding for five seconds (100 fixed ticks); a
reconnect for the same account and character rebinds the existing runtime
entity and receives a fresh private snapshot, while expiry issues the normal
authoritative leave. This is development-session grace, not production session
resume or gateway failover.

## Large encounter requirements

The server must be benchmarked with:

- 200 players in one layer.
- One shared combat activity.
- Multiple roles and abilities active.
- Area-of-effect abilities.
- Boss AI and adds.
- Threat, damage, healing, auras, and interrupts.
- High-frequency movement and combat replication.
- Players joining and leaving.
- Packet loss, latency, and reconnect scenarios.

The test must measure region tick duration, network bandwidth, memory, CPU, packet loss, client frame time, and database activity.

## Protocol requirements

The protocol should have:

- Explicit protocol versioning.
- Content/build compatibility checks.
- Length and rate limits.
- Malformed-message handling.
- Sequence numbers or tick numbers.
- Session authentication.
- Duplicate-command protection.
- Request/response correlation for immediate session and bootstrap operations.
- Reconnect and session-resume behavior.
- Clear separation between gameplay, chat, and administration traffic.

The transport choice remains open. The important requirement is support for reliable operations and non-blocking frequent state updates.

## Client responsibilities

The client may:

- Predict local movement.
- Interpolate remote movement.
- Display effects before confirmation where safe.
- Render server results.
- Cache static content.

The client must not decide:

- Its authoritative position.
- Damage or healing results.
- Loot outcomes.
- Quest completion.
- Currency changes.
- Ability legality.

## Initial networking experiment

Create a headless simulated-client test that can maintain 5,000 mostly idle connections and separately drive 200 active players in one world-boss scenario. Record the results in `docs/experiments/` before accepting a production network design.
# Typed client session boundary

The renderer-independent [`mmorpg-client-session`](../../crates/mmorpg-client-session/)
crate owns the typed development handshake: authentication, character-list
receipt, explicit character selection, world entry, bootstrap, and ready
state. Socket workers submit decoded `mmorpg-wire` messages to this boundary;
the renderer does not decide whether gameplay intent is legal.

Disconnect clears the presented-world marker, selected character, bootstrap,
and pending gameplay intents. Reconnect therefore starts a new authenticated
development session and requires explicit selection again. Commands submitted
before `Ready` are rejected, and command output is bounded by count and encoded
payload bytes.

This is an implementation step toward the session policy in `PLAN.md`; the
live development wire path now carries sequence numbers and the client
session applies gap-recovery policy, but it does not provide production session
resume.

The session boundary now also exposes a sequenced-message policy for the
schema migration: complete snapshots establish the baseline atomically,
contiguous events advance it, duplicates are ignored, and gaps or bounded
bootstrap-buffer overflow request a fresh snapshot. The typed development
listener carries the sequence wrapper; retained previous-client fixtures and
full old-client interoperability evidence are still required before a
gameplay protocol-version bump.

The typed command schema also has a session-local request wrapper. A nonzero
request ID is carried from `ClientCommand::Request` to
`ServerMessage::Response` for immediate authentication, character, content,
and world-connection results, including world connection completion that is
delayed until the next simulation join step. The per-session delivery sequence
still orders those responses. This first boundary deliberately does not assign
request IDs to every asynchronous gameplay event: those remain correlated by
the authoritative event payload and delivery sequence. Retryable economic and
quest commands retain their separate durable operation IDs for deduplication
and restart recovery.
