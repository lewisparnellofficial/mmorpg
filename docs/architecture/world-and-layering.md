# World, Layers, Regions, and Instances

**Status:** Proposed

## Definitions

### Realm

The persistent logical game world and social/economic boundary. A realm owns accounts, characters, guilds, economy, durable world state, and content versioning.

### Overworld

The ordinary persistent world containing towns, fields, travel routes, NPCs, enemies, and world events.

### Layer

A runtime partition of the overworld used to control local population and simulation pressure. Layers use the same geographic content but may contain different transient populations and encounters.

Layers are not ordinary dungeons. They should generally be invisible to players and should not require manual selection.

### Region

A geographic simulation unit within a layer. A layer may contain one or more regions, each assigned to a simulation worker or actor.

### Instance

A separately created simulation for a dungeon, raid, or battleground. Instances have explicit membership, lifecycle, and reward rules.

## World model

```text
Realm
 +-- Overworld
 |    +-- Default Layer
 |    |    +-- Town Region
 |    |    +-- Field Region
 |    +-- Additional Layer, created only under pressure
 |    +-- Additional Layer, created only under pressure
 |
 +-- Dungeon Instance
 +-- Raid Instance
 +-- Battleground Instance
```

The initial vertical slice should run with one overworld layer. The implementation should still carry layer identity so that adding additional layers later does not require redesigning every entity and network message.

## Layer creation policy

Layers should be created based on local simulation load, not merely total realm population. Candidate signals include:

- Players within a spatial cell.
- Players currently in combat.
- Active NPC count.
- AI work per tick.
- Area effects and projectiles.
- Replication bandwidth.
- Region tick duration.
- Scheduled event participation.

The layer manager should maintain soft and hard budgets. A soft limit begins steering new arrivals elsewhere; a hard limit prevents additional assignments that would threaten the service target.

## Assignment behavior

Players should be assigned automatically. The server should:

1. Keep a player in their current layer when possible.
2. Keep parties and raids together.
3. Prefer a layer containing the player's group.
4. Create a layer when local activity would exceed a budget.
5. Avoid moving players during combat, casting, trading, looting, or other unsafe transitions.
6. Retire or merge quiet layers only at safe points.

There should be no normal player-facing layer number or layer-selection screen.

## Layer migration

A migration is an ownership handoff, not a client-side teleport. It should include:

- Source-worker validation.
- Entity serialization.
- Destination-worker admission.
- Ownership transfer.
- Nearby-entity rebuild.
- Group and encounter subscription restoration.
- Duplicate-ownership prevention.
- Recovery if the destination worker fails.

The initial implementation may restrict migration to safe states. Active combat migration can be added only after it has a clear correctness model.

## Persistent versus layer-local state

Layer-local state may include:

- Nearby ordinary NPCs.
- Combat state.
- Temporary creatures and objects.
- Local movement and visibility.
- Local spawn state.

Realm-wide or durable state should include:

- Player progression.
- Inventory and currency.
- Quest state.
- Guild and social state.
- Global event state.
- Reward eligibility.
- Content version.

The exact treatment of persistent world objects, such as a destroyed gate or harvested resource, needs to be defined per content type.

## World-boss policy

The 200-player world-boss case must use one layer and one encounter context. Layering must not be used to avoid this requirement.

For demand above 200, possible future policies are:

- One canonical encounter with an admission ceiling.
- Multiple event replicas associated with one global event ID.
- A hybrid policy selected by event definition.

If replicas are supported, the system must prevent reward duplication and define whether progress is shared, independent, or aggregated. The first vertical slice should support one canonical event only.

## Social behavior

Chat, guilds, friends, and group search should operate at realm scope even when players are in different layers. Party and raid membership should influence layer assignment. Direct interaction with players in another layer should be an explicit game rule rather than an accidental limitation.

## Instance behavior

Instances should have:

- Instance ID.
- Type and content version.
- Membership or matchmaking record.
- Owning worker.
- Creation and expiration time.
- Local world state.
- Lockout and reward policy.
- Recovery behavior.

The instance manager should be able to create many independent instances without changing ordinary overworld code.
