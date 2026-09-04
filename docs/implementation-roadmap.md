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
- Runtime quest state, authoritative kill progress, town turn-in, and
  exactly-once quest rewards.
- Development-protocol commands for quest offers, acceptance, and turn-in.
- Renderer-independent client presentation projection for authoritative
  player/NPC, combat, economy, and quest events.
- Standalone starter-catalog validation CLI for the shared content boundary.
- Bevy Linux client shell that opens a window and displays a primitive
  town/field scene from the shared starter catalog.
- Typed temporary-protocol command encoder with input validation.
- Device-neutral pen-tablet and heightmap editing core with deterministic
  source persistence and stroke-level undo/redo.
- Client presentation replay experiment covering the full starter quest,
  economy, combat, and loot loop.

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

## Completed batch: quests and progression

The shared content boundary now drives the first runtime quest. The starter
loop is playable through the development protocol: accept the quest in town,
defeat the field wolves, return to town, and claim the reward.

### Batch 3: quests and progression

- Data-driven quest definitions.
- Quest objectives and player quest state.
- Quest-giver interaction.
- Quest completion and rewards.
- A small starter quest chain.

## Current batch: client and editor technology spikes

The shared schema is stable enough for small, disposable technology spikes.
These spikes should prove the Linux runtime choices without committing the
project to a full production client or editor architecture yet:

- Linux client window, input, camera, and basic scene rendering.
- Loading the shared starter catalog and displaying zone/NPC data.
- A minimal network connection to the development server.
- Editor window and pen-tablet input discovery.
- Heightmap brush prototype with save/load of a small source document.
- Asset placement and particle preview feasibility.
- UI scripting runtime feasibility and protected-action boundary research.

The client and editor remain presentation and authoring tools. They must not
become alternate sources of authoritative gameplay state.

The first foundations from this batch now exist:

- `mmorpg-client-model` projects authoritative events and snapshots into
  renderer/UI-friendly state, with no networking, rendering, or gameplay
  command dependencies.
- `mmorpg-content-check` validates the starter catalog and prints a compact
  content summary for local tooling and CI smoke checks.
- Client and editor research records the provisional technology candidates;
  these recommendations still require local windowing, rendering, tablet, and
  scripting spikes before they become accepted decisions.

The window and terrain-core spikes now run locally. The remaining technology
work in this batch is to connect native tablet events, feed a real network
adapter into the client presentation model, and validate the protected UI
scripting boundary with an embedded runtime.

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

### Batch 8: production client and tools

- Production client runtime and renderer integration.
- Versioned network transport and authoritative event decoder.
- Real tablet-device editor shell.
- Terrain tiles, materials, placement, particles, and runtime packaging.
- UI scripting runtime, protected input context, and sandbox test suite.

## Working rule

Each batch should leave behind a runnable slice, focused tests, updated
documentation, and a Conventional Commit. Do not introduce production-scale
infrastructure before the preceding gameplay boundary has a reproducible local
test.
