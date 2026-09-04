# Research: Transparent Overworld Layering

**Status:** Preliminary findings recorded; policy and experiment required

**Started:** 2026-09-04

## Question

How should the project preserve a shared persistent overworld while transparently creating additional layers when a local activity becomes too large?

## Current constraints

- The overworld is not normally instanced.
- Multiple layers are allowed.
- Players should not need to think about layers.
- Approximately 200 players must be able to participate in one activity on one layer.
- Dungeons, raids, and battlegrounds are explicit instances.
- Layering is intended for local hotspots such as world-boss events.

## Evidence

The paper [A Distributed Architecture for MMORPG](https://www.comp.nus.edu.sg/~bleong/hydra/related/assiotis06mmorpg.pdf) proposes splitting a large virtual world into regions handled by different servers. It identifies strong locality of interest as a basis for partitioning, notes that static divisions cannot respond well to hotspots, and discusses dynamic reorganization and transfer of objects between regions. This is older research and does not validate a particular implementation or current hardware target, but it is directly relevant architectural precedent.

Epic's replication documentation also describes grouping replicated objects spatially and maintaining persistent lists for clients, reinforcing the need to separate world geography, simulation ownership, and per-client relevance.

- [Epic: Replication Graph](https://dev.epicgames.com/documentation/en-us/unreal-engine/replication-graph-in-unreal-engine)

## Preliminary recommendation

Use a two-level partitioning model:

1. A transparent overworld layer controls the local population and transient event context.
2. Regions or spatial cells inside that layer control simulation ownership and replication locality.

The first vertical slice should run one layer, but every player and entity should carry a layer identity. Layer creation, assignment, and retirement can be added after the single-layer 200-player test passes.

## Layer manager responsibilities

The realm coordinator or layer manager should track:

- Layer population.
- Players in active combat.
- Spatial density.
- NPC and effect counts.
- Simulation tick time.
- Replication pressure.
- Group membership.
- Scheduled event participation.
- Worker health.

Layer assignment should:

- Keep players in their current layer when possible.
- Keep groups together.
- Prefer layers containing a player's group.
- Create a layer when local activity budgets are exceeded.
- Avoid moving players in combat or during durable operations.
- Retire layers only after they are empty or safe to drain.

## Layer migration

Migration is an authoritative ownership handoff:

```text
Source validates safe point
  -> serialize player/transient state
  -> destination admits entity
  -> ownership changes
  -> destination rebuilds visibility
  -> source releases entity
```

The initial implementation should allow migration only when the player is out of combat, not casting, not trading, not looting, and not in the middle of a quest or inventory transaction. Active-combat migration should be treated as a separate experiment.

## Persistent-world rules

Layers should not independently own durable player or economy state. Durable operations must be realm-wide and idempotent.

The project must define each world-object category as one of:

- Realm-canonical.
- Layer-local transient.
- Layer-local but reconciled.
- Instance-local.

Examples:

| Object | Initial recommendation |
|---|---|
| Player character | Realm-canonical, currently assigned to one layer |
| Inventory and currency | Realm-canonical |
| Quest progress | Player-canonical |
| Ordinary enemy in combat | Layer-local transient |
| Static vendor | Realm-defined, replicated into each layer |
| World-boss lifecycle | Realm-canonical coordinator with one active encounter initially |
| Dungeon boss | Instance-local with durable reward/lockout records |
| Temporary area effect | Layer-local transient |

## World-boss policy

The first implementation should support a single canonical world-boss encounter containing up to 200 players in one layer.

For demand above that number, the future policy remains open:

- Admission ceiling and queue.
- Multiple replicas tied to one global event ID.
- A configurable hybrid.

If event replicas are added, the design must address:

- Reward eligibility across replicas.
- Whether damage/progress is shared.
- Whether a player can participate in more than one replica.
- Which layer owns the canonical boss lifecycle.
- What happens when a replica worker fails.
- How groups are assigned.
- How event completion is announced realm-wide.

Creating a second overworld layer for a world-boss is technically different from creating a dungeon instance, but it has some of the same state-isolation and reward problems. The player-facing distinction should remain clear even if the implementation shares infrastructure.

## Alternatives considered

### No layers

Simpler and maximally shared, but local hotspots can overload one simulation and replication domain.

### Permanent layers

Predictable capacity, but players are divided even when the world is quiet and the world feels less shared.

### Transparent dynamic layers

Best fit for the stated requirements, but requires assignment, migration, group cohesion, and event-consistency logic.

### Treat every hotspot as a dungeon instance

Provides easy capacity control, but violates the requirement that the ordinary overworld not be instanced and changes the social meaning of a world event.

## Experiment required

Prototype a layer manager with simulated players converging on one hotspot. Test:

- Layer creation.
- Group cohesion.
- Safe-point migration.
- Layer retirement.
- No duplicate entity ownership.
- No duplicate rewards.
- Chat and social behavior across layers.
- A canonical world event with one layer.

## Confidence

**Moderate.** Dynamic geographic partitioning and hotspot reorganization are established architectural patterns. The correct player-facing policy and world-boss semantics are game-specific and should be validated through a prototype and design review.
