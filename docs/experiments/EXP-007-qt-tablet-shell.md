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
`QQuickView` subclass receives tablet press/move/release events, exposes input
diagnostics to QML, accepts mouse input as pressure `1.0`, and suppresses
compatibility mouse handling after a tablet press. The QML shell includes a
Quick3D preview placeholder and editing controls.

This is a directly measured local build result, not hardware evidence. The
current host has a Wayland session and `/dev/input` devices, but no physical
tablet run, event capture, latency distribution, replay equivalence, or
save/reload result has been recorded yet. Cancellation, conversion into
`NativeTabletEvent`, and Rust `TerrainEditor` command wiring remain open.

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

## Required measurement record

When a physical run is available, record the exact commit, distribution and
kernel, Wayland compositor, Qt/CXX-Qt versions, GPU/driver, tablet/tool,
display scale, axis availability, event and stroke counts, sample spacing,
latency/frame-time distributions, peak history memory, and the results of
proximity, pressure, motion, release, eraser, focus/proximity cancellation,
mouse fallback, replay, save/reload, undo, and redo.
