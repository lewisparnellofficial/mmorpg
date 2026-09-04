# Requirements Baseline

**Status:** Accepted baseline

This document records the current project requirements supplied by the project owner. It should change only when the owner changes the desired behavior or scope.

## Product goal

Build a functional, original MMORPG in the style of classic tab-targeted MMORPGs. The game, server, and development tools must run on Linux. The project is a hobby project and is not intended for commercial release, but its technology should be capable of supporting a persistent player community.

The project has three simultaneous goals:

- Learning and skill development.
- Experimentation with MMORPG technology and content workflows.
- Eventually operating a persistent shared player community.

## Player and realm scale

- The target realm must support approximately **5,000 connected clients**.
- Approximately **200 players must be able to participate in one activity simultaneously on one overworld layer**.
- The 200-player target must be tested with a real high-load activity, such as a world-boss encounter. It must not be satisfied merely by putting players into separate layers.
- Multiple overworld layers are allowed as a capacity mechanism.
- Players should not normally know about or manually manage layers.
- The overworld should not normally be instanced.
- Dungeons, raids, and battlegrounds should be explicit instances.

## Initial gameplay scope

The initial feature set should include three classes representing the basic holy trinity:

- Tank.
- Healer.
- Damage dealer.

The first vertical slice should contain:

- One outdoor zone.
- One town that players can return to.
- A field containing enemies.
- Vendors in the town.
- Basic movement and interaction.
- Real-time tab-targeted combat.
- Enough progression, loot, and persistence to make returning to town meaningful.

The first slice does not need to include raids, battlegrounds, guilds, an auction house, professions, or a complete MMORPG feature set.

## Technology constraints

- The server should be written in Rust.
- The client must run on Linux.
- The server must run on Linux.
- Development tools must run on Linux.
- The project should preserve a path to multi-process and multi-machine deployment.
- Development should still be possible on one Linux machine.

## Client scripting

- Players should be able to script UI customizations.
- The default UI should use the same scripting system and public UI API.
- Player scripts must be sandboxed.
- Player scripts must not be able to automate gameplay through the official addon API.
- Dedicated external bot detection is not an initial requirement.
- Server authority and ordinary server-side validation remain mandatory.

## Development tools

The editor suite should support authoring and placement of:

- Terrain heightmaps.
- Terrain materials and painting.
- NPCs and enemies.
- Vendors.
- Quests and dialogue.
- Spawn points and patrols.
- Particle effects.
- Existing visual assets.
- Existing sound and music assets.

Artists should be able to use pen tablets for terrain work, including heightmap sculpting and terrain painting. The editor does not need to author models, skeletal animations, music, or sound effects from scratch. It must be able to import, preview, configure, and place those assets.

## Explicitly provisional choices

The following are requirements or candidates, not yet final implementation decisions:

- Client engine.
- Network transport.
- Database and persistence implementation details.
- UI scripting language.
- Editor framework.
- Exact layer assignment and world-boss policy above 200 participants.
- Exact server tick rate and latency targets.
