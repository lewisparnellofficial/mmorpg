# Client Technology Spike

**Status:** Research complete; recommendation remains provisional

**Date:** 2026-09-04

**Scope:** Linux-friendly technology choices for the Rust MMORPG client and
the closely related editor/runtime boundary. This document evaluates rendering
and input, Linux support, asset workflows, UI integration, network integration,
and future addon scripting. It does not select a production engine, define the
wire protocol, or replace the requirements baseline.

This document was written after inspecting the accepted
[requirements baseline](../architecture/requirements.md) and the proposed
[system overview](../architecture/system-overview.md). No code or production
technology decision is implied by this research record.

## Executive result

The best current prototype direction is:

1. Evaluate **Bevy as the game-client runtime**, using its native window/input,
   3D renderer, asset system, and UI, while keeping the MMORPG protocol and
   presentation model in project-owned crates.
2. Keep the **editor as a separate application boundary**. Do not assume the
   game engine's editor, if it has one, is suitable for terrain sculpting,
   content validation, and pen-tablet workflows.
3. Run a short **pen-tablet frontend spike** before selecting the editor
   toolkit. Qt has a documented tablet-event API with pressure, tilt, rotation,
   and high-frequency event handling; SDL3 also documents pen axes. Bevy's
   normal window/input surface and winit's documented event types do not by
   themselves establish a complete tablet-authoring API.
4. Use **glTF 2.0/GLB as the interchange format for authored 3D assets**, but
   do not treat glTF as the project's complete content format. Terrain,
   placements, quests, NPC definitions, particle parameters, and asset
   references should remain in validated project content documents and be
   compiled into runtime packages.
5. Treat the network client as a **separate asynchronous adapter** that turns
   server snapshots/events into client presentation state and turns local
   input into server intents. The engine must not own authority, persistence,
   or MMORPG rules.
6. Use **Luau as the leading addon-scripting candidate**, subject to a small
   implementation spike. Luau's maintainers document embedding and sandboxing
   features directly, including removal of filesystem/process facilities,
   isolated environments, interruption, and memory accounting. The project
   must still expose only a UI API; a safe VM alone must not be allowed to
   issue gameplay commands.

This is a recommendation based on project fit, not a benchmark result. The
main risks are Bevy's relatively young ecosystem and API churn, the lack of a
first-party Bevy content editor comparable to Godot or Fyrox, and the need to
prove pen-tablet behavior on the Linux display/input stacks used by the
project.

## Project constraints that drive the choice

### Sourced facts from project documents

- The game, server, and tools must run on Linux.
- The server is Rust, and the client must preserve server authority over
  movement, combat, progression, economy, and persistence.
- The client must support player UI customization through the same scripting
  system used by the default UI.
- Player scripts must be sandboxed and must not automate gameplay through the
  official addon API.
- The editor must support terrain heightmaps, terrain painting, world/NPC/
  quest placement, particle effects, and placement of existing visual and
  audio assets.
- Artists must be able to use pen tablets for terrain work.
- The first client prototype only needs to display and interact with the
  starter zone; it does not need to solve the full 5,000-client deployment.

These are requirements from the repository, not claims about any third-party
technology.

### Project inferences

- The most important client scalability boundary is not the renderer's ability
  to draw 5,000 entities. It is the ability to render a bounded,
  interest-managed presentation set while the server owns the complete world.
- A client engine should therefore be selected for development productivity,
  asset and UI workflows, Linux reliability, and the ability to render a
  large local scene. Realm-scale simulation must remain in the server
  architecture.
- The editor and the game client have overlapping 3D viewport needs but
  different interaction needs. A terrain brush, undo/redo stack, content
  validation, and pen-tablet event stream deserve explicit editor interfaces
  rather than accidental reuse of game input code.
- The shared mmorpg-content crate is a useful schema boundary, but it should
  not become coupled to Bevy, Godot, Qt, or any other presentation framework.

## Candidate comparison

The ratings below are project assessments based on the sourced capabilities and
the current repository requirements. They are not measured performance results.

| Candidate | Rendering and Linux runtime | UI and input | Asset/editor fit | Networking and addons | Main concern |
| --- | --- | --- | --- | --- | --- |
| **Bevy** | Strong Rust-native 2D/3D path; winit windowing and wgpu backends include Vulkan and OpenGL/GLES paths on Linux | Native Bevy UI and input; additional UI integrations exist; tablet-specific input is not established by the basic stack | Asset loading and glTF support are useful; no comparable first-party authoring editor | Networking is intentionally application-owned, which fits the server boundary; Luau can be embedded separately | Ecosystem/API churn and a substantial amount of custom editor work |
| **Fyrox** | Strong Rust-native 2D/3D engine and Linux target according to the project site | Built-in UI and scene/editor workflows; input and editor behavior are more integrated | Full editor, hot reload, particles, and scene graph reduce initial tooling work | Network and untrusted addon policy remain project work; Rust plugins are not a player-script sandbox | Greater engine/editor coupling and a smaller ecosystem than Bevy/Godot |
| **Godot 4 + Rust GDExtension** | Mature Linux-capable engine with Vulkan/OpenGL renderer choices and a complete editor | Mature UI, input actions, and editor; tablet behavior still needs validation for the exact tool workflow | Strongest out-of-the-box asset import/editor story among candidates | Rust runs through the community godot-rust GDExtension binding; network and player addon sandbox remain project work | Rust/C++ boundary, engine-native project formats, and keeping the shared content pipeline authoritative |
| **Custom wgpu + winit** | Maximum control and direct Rust ownership; wgpu documents Vulkan and OpenGL/GLES Linux backends | winit plus a UI library gives a small foundation, but nearly all game/editor UX is custom | Complete control over package formats, but no ready-made scene/editor/animation pipeline | Maximum control, maximum implementation burden; all network/UI/addon boundaries are ours | Very high scope and risk of building an engine instead of an MMORPG |

The practical shortlist for an implementation spike is **Bevy**, **Fyrox**, and
**Godot + Rust**. Custom wgpu/winit should remain a fallback or renderer
learning track, not the default product path, unless the other candidates fail
hard requirements.

## Sourced findings

### Bevy

**Sourced facts.** Bevy's official introduction describes it as a data-driven
Rust game engine with 2D and 3D capabilities, modularity, and parallelizable
application logic. Its official getting-started documentation says Bevy is a
Rust dependency and identifies additional Linux operating-system dependencies
for windows, audio, and peripheral input. The official plugin documentation
says DefaultPlugins includes a 2D/3D renderer, asset loading, UI, windows,
input, and a winit-backed event loop. [Bevy introduction](https://bevy.org/learn/quick-start/introduction/),
[Bevy getting started](https://bevy.org/learn/quick-start/getting-started/),
[Bevy plugins](https://bevy.org/learn/quick-start/getting-started/plugins/)

Bevy's documented glTF module provides an asset loader for glTF 2.0 and
exposes meshes, materials, scenes, cameras, lights, skins, and animations.
The loader has settings for selecting which asset components to load and for
validation. [Bevy glTF support](https://docs.rs/bevy/latest/bevy/gltf/),
[Bevy glTF loader settings](https://docs.rs/bevy/latest/bevy/gltf/struct.GltfLoaderSettings.html)

**Project inference.** Bevy's modular plugin architecture maps cleanly to a
client split into platform, renderer, UI, network, prediction/interpolation,
content, and addon-host plugins. It also allows a headless server or test
harness to avoid registering rendering plugins, which supports keeping server
and simulation crates engine-independent.

**Recommendation.** Use Bevy for the first client runtime spike. Keep the
Bevy-facing code in a future client crate and let it depend on mmorpg-content, a
future protocol crate, and presentation-only adapters. Do not put Bevy
component types into mmorpg-core or mmorpg-content.

**Risk.** Bevy's official documentation currently recommends the latest stable
release and warns that large 3D debug builds can take a long time and perform
poorly without suitable build configuration. The project should pin a tested
Bevy version, enable fast developer iteration deliberately, and avoid
architectural dependence on unstable third-party plugins until they are
validated locally. [Bevy setup and build guidance](https://bevy.org/learn/quick-start/getting-started/setup/)

### Fyrox

**Sourced facts.** The Fyrox project site describes a Rust game engine with a
full-featured editor, hot reloading of native code and assets, 2D/3D support,
PBR rendering, UI, animation, particles, scene graph support, and PC Linux
support. [Fyrox feature overview](https://fyrox.rs/)

**Project inference.** Fyrox is attractive if reducing the amount of custom
editor infrastructure is more important than minimizing engine/editor
coupling. Its integrated scene graph and editor could shorten the path to a
visible starter zone, existing asset placement, and particle authoring.

**Recommendation.** Include Fyrox in the time-boxed client/editor spike, but
require a clean export path from engine scenes to project-owned content IDs. A
Fyrox scene file should not become the authoritative representation of NPCs,
quests, spawns, vendor catalogs, or persistent world state.

**Risk.** The official feature overview establishes capability, but it does
not by itself establish the exact Linux pen-tablet behavior, the long-term
stability of every editor workflow, or the suitability of its plugin model for
untrusted player addons. Those require local validation and should not be
assumed from the existence of an editor or Rust scripting/plugin support.

### Godot 4 with Rust

**Sourced facts.** Godot's official documentation describes three renderers:
Forward+, Mobile, and Compatibility. Forward+ and Mobile use Vulkan, Direct3D
12, or Metal through RenderingDevice; Compatibility uses OpenGL. The
documentation explicitly positions Compatibility for older or low-end desktop
hardware and Forward+ for desktop 3D projects with modern hardware.
[Godot renderer overview](https://docs.godotengine.org/en/stable/tutorials/rendering/renderers.html)

Godot documents a built-in InputEvent system covering keyboard, mouse, joypad,
MIDI, touch, and generic actions, as well as an InputMap intended to make
bindings reconfigurable. The Linux platform documentation includes both
Wayland and X11. [Godot InputEvent](https://docs.godotengine.org/en/stable/tutorials/inputs/inputevent.html),
[Godot Input](https://docs.godotengine.org/en/stable/classes/class_input.html),
[Godot Linux platform notes](https://docs.godotengine.org/en/stable/tutorials/platform/linux/index.html)

Godot's official asset-pipeline documentation covers import of images, audio,
translations, and 3D scenes. Its editor is built with Godot's own renderer
and UI system rather than GTK or Qt. [Godot asset pipeline](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/),
[Godot editor development](https://docs.godotengine.org/en/stable/engine_details/editor/introduction_to_editor_development.html)

The godot-rust/gdext project documents Rust bindings for Godot 4's GDExtension
API. It says the binding is an alternative to GDScript and that Rust and
GDScript can be mixed, while also noting that the binding interacts with the
Godot C++ engine and can have breaking changes associated with upstream API
evolution. [godot-rust GDExtension bindings](https://github.com/godot-rust/gdext)

**Project inference.** Godot is the lowest-effort path to a conventional
editor-backed 3D prototype and a rich UI. However, the project would need a
clear distinction between Godot resources/scenes and the validated shared
content package. Rust should remain the owner of authoritative network-facing
behavior; Godot nodes should be views and local interaction controllers.

**Recommendation.** Keep Godot as a serious fallback, particularly if the
custom editor schedule becomes the dominant risk. Do not adopt it solely to
obtain GDScript. The player addon system still needs a deliberately sandboxed
API, and ordinary engine scripting/editor plugins must not be exposed to
untrusted player code.

**Risk.** godot-rust is a community binding rather than a Rust implementation
of the Godot engine. The FFI boundary, Godot version pin, build/export tooling,
hot reload behavior, and ownership rules all need a small prototype before
this path is accepted.

### Custom wgpu and winit

**Sourced facts.** wgpu's current Rust documentation lists Vulkan as supported
on Linux and OpenGL/GLES as a Linux-capable downlevel/best-effort backend. It
supports WGSL by default and can consume GLSL and SPIR-V with the corresponding
features. [wgpu backends](https://docs.rs/wgpu/latest/wgpu/enum.Backend.html),
[wgpu crate documentation](https://docs.rs/wgpu/latest/wgpu/)

winit describes itself as a cross-platform window-creation and event-loop
library. Its documented event types include window events, cursor movement,
keyboard events, and device events, and it documents logical/physical
coordinates and DPI scale-factor changes. [winit documentation](https://docs.rs/winit/latest/winit/)

For UI, egui is an immediate-mode Rust GUI. The official integration
information documents egui-winit and egui-wgpu integrations; egui-wgpu's
feature documentation notes that Linux builds require selecting Wayland or
X11 support for the winit integration. [egui documentation](https://docs.rs/egui/latest/egui/),
[egui-wgpu](https://docs.rs/egui-wgpu/latest/egui_wgpu/)

**Project inference.** This stack is a credible basis for a custom editor and
renderer, but it is not a complete client engine. Scene management, skeletal
animation, particles, terrain, import processing, UI composition, input
mapping, debugging, and tooling would become project responsibilities.

**Recommendation.** Use this stack only when the project specifically wants
engine-level control or when a Bevy/Fyrox/Godot prototype exposes a blocker. A
lightweight custom wgpu viewport may still be useful inside a future editor,
but building the entire game on it now would expand the project beyond the
current vertical slice.

**Risk.** wgpu's backend availability does not guarantee identical feature
levels or driver behavior across Linux GPUs. A compatibility policy, fallback
renderer, shader validation, and a test matrix are still required.

## Pen-tablet and editor findings

### Sourced facts

Qt's official QTabletEvent documentation exposes tablet position and device
information plus pressure, x/y tilt, tangential pressure, rotation, and z
values. It documents tablet press/move/release and proximity events, explains
that high-resolution tablet events may arrive at a higher frequency than
synthetic mouse events, and includes Linux/X11 notes. [Qt QTabletEvent](https://doc.qt.io/qt-6/qtabletevent.html)

Qt's official tablet example demonstrates using pressure, tilt, and rotation
to modulate drawing behavior. [Qt tablet example](https://doc.qt.io/qt-6/qtwidgets-widgets-tablet-example.html)

SDL3's official wiki documents pen axes for pressure, x/y tilt, distance,
rotation, slider, and tangential pressure. It states that unsupported axes are
reported as zero and that the pen-axis API is available since SDL 3.2.0.
[SDL3 pen axes](https://wiki.libsdl.org/SDL3/SDL_PenAxis)

winit's general event documentation is useful for normal game input and DPI,
but the reviewed API surface does not present a first-class tablet-pressure
abstraction comparable to Qt's QTabletEvent or SDL3's pen-axis API. This is an
observation of the reviewed documentation, not proof that no backend or
extension can expose tablet information.

### Project inference

A terrain brush needs more than mouse coordinates. Pressure can control brush
strength or radius; tilt and rotation can later control brush shape or
orientation; proximity and eraser identity can improve authoring ergonomics.
Losing pressure information and falling back to synthetic mouse events would
make the Linux editor fail a stated requirement for serious terrain work.

### Recommendation

The first editor spike should implement a small tablet diagnostic and terrain
canvas before any engine is selected. It should record:

- Device identity and tool type when available.
- Absolute canvas position and timestamps.
- Tip pressure in a normalized range.
- X and Y tilt when available.
- Rotation, tangential pressure, distance, and eraser state when available.
- Whether the event came from a native tablet event or a synthesized mouse
  event.
- Event frequency, dropped samples, and behavior under Wayland and X11.

The editor should convert these platform events to a project-owned TabletSample
structure. Terrain tools must consume that structure rather than depend
directly on Qt, SDL, winit, or a specific windowing backend.

The spike must test the actual tablet models available to the project. A
capability documented by a toolkit is not evidence that every Linux compositor,
driver, tablet, or desktop session reports it identically.

## Asset pipeline

### Sourced facts

Khronos describes glTF as a royalty-free runtime 3D asset-delivery format that
minimizes runtime processing and supports scenes, nodes, meshes, materials,
textures, skins, and animations. The specification explicitly says glTF is not
an authoring format. Khronos also documents GLB as a binary container and KTX
as an efficient texture container used with glTF. [Khronos glTF overview](https://www.khronos.org/gltf/),
[glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.pdf)

Bevy provides a glTF 2.0 loader and custom asset-loader interfaces. Godot
provides an editor import pipeline for images, audio, and 3D scenes. These
systems prove that both candidates can consume common runtime asset formats,
but they do not make their project-resource formats interchangeable.

### Project inference

The project needs two related but distinct asset layers:

~~~text
artist source files
    -> imported/intermediate assets
    -> validated project content package
    -> client runtime assets + server content data
~~~

The client needs renderable assets and references. The server needs static game
rules and stable IDs. The editor needs source documents, undo/redo, previews,
validation errors, and placement metadata. A single engine scene file cannot
serve all three responsibilities safely.

### Recommendation

Adopt the following provisional pipeline:

- Use glTF 2.0/GLB for imported meshes, materials, skeletons, and animations.
- Use standard image formats during authoring and a build step for runtime
  texture conversion/compression as needed.
- Store project-authored terrain, placements, particles, and game content in
  versioned project documents keyed by stable IDs.
- Store references by content ID and asset path/hash, never by mutable editor
  object identity or display name.
- Validate references, bounds, supported formats, animation names, material
  constraints, and content IDs before packaging.
- Build a runtime package that can be loaded without the editor or source
  authoring applications.
- Keep package version and content revision in the client handshake and in
  server logs so a rendered world can be correlated with its server content.

RON is a possible human-editable format for early project documents because its
Rust documentation describes readable structs, enums, maps, comments, and
Serde-compatible data. It should remain an implementation candidate rather
than a public long-term format until migration/versioning requirements are
known. [RON documentation](https://docs.rs/ron/latest/ron/)

## UI integration and addon scripting

### Sourced facts

Bevy provides a built-in UI plugin through its standard plugin set. egui is an
immediate-mode GUI with native winit and wgpu integrations. Godot provides a
mature retained scene/UI model and input-action system. Fyrox advertises an
integrated UI system and editor. These facts establish available building
blocks, not a ready-made addon API for this project.

Luau's official documentation says the language is designed for embedding and
that it is sandboxed by default with a deliberately limited host-facing API.
The sandboxing guide documents removal of io, package, unsafe os, large parts
of debug, dofile, and loadfile; it also documents isolated global contexts,
immutable built-ins, interruption callbacks, and memory tracking.
[Luau overview](https://luau.org/),
[Luau sandboxing guide](https://luau.org/sandbox/),
[Luau C API and sandbox helpers](https://luau.org/api/)

Rhai is a Rust-embedded scripting language whose documented engine API
includes limits for operations, call depth, arrays, maps, strings, variables,
and functions. Wasmtime documents fuel or epoch interruption and resource
limiters for WebAssembly execution. [Rhai engine limits](https://docs.rs/rhai/latest/rhai/struct.Engine.html),
[Wasmtime execution limits](https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html),
[Wasmtime resource limiters](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html)

### Project inference

The addon boundary is more important than the syntax of the language. A
malicious or buggy addon must not be able to:

- Send an ability, movement, target, trade, purchase, or chat command without
  an explicit user action represented by the client.
- Read secrets, authentication tokens, local files, process state, or arbitrary
  network endpoints.
- Hold unrestricted references to engine objects or mutate authoritative
  state.
- Consume unbounded CPU, memory, event subscriptions, timers, or UI nodes.
- Spoof protected UI such as server notices, secure confirmations, or another
  player's private data.

The default UI can use the same language and API as addons only if the default
UI is also treated as an untrusted client of the protected presentation
boundary. Privileged code should be a small native host layer, not a hidden set
of extra functions available to the default UI scripts.

### Recommendation

Make Luau the leading scripting candidate for the client spike because its
primary documentation directly addresses untrusted embedding. Design the API
in layers:

~~~text
native client host
    -> protected UI model and event bridge
        -> default UI Luau VM
        -> one isolated Luau VM per player addon
~~~

The first addon API should be read-only or cosmetic wherever possible:

- Read-only view models for player, target, party, inventory, quest, and combat
  presentation state.
- UI layout, styles, textures, text, and local animations.
- Subscriptions to whitelisted presentation events.
- User-generated UI events that become local UI intents, not server commands.

The host should enforce per-addon memory, instruction/time, update-frequency,
layout-node, and event-subscription quotas. It should reject filesystem,
process, arbitrary socket, reflection, native module, and gameplay-command
capabilities. Script errors should disable only the affected addon and leave
the default UI usable.

A future WASM-based addon system remains worth evaluating if stronger language
and module isolation becomes a requirement. It brings explicit resource
controls, but adds a larger toolchain and ABI/API design burden. Rhai may be
simpler to integrate into Rust, but its safety still depends heavily on the
host's registered functions and resource policy. These are recommendations,
not security proofs.

## Networking integration

### Sourced facts

Quinn describes itself as a pure-Rust, async-compatible implementation of IETF
QUIC, tested on Linux, macOS, and Windows. It provides reliable ordered and
unordered streams, application datagrams, TLS 1.3 integration through rustls,
and a protocol state machine that can operate independently of a particular
I/O runtime. [Quinn introduction](https://quinn-rs.github.io/quinn/quinn.html),
[Quinn Rust API](https://docs.rs/quinn/latest/quinn/)

Renet documents a Rust client/server game-networking library with reliable
ordered, reliable unordered, and unreliable message channels, fragmentation
and reassembly, connection management, and optional authentication/encryption
layers. [Renet documentation](https://docs.rs/renet/latest/renet/)

Tokio documents asynchronous TCP/UDP/timer APIs and a multithreaded runtime.
[Tokio overview](https://tokio.rs/blog/2020-12-tokio-1-0)

### Project inference

No candidate renderer should dictate the MMORPG protocol. A real-time MMO needs
different delivery semantics for commands, snapshots, reliable combat results,
chat, and transient effects. The rendering engine should consume normalized
client messages after transport decoding, not parse transport packets inside UI
or rendering systems.

A client network architecture should resemble:

~~~text
transport (QUIC/UDP/TCP during experiments)
    -> framing, authentication, versioning
        -> protocol messages
            -> client session state
                -> interpolation/interest presentation
                    -> engine entities and UI view models
~~~

### Recommendation

For the immediate client spike, retain the development TCP protocol or use a
loopback adapter. The goal is to prove the ownership boundary and a visible
round trip, not to select a production transport prematurely.

When the first binary protocol is introduced, create a separate protocol crate
with versioned envelopes and explicit categories:

- Client intents: movement, target, ability, interaction, and UI-confirmed
  actions.
- Reliable server state: login/session, authoritative snapshots, inventory,
  quests, and combat outcomes.
- Unreliable or replaceable presentation state: movement samples, facing,
  transient effects, and nearby animation hints.
- Out-of-band services: chat, metrics, and content/package negotiation.

Quinn is a good transport candidate for an encrypted, multiplexed prototype;
Renet is a useful comparison if message-channel semantics are more valuable
than a general QUIC transport. Neither removes the need for server-side
validation, interest management, backpressure, reconnect behavior, and
replay/recovery rules.

## Recommended spike plan

This plan is intentionally small and produces evidence before an engine is
accepted.

### Spike A: client shell

Build a disposable Linux client prototype that:

- Opens a window and reports the graphics backend and adapter.
- Renders a camera, a simple terrain-like field, a town marker, three enemy
  markers, and a player marker.
- Loads a small mmorpg-content catalog without modifying it.
- Converts local keyboard/mouse input into explicit client intents.
- Receives a development-server state update through a narrow adapter.
- Interpolates remote positions without changing authoritative state.
- Displays inventory, quest progress, target, and server rejection messages.

Measure startup time, frame time, asset load time, memory use, and behavior on at
least one Vulkan and one OpenGL/GLES-capable Linux setup if available.

### Spike B: editor viewport and tablet input

Build a separate diagnostic/editor prototype that:

- Runs under the project's normal Linux display sessions, including Wayland
  and X11 where available.
- Draws a small heightmap canvas.
- Applies a pressure-sensitive brush with undo/redo.
- Saves and reloads a source document.
- Converts platform events into the project-owned TabletSample type.
- Places a placeholder NPC and asset reference in a project document.
- Loads and previews one existing glTF/GLB model and one particle definition.
- Reports missing capabilities rather than silently treating every stylus as a
  mouse.

Implement the first pass with the toolkit that provides the clearest tablet
events on the development machine. A native Qt tablet canvas is a credible
baseline; SDL3 pen input is a second baseline. If the final editor uses Bevy,
its viewport can still consume the normalized tablet samples.

### Spike C: UI and addon boundary

Build a non-gameplay UI test that:

- Runs the default UI and one user addon through the same Luau-facing API.
- Exposes a mock read-only player/target/inventory/quest view model.
- Allows a script to create and reposition a panel and respond to a permitted
  local UI event.
- Rejects file, process, socket, native-module, and gameplay-command attempts.
- Terminates an infinite loop or over-budget script.
- Enforces per-addon memory, node, event, and execution budgets.
- Keeps the default UI alive after an addon error.

The test should include negative cases and produce logs suitable for code
review. A successful hello world is not sufficient evidence of sandbox safety.

## Acceptance gates for an engine selection

An engine/runtime combination should not be accepted until it satisfies all of
these gates:

- **Linux:** Repeatable build and launch on the project's supported Linux
  environment, with documented Vulkan fallback behavior.
- **Rendering:** Stable 3D scene, camera, terrain placeholder, animated model,
  particle effect, and UI at the target prototype frame rate.
- **Input:** Keyboard, mouse, focus loss/reacquisition, DPI scaling, and
  configurable bindings. Tablet behavior is evaluated separately in the editor
  spike.
- **Assets:** Load a runtime package without the editor; report missing,
  incompatible, or stale assets clearly.
- **Networking:** Disconnect/reconnect, malformed message, server correction,
  and backpressure behavior do not crash or mutate authoritative state.
- **UI:** Default UI and addons use the same public view-model contract;
  protected host actions remain outside the addon API.
- **Tools:** The editor can save/load a terrain source document, apply a
  pressure-sensitive brush, place stable-ID content, preview assets, and run
  validation.
- **Iteration:** A change to a UI layout, content definition, or placed asset
  can be tested without rebuilding the server or contaminating the simulation
  crate with presentation dependencies.

## Unresolved risks and questions

1. **Bevy version and ecosystem stability.** Which pinned release gives the
   best combination of renderer, UI, glTF, animation, and Linux-driver support
   for the duration of the first playable client?
2. **Tablet event coverage.** Does the selected editor toolkit report pressure,
   tilt, eraser, proximity, and high-frequency samples consistently under both
   Wayland and X11 for the actual tablets in use?
3. **Editor integration.** Is a native Qt/SDL3 editor viewport worth the
   cross-language or binding complexity, or does a custom Bevy/Fyrox viewport
   with a small native tablet adapter provide a better long-term workflow?
4. **Godot fallback boundary.** If Godot wins the editor productivity test,
   can its imported resources be treated as disposable presentation assets
   while all stable content IDs and validation remain in project-owned files?
5. **Runtime asset packaging.** Which texture compression, shader packaging,
   animation naming, and cache invalidation policy are required for Linux
   distribution without relying on an editor installation?
6. **Addon bytecode and content trust.** Are scripts distributed as source,
   validated bytecode, signed packages, or a combination? How are package
   revisions and rollback handled?
7. **Script scheduling.** How much time per frame may each addon consume, and
   should scripts run once per frame, on event delivery, or in a budgeted
   cooperative queue?
8. **Protocol choice.** Does the project need QUIC streams/datagrams, a game
   message-channel layer, or a custom UDP protocol once replication experiments
   quantify packet loss, bandwidth, and head-of-line behavior?
9. **Linux display matrix.** What minimum GPU features and driver families are
   acceptable, and is OpenGL/GLES a supported fallback or merely a diagnostic
   path?
10. **Client/server content compatibility.** How are content package revisions
    negotiated when a client has an older asset package than the server?

## Recommendation summary by concern

| Concern | Provisional direction | Confidence |
| --- | --- | --- |
| Game-client runtime | Bevy spike first; Fyrox and Godot + Rust remain viable alternatives | Medium |
| Low-level rendering | wgpu through the selected engine; Vulkan first, OpenGL/GLES fallback where validated | Medium |
| Normal game input | Engine input abstraction with explicit action mapping; do not send raw input as authority | High |
| Pen-tablet input | Normalize Qt/SDL3/native events into a project-owned sample type | Medium-low |
| Runtime 3D assets | glTF 2.0/GLB interchange plus project-owned runtime packages | Medium-high |
| Editor | Separate application boundary; toolkit selected by tablet and workflow spike | Medium |
| UI | Engine-native UI for rendering, project-owned view models for API stability | Medium |
| Addons | Luau leading candidate with isolated VMs and strict UI-only host bindings | Medium |
| Networking | Engine-independent adapter; retain TCP for the first visible round trip | High |
| Production transport | Quinn and Renet are comparison candidates after protocol/replication tests | Low-medium |

## Source classification

The following source categories are used throughout this document:

- **Sourced fact:** A capability, API, or documented behavior stated by a
  primary project, standards body, or official API reference.
- **Project inference:** A conclusion drawn from those facts and this
  repository's requirements.
- **Recommendation:** A proposed project choice that has not yet been accepted
  in an ADR or proven by a local experiment.
- **Unresolved risk:** A question that documentation cannot settle and that
  needs a local prototype, benchmark, or explicit owner decision.

The primary sources consulted were:

- [Bevy official introduction](https://bevy.org/learn/quick-start/introduction/)
- [Bevy official plugins documentation](https://bevy.org/learn/quick-start/getting-started/plugins/)
- [Bevy official setup documentation](https://bevy.org/learn/quick-start/getting-started/setup/)
- [Fyrox official feature overview](https://fyrox.rs/)
- [Godot official renderer documentation](https://docs.godotengine.org/en/stable/tutorials/rendering/renderers.html)
- [Godot official asset pipeline](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/)
- [Godot official input documentation](https://docs.godotengine.org/en/stable/tutorials/inputs/inputevent.html)
- [godot-rust GDExtension project](https://github.com/godot-rust/gdext)
- [wgpu Rust API documentation](https://docs.rs/wgpu/latest/wgpu/)
- [winit Rust API documentation](https://docs.rs/winit/latest/winit/)
- [egui Rust API documentation](https://docs.rs/egui/latest/egui/)
- [Qt 6 tablet-event documentation](https://doc.qt.io/qt-6/qtabletevent.html)
- [SDL3 pen-axis documentation](https://wiki.libsdl.org/SDL3/SDL_PenAxis)
- [Khronos glTF overview](https://www.khronos.org/gltf/)
- [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.pdf)
- [Luau official sandboxing guide](https://luau.org/sandbox/)
- [Luau official embedding overview](https://luau.org/)
- [Quinn official introduction](https://quinn-rs.github.io/quinn/quinn.html)
- [Renet Rust API documentation](https://docs.rs/renet/latest/renet/)
- [Tokio official overview](https://tokio.rs/blog/2020-12-tokio-1-0)
- [RON Rust documentation](https://docs.rs/ron/latest/ron/)

The project should revisit volatile dependency facts before implementing a
production client. Version numbers, Linux backend support, editor APIs, and
binding maturity can change; this record captures the evidence and reasoning
available on 2026-09-04.
