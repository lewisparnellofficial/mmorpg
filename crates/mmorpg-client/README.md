# MMORPG client technology spike

This is the first Linux client shell prototype. It is intentionally a
technology spike, not a production client or an engine commitment.

The binary uses Bevy `0.19.1` to open a desktop window, create a 3D camera and
directional light, draw a primitive town and field, and instantiate the NPC
placements from the shared `mmorpg-content` starter catalog. It connects to
the local development server on a background TCP worker, requests the bounded
`snapshot` bootstrap response, and supports keyboard movement, target cycling,
and server-authoritative basic attacks. The vendor and enemy markers use
different colors. Startup logs also identify the content definitions and
placements that were instantiated.

Decoded snapshots and events are also projected through the standalone
`mmorpg-client-adapter` into `mmorpg-client-model`. The Bevy scene still has a
small local projection while the renderer migration is in progress, but the
authoritative presentation model is now exercised by the live client path.

## Run

From the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client/Cargo.toml -- --check
cargo check --manifest-path crates/mmorpg-client/Cargo.toml
cargo run --manifest-path crates/mmorpg-client/Cargo.toml
```

The window can be closed using the normal window controls. With the server
running in another terminal, use `WASD` to move, `Tab` to select the next
known NPC, and `Space` to attack the selected target. The first build
may take several minutes because Bevy and its graphics dependencies are
compiled locally.

On Linux, the host needs a working desktop session and graphics stack. Bevy's
window and renderer may require distribution-specific X11/Wayland, Vulkan,
OpenGL, audio, or input development libraries. The exact package names are
intentionally not prescribed here because they vary across Linux
distributions; consult the Bevy setup documentation for the selected host.

## Deliberate limitations

- The connection uses a temporary development TCP adapter and does not yet
  provide authentication, encryption, reconnection, replication interest
  management, or production backpressure. The local spike does use a bounded
  input/deferred-command queue and a five-second connection timeout.
- The client has only a fixed camera, keyboard input, basic targeting, and a
  compact diagnostic UI. It does not yet provide a full character controller,
  inventory/quest panels, persistence, or addon scripting.
- The scene uses primitive meshes and hard-coded presentation layout. It does
  not validate or load authored terrain, models, animations, particles, sound,
  or music packages.
- NPC marker identity and lifecycle now come from completed authoritative
  snapshots; the static content catalog is used for scene/content validation,
  not for inventing runtime entity IDs.
- NPC labels are written to the terminal rather than rendered as in-world UI.
- The shared catalog is compiled into `mmorpg-content`; an external content
  package format and hot reload pipeline remain future work.
- Bevy remains a provisional runtime choice. This spike only establishes that
  the pinned dependency can open a Linux-oriented 3D shell and consume the
  current content schema; it is not a performance, compatibility, or
  production-readiness result.

The client now uses the shared bounded development decoder and the
protocol-to-presentation adapter for snapshot and event records. The current
temporary snapshot does not yet include inventory stacks or quest progress;
the adapter documents and bounds that limitation. The next client spike is
to make the renderer consume the model directly, then replace the temporary
line transport with the versioned wire protocol. Pen-tablet input and the
separate content editor remain independent technology spikes.
