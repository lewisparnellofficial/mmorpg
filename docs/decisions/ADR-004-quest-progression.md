# ADR-004: In-memory authoritative quest progression

**Status:** Accepted for the initial quest vertical slice

**Date:** 2026-09-04

## Context

The starter zone now has a meaningful town/field loop:
players can fight field enemies, claim authoritative loot, return to town,
and use a vendor. The progression loop gives players a reason to move between
the town and the field.

The first quest must fit the existing boundaries:

- Static content is shared by the server, future client, and development
  tools through `mmorpg-content`, as established by ADR-003.
- Mutable gameplay state is owned and advanced by the authoritative,
  engine-independent simulation in `mmorpg-core`, as established by ADR-001.
- Rewards and inventory changes must use the same authoritative transaction
  boundary as the in-memory economy, as established by ADR-002.
- The first implementation must remain dependency-free, runnable on Linux,
  and usable in the single-process development server.

Quest progress cannot be accepted from the client as a claimed fact. A client
may request that a player accept a quest or turn one in, but the server must
derive objective progress from events that the authoritative simulation has
already resolved. Otherwise a modified client could submit arbitrary kill
counts or complete quests without performing the required activity.

The initial slice needs one small quest, `Clear the Field`, whose
objective is to defeat a specified number of field wolves and whose reward is
defined by shared content. The design should nevertheless establish the
boundary that later quests, dialogue, objectives, and rewards can extend.

## Decision

Implement quest progression as an in-memory, server-authoritative subsystem
split across the existing content and simulation crates.

### Static definitions live in `mmorpg-content`

`mmorpg-content` remains the source of truth for immutable quest definitions.
Each definition uses stable content IDs and contains, at minimum:

- Quest ID, name, and player-facing description.
- Quest-giver and turn-in NPC references.
- One or more objective definitions.
- Objective target identifiers and required counts.
- A reward definition containing gold and/or item rewards.

The initial objective kind is a kill objective that identifies the eligible
NPC template or enemy kind and a required count. The initial reward uses the
existing item definitions and gold representation. Quest definitions refer to
content, not live entity instances; a quest should remain valid if a specific
field wolf entity is defeated, respawned, or replaced.

The catalog validator must reject malformed quest content, including empty
names or objectives, zero objective counts, invalid NPC references, invalid
item references, and zero or otherwise invalid reward quantities. The
validated catalog is compiled into the binary for this increment, consistent
with ADR-003. It is not a database or an editor document format.

### Runtime state lives in `mmorpg-core`

The authoritative world stores per-player quest state in memory. Runtime state
is keyed by stable player identity and references a `QuestId`; it does not copy
the complete static definition into mutable player state.

The minimum state for the first objective model is:

- Quest ID.
- Lifecycle status.
- Objective progress counters.

The lifecycle must distinguish at least:

- Not accepted.
- Active and in progress.
- Ready to turn in.
- Completed and rewarded.

An active quest becomes ready to turn in when its authoritative objective
requirements are met. A completed quest is terminal for the initial slice.
The terminal state is what prevents a repeated turn-in from granting another
reward.

### Progress derives from authoritative enemy defeat events

When the simulation resolves an attack that defeats an eligible enemy, it
emits the existing authoritative enemy-defeat event. Quest progression
consumes that result inside the same simulation ownership boundary and updates
eligible active quest states.

The client never supplies the target kind, kill count, or completed status as
part of a progression command. Progress is derived from the defeated enemy's
authoritative runtime identity and static content classification. A defeat of
an unrelated NPC kind does not advance a kill objective. Progress is capped at
the objective requirement so additional valid defeats cannot create an invalid
counter.

The observable event order for a qualifying defeat is:

1. The server resolves the attack and marks the enemy defeated.
2. The server emits the authoritative enemy-defeat result.
3. The server advances eligible quest objective counters.
4. The server emits quest-progress or ready-to-turn-in events as appropriate.

This is an in-process command/event boundary, not yet a durable event-sourced
log. The ordering is nevertheless explicit so a future persistence or
replication adapter can preserve the causal relationship.

### Acceptance and turn-in are authoritative commands

The core exposes intent commands equivalent to:

- List available quest offers for a quest-giver.
- Accept a quest.
- Turn in a quest.

The development server may expose these through temporary line-oriented
commands, but the protocol is only an adapter. The core remains responsible
for all validation and state transitions.

Quest acceptance validates, at minimum:

- The player exists and is authorized to act for the supplied player ID.
- The quest and quest-giver references exist in the validated content catalog.
- The player is within the quest-giver interaction range and appropriate
  world location.
- The quest is available to that player.
- The player does not already have the quest active or completed.
- Any initial prerequisite rules represented by the first schema are met.

Quest turn-in validates, at minimum:

- The player exists and is near the quest's turn-in NPC.
- The quest is active and ready to turn in.
- The authoritative progress satisfies every objective.
- The quest has not already been completed or rewarded.

Rejected commands produce authoritative rejection events and do not mutate
quest progress, completion state, inventory, gold, or any other reward state.

### Rewards and completion are one atomic transition

Turning in a ready quest computes and validates the complete reward before
mutating player state. The operation then grants all configured rewards and
changes the quest state to completed as one simulation-owned transition.

If any reward cannot be granted—for example, because inventory capacity is
insufficient or a referenced item is invalid—the operation fails without:

- Removing or changing gold.
- Adding a partial item reward.
- Marking the quest completed.
- Making a later valid turn-in impossible.

After a successful transition, subsequent acceptance and turn-in attempts are
rejected. This provides exactly-once reward behavior within the current
single-owner in-memory world, including when duplicate commands are queued in
the same simulation step.

Exactly-once behavior here is not a claim about process crashes or multiple
servers. Durable idempotency, crash recovery, and cross-worker concurrency are
explicitly deferred to the persistence and multi-worker work described in the
roadmap.

### Preserve the dependency-free core

`mmorpg-core` must continue to depend only on the shared content crate and
Rust’s standard library. Quest progression must not add dependencies on:

- A database or persistence client.
- Network transports or protocol encoders.
- Rendering, client, or editor frameworks.
- An asynchronous runtime.

The world remains the single mutable owner while the first implementation is
running in one process. Future region, layer, and instance workers can retain
the same command/event boundary and make cross-owner operations explicit.

## Consequences

Positive:

- The vertical slice gains a complete town-to-field-to-town progression loop.
- Clients cannot forge objective progress or reward results.
- Quest definitions are available to the future client and editor without
  making either one authoritative.
- Quest reward transactions reuse the economy's validation and inventory
  semantics.
- The core remains straightforward to unit test without a running server or
  database.
- The lifecycle gives future UI code clear states for quest offers, active
  objectives, completion, and turn-in errors.

Negative and limitations:

- Quest state is lost when the process exits or the player is removed unless a
  future persistence layer saves it.
- There is initially no quest history, repeatable-quest policy, daily reset,
  abandonment, sharing, party credit, level gating, prerequisite graph, or
  localization system.
- The first objective model is limited to defeating a classified NPC kind; it
  does not yet cover collection, exploration, escort, dialogue, interaction,
  timed, or multi-stage objectives.
- The first reward model is limited to the existing in-memory gold and item
  structures. It does not support item instances, bind rules, choice rewards,
  reputation, or alternate currencies.
- The current event stream is in-process and not durable. A crash between
  command processing and a future persistence write could still require
  recovery rules.
- Static definitions are currently compiled Rust data rather than authorable
  source files. The editor will need a versioned content package workflow
  later.
- Quest progress is evaluated by the current simulation owner. Cross-layer,
  cross-instance, group-credit, and world-event policies remain unresolved.

## Alternatives considered

### Let the client report completed objectives

Rejected. The client is untrusted and must not determine kills, progress,
completion, or rewards. Client-reported progress would make quest completion a
cheating surface and would make authoritative auditing difficult.

### Put quest definitions and progress in `mmorpg-core`

Rejected. Static definitions are shared content and should be usable by the
server, client, and tools without making the simulation crate the content
authoring boundary. Runtime progress is mutable player/world state and
belongs in the core.

### Grant the reward when the final enemy is defeated

Rejected for the initial model. Requiring an explicit turn-in preserves the
town return loop, makes interaction range testable, and separates objective
completion from reward delivery. It also gives a future client a clear
ready-to-turn-in state.

### Mark the quest completed before attempting rewards

Rejected. A failed inventory or reward validation would then consume the quest
without granting the promised result. Reward validation and application must
be part of one atomic transition.

### Persist each progress increment immediately

Deferred. Durable progress writes are required for a persistent community, but
adding a database dependency or blocking I/O to the simulation-critical path
would violate ADR-001. The future persistence design should batch or journal
authoritative operations off the simulation path and define its recovery
commit point.

### Build a general quest graph before implementing the first quest

Deferred. The first slice needs a small, validated objective and reward model.
A larger graph system should be introduced when concrete content requires it,
so its schema is driven by real authoring needs rather than speculative
generality.

## Validation

The initial implementation includes focused dependency-free core tests covering:

- Valid starter quest content and invalid quest references.
- Quest offer listing from the correct NPC.
- Successful acceptance and rejection of duplicate acceptance.
- Progress from a qualifying authoritative enemy defeat.
- Rejection of turn-in before all objectives are complete.
- Successful atomic reward grant and completion transition.
- Multiple qualifying defeats producing deterministic progress.
- Repeated turn-in rejection after the reward has been claimed.

Follow-up tests should add explicit coverage for invalid quest-giver kinds,
outside-range interactions, reward-capacity failure, unrelated defeat types,
and client attempts to supply progress.

The server adapter includes a local protocol smoke path for:

1. Connecting a player and listing the starter quest.
2. Accepting the quest in town.
3. Defeating the required field enemies through authoritative combat.
4. Observing progress or ready-to-turn-in output.
5. Returning to town and turning in the quest.
6. Observing the reward and verifying that a repeated turn-in is rejected.

Validation results must be recorded separately from modeled capacity estimates.
The tests demonstrate correctness of the first implementation; they do not
demonstrate 5,000-client capacity or 200-player encounter capacity.

## Revisit conditions

Revisit this decision when:

- Player quest state must survive disconnects, restarts, or failover.
- Quest progress or rewards can be produced by multiple simulation workers.
- Group, raid, layer, instance, or world-boss credit rules are introduced.
- Quest sharing, branching prerequisites, repeatability, abandonment, or
  timed objectives become required.
- The objective event model cannot express new authoritative activity types
  without leaking subsystem implementation details.
- Quest progression or reward replication exceeds acceptable bandwidth or
  event-log costs.
- The editor needs a richer serialized schema, hot reload, localization, or
  content version compatibility guarantees.
- Testing shows that the in-memory state machine cannot preserve exactly-once
  semantics at the persistence commit boundary.
