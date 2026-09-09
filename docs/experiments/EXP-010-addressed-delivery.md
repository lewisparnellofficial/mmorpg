# EXP-010: Minimum addressed delivery and interest filtering

**Status:** Development boundary integrated; production replication remains
provisional

## Objective

Prove the first server-side delivery boundary required by Milestone 8:
private results must remain addressed, public entity events must not be sent to
far-away players, and high-frequency position updates must not accumulate
unbounded stale state.

## Implementation

The typed server now applies a 45-unit nearby filter to public movement and
combat events before enqueueing them for a bound session. Private vendor,
transaction, quest, loot, and party events retain their explicit recipient or
party-member visibility rules.

Each wire client has two bounded delivery classes:

- reliable ordered messages are encoded immediately and retain sequence order;
- replaceable position messages are held by entity key and overwrite older
  pending positions before materialization.

The replaceable map is capped at 256 entities per client. The existing output
byte bound still applies when replaceable messages are materialized, and a
stale in-flight TCP window is not silently reordered.

## Evidence

- **Measured local result:** server tests cover far-player rejection, self
  visibility, party stranger privacy, and same-entity position coalescing.
  Workspace tests and the aggregate validation script pass.
- **Implementation evidence:** recipient filtering occurs before queue
  admission; reliable and replaceable queues have separate bounded paths.
- **Project inference:** a 45-unit development radius is a useful minimum
  interest boundary because it matches the current combat leash, but it is not
  a production spatial-index or bandwidth result.

## Remaining uncertainty

Snapshots still expose only the bound player's detailed player record, and the
wire schema does not yet carry a separate remote-party-summary record.
Production spatial indexing, delta snapshots, bandwidth budgets, slow-client
eviction policy, and TCP in-flight reliability windows require the later
replication and capacity work. Durable disconnect grace remains an identity
and persistence concern.
