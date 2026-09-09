# EXP-011: Graphical client runtime gate

**Status:** Repeatable gameplay/reconnect evidence; renderer-quality and
physical-input gates remain open

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
- **Aggregate gate:** `scripts/validate-all.sh` now invokes the three-window
  reconnect smoke after the renderer-active gameplay and role smokes, so the
  reconnect evidence is part of the repeatable graphical validation boundary.
- **Headless slow-client result:** A real TCP gate sent 12,800 snapshot
  requests from a non-reading client while a second client remained able to
  receive a snapshot. The server recorded bounded output saturation and
  isolated the slow session. This is network/queue evidence, not graphical
  slow-client evidence or a production backpressure measurement.
- **Measured renderer-active result:** The standalone graphical gameplay smoke
  launched a real Bevy window with `--acceptance-smoke` and observed
  authoritative vendor purchase, quest acceptance, movement into range, three
  enemy defeats, three loot rewards, and quest turn-in through the normal typed
  worker and presentation adapter. Vulkan validation errors were still present
  on this host; this is gameplay/presentation evidence, not renderer-quality
  acceptance evidence.
- **Measured renderer-active role result:** The graphical three-role smoke
  launched three real Bevy clients concurrently and observed the damage
  character's purchase, quest, combat, loot, and turn-in flow, plus the tank's
  authoritative taunt and the healer's party-invite acceptance and recovery
  event. It also verified that the damage client, which remained outside the
  tank/healer party, received no private party event. The run passed on the
  same host, but it retains the Vulkan validation limitation and uses fixed
  local smoke IDs.
- **Headless companion result:** The typed three-role gate subsequently passed
  with tank player 5, healer player 6, damage player 7, and party 1. It
  verified own-player-only detailed snapshots, member-only party summaries,
  stranger privacy, a town purchase and quest acceptance, tank taunt, an
  authoritative healer recovery of the tank, three quest kills with
  retryable loot, quest turn-in, and two enemy respawn generations. This does
  not substitute for the graphical run.

## Interpretation

The client shell, bounded graphical role encounter, and three-window restart
reconnect now have repeatable local evidence. This is still not a passing
Milestone 13 result: Vulkan validation errors require investigation before
using this environment for a renderer-quality acceptance record; graphical
slow-client behavior and physical renderer-quality evidence remain unverified.
The headless harness still provides the stronger privacy, retry, persistence,
and respawn evidence; it does not substitute for those remaining graphical
scenarios.
