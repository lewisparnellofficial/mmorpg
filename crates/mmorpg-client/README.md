# MMORPG client technology spike

This is the first Linux client shell prototype. It is intentionally a
technology spike, not a production client or an engine commitment.

The binary uses Bevy `0.19.1` to open a desktop window, create a 3D camera and
directional light, draw a primitive town and field, and instantiate the NPC
placements from the shared `mmorpg-content` starter catalog. The vendor and
enemy markers use different colors. Startup logs also identify the content
definitions and placements that were instantiated.

## Run

From the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client/Cargo.toml -- --check
cargo check --manifest-path crates/mmorpg-client/Cargo.toml
cargo run --manifest-path crates/mmorpg-client/Cargo.toml
```

The window can be closed using the normal window controls. The first build
may take several minutes because Bevy and its graphics dependencies are
compiled locally.

On Linux, the host needs a working desktop session and graphics stack. Bevy's
window and renderer may require distribution-specific X11/Wayland, Vulkan,
OpenGL, audio, or input development libraries. The exact package names are
intentionally not prescribed here because they vary across Linux
distributions; consult the Bevy setup documentation for the selected host.

## Deliberate limitations

- There is no server connection, protocol, authentication, replication, or
  authoritative simulation integration.
- There is no camera controller, character movement, combat, targeting, UI,
  persistence, or addon scripting.
- The scene uses primitive meshes and hard-coded presentation layout. It does
  not validate or load authored terrain, models, animations, particles, sound,
  or music packages.
- NPC labels are written to the terminal rather than rendered as in-world UI.
- The shared catalog is compiled into `mmorpg-content`; an external content
  package format and hot reload pipeline remain future work.
- Bevy remains a provisional runtime choice. This spike only establishes that
  the pinned dependency can open a Linux-oriented 3D shell and consume the
  current content schema; it is not a performance, compatibility, or
  production-readiness result.

The next useful spike is a renderer-independent network adapter that feeds
`mmorpg-client-model` events into this presentation shell. Pen-tablet input and
the separate content editor remain independent technology spikes.
