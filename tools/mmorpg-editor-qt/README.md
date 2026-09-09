# mmorpg-editor-qt

This is the first native Qt shell proof for the device-neutral
`mmorpg-editor-core` tablet boundary. It is intentionally a standalone CMake
target rather than a Cargo workspace member: Qt Quick, Qt Quick3D, and the
native event loop are host dependencies, while the Rust editor core remains
GUI-independent.

The shell currently provides:

- a Qt Quick window with a Qt Quick3D heightmap mesh driven by Rust preview
  samples;
- native `QTabletEvent` press/move/release handling;
- mouse fallback with pressure `1.0`;
- suppression of compatibility mouse input after a handled tablet press;
- a visible input-diagnostics panel;
- a `QProcess` bridge to the Rust `mmorpg-editor-core` command protocol for
  native stroke application, brush selection, undo/redo, validated atomic
  save/reload, captured-stroke export, and captured-stroke replay.

The C++ shell remains free of terrain rules: it formats native samples into a
small process boundary, while `TabletEventBridge`, `TerrainEditor`,
`CapturedStroke`, and the source parser remain Rust-owned. This is still not
the completed editor milestone because physical pen measurements remain to be
recorded in `EXP-007`; the mesh and replay path are automated development
proof, not hardware evidence.

## Headless configure and build

```bash
cmake -S tools/mmorpg-editor-qt -B /tmp/mmorpg-editor-qt-build \
  -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/mmorpg-editor-qt-build --parallel
```

The build does not launch a window and therefore belongs in aggregate
validation. Build the Rust bridge before launching the shell; the convenience
launcher does this automatically. Run the shell with `./scripts/run-editor.sh`
from a Linux desktop session. Override the bridge and output paths with
`MMORPG_EDITOR_CORE_BRIDGE`, `MMORPG_EDITOR_TERRAIN_OUTPUT`, and
`MMORPG_EDITOR_CAPTURE_OUTPUT` when needed.
