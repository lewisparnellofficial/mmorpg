# ADR-003: Shared static content schema

**Status:** Accepted for the initial vertical slice

**Date:** 2026-09-04

## Context

The client, authoritative server, and Linux development tools need to work
from the same definitions for items, NPCs, quests, rewards, vendors, and zone
placement. If each component invents its own temporary representation, the
first client and editor prototypes will create migration work and can disagree
about stable IDs or gameplay content.

The simulation must continue to own mutable state. Static content should be
versioned and validated separately from active players, creatures, combat,
inventory, and quest progress. The project also needs to keep the simulation
crate free of rendering, networking, database, and editor dependencies.

## Decision

Add `crates/mmorpg-content` as a dependency-free Rust crate containing
immutable content definitions and a compiled starter catalog.

The initial schema includes:

- Stable `ItemId`, `NpcTemplateId`, and `QuestId` values.
- Item definitions and maximum stack sizes.
- NPC templates and content-level archetypes.
- Vendor listing definitions with initial stock and unit prices.
- Quest definitions, kill objectives, and item/gold rewards.
- Zone definitions and NPC spawn coordinates.
- Catalog validation for duplicate IDs, empty names, zero values, and broken
  item/NPC/quest references.

The starter catalog defines the existing town vendor, field wolves, vendor
items, three wolf spawns, and a `Clear the Field` quest definition. The quest
definition is content only; runtime quest progress will be added to the
authoritative core in the next batch.

`mmorpg-core` depends on `mmorpg-content` and re-exports the existing item
definition API. This preserves the current economy boundary while ensuring
that item IDs and item definitions have one source of truth.

The catalog is compiled into the binary for now. A future content build tool
may load source documents and emit a validated runtime package, but the client
and server should continue to consume the validated schema rather than raw
editor graphs or unchecked files.

## Consequences

Positive:

- The client, server, and tools have a shared vocabulary for static content.
- Stable IDs and cross-reference validation are tested before editor work
  begins.
- The authoritative core remains independent of graphics, sockets, storage,
  and desktop UI frameworks.
- The editor can later target a versioned package format without changing the
  dynamic simulation ownership model.

Negative and limitations:

- Definitions are currently Rust source rather than artist-friendly files.
- The starter simulation still hard-codes some runtime NPC spawning and
  template-to-runtime-kind mapping; moving those paths fully to the catalog is
  a follow-up.
- The schema does not yet cover dialogue graphs, terrain tiles, materials,
  particle graphs, localization, abilities, or asset import metadata.
- No runtime content hot reload or package compatibility policy exists yet.

## Alternatives considered

### Keep definitions inside `mmorpg-core`

Rejected as the long-term boundary. It would make the simulation crate the
owner of content authoring concerns and prevent the client/editor from sharing
definitions cleanly.

### Use JSON or another external format immediately

Deferred. An external format is required for the eventual tools, but a
dependency-free typed schema and validation suite provide a smaller first step
and prevent an unvalidated file format from becoming an accidental runtime
contract.

### Let the editor and server use separate schemas

Rejected. Separate schemas would make stable IDs, references, and validation
behavior drift between authoring and execution.

## Validation

- `mmorpg-content` tests validate the starter catalog.
- Broken references, duplicate/empty definitions, and zero-value content are
  rejected by catalog validation tests.
- `mmorpg-core` and `mmorpg-server` workspace tests continue to pass while the
  item API is re-exported through the new crate.

## Revisit conditions

Revisit this decision when:

- The first editor source format is selected.
- Content packages need independent versioning or hot reload.
- Runtime schemas need serialization for client/server compatibility.
- Terrain, particles, dialogue, localization, and abilities require richer
  graph or asset-reference types.
