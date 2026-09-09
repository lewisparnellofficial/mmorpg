# EXP-007: Qt Wayland tablet shell

**Status:** Shell build proof complete; physical tablet evidence pending

## Purpose

Connect the Linux Qt shell to the device-neutral editor boundary and establish
whether a real Wayland tablet can drive the terrain authoring lifecycle without
duplicate compatibility mouse events, dropped strokes, or unacceptable preview
latency. Synthetic editor-core tests are supporting evidence only; they do not
prove this experiment.

## Fixed gates before measurement

- No duplicated or unterminated strokes and no visible bridge-created gaps.
- p95 callback-to-preview latency is no greater than one 60 Hz frame and the
  maximum is below 50 ms on the documented host.
- p95 preview frame time is at or below 16.7 ms for the small proof document.
- A captured physical stroke replays to byte-identical saved terrain source.

## Current implementation evidence

The standalone `tools/mmorpg-editor-qt` target now configures and compiles
against Qt 6.11.2 with Qt Quick, Qt Quick3D, and Qt Quick Controls. Its native
`QQuickView` subclass receives tablet proximity, press/move/release, proximity
loss, and focus-loss lifecycle events, exposes input diagnostics to QML,
accepts mouse input as pressure `1.0`, adds monotonic nanosecond diagnostics,
and suppresses compatibility mouse handling after a tablet press. The QML
shell includes a Quick3D preview placeholder and editing controls. The shell
now starts the Rust `mmorpg-editor-core --bridge` process and sends native
samples through a bounded line protocol; the Rust process owns
`NativeTabletEvent`, `TabletEventBridge`, `TerrainEditor`, `CapturedStroke`,
atomic save/reload, and undo/redo. The device-neutral Rust bridge preserves
`Pen`, `Eraser`, and `Mouse` source and rejects backward timestamps.

The editor core also provides a bounded `CapturedStroke` text format with
round-trip validation and bridge-mediated replay tests. The headless editor
bridge smoke now proves native-lifecycle input, terrain mutation, capture
export, save/reload, and undo/redo through that process boundary. This is
still not physical tablet evidence.

This is a directly measured local build result, not hardware evidence. The
current host has a Wayland session and `/dev/input` devices, but no physical
tablet run, event capture, latency distribution, or replay equivalence has
been recorded yet. The Quick3D surface remains a visual placeholder rather
than a heightmap mesh, and the process bridge is a local proof rather than a
production editor IPC protocol.

## Reproduction

Headless configure/build:

```bash
cmake -S tools/mmorpg-editor-qt -B /tmp/mmorpg-editor-qt-build \
  -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/mmorpg-editor-qt-build --parallel
```

Desktop launch:

```bash
./scripts/run-editor.sh
```

The deterministic core-only path remains separate:

```bash
./scripts/run-editor-core.sh --output /tmp/starter-terrain.mmterrain
```

The headless Qt-to-Rust process-boundary regression is:

```bash
./scripts/smoke-editor-bridge.sh
```

## Required measurement record

When a physical run is available, record the exact commit, distribution and
kernel, Wayland compositor, Qt/CXX-Qt versions, GPU/driver, tablet/tool,
display scale, axis availability, event and stroke counts, sample spacing,
latency/frame-time distributions, peak history memory, and the results of
proximity, pressure, motion, release, eraser, focus/proximity cancellation,
mouse fallback, replay, save/reload, undo, and redo.
