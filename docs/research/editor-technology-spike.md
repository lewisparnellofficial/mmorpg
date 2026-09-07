# Research: Linux Editor Technology Spike

**Status:** Preliminary findings recorded; input-normalization spike measured;
full editor implementation remains required

**Started:** 2026-09-04

## Question

What Linux desktop technologies and content formats should be evaluated for a
development-tool suite that can author terrain heightmaps, paint terrain,
accept pen-tablet pressure and tilt, place existing assets, author particle
effects, and produce validated content for the Rust client and server?

## Scope and non-goals

This document investigates the editor-facing technology boundary. It covers:

- Linux desktop application frameworks and viewport integration.
- Heightmap and terrain-material authoring.
- Pen-tablet input and pressure handling.
- Placement of existing models, animations, textures, sounds, and music.
- Particle-effect authoring and preview.
- Shared content schemas, source files, validation, and runtime packaging.

It does not select the game client renderer, the final asset-production tools,
the UI scripting runtime, or a database. Artists do not need to model,
rig, animate, record, or compose assets in this editor. The editor must still
import, preview, configure, and place those externally authored assets.

## Current project constraints

The requirements and system overview establish the following constraints:

- The editor must run on Linux.
- The server is Rust; the client and tools should preserve a strong Rust
  integration path, although the requirements do not mandate that every GUI
  toolkit be written in Rust.
- Terrain, NPCs, enemies, vendors, quests, dialogue, spawn points, patrols,
  particles, and references to existing visual/audio assets are in scope.
- Pen tablets are a required authoring input for terrain sculpting and
  painting.
- The authoritative server consumes validated static content. Runtime state
  must remain separate from editable content.
- Development must remain practical on one Linux machine and retain a path to
  multi-process and multi-machine deployment.
- The editor is an internal hobby tool, but dependency licenses and
  redistribution obligations still need to be tracked.

The current [`mmorpg-content`](../../crates/mmorpg-content/src/lib.rs) crate is
the first shared typed boundary. It currently contains compiled starter
definitions and validation, not a source-document format or an asset build
pipeline.

## Executive result

The leading editor candidate is a **Qt 6 desktop shell using Qt Quick/QML and
Qt Quick 3D for the viewport, with the editor/domain model kept in Rust and
exposed through a narrow CXX-Qt boundary**. This is a proposed direction, not
an accepted architecture decision.

The recommendation is driven by the combination of:

1. First-class Linux desktop interaction patterns such as dockable panels,
   model/view widgets, and undo/redo.
2. First-class tablet events carrying pressure, tilt, rotation, eraser, and
   related device information.
3. A 3D viewport that can compose with 2D UI and has a built-in particle
   module useful for preview.
4. A practical way to keep content rules, serialization, validation, and
   document mutation in Rust.

The main risk is the Rust/Qt boundary and the need to verify stylus events in
the actual 3D viewport, not merely in a separate test widget. Qt's open-source
licensing obligations are another risk to record before distributing the tool
or its runtime dependencies.

### Local input-bridge result

The device-neutral editor core now includes a bounded native-event bridge. Its
Qt-shaped adapter accepts proximity, press, motion, release, cancellation, and
proximity-loss phases; normalizes pressure, x/y tilt, rotation, eraser state,
and range metadata; and produces bounded `TabletPoint` stroke output. Focused
tests cover axis conversion, lifecycle transitions, invalid ranges, the stroke
point limit, and cancellation when the pen leaves proximity.

This is a measured normalization/lifecycle prototype, not evidence of real
hardware capture. The actual Qt or SDL event callback, QML/Quick 3D delivery,
mouse fallback, and real-pen responsiveness still require the disposable
editor-shell spike described below.

SDL3 is a credible low-level alternative and has an explicit pen API, but it
does not provide the editor application layer. A Rust SDL3 plus immediate-mode
GUI stack would require more project-owned work for docking, property editing,
undo/redo, asset browsers, and tablet-event plumbing. GTK4 also exposes stylus
input, but the 3D viewport, authoring framework, and particle editor would be
more bespoke.

## Candidate approaches

### Qt 6 with Qt Quick/QML and Qt Quick 3D

#### Sourced facts

Qt Quick 3D provides a high-level 3D API and can mix 2D Qt Quick content with
3D content in the same scene. Its documented modules include asset utilities,
helpers, and a `Particles3D` module. Qt describes the Qt Quick scene graph as
retained and capable of batching primitives; the rendering path can use
available graphics APIs through Qt's rendering hardware interface.

- [Qt Quick 3D overview](https://doc.qt.io/qt-6/qtquick3d-index.html)
- [Qt Quick 3D architecture](https://doc.qt.io/qt-6/qtquick3d-architecture.html)
- [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html)
- [Qt graphics and rendering paths](https://doc.qt.io/qt-6/topics-graphics.html)

Qt's `QTabletEvent` exposes tablet position, pressure, x/y tilt, tangential
pressure, rotation, z position, buttons, and tool/device information when the
hardware and platform provide them. Qt documents a tablet event handler on
`QWindow` and provides a tablet drawing example. Qt also notes that high-
resolution drawing applications should handle tablet events directly because
they can arrive at a higher frequency than ordinary mouse events.

- [QTabletEvent](https://doc.qt.io/qt-6/qtabletevent.html)
- [QWindow tablet events](https://doc.qt.io/qt-6/qwindow.html)
- [Qt tablet example](https://doc.qt.io/qt-6/qtwidgets-widgets-tablet-example.html)

Qt Widgets provides `QDockWidget` for dockable/floating panels. The Qt Undo
Framework provides command objects, undo stacks, command compression, macros,
and clean-state tracking. Qt's model/view APIs are intended to separate data
models from views and reduce duplicate UI-owned data.

- [QDockWidget](https://doc.qt.io/qt-6/qdockwidget.html)
- [Qt Undo Framework](https://doc.qt.io/qt-6/qundo.html)
- [QUndoStack](https://doc.qt.io/qt-6/qundostack.html)
- [Qt model/view tutorial](https://doc.qt.io/qt-6/modelview.html)

Qt Quick 3D's particle module has a root particle-system type, emitters,
particle types, directions, shapes, affectors, trails, and logging. The
particle-system API supports a user-controlled seed, and the documented
testbed demonstrates configurable fire, smoke, and sparkle-style effects.

- [ParticleSystem3D](https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html)
- [ParticleEmitter3D](https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particleemitter3d.html)
- [Qt Quick 3D particle types](https://doc.qt.io/qt-6/qtquick3d-qmlmodule.html)
- [Particles 3D testbed](https://doc.qt.io/qt-6/qtquick3d-particles3d-example.html)

KDAB's CXX-Qt project provides Rust crates and code generation for bidirectional
Rust/Qt bindings, including Rust-defined QObjects usable from C++, QML, and
JavaScript. Its project documentation says it is tested on Linux, but also
describes the project as early development with an API that can change.

- [CXX-Qt repository and project scope](https://github.com/KDAB/cxx-qt)
- [CXX-Qt documentation](https://kdab.github.io/cxx-qt/book/)

Qt is dual-licensed. Qt's official licensing material says the open-source
option includes LGPLv3 and GPLv3 components, that some modules are not
available under LGPLv3, and that LGPL use carries obligations such as providing
corresponding source for the Qt libraries used and respecting dynamic/static
linking requirements.

- [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- [Qt open-source licensing obligations](https://www.qt.io/development/open-source-lgpl-obligations)

#### Project inference

Qt covers more of the editor-specific surface area than the other candidates
examined here. A terrain editor is not only a renderer: it needs document
windows, inspectors, lists and trees, keyboard focus, property editing,
selection, undo/redo, toolbars, file workflows, and long-lived application
state. Qt has documented primitives for these concerns.

The most coherent Rust boundary is not to expose the entire content crate as
an arbitrary QObject graph. Instead, expose a small editor document API:

```text
Rust editor-core
  - document model
  - typed content schema
  - validation
  - undoable edit commands
  - import/package jobs
          ^
          | narrow CXX-Qt properties, signals, and slots
          v
Qt shell and QML viewport
  - panels, inspectors, asset browser
  - pointer/tablet event bridge
  - viewport presentation
```

QML/Qt Quick is attractive for composing the viewport and panels, while Qt
Widgets is attractive for mature desktop docking. Mixing them is possible but
should not be assumed to be free: the first spike must establish whether a
single QML-based shell, a Widgets shell hosting a Quick view, or a custom QML
docking layout gives the better interaction and build result.

Qt Quick 3D particles should be treated as a preview backend rather than the
authoritative particle file format. Otherwise the server/client runtime would
be coupled to QML and Qt's particle semantics.

#### Benefits

- Strongest documented tablet data model among the desktop candidates.
- Mature desktop application concepts relevant to an editor.
- 2D UI and 3D viewport integration in one framework.
- Built-in particle preview primitives.
- Rust can remain the owner of content semantics and validation.

#### Risks

- CXX-Qt is an additional build and ABI boundary, and its API stability must
  be validated before it becomes a foundational dependency.
- Tablet events may be easy to receive in a QWidget but require a deliberate
  bridge when the brush surface is a QML/Quick 3D view.
- Qt packaging and licensing require a project decision and a reproducible
  Linux deployment test.
- Qt Quick 3D's particle semantics are not automatically compatible with the
  eventual game renderer.

### GTK4 with a separate 3D renderer

#### Sourced facts

GTK4 provides `GtkGestureStylus`, which recognizes tablet stylus input and
relays proximity, down, motion, and up events. It provides access to axes and
the current device tool, and includes a backlog API for accumulated tracking
information.

- [GtkGestureStylus](https://docs.gtk.org/gtk4/class.GestureStylus.html)

The GTK documentation establishes a stylus event surface, but it is not a
terrain or 3D authoring framework. A 3D viewport and effect-preview system
would need to be integrated separately.

#### Project inference

GTK4 is a viable Linux-native desktop shell if the project strongly prefers
the GNOME/GTK ecosystem. It would leave more of the editor-specific
integration to project code: viewport embedding, 3D picking, terrain brush
rendering, particle preview, and likely more custom inspector infrastructure.
It is therefore a useful fallback, not the leading candidate for this project.

#### Benefits

- Native Linux desktop toolkit with explicit stylus gestures.
- Avoids introducing Qt's licensing model if a GTK-compatible stack is already
  preferred.
- Good fit for ordinary application panels and accessibility-oriented UI.

#### Risks

- More custom work around a 3D editor viewport and particles.
- The project would still need to choose and bind a renderer.
- GTK stylus events and the chosen renderer's coordinate/picking system need a
  project-owned bridge.

### SDL3 plus a Rust immediate-mode GUI

#### Sourced facts

SDL3 documents a dedicated pressure-sensitive pen API with proximity, down,
up, motion, button, and axis events. The pen-axis API includes pressure,
x/y tilt, distance, rotation, slider, and tangential-pressure axes. SDL
normalizes pressure and several axes, while noting that unsupported axes are
reported as zero. The pen API is documented as available since SDL 3.2.0.

- [SDL pen-event category](https://wiki.libsdl.org/SDL3/CategoryPen)
- [SDL pen-axis events](https://wiki.libsdl.org/SDL3/SDL_PenAxisEvent)
- [SDL pen-axis enumeration and ranges](https://wiki.libsdl.org/SDL3/SDL_PenAxis)
- [SDL event types](https://wiki.libsdl.org/SDL3/SDL_EventType)

The egui project describes itself as a portable Rust GUI that runs natively and
can be integrated with a game engine. Its current `egui-winit` source shows
mouse, touch, and touchpad handling, but the inspected integration does not
constitute evidence that SDL3 pen axes are automatically delivered to egui's
widgets.

- [egui project](https://github.com/emilk/egui)
- [egui-winit event integration](https://github.com/emilk/egui/blob/main/crates/egui-winit/src/lib.rs)

#### Project inference

SDL3 is attractive for a Rust-first client or a custom viewport because it
provides a direct cross-platform window/input layer and now explicitly models
pen input. It is less attractive as the complete editor foundation. The
project would need to build or adopt docking, property grids, asset browsers,
document lifecycle, command history, and robust stylus integration on top.

This option should remain in the technology-spike matrix because it may be the
best route if Qt/Rust integration proves too costly. It should not be selected
for the editor solely because the game client eventually uses SDL or another
low-level windowing layer.

#### Benefits

- Rust-friendly low-level window and input boundary.
- Explicit pen events and normalized pressure/tilt data.
- Keeps the editor renderer and UI technology under project control.
- Potentially shares more infrastructure with a future client.

#### Risks

- Much larger editor-framework burden.
- Pen events may be lost between SDL/winit and the chosen GUI layer unless the
  application owns the full event bridge.
- Docking, model/view, undo, accessibility, and large asset-browser behavior
  become project responsibilities or additional dependencies.

## Terrain authoring requirements

### Sourced facts

OpenEXR supports 16-bit and 32-bit floating-point pixel data and can store
multiple named channels. Its technical documentation identifies `HALF` and
`FLOAT` as available channel types and discusses use of float channels for
data that needs more range or precision than ordinary 8-bit images.

- [OpenEXR technical introduction](https://openexr.com/en/latest/TechnicalIntroduction.html)

PNG is specified as a lossless, portable, well-compressed raster format. It is
therefore useful for interchange and previews, but the specification alone
does not define terrain units, tile addressing, coordinate conventions, or
editor metadata.

- [PNG specification](https://www.libpng.org/pub/png/spec/1.2/png-1.2.pdf)

#### Project inference

The editor should treat a heightmap as a world-space scalar field, not merely
as an image. Each document needs explicit:

- Sample dimensions and tile dimensions.
- Horizontal world scale and vertical unit/scale.
- Origin and coordinate convention.
- Height encoding and missing/invalid-value policy.
- Tile borders or neighbor sampling policy.
- Version and content hash.
- Optional holes, water, walkability, and navigation metadata.

For source authoring, a high-precision format such as tiled OpenEXR is a good
candidate because it preserves useful sculpting headroom. A simple PNG export
can remain a convenient interchange path for external tools. Neither source
format should be sent directly to clients without a build step.

The runtime package should be tiled and streamable. A terrain build should
derive or package:

- Runtime height samples and lower-resolution mip levels.
- Normals or data needed to derive them.
- Terrain-material weight maps.
- Hole/visibility masks.
- Tile bounds and neighbor references.
- Collision and navigation inputs or baked products.

The editor should make tile seams visible and testable. Sculpting near an edge
must update the neighboring tile or use an overlap policy; independently
editing two copies of a shared seam is an avoidable source of cracks.

### Heightmap sculpting operations

The first tool should support a small, deterministic brush vocabulary:

- Raise/lower.
- Smooth.
- Flatten to sampled height.
- Stamp a falloff or imported mask.
- Set or clear terrain holes if holes are part of the initial runtime.

Every brush stroke should record a compact edit command rather than only a
final pixel diff. The command should include the affected tile region, brush
parameters, sampled stroke points, pressure values, and the document revision.
This supports undo/redo, deterministic replay, and later collaboration or
review without making collaboration an initial requirement.

Pressure should influence at least brush strength or flow. Radius, hardness,
spacing, smoothing, and pressure curves should be explicit tool settings. The
editor should display a pressure-disabled fallback when the device reports no
pressure rather than silently producing a different terrain result.

## Terrain-material painting

Terrain painting should be represented as material weights or masks associated
with the same tiled terrain coordinate system as the height field. A first
implementation can use a bounded set of layers per tile and RGBA mask pages,
but the content model must leave room for more than four materials and for
per-layer tiling/UV parameters.

Recommended document data:

```text
TerrainDocument
  schema_version
  world_transform
  height_tiles[]
  material_layers[]
  weight_tiles[]
  hole_tiles[]
  placed_objects[]
  validation_metadata
```

Material painting requires explicit policies for normalization, painting over
existing weights, empty/default material, edge bleeding, and generated
mipmap behavior. The editor should preview the same blend and texture
coordinate rules used by the client. If the preview shader and runtime shader
diverge, artists will author against misleading results.

Pressure can control blend strength/flow. Tilt is less important for a basic
terrain brush but should remain available to specialized brushes, such as
directional stamp or vegetation-scatter tools.

## Pen-tablet input architecture

### Sourced facts

Linux's libinput tablet documentation describes pressure, distance, and tilt
axes, with pressure and distance normalized to `[0, 1]` and tilt represented as
an angle or normalized axis depending on the API/documentation version. It also
documents proximity/tip behavior, touch arbitration, tablet-area transforms,
and device/tool identity considerations.

- [libinput tablet events](https://wayland.freedesktop.org/libinput/doc/latest/api/group__event__tablet.html)
- [libinput tablet support](https://wayland.freedesktop.org/libinput/doc/latest/tablet-support.html)
- [libinput tablet configuration](https://wayland.freedesktop.org/libinput/doc/latest/api/group__config.html)

Qt and SDL3 both expose a higher-level tablet/pen API. Their data models are
not identical, so the editor should not let either framework's event type
become the content or brush API.

### Recommended normalized event boundary

Convert platform events into a project-owned value before tool dispatch:

```text
StrokeSample
  timestamp
  document_position
  viewport_position
  pressure: 0..1
  tilt_x
  tilt_y
  rotation
  distance
  tool_kind: pen | eraser | mouse | other
  buttons
  device_id / tool_id (optional)
  tip_down
```

The input adapter should preserve raw values where possible and derive a
normalized value for brush code. Missing axes must be represented explicitly
or defaulted according to a documented policy; zero can mean either “not
supported” or a real physical value for some axes.

The brush system should consume a stream of `StrokeSample` values, not Qt,
GTK, SDL, or libinput objects. That lets the same brush behavior be tested
with recorded samples and makes mouse-based automated tests possible.

The first local validation must check:

- Wacom or other available tablet on X11 and Wayland if both are relevant.
- Pressure range and response curve.
- Proximity without tip contact.
- Eraser end and barrel buttons where available.
- High-frequency motion without dropped samples that create visible gaps.
- Viewport coordinate conversion while zooming, panning, and rotating.
- Undoing and redoing a long stroke as one logical action.
- Mouse fallback on a machine without a tablet.

## Asset placement and interchange

### Sourced facts

The Khronos glTF 2.0 specification defines glTF as a compact, efficient,
vendor- and runtime-neutral transmission format for 3D assets. glTF supports
scenes, nodes, meshes, materials, textures, skins, and animations, with
extensions declared in the asset. Bevy's documented glTF loader is one example
of a Rust runtime consuming glTF scenes and named scene labels.

- [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.pdf)
- [Bevy glTF loader documentation](https://docs.rs/bevy/latest/bevy/gltf/index.html)

### Project recommendation

Use glTF 2.0 as the preferred interchange format for externally authored
models, skeletal animations, and material references where it fits the
pipeline. Keep an importer abstraction so a different source format can be
converted during asset build.

The world document should store stable project asset references and placement
data, not duplicate the source model:

```text
PlacedAsset
  instance_id
  asset_id
  transform
  parent_id (optional)
  visibility / editor flags
  collision and navigation flags
  gameplay template reference (optional)
  property overrides
```

The asset registry should map `asset_id` to source path, import settings,
derived cache artifacts, bounds, dependencies, and content hash. Source paths
are useful for the editor but should not be the persistent identity used by
server content or saved world references.

Sound and music follow the same pattern even though the editor does not author
them. Placement needs stable asset IDs, attenuation/streaming settings,
trigger or ambient-volume references, and preview controls. Audio import and
runtime codecs remain a separate research track.

Picking should return the stable `instance_id` and the hit position/normal,
then edit the document through an undoable command. It should not mutate a
renderer-owned scene object directly.

## Particle-effect authoring

### Sourced facts

Qt Quick 3D's documented particle system separates a system, particles,
emitters, and affectors. The system has a seed and optional logging. Emitters
support continuous rates and bursts, plus lifespan, variation, scale,
rotation, shape, and velocity settings. Particle types include sprite, model,
line, and model-blend variants, while affectors include gravity, attraction,
repulsion, wandering, scaling, and direction controls.

- [ParticleSystem3D](https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particlesystem3d.html)
- [ParticleEmitter3D](https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particleemitter3d.html)
- [Particle3D](https://doc.qt.io/qt-6/qml-qtquick3d-particles3d-particle3d.html)
- [Qt Quick 3D particle module](https://doc.qt.io/qt-6/qtquick3d-qmlmodule.html)

### Project recommendation

Define an engine-neutral effect schema and build preview/runtime adapters:

```text
ParticleEffectDefinition
  effect_id
  schema_version
  deterministic_seed_policy
  systems[]

ParticleSystemDefinition
  max_particles
  emitters[]
  particle_renderers[]
  affectors[]
  curves[]
  referenced_assets[]
```

The first editor should author a deliberately small subset:

- Sprite or billboard particle.
- Optional model particle.
- Continuous emission and bursts.
- Lifetime and lifetime variation.
- Initial velocity and spread.
- Gravity/drag.
- Color and scale-over-life curves.
- Texture/material reference.
- Fixed seed and preview time controls.

Use the same seed and curve evaluation rules in preview and client runtime.
Preview adapters may implement more visual features, but an unsupported feature
must be reported by validation rather than silently dropped during packaging.

Do not make QML, Qt particle objects, or editor widget state the saved source
format. The editor should produce data that the client renderer can consume
without loading the editor framework.

Particle authoring should include a small preview profiler: active count,
maximum count, update time, draw calls or batches where available, and memory
estimate. This is especially important because effects can be multiplied by a
large encounter and must not be judged only by an isolated preview.

## Shared content schema and packaging

### Sourced facts

Serde provides generic serialization and deserialization traits for Rust data
structures and supports using one data model with multiple formats. RON is a
human-readable Serde format designed to represent structs, enums, tuples,
maps, lists, and primitive data, with comments and a Rust-like syntax.

- [Serde documentation](https://docs.rs/serde/latest/serde/)
- [RON documentation](https://docs.rs/ron/latest/ron/)

### Project recommendation

Use three explicit layers:

```text
Authoring source
  - human-readable documents and external asset references
  - editor-only metadata and history
          |
          v
Content compiler/validator
  - schema/version checks
  - reference resolution
  - cross-document validation
  - asset import and derived-data generation
          |
          v
Runtime package
  - immutable validated definitions
  - content manifest and hashes
  - streamable terrain/assets
  - client/server compatible IDs
```

The source model should be Serde-compatible Rust types shared by the editor,
compiler, and server-side validation. RON is a reasonable first source format
for a Rust-centric hobby project because it is readable, supports comments,
and maps naturally to the existing typed definitions. It should be treated as
an implementation candidate rather than a permanent public contract: pin the
format version, add explicit `schema_version` fields, and provide migration
code.

If external tools or non-Rust contributors become important, JSON or another
language-neutral interchange format can be added at the compiler boundary.
The runtime package should not depend on repeatedly parsing authoring files at
runtime. Its exact binary format should be chosen after measuring load time,
size, memory mapping, and patching needs.

Static content should be divided into independently addressable documents or
packages, for example:

- `items` and `npcs` definitions.
- `quests` and dialogue.
- `zones` and terrain tiles.
- `placements` and spawn/patrol data.
- `particles` and visual/audio references.

All cross-references should use stable typed IDs. The compiler should reject
unknown references, duplicate IDs, invalid transforms, missing assets,
unsupported particle features, terrain seams, invalid material weights, and
content package version mismatches. The server should load only validated
runtime packages and report their manifest/version in diagnostics.

Editor documents should be revisioned and saved atomically. The editor must
not write half of a terrain tile or a placement file and then leave a package
that the server can partially load. A temporary-file-plus-rename strategy is a
reasonable local implementation, with backups or a journal if the editing
session becomes long-lived.

## Comparison

The following is a project assessment, not a vendor benchmark. “High” means
the candidate has a strong documented or practical fit for the criterion;
“medium” means viable with integration work; “low” means substantial custom
work or unresolved compatibility risk.

| Criterion | Qt 6 + Qt Quick 3D | GTK4 + separate renderer | SDL3 + Rust GUI |
|---|---:|---:|---:|
| Linux desktop editor shell | High | High | Low/medium |
| Tablet pressure/tilt boundary | High | Medium/high | High at SDL layer |
| Docking and inspectors | High | Medium | Low/medium |
| Undo/redo and document workflow | High | Medium | Low/medium |
| Integrated 3D viewport | High | Medium | Medium |
| Particle preview primitives | High | Low | Low/medium |
| Rust domain-core integration | Medium | Medium/high | High |
| Reuse with eventual game client | Medium | Medium | Potentially high |
| Build/deployment complexity | Medium | Medium | Medium |
| Licensing/process risk | Medium | Medium | Medium |

The Qt rating is not a claim that Qt is the best game renderer. It is a claim
that Qt currently offers the most complete editor shell and preview surface for
the requested Linux authoring workflow. The game client can use a separate
renderer while consuming the same generated content package.

## Proposed recommendation

Proceed with a short, disposable **Qt 6 + Rust editor-core spike** before
adding a permanent editor crate to the workspace.

The spike should use:

- Qt 6 Quick/QML for the application shell and an initial 3D viewport.
- Qt Quick 3D for a terrain preview, camera, object picking, and a basic
  particle preview.
- A Rust document model with stable IDs, one terrain tile, one material mask,
  one placed glTF asset, one particle effect, and one NPC marker.
- CXX-Qt only at the narrow boundary needed to expose document state and edit
  commands to QML.
- A platform-input adapter that converts tablet data into `StrokeSample`.
- Serde-backed source documents, with RON as the first format candidate.
- A validator executable or library that can run without starting the GUI.

The spike is successful only if it demonstrates all of the following in one
Linux build:

1. A real stylus can sculpt a heightmap using pressure.
2. A real stylus can paint a terrain-material mask.
3. The viewport can select and transform a placed asset.
4. The same document can be saved, reloaded, validated, and rendered again.
5. A particle effect can be authored, seeded, previewed, and exported through
   the engine-neutral schema.
6. Undo/redo treats a complete brush stroke and object transform as logical
   commands.
7. The compiler rejects a deliberately broken cross-reference and reports a
   useful source location.
8. The generated content can be loaded by a small non-GUI Rust validation
   program.

Do not accept the framework based on a screenshot. The stylus event path,
document round-trip, undo semantics, and package validation are the essential
proof points.

## Suggested first schema

The current content crate should evolve toward serializable, owned or
borrowed-at-load definitions without coupling them to GUI types. A future
source model can begin with:

```text
ContentPackage
  schema_version
  package_id
  package_revision
  items[]
  npc_templates[]
  quest_definitions[]
  zones[]
  terrain_documents[]
  placements[]
  particle_effects[]
  asset_registry[]
```

The editor document may include additional fields such as selection state,
camera bookmarks, brush presets, preview settings, and editor-only comments.
Those fields should be stripped or isolated before runtime packaging.

## Risks and unresolved questions

### Unresolved technology questions

- Does Qt Quick/QML receive tablet events at sufficient frequency and with
  pressure/tilt intact when the pointer is over the actual Quick 3D viewport?
- Is a QML-only shell adequate for desktop docking, or is a Widgets shell with
  an embedded Quick view more productive?
- Does the selected Qt version and CXX-Qt release build reproducibly on the
  supported Linux distribution(s)?
- What are the practical Qt packaging and LGPL/GPL obligations for the private
  tool and any distributed client runtime?
- Which Rust graphics/runtime stack will consume the generated terrain and
  particle packages?
- Should terrain source use tiled OpenEXR, a custom binary source, or a hybrid
  with imported PNG support?
- How large can a terrain tile be before brush latency, memory, and package
  patching become unacceptable?
- Should navigation/collision data be generated in the editor, in a build
  worker, or by the server/runtime pipeline?
- Which glTF extensions and compression paths are safe for the eventual client?
- What subset of particle behavior can be guaranteed consistently across
  editor preview, client rendering, and low-performance hardware?
- How should source locations be preserved through nested documents and
  generated packages for useful validation diagnostics?

### Operational and workflow risks

- A GUI document model can accidentally become the source of truth instead of
  the validated Rust schema. Keep validation executable without the GUI.
- Large strokes can produce large undo memory usage. Use tile-region snapshots,
  compressed diffs, or command coalescing after measuring actual authoring
  behavior.
- Artists will notice tile seams and preview/runtime differences quickly. Add
  seam and shader parity checks early.
- External asset paths and imported-cache paths must be portable across Linux
  machines. Persistent references should be IDs plus manifest metadata, not
  absolute workstation paths.
- Particle effects that look acceptable alone may become expensive when
  multiplied across a 200-player encounter. Add budget metadata and load tests.
- A Qt preview does not prove that the eventual client renderer can reproduce
  the effect. Keep the engine-neutral schema small until the client renderer
  exists.

## Prototype and validation plan

### Phase 1: framework and tablet proof

- Build a minimal Qt 6 application with the selected Rust/Qt integration.
- Render a Quick 3D scene and a 2D panel.
- Capture tablet proximity, motion, pressure, tilt, rotation, and eraser data.
- Record samples and replay them without hardware.
- Test both a tablet and mouse fallback.

### Phase 2: terrain document proof

- Load one height tile and one material-weight tile.
- Implement raise, smooth, and paint brushes.
- Apply pressure to strength/flow.
- Make tile borders visible and test edge edits.
- Add command-based undo/redo and atomic save/reload.

### Phase 3: placement and particle proof

- Import one glTF scene and display its bounds.
- Pick, move, rotate, duplicate, and delete a placement.
- Author a seeded sprite effect with one emitter and one affector.
- Export and reload the engine-neutral particle definition.

### Phase 4: content validation proof

- Validate references between terrain, assets, NPC templates, quest data, and
  particles.
- Emit source-located diagnostics.
- Build a runtime package.
- Load the runtime package from a non-GUI Rust program.
- Record build time, package size, load time, memory, and failure behavior as
  measured experiment results under `docs/experiments/`.

## Confidence

**Moderate.** Qt's documented tablet, desktop, 3D, particle, model/view, and
undo capabilities make it the strongest initial editor candidate. The central
unknowns are project-specific: the ergonomics and stability of the Rust/Qt
bridge, the actual stylus path through the chosen viewport composition, and
whether Qt preview behavior can remain a useful approximation of the future
client renderer. Those questions require a local prototype rather than more
reading.

## Sources and evidence classification

The framework/API descriptions in this document are paraphrases of the linked
Qt, GTK, SDL, libinput, Khronos, OpenEXR, Serde, RON, and CXX-Qt documentation.
The terrain data model, engine-neutral particle schema, comparison ratings,
and phased recommendation are project-specific inferences and recommendations
based on the current requirements. No capacity claim is made here, and no
local hardware-tablet, QML/Quick 3D, or rendering measurements have yet been
recorded; the local result so far is limited to deterministic event
normalization and stroke lifecycle tests.
