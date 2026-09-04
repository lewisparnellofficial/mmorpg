# Research: Networking and Replication

**Status:** Preliminary findings recorded; experiment required

**Started:** 2026-09-04

## Question

What network model can support 5,000 connected clients while allowing approximately 200 players to participate in one active overworld encounter?

## Current constraints

- The server is authoritative.
- Movement and frequent state updates are real-time.
- Durable commands must be reliable and idempotent.
- Stale movement state must not block newer state.
- A single layer must support the 200-player activity requirement.
- Players should not receive entities they cannot legitimately perceive.

## Evidence

QUIC provides secure, multiplexed reliable streams. RFC 9221 defines an unreliable datagram extension that can use the same connection and authentication context. The datagram extension does not retransmit lost datagrams, which makes it a candidate for fresh movement or snapshot state while reliable streams carry control and durable operations.

- [RFC 9000: QUIC](https://www.rfc-editor.org/rfc/rfc9000.html)
- [RFC 9221: QUIC unreliable datagrams](https://www.rfc-editor.org/rfc/rfc9221.html)

RFC 9221 also states that datagrams have no explicit application flow-control signaling and cannot be fragmented at the QUIC layer. Any protocol using them would need explicit payload-size limits, prioritization, expiration, and application-level handling for messages that cannot be lost.

Epic's Replication Graph documentation describes a persistent graph of replication lists, spatial grouping, prioritization, dormancy, and special always-relevant sets. Although it is engine-specific and not evidence that this project should use Unreal Engine, it supports the general architectural conclusion that replication candidates should be pre-indexed by location and behavior rather than every actor checking every client every frame.

- [Epic: Replication Graph](https://dev.epicgames.com/documentation/en-us/unreal-engine/replication-graph-in-unreal-engine)
- [Epic: Actor relevancy](https://dev.epicgames.com/documentation/en-us/unreal-engine/actor-relevancy-in-unreal-engine)
- [Epic: Network profiler](https://dev.epicgames.com/documentation/en-us/unreal-engine/using-the-network-profiler-in-unreal-engine)

## Preliminary recommendation

Design the application protocol around three logical delivery classes:

### Reliable ordered stream

Use for login, chat, inventory, quest state, group operations, and important world events.

### Reliable retryable command

Use for loot, vendor purchases, trades, and rewards. These commands need operation IDs or revisions so that retransmission cannot duplicate a result.

### Unreliable or replaceable state

Use for movement snapshots, facing, animation state, and other data where a newer update supersedes an older one.

QUIC with reliable streams and datagrams should be prototyped as one candidate. A custom UDP protocol with separate reliability channels remains a valid alternative. The project should not accept QUIC solely because it has a useful feature list; the Rust implementation, MTU behavior, observability, deployment, and 200-player performance must be measured.

## Replication design

Use a spatial replication index with additional behavior categories:

- Nearby active players.
- Nearby active NPCs.
- Group and raid members.
- Always-relevant realm or event objects.
- Dormant placed objects.
- Temporarily revealed or detected entities.
- High-priority combatants.
- Low-priority background entities.

For each client, construct a prioritized candidate list from the index and emit deltas within a bandwidth budget. Do not make every entity test itself against every connected client.

The 200-player encounter is a special case. The server should support a dense encounter interest set without assuming that every player needs every field of every entity at the same frequency. Combat results and relevant state must remain authoritative, while distant or low-priority visual state can be throttled.

## Provisional timing model

- Fixed server tick, initially test 20 and 30 Hz.
- Client rendering independent from server tick.
- Local movement prediction with reconciliation.
- Remote interpolation.
- Event messages for combat outcomes.
- Snapshot or delta state for movement and presentation.

These rates are starting points for experiments, not accepted requirements.

## Capacity model

The benchmark must measure at least:

- 5,000 mostly idle connections.
- 200 active players in one layer.
- 200 players, a boss, adds, area effects, and multiple simultaneous casts.
- Several world layers under a hotspot surge.
- Many dungeon instances while the overworld remains active.

The benchmark must report:

- Bytes sent and received per client.
- Packets and messages by class.
- Replication candidate-generation time.
- Serialization time.
- Region tick time.
- Queue depth and backpressure.
- Memory per connection and entity.
- Packet loss and recovery behavior.
- Client frame time in the dense encounter.

## Alternatives considered

### TCP-only gameplay protocol

Simpler to implement, but stale frequent data can block newer data under packet loss. It remains useful for early control traffic or a prototype, but should not constrain the final protocol if movement and replication require replaceable updates.

### Custom UDP reliability layer

Gives direct control over packet layout, reliability, ordering, pacing, and expiration. It also creates more security, congestion-control, NAT, observability, and maintenance work.

### QUIC

Provides standardized security, multiplexed streams, and optional datagrams. It may reduce custom transport work, but the project must validate library maturity, datagram limits, server performance, and operational behavior on the target Linux environment.

## Experiment required

Implement the same application protocol over two candidate transports or transport abstractions. Drive the 200-player encounter under controlled latency and packet loss. Compare:

- End-to-end action latency.
- State freshness.
- Bandwidth.
- CPU.
- Memory.
- Recovery after loss.
- Implementation complexity.
- Debugging quality.

## Confidence

**Moderate to high** for spatial, prioritized replication as a requirement. **Moderate** for the transport recommendation because project-specific load and implementation quality remain unknown.
