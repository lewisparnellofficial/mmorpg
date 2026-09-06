# System Overview

**Status:** Proposed

## Architectural principle

The client is an untrusted presentation and input device. The server owns the authoritative world, combat, progression, economy, and persistence.

The world should be logically shared while its simulation is partitioned into regions, layers, and instances. A single process is acceptable during early development, but logical ownership boundaries should exist from the beginning.

## Logical components

```text
                         +---------------------+
                         |   Content Tools     |
                         | terrain / quests /  |
                         | NPCs / effects      |
                         +----------+----------+
                                    |
                           validated content
                                    v
+--------------+          +---------------------+
| Linux Client |<-------->| Login / Gateway     |
| renderer     | gameplay | auth / sessions     |
| input        |          | rate limits         |
| addon VM     |          +----------+----------+
+--------------+                     |
                                     v
                           +---------------------+
                           | Realm Coordinator   |
                           | layers / ownership  |
                           | instances / events  |
                           +----------+----------+
                                      |
                    +-----------------+-----------------+
                    v                 v                 v
             +-------------+  +-------------+  +-------------+
             | World Layer |  | World Layer |  | Instance    |
             | Workers     |  | Workers     |  | Workers     |
             +------+------+  +------+------+  +------+------+
                    |                 |                 |
                    +-----------------+-----------------+
                                      v
                           +---------------------+
                           | Persistence         |
                           | relational DB       |
                           | journal/snapshots   |
                           +---------------------+
```

These are logical boundaries, not an instruction to deploy every component as a separate service immediately.

## Recommended initial deployment

The first playable slice can run as:

- One client process.
- One Rust server process containing gateway, realm coordination, and one or more simulation actors.
- One PostgreSQL database or equivalent durable store.
- One content build and validation toolchain.
- Optional local admin/debug interfaces.

The repository now contains the first implementation of this shape:

- `crates/mmorpg-content` — dependency-free immutable content definitions and
  starter-catalog validation shared by runtime and tools.
- `crates/mmorpg-client-model` — renderer-independent presentation state that
  accepts authoritative events and snapshots without providing gameplay
  authority or a client command API.
- `crates/mmorpg-client-adapter` — standalone translation boundary from the
  temporary decoded protocol into the presentation model; it resolves
  content metadata by stable ID and has no socket or renderer dependencies.
- `crates/mmorpg-client` — standalone Bevy Linux client-shell technology spike
  that opens a window, presents the starter catalog with primitive geometry,
  connects to the development server, and sends basic authoritative intents.
- `crates/mmorpg-client-protocol` — standalone typed command-line encoder and
  bounded decoder for the temporary development server adapter; it deliberately
  rejects unrelated human-readable diagnostics.
- `crates/mmorpg-client-transport` — standalone bounded blocking TCP adapters
  for the temporary line connection and a versioned-wire bridge that carries
  validated command/event payloads.
- `crates/mmorpg-wire` — standalone versioned length-prefixed envelope
  prototype with explicit message kinds and payload boundaries.
- `crates/mmorpg-core` — dependency-free authoritative starter-zone simulation.
- `crates/mmorpg-server` — Linux headless development server with a temporary
  nonblocking TCP line protocol and an opt-in versioned-wire listener that
  routes typed commands into the same authoritative world.

The current development tooling also includes `tools/mmorpg-content-check`, a
standalone catalog validation command. It validates the same typed content
catalog that runtime code consumes; it is not yet the full terrain, placement,
particle, NPC, or quest editor.

The editor-side foundation is `tools/mmorpg-editor-core`, a standalone,
dependency-light heightmap and tablet-input model. It currently provides a
device-neutral tablet sample, pressure-aware terrain brushes, deterministic
source persistence, and stroke-level undo/redo; a Linux GUI/device shell is
still a future spike.

The authoritative core now also exposes an explicit fixed-tick combat path for
cast-time and cooldown experiments. The original immediate development path
remains available for compatibility, while timed combat state is owned by the
world owner and resolves through the same authoritative events. Enemy patrol,
aggro, leash, and respawn behavior remains a standalone experiment until its
navigation, persistence, and ownership boundaries are integrated into the
server.

The current server is intentionally a development process. It does not yet
provide authentication, durable persistence, structured binary event/snapshot
payloads, interest-managed replication, or multi-worker deployment.

The wire and transport crates are preparatory boundaries, not a production
network stack. The current server still supports its temporary line protocol,
and the graphical client still uses its own background line-protocol worker.
The opt-in server wire listener now accepts typed command payloads and emits
event envelopes carrying temporary diagnostic payloads. Authentication,
structured binary event/snapshot schemas, asynchronous backpressure, and
interest-managed replication remain future work. The current player/NPC
renderer consumes the presentation model directly.

The code should retain interfaces for separating these later:

- Gateway/session process.
- Realm coordinator.
- Overworld layer workers.
- Instance workers.
- Persistence workers.
- Chat/social services.

## Server authority

The server authoritatively resolves:

- Movement validity and position.
- Target validity.
- Ability execution.
- Cast timing and interruption.
- Damage, healing, threat, and crowd control.
- Loot and rewards.
- Inventory and currency.
- Quest progression.
- NPC AI and respawn.
- World events.

The client may predict and interpolate for responsiveness, but it cannot submit authoritative results.

The client presentation model follows this boundary: its mutable state is
updated only from authoritative events or snapshots. Rendering, input, and
future UI scripting adapters remain outside the model and must translate user
intent into server commands rather than applying gameplay results locally.

## Simulation ownership

Every live entity should have:

- A globally unique ID.
- A current layer or instance ID.
- A region owner.
- A lifecycle state.
- A spatial index entry.

Only one simulation owner should mutate an entity at a time. Cross-region and cross-layer operations should be explicit messages or commands.

## Static content and dynamic state

Static content includes terrain, NPC templates, spells, items, quests, loot tables, and visual references. It should be versioned and packaged. The initial Rust content crate provides the typed shared boundary and validation; an editor source format and runtime package builder remain future work.

Dynamic state includes active creatures, combat, timers, world-event progress, and player state. Active state should live in memory and be persisted through controlled durable operations rather than through per-frame database writes.

## Process and thread boundaries

Real-time simulation must not block on:

- Database queries.
- File access.
- Asset loading.
- External HTTP requests.
- Chat persistence.
- Logging sinks.

Network I/O, persistence, tooling connections, and other blocking work should be asynchronous or offloaded from the simulation loop.

## Architectural risks

The highest-risk areas are:

- 200-player encounter simulation and replication.
- Transparent layer migration.
- Exactly-once rewards and inventory transactions.
- UI scripting sandbox boundaries.
- Content authoring efficiency.
- Debugging and recovery after server failure.
