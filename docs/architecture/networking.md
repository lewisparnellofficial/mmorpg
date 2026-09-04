# Networking and Replication

**Status:** Proposed; experiment required

## Core model

The server is authoritative. Clients send input or intent, and the server sends authoritative state and event results.

The protocol should support independent delivery classes rather than treating every packet identically.

## Message classes

### Reliable ordered

- Login and session state.
- Character selection.
- Chat messages.
- Inventory and quest updates.
- Group and guild operations.
- Important world events.

### Reliable retryable commands

- Loot requests.
- Vendor purchases.
- Trades.
- Quest reward claims.
- Other operations with unique idempotency keys.

### Unreliable state updates

- Position snapshots.
- Facing direction.
- Movement state.
- Animation state.
- Nonessential nearby-object updates.

New state must not be blocked behind stale movement or animation state.

## Simulation timing

A provisional model is:

- Fixed server simulation tick, likely in the 20–30 Hz range.
- Client rendering at its own frame rate.
- Client interpolation for remote entities.
- Limited client prediction for local movement.
- Server reconciliation when prediction diverges.
- Tick numbers or server timestamps on authoritative events.

The exact tick rate must be established through profiling, especially for the 200-player encounter.

## Interest management

The server must avoid broadcasting the entire world to every connected client. It should use:

- Spatial grids, cells, or another spatial index.
- Visibility and perception rules.
- Delta snapshots.
- Entity update priorities.
- Different update frequencies by distance and activity.
- Bandwidth budgets per client.
- Backpressure handling.

Players should not receive information about entities they cannot legitimately perceive. This is both a scalability and gameplay-security requirement.

## Large encounter requirements

The server must be benchmarked with:

- 200 players in one layer.
- One shared combat activity.
- Multiple roles and abilities active.
- Area-of-effect abilities.
- Boss AI and adds.
- Threat, damage, healing, auras, and interrupts.
- High-frequency movement and combat replication.
- Players joining and leaving.
- Packet loss, latency, and reconnect scenarios.

The test must measure region tick duration, network bandwidth, memory, CPU, packet loss, client frame time, and database activity.

## Protocol requirements

The protocol should have:

- Explicit protocol versioning.
- Content/build compatibility checks.
- Length and rate limits.
- Malformed-message handling.
- Sequence numbers or tick numbers.
- Session authentication.
- Duplicate-command protection.
- Reconnect and session-resume behavior.
- Clear separation between gameplay, chat, and administration traffic.

The transport choice remains open. The important requirement is support for reliable operations and non-blocking frequent state updates.

## Client responsibilities

The client may:

- Predict local movement.
- Interpolate remote movement.
- Display effects before confirmation where safe.
- Render server results.
- Cache static content.

The client must not decide:

- Its authoritative position.
- Damage or healing results.
- Loot outcomes.
- Quest completion.
- Currency changes.
- Ability legality.

## Initial networking experiment

Create a headless simulated-client test that can maintain 5,000 mostly idle connections and separately drive 200 active players in one world-boss scenario. Record the results in `docs/experiments/` before accepting a production network design.
