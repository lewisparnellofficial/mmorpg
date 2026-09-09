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
character-selection path. It is a bounded startup/role-selection check, not a
substitute for the full encounter, privacy, persistence, or renderer-quality
acceptance gate.

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
- **Headless companion result:** The typed three-role gate subsequently passed
  with tank player 5, healer player 6, damage player 7, and party 1. It
  verified own-player-only detailed snapshots, member-only party summaries,
  and stranger privacy. This does not substitute for the graphical run.

## Interpretation

The client shell and renderer can start on this host, but this is not a passing
Milestone 13 result. The Vulkan validation errors require investigation before
using this environment for a repeatable graphical acceptance record. The
three-client headless/graphical harness, role encounter, multiple loot
generations, restart/retry, slow-client, and stranger-privacy scenarios remain
unverified.
