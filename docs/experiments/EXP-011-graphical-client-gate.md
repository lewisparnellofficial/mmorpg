# EXP-011: Graphical client runtime gate

**Status:** Partial runtime evidence; Milestone 13 gate remains open

## Objective

Exercise the real Bevy client against the typed development server on the
Linux desktop session, and distinguish window/renderer startup from the full
three-client gameplay acceptance gate.

## Run

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4000
printf '\n' | timeout 8s crates/mmorpg-client/target/debug/mmorpg-client \
  127.0.0.1:4000
```

The client binary was rebuilt through its standalone manifest before the run.
The development server was bound to loopback and the client used the default
typed wire path.

For the opt-in three-window startup smoke, use:

```bash
./scripts/smoke-graphical-three-client.sh
```

The script launches three real clients with `--character-id 1`, `2`, and `3`,
then checks that each process loads `Greenfield` and reaches the typed startup
character-selection path. It also checks the server-confirmed role and world
player binding for each client, plus at least three accepted typed peers. While
the clients remain alive, the script restarts the typed server once and checks
that each client re-authenticates, re-selects its preferred character, and
re-enters the world with the same role. It is a bounded startup/reconnect
check, not a substitute for the full encounter, privacy, durable-persistence,
or renderer-quality acceptance gate.

## Evidence

- **Measured local result:** Bevy created the window named
  `MMORPG Client — interactive slice (65v0)`, selected the Vulkan backend, and
  detected an NVIDIA GeForce RTX 5070. The client loaded the `Greenfield`
  starter zone and remained alive until the eight-second bounded observation
  ended.
- **Host:** Linux CachyOS rolling, kernel `7.2.2-1-cachyos`, AMD Ryzen 7
  2700X, 8 cores, 31.2 GiB memory, NVIDIA driver `610.57.04`, Wayland/X
  desktop session.
- **Runtime limitation:** Vulkan validation output reported swapchain image
  layout and acquire-semaphore errors, and the run did not provide evidence of
  three simultaneous graphical clients, role interaction, stranger privacy,
  restart/retry, or the full encounter loop.
- **Implementation update:** The graphical client now accepts an optional
  `--character-id <id>` argument. The worker selects that character only after
  receiving the authoritative character list; without the option, the
  existing Enter-key flow remains unchanged.
- **Presentation update:** The graphical HUD now displays the authoritative
  player role, health, target health, and private party membership summary in
  addition to inventory, quest, vendor, server notification state, and the
  server-published combat cooldown-ready tick. After an enemy attack, the HUD
  also shows the last authoritative threat target for the selected enemy and
  clears it when that enemy is defeated or respawned.
- **Measured local result:** The three-window smoke passed startup, role
  selection, one typed-server restart, and graphical client reconnect/re-entry
  for all three requested characters. This verifies the bounded worker
  reconnect path while the clients remain alive; it does not verify durable
  world state across the process restart.
- **Headless companion result:** The typed three-role gate subsequently passed
  with tank player 5, healer player 6, damage player 7, and party 1. It
  verified own-player-only detailed snapshots, member-only party summaries,
  stranger privacy, a town purchase and quest acceptance, tank taunt, an
  authoritative healer recovery of the tank, three quest kills with
  retryable loot, quest turn-in, and two enemy respawn generations. This does
  not substitute for the graphical run.

## Interpretation

The client shell and renderer can start on this host, but this is not a passing
Milestone 13 result. The Vulkan validation errors require investigation before
using this environment for a repeatable graphical acceptance record. The
graphical role encounter, retry client flow, slow-client graphical behavior,
and physical renderer-quality gate remain unverified. The headless harness now
covers the town/field role encounter and two loot generations; it does not
substitute for those graphical scenarios.
