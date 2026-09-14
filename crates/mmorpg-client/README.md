# MMORPG client technology spike

This is the first Linux client shell prototype. It is intentionally a
technology spike, not a production client or an engine commitment.

The binary uses Bevy `0.19.1` to open a desktop window, create a 3D camera and
directional light, draw a primitive town and field, and instantiate the NPC
placements from the shared `mmorpg-content` starter catalog. It connects to
the local development server on a background TCP worker, requests the bounded
`snapshot` bootstrap response, and supports keyboard movement, target cycling,
loot, vendor, and quest intents, server-authoritative basic attacks, and the
typed healer ability. The vendor and enemy markers use different colors.
Startup logs also identify the
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

The client follows the same sequence as normal play: authenticate the account,
receive its character list, select a character, pass the content check, enter
the world, and apply the authoritative bootstrap snapshot. For repeatable
local testing, the graphical client supports these startup options:

```text
<server-address>                 typed server address (default 127.0.0.1:4000)
--token <value>                  development authentication token
--character-id <id>              select this listed character automatically
--acceptance-smoke               run the bounded starter loop after world entry
```

`--token` is an account-login fixture in the loopback-only development server;
the server derives the authenticated account from it. `--character-id` only
selects an entry returned by the authenticated character list and cannot bind
an arbitrary character. Omitting `--character-id` keeps the interactive
character-selection prompt. All of these options use the normal typed session
state machine and do not bypass authentication, content compatibility, or
server authorization.

The default connection is typed wire. To use a second typed listener during
staged smoke testing, pass it with `--wire-address`:

```bash
cargo run --manifest-path crates/mmorpg-client/Cargo.toml -- \
  127.0.0.1:4000 --wire-address 127.0.0.1:4001
```

The client normally loads its two minimal UI proof runners from built-in
source. To exercise bounded filesystem package discovery and manifest/source
integrity validation in the real client process, pass `--addon-root PATH`. The
root must contain at least two numeric package directories, each with a
validated `manifest.toml` and source entry; packages are loaded in deterministic
dependency order and the first two become the default and ordinary UI proof
panels. This is a local development path and does not provide a remote package
repository or signature privilege.

For the Wasmi isolation comparison, `--addon-process-host PATH` starts an
explicit supervised process host, waits for its `READY` record, requests a
contract-backed panel, and retains the child until client shutdown. The host
executable is launched as `PATH --process-host`. Add
`--addon-process-package-root PATH` to make that host load a manifest-declared,
SHA-256-verified WAT entry instead of its built-in fixture. This is lifecycle
and failure-boundary evidence for the alternative runtime, not the production
addon decision; the default client still uses the embedded Luau adapter.

The background worker sends the loopback-only development token,
waits for the typed `Authenticated` response, lists available characters,
displays the account's available characters, waits for the user to press
`Enter` to select the highlighted development character, enters the world,
requests a typed bootstrap snapshot, and passes typed
`ServerMessage` values through `mmorpg-client-adapter`. This handshake is
not production authentication or an internet-safe credential flow. If the
typed socket closes or the development server is restarted, the worker retries
after 500 ms and repeats the full authentication, character-selection, and
bootstrap sequence. The server therefore supplies a fresh authoritative
snapshot; this is reconnect-by-restore, not a production session-resume
protocol.

The client test suite includes a local typed-wire peer that drops the first
connection after bootstrap. It verifies that the real background worker
reconnects, re-authenticates, waits for a second explicit character-selection
intent, enters the world again, and requests a fresh bootstrap snapshot.

The window can be closed using the normal window controls. With the server
running in another terminal, press `Enter` at the character-selection prompt,
then use `WASD` to move, `Tab` to select the next known NPC, `Space` to attack,
and `L` to loot the selected target. `V` lists vendor stock, `B` buys one unit
of the first listing, `O` requests quest offers, `E` accepts the first offer,
and `R` attempts to turn in the first quest. The typed healer ability is
available to healer characters through the typed command path; the graphical
spike does not yet bind it to a keyboard shortcut. The first build may take several
minutes because Bevy and its graphics
dependencies are compiled locally.

At startup the client also runs the narrow `ui.v1` host proof: a default UI
runner and an ordinary Luau addon each describe a bounded panel, and the host
projects those labels into the secure-action HUD. Native input is still the
only path that can mint the trusted basic-attack dispatch; addon code cannot
call the protected action.

For a bounded renderer-active starter-loop smoke, pass
`--character-id 1 --acceptance-smoke`. This opt-in mode submits the same typed
vendor, quest, movement, target, attack, loot, and turn-in intents through the
normal worker after the authoritative session reaches the world. It does not
grant authority or replace the server validation path; it exists to exercise
the real Bevy process and presentation projection without depending on a
compositor's virtual-keyboard protocol.

The three-role graphical gate runs this mode for characters 1, 2, and 3. The
damage character performs the starter purchase, quest, combat, loot, and
turn-in loop; the tank submits movement, party invite, target, and taunt
intents; and the healer accepts the invite and submits recovery. Those fixed
character and entity IDs are local development-smoke assumptions, not a
general client targeting contract.

The renderer backend can be requested explicitly for a controlled diagnostic:
`--render-backend auto` (the default), `--render-backend vulkan`, or
`--render-backend gl`. The request is applied to Bevy's wgpu initialization and
is reported as `render_backend_request=...`; a backend that has no usable
adapter fails at startup rather than silently being reported as validated.

Pass `--frame-time-stats` to collect a bounded real Bevy frame-time
distribution after a two-second warm-up and five-second sample window. The
client reports p50, p95, p99, and maximum milliseconds; this is diagnostic
instrumentation and does not claim that the PLAN thresholds pass.

On Linux, the host needs a working desktop session and graphics stack. Bevy's
window and renderer may require distribution-specific X11/Wayland, Vulkan,
OpenGL, audio, or input development libraries. The exact package names are
intentionally not prescribed here because they vary across Linux
distributions; consult the Bevy setup documentation for the selected host.

## Deliberate limitations

- The connection uses a temporary development TCP adapter. Wire mode has a
  loopback-only development token handshake, but the project does not yet
  provide production authentication, encryption, session fencing, replication
  interest management, or production backpressure. The local spike retries a
  dropped typed-wire connection and reboots its development session, but does
  not preserve in-flight commands or use a reconnect token. It uses bounded
  input, deferred-command, and wire-frame queues plus a five-second connection
  timeout.
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

The client uses the typed server-message adapter for snapshot and event
records, and the
renderer reads player/NPC state directly from `ClientWorld`. The compact HUD
now displays inventory, vendor listings, quest offers, quest progress, and
authoritative notifications. `V` lists vendor stock, `B` buys one unit of the
first displayed listing, `O` requests quest offers, `E` accepts the first
displayed offer, `R` attempts to turn in the first displayed quest, and `L`
submits a loot request for the selected target. These keys only submit server
intents; they do not mutate gameplay state locally. Snapshot schema version 3
supplies each player's inventory capacity explicitly and carries an optional
private party membership/leadership summary; the client no longer assumes a
fixed starter value or receives remote private fields when applying a complete
snapshot. Version-2 snapshots remain decodable. Pen-tablet
input and the separate content editor remain independent technology spikes.
