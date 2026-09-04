# ADR-002: In-memory economy and vendor transactions

**Status:** Accepted for the initial vertical slice

**Date:** 2026-09-04

## Context

The first playable slice needs a small economy loop that makes returning to
the town meaningful. Players need gold, stackable inventory items, vendor
purchases, and a clear reward flow after defeating starter-zone enemies.

The authoritative simulation core must remain dependency-free and must not
couple gameplay rules to a database, network transport, or asynchronous
runtime. The current starter zone has one town vendor and field wolves. The
initial server is a development process with a temporary line-oriented
protocol, so this decision covers the core simulation API only.

## Decision

Add a small in-memory economy model to `crates/mmorpg-core`.

The core provides:

- Stable `ItemId` values and immutable `ItemDefinition` records.
- Starter definitions for Field Wolf Pelt, Town Ration, and Minor Healing
  Potion.
- A slot-limited `Inventory` containing stackable `ItemStack` values.
- Player gold initialized to starter gold for the vertical slice.
- Starter vendor stock with unit prices and finite remaining quantities.
- `ListVendor`, `BuyItem`, and `LootEnemy` authoritative commands.
- `VendorListed`, `ItemPurchased`, `LootRewarded`, and
  `TransactionRejected` authoritative events.
- Validation of player identity, vendor identity, town location, vendor
  proximity, item identity, positive quantity, vendor stock, price overflow,
  gold, inventory capacity, enemy defeat state, reward ownership, and
  one-time reward claiming.

The first player to damage an enemy becomes the owner of that enemy's starter
reward. A defeated enemy's reward is claimed explicitly with `LootEnemy`, and
only the owning player may claim it. Each enemy has one Field Wolf Pelt reward
and can be claimed once.

All validation occurs before economy state is mutated. A rejected transaction
does not change gold, inventory, vendor stock, or reward-claim state.

The implementation remains single-owner and synchronous: the `World` owns all
mutable economy state and changes it while processing commands in
`World::step`.

## Consequences

Positive:

- The vertical slice has a complete authoritative buy-and-reward loop without
  adding runtime dependencies.
- Inventory stacking and capacity behavior can be tested independently of the
  client and network server.
- Gold, stock, and reward mutations have explicit command and event
  boundaries suitable for a future wire protocol.
- Rejected economy commands are observable as authoritative events rather than
  silently failing.
- Existing movement, target selection, and basic attack APIs remain intact.

Negative and limitations:

- Economy state exists only in process memory and is lost when the world
  process exits.
- Player gold, inventory, vendor stock, and enemy reward state are not loaded
  from or saved to durable storage.
- The starter model has no item instances, binding rules, equipment, selling,
  trading, mail, auction house, currencies other than gold, or loot tables.
- Enemy reward ownership is a simple first-damager rule and does not yet
  model groups, contribution, tags, threat, or shared loot eligibility.
- Vendor stock is hard-coded starter content rather than versioned content
  loaded from the content pipeline.
- The current development server exposes these commands through its temporary
  line protocol, but the protocol is intentionally not a production wire
  format.
- `PlayerSnapshot` now carries a clone of the in-memory inventory; a future
  replication API should use purpose-built delta messages instead of sending
  complete economy state on every snapshot.

## Persistence replacement path

When durable persistence is introduced, preserve the command and event
boundary while moving the durable state behind a persistence adapter. The
simulation should continue to validate commands and produce an operation
result, while a persistence worker records transactional economy operations.

The replacement should provide:

1. A durable player-economy record keyed by stable character ID.
2. Transactional gold and inventory updates for purchases and rewards.
3. Idempotency keys for purchase and reward operations so retries cannot
   duplicate items or currency.
4. Revision numbers or equivalent optimistic-concurrency checks.
5. Durable vendor stock and reward-claim records where the game design makes
   them persistent.
6. Recovery ordering that does not allow a confirmed purchase or reward to be
   lost after the client receives its authoritative result.
7. Migration code from the current in-memory structures to the durable schema.

The in-memory `Inventory`, `ItemId`, and command/event types may remain useful
as the simulation representation, but database rows must not become the
frame-by-frame mutation API. A future persistence ADR must define the commit
point, retry behavior, crash recovery, and whether individual world rewards
are durable or regenerated.

## Alternatives considered

### Add a database dependency to `mmorpg-core`

Rejected for this slice. It would couple deterministic gameplay tests to
database setup and introduce blocking or asynchronous concerns into the
engine-independent simulation library.

### Grant loot automatically on enemy defeat

Rejected for this slice. An explicit `LootEnemy` command makes reward
ownership, inventory capacity, duplicate claims, and eventual retry semantics
visible and testable.

### Use unstacked item instances immediately

Deferred. Item instances will be needed for durability, random properties, or
binding, but stackable definition-backed items are sufficient for the starter
economy and keep the first API small.

## Validation

- `cargo fmt --manifest-path crates/mmorpg-core/Cargo.toml` passes.
- `cargo test --manifest-path crates/mmorpg-core/Cargo.toml` passes with 10
  unit tests and no failing doctests.
- Tests cover purchase success, stack merging, insufficient gold, invalid
  vendor/item, inventory capacity, owner-only loot, duplicate loot claims,
  and the pre-existing movement and combat behavior.
- The server adapter covers parsing and formatting for vendor listing,
  purchase, loot, and inventory commands, and a local TCP smoke test exercises
  purchase and loot end to end.

## Revisit conditions

Revisit this decision when:

- Character state must survive a server restart.
- Multiple simulation workers can mutate the same economy record.
- Vendor stock or rewards need cross-layer or cross-instance consistency.
- Group loot, contribution, or shared reward rules are introduced.
- Item instances, trading, selling, or additional currencies are required.
- The current full-inventory snapshot is too expensive for replication.
