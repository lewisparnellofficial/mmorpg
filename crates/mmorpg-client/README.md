# MMORPG client technology spike

This is the first Linux client shell prototype. It is intentionally a
technology spike, not a production client or an engine commitment.

The binary uses Bevy `0.19.1` to open a desktop window, create a 3D camera and
directional light, draw a primitive town and field, and instantiate the NPC
placements from the shared `mmorpg-content` starter catalog. It connects to
the local development server on a background TCP worker, requests the bounded
`snapshot` bootstrap response, and supports keyboard movement, target cycling,
loot, vendor, and quest intents, and server-authoritative basic attacks. The
vendor and enemy markers use different colors. Startup logs also identify the
content definitions and placements that were instantiated.

Decoded snapshots and events are also projected through the standalone
`mmorpg-client-adapter` into `mmorpg-client-model`. The Bevy scene retains
connection/transport state locally, while rendered player and NPC state comes
directly from the authoritative presentation model.

## Run

From the repository root:

```bash
cargo fmt --manifest-path crates/mmorpg-client/Cargo.toml -- --check
cargo check --manifest-path crates/mmorpg-client/Cargo.toml
cargo run --manifest-path crates/mmorpg-client/Cargo.toml
```

To use the typed wire listener, pass the line address first and the optional
wire address second:

```bash
cargo run --manifest-path crates/mmorpg-client/Cargo.toml -- \
  127.0.0.1:4000 --wire-address 127.0.0.1:4001
```

In wire mode the background worker sends the loopback-only development token,
waits for the typed `Authenticated` response, lists available characters,
selects the first character, enters the world, requests a typed bootstrap
snapshot, and passes typed
`ServerMessage` values through `mmorpg-client-adapter`. The default line mode
remains available while the two paths are compared locally. This handshake is
not production authentication or an internet-safe credential flow.

The window can be closed using the normal window controls. With the server
running in another terminal, use `WASD` to move, `Tab` to select the next
known NPC, `Space` to attack, and `L` to loot the selected target. `V` lists
vendor stock, `B` buys one unit of the first listing, `O` requests quest offers,
`E` accepts the first offer, and `R` attempts to turn in the first quest. The
first build may take several minutes because Bevy and its graphics
dependencies are compiled locally.

On Linux, the host needs a working desktop session and graphics stack. Bevy's
window and renderer may require distribution-specific X11/Wayland, Vulkan,
OpenGL, audio, or input development libraries. The exact package names are
intentionally not prescribed here because they vary across Linux
distributions; consult the Bevy setup documentation for the selected host.

## Deliberate limitations

- The connection uses a temporary development TCP adapter. Wire mode has a
  loopback-only development token handshake, but the project does not yet
  provide production authentication, encryption, reconnection, replication
  interest management, or production backpressure. The local spike does use
  bounded input, deferred-command, and wire-frame queues plus a five-second
  connection timeout.
- The client has only a fixed camera, keyboard input, basic targeting, and a
  compact starter-loop UI. It does not yet provide a full character
  controller, persistence, or addon scripting.
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

The client uses the shared bounded development decoder and the
protocol-to-presentation adapter for line-mode records. Its opt-in wire mode
uses the typed server-message adapter for snapshot and event records, and the
renderer reads player/NPC state directly from `ClientWorld`. The compact HUD
now displays inventory, vendor listings, quest offers, quest progress, and
authoritative notifications. `V` lists vendor stock, `B` buys one unit of the
first displayed listing, `O` requests quest offers, `E` accepts the first
displayed offer, `R` attempts to turn in the first displayed quest, and `L`
submits a loot request for the selected target. These keys only submit server
intents; they do not mutate gameplay state locally. Inventory capacity remains
a temporary fixed value until the protocol is versioned. Pen-tablet input and
the separate content editor remain independent technology spikes.
