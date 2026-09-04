# Implementation Roadmap

**Status:** Active

This roadmap tracks the path from the current development server to the first
playable vertical slice. It is intentionally implementation-oriented; broader
architecture and research remain in `docs/architecture/` and
`docs/research/`.

## Completed

- Git repository and contribution guide.
- Architecture and requirements dossier.
- Initial validation experiments for Rust simulation, replication workload,
  and transparent layering.
- Rust workspace.
- Engine-independent authoritative simulation core.
- Linux headless development server.
- Temporary local TCP command protocol.
- Starter town, field, vendor, and enemies.
- Player roles, movement, targeting, and basic combat.
- Data-driven starter item definitions and stable item IDs.
- Authoritative starter gold, stack-aware inventory, vendor transactions, and
  exactly-once enemy loot claims.
- Development-protocol commands for vendor listing, purchasing, loot, and
  inventory inspection.
- Shared dependency-free content catalog for item, NPC, vendor, quest, reward,
  and starter-zone spawn definitions.

## Completed batch: town/field gameplay loop

The current batch makes the first zone loop meaningful:

- Item definitions and stable item IDs.
- Player gold and inventory stacks.
- Enemy defeat rewards or loot.
- Vendor stock and listing.
- Vendor purchase validation.
- Development-protocol commands for inspecting and using the vendor.
- Focused core and TCP integration tests.

This batch remains in-memory and persistence-agnostic. Durable storage is a
later batch and should replace the storage boundary without changing gameplay
command semantics. The line protocol remains a development adapter, not a
production client protocol.

## Current batch: quests and progression

The first shared content boundary is now in place. The next implementation
increment will use its quest definitions to add runtime quest state and the
town-to-field progression loop.

### Batch 3: quests and progression

- Data-driven quest definitions.
- Quest objectives and player quest state.
- Quest-giver interaction.
- Quest completion and rewards.
- A small starter quest chain.

The client and editor technology spikes can begin alongside this batch once
the initial schemas are stable. They should consume the shared definitions and
must not become alternate sources of authoritative gameplay state.

### Batch 4: real simulation scheduling

- Cast times and cooldowns.
- Server tick deadlines.
- Basic enemy AI and respawn.
- Combat state machines.
- Threat and role-relevant abilities.
- Healing and tanking behavior.

### Batch 5: protocol and replication foundation

- Explicit command/event envelopes.
- Versioned binary serialization.
- Per-client outbound queues.
- Spatial interest management.
- Delta snapshots and combat event channels.
- Rust replication benchmark.

### Batch 6: persistence

- Persistence trait and in-memory test implementation.
- PostgreSQL schema design.
- Idempotent economy operations.
- Snapshots and operation journal.
- Crash/restart recovery tests.

### Batch 7: instances and layers

- Instance lifecycle abstraction.
- Dungeon/raid/battleground worker ownership.
- Transparent overworld layer manager.
- Safe-point migration.
- Group cohesion and event assignment.

### Batch 8: client and tools

- Client engine selection.
- Linux client shell.
- Shared content schemas.
- Terrain and pen-tablet editor.
- Asset placement.
- Particle authoring.
- UI scripting runtime and sandbox.

## Working rule

Each batch should leave behind a runnable slice, focused tests, updated
documentation, and a Conventional Commit. Do not introduce production-scale
infrastructure before the preceding gameplay boundary has a reproducible local
test.
