# EXP-005: durable character checkpoint prototype

## Purpose

Validate the first end-to-end durable-character checkpoint before adding a
database to the development server. The prototype must restore a selected
character through the existing account/character repository boundary without
performing file or database I/O in the simulation tick.

## Required checkpoint state

- Stable character ID and owning account ID.
- Name and selected role.
- Safe position checkpoint.
- Gold.
- Inventory stacks and inventory capacity.
- Quest IDs, progress, required counts, and statuses.
- Monotonic revision and content-version marker.

Health, current target, casts, combat cooldowns, pending attacks, nearby NPC
state, and live entity IDs are transient. They must be reset or reconstructed
on world entry rather than persisted as part of this first checkpoint.

## Required core boundary

The authoritative core needs a validated player-restore command or constructor
that accepts only a complete durable character state. It must validate:

- stable IDs and account ownership at the repository boundary;
- finite, in-bounds checkpoint positions;
- known item and quest content IDs;
- nonzero stack quantities and maximum stack sizes;
- inventory slot capacity;
- valid quest objective/status combinations; and
- overflow-safe currency and revision handling.

The normal new-character path must still use starter defaults. Restoring a
character must produce the same authoritative player projection used by normal
snapshots and wire messages.

## Checkpoint and recovery policy to test

1. After an authoritative state-changing event, build a checkpoint from the
   world owner after the event is committed in memory.
2. Persist it outside the simulation-critical path using a revisioned,
   atomic-replace file in the local prototype.
3. On restart, load and validate the last complete checkpoint before allowing
   `EnterWorld`.
4. If a checkpoint is malformed, truncated, unknown-content, or stale relative
   to a newer journal record, reject world entry safely and retain the prior
   complete state.

For economy, loot, and quest rewards, the eventual implementation needs a
journal or transaction ID in addition to a checkpoint so a crash cannot create
duplicate rewards. A checkpoint alone is not sufficient proof of exactly-once
durability.

## Acceptance tests

- Purchase one Town Ration, restart the server, and observe the same gold and
  inventory stack after character selection.
- Accept and partially progress `Clear the Field`, restart, and observe the
  same accepted quest and progress.
- Turn in the quest, restart, and verify the reward is present exactly once.
- Write a deliberately truncated checkpoint and verify the server rejects the
  affected character without corrupting the previous complete checkpoint.
- Attempt to load a checkpoint containing an invalid item ID, oversized stack,
  invalid quest status, or out-of-bounds position and verify validation fails
  before the character becomes a live player.

## Current status

Planned. The server now has an `AccountCharacterRepository` replacement seam,
but the core still creates starter-default players only and the active
repository is non-durable. This experiment defines the next implementation
boundary; it does not claim persistence is implemented.
