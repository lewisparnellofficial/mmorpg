# MMORPG Gameplay Loop Plan

**Status:** Planning baseline

**Scope:** The next path from the current technical prototype to a basic,
recognizably MMORPG-like gameplay loop.

**Immediate target:** A small, repeatable, three-role party encounter in which
a tank attracts enemy threat, a healer restores party health, a damage dealer
kills the enemy, the enemy dies and respawns, and the group can return to town
to buy items, complete a quest, and persist progress.

## Table of contents

- [Current state](#current-state)
- [What is missing](#what-is-missing)
- [Implementation sequence](#implementation-sequence)
  - [1. Canonical typed gameplay path](#1-canonical-typed-gameplay-path)
  - [2. Player combat state](#2-player-combat-state)
  - [3. Enemy AI, threat, and respawn](#3-enemy-ai-threat-and-respawn)
  - [4. Minimal party model](#4-minimal-party-model)
  - [5. Client combat usability](#5-client-combat-usability)
  - [6. Three-client encounter validation](#6-three-client-encounter-validation)
  - [7. Durable group progression](#7-durable-group-progression)
  - [8. Development content workflow](#8-development-content-workflow)
  - [9. UI scripting and sandboxing](#9-ui-scripting-and-sandboxing)
- [Deferred work](#deferred-work)
- [Acceptance criteria for the basic MMORPG loop](#acceptance-criteria-for-the-basic-mmorpg-loop)
- [Recommended dependency graph](#recommended-dependency-graph)

## Current state

The repository already contains a functional development prototype with:

- Linux Rust server startup.
- Linux Bevy client startup.
- A starter town, field, vendor, and three field enemies.
- Three role labels: tank, healer, and damage dealer.
- Server-authoritative movement and bounds validation.
- Tab targeting and basic attacks.
- Fixed-tick combat timing experiments.
- Enemy defeat and exactly-once loot ownership.
- Stack-aware inventory and vendor purchases.
- Quest offers, acceptance, kill progression, completion, and rewards.
- A typed wire protocol with bounded frames and payload validation.
- Loopback-only development authentication.
- Character listing and explicit character selection.
- A renderer-independent client presentation model.
- A graphical client HUD for the current starter loop.
- Client reconnect behavior through a fresh authentication and bootstrap.
- An opt-in local character checkpoint implementation.
- A repeatable server-restart persistence smoke test.
- A terrain and pen-tablet editing core prototype.
- A standalone experiment for enemy patrol, aggro, leash, death, and respawn.

The current smoke test proves a single damage-dealer character can complete
the town/field loop. It does not yet prove a party encounter or meaningful
holy-trinity gameplay.

## What is missing

The most important missing gameplay elements are:

- Real tank, healer, and damage-dealer combat behavior.
- Abilities beyond role labels and role-dependent basic-attack damage.
- Enemy attacks against players.
- Threat tables and server-authoritative target selection.
- Player damage, healing, death, and recovery states.
- Party membership and friendly-target validation.
- Integrated enemy AI and respawn in the authoritative world.
- A three-client party encounter test.
- A multi-character durable storage namespace.
- Production persistence, authentication, transport, and replication.
- A graphical editor rather than only an editor-core CLI spike.
- Native pen-tablet device integration.
- UI scripting and its protected-action sandbox.
- Instances, overworld layers, and the 5,000-client scale test.

Some architecture documentation also describes the graphical client as still
using the line protocol by default, while the current typed-wire path now
contains the more complete authentication, selection, persistence, and
reconnection behavior. Documentation should be aligned as part of the first
implementation milestone.

## Implementation sequence

### 1. Canonical typed gameplay path

The project currently has two development paths:

```text
legacy line protocol  -> terminal debugging and compatibility
typed wire protocol   -> authentication, selection, persistence, reconnect
```

Make the typed path canonical for all new gameplay work.

Tasks:

- Make root launch documentation consistently use the typed wire listener.
- Make the convenience client command clearly target the typed path.
- Keep the line protocol only as a local debugging adapter.
- Remove stale documentation claiming that the graphical client primarily
  uses line mode.
- Add a typed protocol capability or compatibility check.
- Make client connection states visible:
  - connecting;
  - authenticating;
  - selecting character;
  - entering world;
  - in world;
  - reconnecting;
  - failed.

Acceptance criteria:

- A new developer can launch the server and client using one documented
  sequence.
- The graphical client reaches character selection through the typed path.
- All new gameplay commands use the typed path.
- The line protocol remains usable for terminal debugging.

### 2. Player combat state

Add the player-side state required for an encounter in `mmorpg-core`:

- Current and maximum health.
- Death or incapacitation state.
- Recovery behavior.
- Ability cooldowns.
- Optional role resources.
- Target restrictions.
- Range validation.
- Combat event ticks or timestamps.

Start with three small abilities:

| Role | Ability | Purpose |
| --- | --- | --- |
| Tank | Taunt or Shield Strike | Generates threat and keeps an enemy focused on the tank. |
| Healer | Heal | Restores a valid friendly target's health. |
| Damage dealer | Heavy Strike | Produces the highest sustained damage. |

Use a small authoritative intent such as:

```text
CastAbility {
    player_id,
    ability_id,
    target_id
}
```

The client submits intent only. The server decides whether the player knows
the ability, whether the target is legal, whether resources and cooldowns are
valid, whether the target is in range, and what damage, healing, and threat
result occurs.

Acceptance criteria:

- A healer cannot heal an enemy.
- A player cannot cast another role's ability.
- A dead player cannot cast.
- A client cannot submit damage or healing amounts.
- Cooldown and resource validation are server-authoritative.
- Failed casts do not partially mutate state.
- All three role abilities have focused core tests.

### 3. Enemy AI, threat, and respawn

Integrate the existing patrol/aggro/leash/respawn experiment into the
authoritative simulation. Begin with a small deterministic model:

- Idle or patrol state.
- Aggro radius.
- Threat map per enemy.
- Threat-based target selection.
- Basic enemy attack.
- Leash distance.
- Return-to-spawn behavior.
- Death state.
- Respawn timer.
- Loot availability during the dead interval.
- Respawn event.

The desired encounter is:

```text
tank enters enemy range
        -> enemy acquires tank
        -> tank uses taunt
        -> healer restores tank health
        -> damage dealer attacks
        -> enemy dies
        -> party loots
        -> enemy respawns after a delay
```

Ownership rules:

- The region/world owner mutates enemies, threat, player combat state, and
  respawn timers.
- Network code translates intents and events only.
- Durable storage records rewards and safe checkpoints, not every transient
  AI decision.

Acceptance criteria:

- Enemy attacks are resolved by the server.
- Threat is never supplied by the client.
- Tank taunt changes enemy target according to server rules.
- Damage dealer threat can overtake tank threat when designed to do so.
- Dead enemies cannot be attacked.
- Loot is available exactly once.
- Enemies respawn deterministically.
- Patrol, aggro, leash, death, and respawn are integrated core tests.

### 4. Minimal party model

Build only the party mechanics needed for the first encounter. Do not add
guilds, matchmaking, or a complete social system yet.

Add:

- Party ID.
- Party membership.
- Party creator or leader.
- Invite/accept commands, or a controlled development-only party setup.
- Friendly-target validation.
- Party-member visibility.
- Party-aware healing and combat events.
- Disconnect handling for party members.

A development shortcut is acceptable initially: a local test command may create
a party, or a controlled integration scenario may group three test characters.
The simulation should still represent explicit party membership rather than
inferring it from proximity.

Acceptance criteria:

- A healer can heal a party member but not an unrelated player.
- Party membership is server-authoritative.
- A disconnected member is removed or enters a defined grace state.
- Tank, healer, and damage dealer can participate in one encounter.
- A headless integration test drives three clients concurrently.

### 5. Client combat usability

Extend the current compact HUD so a user can understand the encounter:

- Player health bar.
- Target health bar.
- Target name and type.
- Current target indicator.
- Ability key bindings.
- Cooldown display.
- Role indicator.
- Party-member health display.
- Combat log entries for damage and healing.
- Enemy state: alive, attacking, dead, respawning.
- Town/field location display.
- Error notifications for invalid range, cooldown, target, or resource state.

Suggested temporary bindings:

```text
1       tank ability / heavy strike
2       healer ability / heal
3       damage ability
Tab     cycle targets
WASD    move
L       loot
V       list vendor
B       buy
O       list quest offers
E       accept quest
R       turn in quest
```

The client must not locally alter health bars, cooldowns, inventory, or quest
state. It displays authoritative snapshots and events.

Acceptance criteria:

- The user can identify the active role.
- The user can see ability readiness.
- The user can see target range/state.
- The user can see damage and healing results from server events.
- The encounter can be completed through the graphical client.

### 6. Three-client encounter validation

Replace the single-character starter smoke's definition of success with a
repeatable three-role scenario:

- One tank client.
- One healer client.
- One damage-dealer client.
- One or more enemies.
- One shared starter-zone server.
- Deterministic encounter commands.
- Optional packet delay or disconnect injection.

The test should verify:

1. All three clients authenticate.
2. All three select characters.
3. All three enter the same world.
4. The party forms.
5. The tank attracts the enemy.
6. The healer heals the tank.
7. The damage dealer damages the enemy.
8. The enemy dies.
9. Loot is assigned exactly once.
10. The enemy respawns.
11. Players return to town.
12. A vendor transaction succeeds.
13. The quest is accepted and turned in.
14. A client disconnects.
15. The client reconnects.
16. The server restores the correct durable state.
17. No duplicate loot, quest rewards, or purchases occur.

This is the primary gate for calling the first slice an MMORPG gameplay loop.

### 7. Durable group progression

The current local checkpoint is useful for development but is not a production
repository. It currently lacks:

- A multi-character namespace.
- Database transactions.
- An operation journal.
- Idempotency keys.
- Parent-directory fsync.
- Concurrent-writer handling.
- Migration support.
- Durable session fencing.
- Crash recovery for in-flight transactions.

After the party loop exists, improve persistence in this order:

1. Keep simulation state in memory.
2. Emit durable operations from the simulation owner.
3. Process persistence asynchronously outside the simulation tick.
4. Use operation IDs or revisions.
5. Commit loot, purchases, and quest rewards transactionally.
6. Checkpoint movement periodically and at safe logout.
7. Restore only validated durable state.
8. Reset transient combat state on reconnect.
9. Add PostgreSQL and migration tests.

Do not use a database as the per-frame simulation data structure.

### 8. Development content workflow

The editor core is useful, but it is not yet an editor suite. After the
gameplay model stabilizes, build the minimum workflow needed to stop hard-coding
the starter zone:

1. Content project and package format.
2. Tiled terrain source documents.
3. Terrain material layers.
4. Spawn placement.
5. NPC and enemy definitions.
6. Vendor inventory editing.
7. Quest and dialogue editing.
8. Patrol path editing.
9. Particle effect authoring.
10. Asset placement.
11. Validation and preview.
12. Linux GUI shell.
13. Native pen-tablet integration.

The editor should produce the same validated content structures consumed by the
server and client. It should not execute arbitrary gameplay logic directly.

### 9. UI scripting and sandboxing

UI scripting is required by the product, but it should follow the first
combat loop rather than delay it.

Implement in this order:

1. Public UI API.
2. Display APIs separated from protected gameplay-intent APIs.
3. Event subscriptions.
4. Layout and widget APIs.
5. Instruction, memory, event, timer, and widget quotas.
6. Protected input handling that prevents gameplay automation.
7. Adversarial sandbox tests.
8. Rebuild the default HUD using the same public API.

The protected boundary must prevent scripts from directly injecting movement,
querying hidden targets, automating casts, mutating inventory, or bypassing
secure input handling.

## Deferred work

The following should not be the next implementation target:

- Dungeons.
- Raids.
- Battlegrounds.
- Overworld layering.
- 5,000-client optimization.
- World-boss replication.
- PostgreSQL deployment before the party model exists.
- Full production authentication.
- Advanced editor collaboration.
- Complete addon scripting.
- Full asset pipeline.
- Large-scale class design.

These become more meaningful after the three-role encounter exists.

## Acceptance criteria for the basic MMORPG loop

The basic MMORPG gameplay loop is complete when all of the following are true:

- A Linux user can launch the server and client using documented commands.
- The client authenticates through the typed development path.
- The user explicitly selects a character.
- Three role characters can enter one shared starter-zone world.
- The tank, healer, and damage dealer have meaningfully different abilities.
- Enemies acquire and change targets using server-owned threat.
- Enemies can damage players.
- Healers can restore valid friendly targets.
- Players can die or become incapacitated and recover according to defined rules.
- Enemies can die, be looted exactly once, and respawn.
- Players can return to town and use a vendor.
- Players can accept, progress, and turn in a quest.
- A client can disconnect and reconnect.
- Character progress survives a server restart.
- The full flow is exercised by a repeatable multi-client integration test.
- Core, protocol, server, client, and relevant standalone tests pass.
- The implementation remains server-authoritative.
- Database and file I/O remain outside the simulation-critical step.

## Recommended dependency graph

```text
canonical typed path
        -> player combat state
        -> three role abilities
        -> enemy AI, threat, and respawn
        -> minimal party model
        -> graphical combat HUD
        -> three-client encounter test
        -> durable group progression
        -> content editor workflow
        -> UI scripting sandbox
        -> instances, layers, replication, and scale validation
```

Each implementation milestone should leave behind a runnable slice, focused
tests, updated documentation, and a Conventional Commit. Do not introduce
production-scale infrastructure before the preceding gameplay boundary has a
reproducible local test.
