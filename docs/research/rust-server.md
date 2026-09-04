# Research: Rust Server and Region Simulation

**Status:** Preliminary findings recorded; experiment required

**Started:** 2026-09-04

## Question

What Rust server structure best supports 5,000 connected clients, approximately 200 players in one active overworld activity, transparent dynamic layers, explicit instances, and Linux deployment?

## Current constraints

- Rust is the required server language.
- The server must be authoritative.
- The overworld is logically shared but may use transparent dynamic layers.
- Dungeons, raids, and battlegrounds are explicit instances.
- The server must work in a single-machine development mode and retain a path to multiple workers or machines.

## Evidence

Tokio describes itself as an asynchronous Rust runtime for networking applications, with multi-threaded and single-threaded runtime variants and an ecosystem of networking utilities. Its documentation specifically frames async/await as useful for handling many concurrent operations, which makes it a candidate for gateway, connection, and service I/O rather than proof that the gameplay simulation itself should be async.

- [Tokio tutorial](https://tokio.rs/tokio/tutorial)

Tokio's shared-state guidance distinguishes simple shared data protected by a mutex from resources that benefit from a task managing the state through message passing. This supports a design in which each region or layer has an explicit owner and receives commands through a queue, while database and external I/O remain outside the simulation owner.

- [Tokio shared state](https://tokio.rs/tokio/tutorial/shared-state)
- [Tokio channels](https://tokio.rs/tokio/tutorial/channels)

The Rust book explains that message passing transfers ownership through channels and that shared mutable state introduces synchronization complexity. This is consistent with using ownership boundaries to make cross-region mutation explicit.

- [The Rust Programming Language: shared-state concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html)
- [The Rust Programming Language: fearless concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html)

## Preliminary recommendation

Use asynchronous Rust for connection handling and service I/O, but model gameplay simulation as fixed-tick, single-owner region or layer workers.

```text
Network task
  -> validated gameplay command
  -> region/layer command queue
  -> fixed-tick simulation owner
  -> authoritative state changes
       -> replication output
       -> durable operation/event
       -> metrics/replay
```

The initial server can contain multiple region actors in one process. The interfaces should allow a region or layer to move to another process later.

Recommended ownership rules:

- One simulation owner mutates a live entity.
- Cross-region actions are messages or commands.
- Database writes never occur inside the critical simulation step.
- Blocking I/O never runs on the simulation thread.
- Durable operations carry an idempotency key or revision.
- Region workers expose health and tick-time metrics.

This is a recommendation, not an accepted crate or framework selection. Tokio is a candidate runtime for networking and asynchronous services; the simulation loop may use ordinary Rust timing and worker threads inside or alongside the Tokio process.

## Alternatives considered

### Shared global world state with broad locking

This is simple to start but risks contention and makes it difficult to reason about cross-region operations, worker failure, or later process separation.

### Fully asynchronous entity logic

Allowing every entity and gameplay operation to await independently could make I/O integration convenient, but it risks nondeterministic ordering, unbounded scheduling overhead, and accidental blocking or lock contention in combat-critical paths.

### One monolithic simulation loop forever

This is viable for the first slice and should be supported in development, but it should not be the only ownership model if the long-term target includes hotspot scaling and multiple workers.

## Proposed server boundaries

```text
Gateway/session I/O
    -> Realm coordinator
        -> overworld layer/region actor
        -> instance actor
        -> persistence queue
        -> chat/social subsystem
```

The realm coordinator should own assignments and worker health, not gameplay rules. Gameplay rules should be shared by overworld and instance simulations.

## Experiment required

Build a minimal Rust prototype with:

- One gateway accepting simulated connections.
- One fixed-tick region worker.
- Command queues for movement and ability requests.
- A spatial index.
- 200 simulated players and a configurable number of NPCs.
- Replication work measured separately from simulation work.
- A persistence queue that can be slowed or stopped without stopping the tick loop.

Measure:

- Tick duration, especially p95 and p99.
- Command queue depth.
- CPU and memory.
- Number of entities and interactions.
- Replication preparation time.
- Behavior when persistence is unavailable.

## Open questions

- Which actor/channel implementation is appropriate?
- Should region workers be OS threads, Tokio tasks, or a hybrid?
- Which ECS, if any, improves rather than obscures ownership and debugging?
- How should a region worker hand off a player or NPC to another worker?
- What is the acceptable tick budget for the 200-player encounter?

## Confidence

**Moderate.** The ownership/message-passing direction is well supported by Rust's concurrency model and Tokio's documented patterns. The appropriate worker granularity and performance characteristics require project-specific measurement.
