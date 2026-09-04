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

- `crates/mmorpg-core` — dependency-free authoritative starter-zone simulation.
- `crates/mmorpg-server` — Linux headless development server with a temporary
  nonblocking TCP line protocol.

The current server is intentionally a development process. It does not yet
provide authentication, durable persistence, binary protocol versioning,
interest-managed replication, or multi-worker deployment.

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

## Simulation ownership

Every live entity should have:

- A globally unique ID.
- A current layer or instance ID.
- A region owner.
- A lifecycle state.
- A spatial index entry.

Only one simulation owner should mutate an entity at a time. Cross-region and cross-layer operations should be explicit messages or commands.

## Static content and dynamic state

Static content includes terrain, NPC templates, spells, items, quests, loot tables, and visual references. It should be versioned and packaged.

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
