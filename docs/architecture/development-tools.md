# Development Tools

**Status:** Proposed

## General requirements

The tools must run on Linux and should use the same content schemas consumed by the client and server. Editing should produce version-controlled source data and validated runtime packages.

The editor does not need to create models, animations, music, or sound effects from scratch. It does need to import, configure, preview, and place those assets.

## Project and content browser

The content browser should provide:

- Search by stable ID, name, tags, and references.
- Dependency inspection.
- Broken-reference detection.
- Unused-asset detection.
- Revision comparison.
- Draft/review/published/deprecated status.
- Appropriate editor opening by content type.
- Content validation results.

## Terrain and world editing

Required terrain capabilities include:

- Heightmap sculpting.
- Raise, lower, smooth, flatten, erode, and terrace brushes.
- Terrain material painting.
- Brush radius, hardness, falloff, opacity, and spacing.
- Pressure-sensitive brush size or strength.
- Pen-tablet support.
- Undo and redo for strokes.
- Water bodies.
- Roads and paths.
- Foliage and decoration placement.
- Spawn and trigger volumes.
- Collision visualization.
- Navigation visualization.
- Streaming-cell visualization.
- Region-boundary visualization.

Large terrain should be stored in tiles or layers rather than one unmergeable monolithic file. The format should support incremental builds and source-control-friendly changes.

## Asset placement

The editor should place and preview imported:

- Meshes.
- Materials.
- Animations.
- Sound effects.
- Music.
- Particle effects.

Placed assets need metadata for transform, scale, collision, navigation blocking, LOD, culling, animation set, and attached effects or sounds.

## Particle authoring

Particle authoring should support:

- Emission rate and bursts.
- Lifetime.
- Velocity.
- Gravity and drag.
- Color and size curves.
- Texture or mesh particles.
- Billboarding.
- Trails.
- Local-space and world-space modes.
- Collision options.
- Preview, pause, and scrubbing.
- LOD settings.
- Performance estimates.

Particle definitions should compile into a runtime representation used by the client.

## NPC and enemy authoring

The editor should support:

- Stats and level ranges.
- Factions.
- Aggro and detection rules.
- Patrol paths.
- Leash areas.
- Abilities.
- Resistances.
- Threat behavior.
- Death behavior.
- Respawn rules.
- Loot tables.
- Vendor inventories.
- Dialogue.
- Quest relationships.
- AI behavior assignment.

## Quest and dialogue authoring

A graph editor should support:

- Prerequisites.
- Kill, collect, escort, interact, explore, defend, and dialogue objectives.
- Conditional dialogue.
- Branching paths.
- Rewards.
- Follow-up quests.
- Reputation effects.
- Timers and expiration.
- Repeatability.
- Failure states.
- Localization keys.

Graphs should compile to validated runtime data. Arbitrary editor graphs should not execute directly in the server without a controlled runtime model.

## Preview and validation

The editor should support previewing:

- The actual zone geometry.
- Collision and navigation.
- NPC paths.
- Quest flow.
- Dialogue.
- Particle effects.
- Sound and music placement.
- Spawn density.
- Streaming boundaries.

Validation should detect missing references, duplicate IDs, impossible quest paths, invalid spawns, missing localization, absent assets, invalid loot probabilities, and incompatible content versions.

## Live debugging

A future GM/developer inspector should be able to inspect:

- Player and entity state.
- Region and layer ownership.
- AI state.
- Threat tables.
- Active auras.
- Quest state.
- Inventory.
- Transaction history.
- Server tick timing.

Privileged actions such as teleporting, spawning, or changing player state must be authenticated, authorized, and audited.
