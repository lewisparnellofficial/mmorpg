# mmorpg-editor-qt

This is the first native Qt shell proof for the device-neutral
`mmorpg-editor-core` tablet boundary. It is intentionally a standalone CMake
target rather than a Cargo workspace member: Qt Quick, Qt Quick3D, and the
native event loop are host dependencies, while the Rust editor core remains
GUI-independent.

The shell currently provides:

- a Qt Quick window with a Qt Quick3D terrain-preview placeholder;
- native `QTabletEvent` press/move/release handling;
- mouse fallback with pressure `1.0`;
- suppression of compatibility mouse input after a handled tablet press;
- a visible input-diagnostics panel.

This is a build and event-routing proof, not the completed editor milestone.
Cancellation, conversion into `NativeTabletEvent`, persistence, replay,
undo/redo commands, and physical pen measurements remain to be wired and
recorded in `EXP-007`.

## Headless configure and build

```bash
cmake -S tools/mmorpg-editor-qt -B /tmp/mmorpg-editor-qt-build \
  -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/mmorpg-editor-qt-build --parallel
```

The build does not launch a window and therefore belongs in aggregate
validation. Run the shell with `./scripts/run-editor.sh` from a Linux desktop
session.
