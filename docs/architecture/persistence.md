# Persistence and Recovery

**Status:** Proposed

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
