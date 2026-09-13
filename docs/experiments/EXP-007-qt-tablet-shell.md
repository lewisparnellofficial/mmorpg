# EXP-007: Qt Wayland tablet shell

**Status:** Shell build proof complete; physical tablet run measured; acceptance pending

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
shell includes a Quick3D heightmap mesh and editing controls. The shell now
starts the Rust `mmorpg-editor-core --bridge` process and sends native
samples through a bounded line protocol; the Rust process owns
`NativeTabletEvent`, `TabletEventBridge`, `TerrainEditor`, `CapturedStroke`,
atomic save/reload, replay, and undo/redo. Preview samples are returned from
the Rust document and converted into a bounded indexed Quick3D mesh. The
device-neutral Rust bridge preserves `Pen`, `Eraser`, and `Mouse` source and
rejects backward timestamps.
The QML viewport registers its window-space rectangle with the C++ bridge,
which maps resized viewport pixels into the bounded 32×32 document domain
before serialization. Compatibility mouse-release suppression remains armed
until the synthesized release is consumed.
Tablet events outside that registered viewport are delegated back to Qt Quick
so pen activation of editor controls is not mistaken for a terrain stroke.

The editor core also provides a bounded `CapturedStroke` text format with
round-trip validation and bridge-mediated replay tests. The headless editor
bridge smoke now proves native-lifecycle input, terrain mutation, capture
export, save/reload, replay through the same bridge, and undo/redo through
that process boundary. The Qt target builds and starts offscreen without
runtime diagnostics. These are still not physical tablet measurements.

This is a directly measured local build result, not hardware evidence. The
current host has a Wayland session and `/dev/input` devices, but no physical
tablet run, event capture, latency distribution, or physical replay
equivalence has been recorded yet. The process bridge is a local proof rather
than a production editor IPC protocol.

## 2026-09-12 physical-gate setup

The tablet was connected before this run and is present in the authoritative
Linux input inventory:

- Session: KDE Wayland (`WAYLAND_DISPLAY=wayland-0`).
- Tablet: Wacom Intuos Pro M (`USB Vendor=056a`, `Product=0315`).
- Tool interface: `Wacom Intuos Pro M Pen`, `/dev/input/event27`.
- Tablet control interface: `Wacom Intuos Pro M Pad`, `/dev/input/event28`.
- Finger interface: `Wacom Intuos Pro M Finger`, `/dev/input/event29`.
- Qt: 6.11.2; Qt Quick, Quick 3D, and Quick Controls 2.
- Shell revision under test: working-tree revision after the QML root-item
  and Quick3D index-attribute fixes; exact commit is pending until the gate
  is completed.
- Release shell configure/build: passed.
- Automated bridge smoke: passed native lifecycle, terrain mutation, save,
  capture, replay, reload, undo, and redo.

This setup confirms that a physical tablet is available; it is not itself
physical input evidence. Stroke counts, axes, latency distributions,
frame-time distributions, memory, replay equivalence, and cancellation results
remain pending until the fixed manual sequence is performed in the visible Qt
shell.

### Initial physical interaction result

The physical pen was used after the viewport-routing fix. Pen activation of
the Qt Quick controls was confirmed to work, including the editor controls
that previously responded only to the mouse. This confirms the control/brush
event-routing portion of the gate. It does not yet establish the required
tablet lifecycle, duplicate suppression, latency, frame-time, cancellation,
or byte-identical replay results.

### 2026-09-12 measured physical run

The second physical run produced `/tmp/mmorpg-editor-qt-diagnostics-run2.csv`
and a pen capture with 750 points. The capture was made before the later
toolbar interactions and identifies its source as `pen`; the generated files
remain outside the repository.

Measured results from the shell diagnostics:

- Native tablet records: 7,562 pen records, including 16 pen presses and 16
  pen releases. No eraser source was reported.
- Pressure: 1,919 native records had nonzero pressure; observed range was
  `0.0000`–`1.0000`. Tilt values were nonzero in the physical records.
- Bridge records: 1,445 pen moves, 4 pen presses, 3 pen releases, and one
  pen cancellation. The additional native press/release pair was outside the
  viewport and was delegated to Qt Quick.
- Cancellation: one `focus lost` cancellation was recorded. No explicit
  proximity-enter or proximity-leave record was delivered by this Qt/Wayland
  run.
- Preview callback latency: 10 measured returns; p95 `6.595 ms`, maximum
  `7.571 ms`.
- Render duration: 1,286 measured frames; p95 `0.243 ms`, maximum `81.701 ms`.
- Event timestamps were monotonic.

The run exposed a compatibility-mouse routing defect: four pen presses on
controls also reached the window mouse fallback as mouse strokes. The Qt
shell was then tightened so mouse fallback, like pen capture, is restricted to
the terrain viewport. The final isolated run retested control-click side
effects; explicit proximity handling and replay equivalence remained pending
at that point.

### 2026-09-12 final physical capture

The final isolated run used fresh outputs under `/tmp` and contained one
viewport stroke followed by pen Capture and Save actions. It produced a
1,258-point capture whose source is `pen`. The diagnostics contained one
viewport pen press/release pair and two additional pen press/release pairs on
the controls; none of those control interactions produced bridge mouse events.
Pressure reached `1.0000`, and the capture replay changed 693 samples.

For the replay gate, a clean zero-valued terrain baseline was saved through
the same Rust bridge, the physical capture was replayed, and the replayed
terrain was saved. The physical saved terrain and replayed terrain are
byte-identical. Both files have SHA-256
`23d662cece9835f4dfad18c4b7614a859ddbf7460e99f559f8c43d51ac25ea3c`.

The final run measured one preview callback at `11.412 ms`; the measured
render durations had p95 `0.228 ms` and maximum `81.938 ms`. The earlier
physical run remains the larger timing sample: preview p95 `6.595 ms`, maximum
`7.571 ms`; render p95 `0.243 ms`. Across the available captures, timestamps
were monotonic and no control-click compatibility mouse events reached the
bridge after the viewport-only mouse-fallback fix.

The fixed gate is not fully closed: no explicit proximity-enter or
proximity-leave event was delivered by this Qt/Wayland path. Focus-loss
cancellation was observed in the earlier run. These are recorded as unresolved
platform/device evidence, not inferred passes.

### 2026-09-12 proximity backend audit

The Wacom pen event node was monitored directly while the pen was moved above
the tablet, over the editor, away from the tablet, and back. A filtered raw
event monitor for `EV_KEY` codes `BTN_TOOL_PEN` (320) and `BTN_TOOL_RUBBER`
(321) produced no transitions. The authoritative device capabilities for
`/dev/input/event27` expose axes and motion, but do not advertise those tool
identity/proximity key codes. Qt also emitted `Can't send tablet event with no
proximity surface` while the shell was active.

This establishes a device/input-stack limitation rather than an untested Qt
branch: the current Wacom interface does not provide a physical tool
proximity signal that this shell can observe. The shell must not infer a
physical proximity pass from the first motion or press event. Closing this
criterion requires either a tablet/tool/backend that exposes proximity through
Linux/Wayland, or a separately approved native backend integration that can
observe the device's proprietary proximity state.

### 2026-09-12 combined hover and eraser run

The combined physical run produced 751 normal-pen hover records and 1,156
eraser records. The eraser tool was identified by Qt as `source=eraser` and
produced one complete press/motion/release lifecycle through the Rust bridge;
233 eraser records had nonzero pressure. No explicit proximity-enter or
proximity-leave event was delivered during the raised-pen hover cycle. This
closes the eraser observation criterion and confirms hover motion without
terrain mutation, while the explicit proximity event criterion remains a
platform limitation for this Qt/Wayland session.

## 2026-09-09 hardware-gate audit

The current Wayland session was inspected before attempting the physical gate.
The host is CachyOS Linux on kernel `7.2.2-1-cachyos`, running `kwin_wayland`
with Qt `6.11.2` and an NVIDIA GeForce RTX 5070. The authoritative
`/proc/bus/input/devices` inventory contained two keyboard-class devices,
two mouse devices, virtual keyboard/pointer devices, and audio controls, but
no stylus, tablet, Wacom, or other pressure-sensitive input device. Therefore
no physical pen event was generated or counted, and the fixed latency,
duplicate-stroke, axis-availability, and physical replay gates remain
**pending**. Mouse fallback and the synthetic/headless bridge tests are not
substitutes for this requirement.

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

The desktop shell writes a fresh CSV diagnostics capture to
`/tmp/mmorpg-editor-qt-diagnostics.csv` by default. Override it with
`MMORPG_EDITOR_DIAGNOSTICS=/path/to/capture.csv`. The capture contains
monotonic timestamps for native tablet phases, pressure/tilt/rotation,
normalized bridge events, preview returns with callback latency, and render
durations. It also records whether tablet events were handled or delegated,
explicit editor interaction-state transitions, cancellation reasons, and a
shutdown summary including the completed/cancelled counts and whether any
stroke remained active. Do not commit the generated capture.

## Required measurement record

When a physical run is available, record the exact commit, distribution and
kernel, Wayland compositor, Qt/CXX-Qt versions, GPU/driver, tablet/tool,
display scale, axis availability, event and stroke counts, sample spacing,
latency/frame-time distributions, peak history memory, and the results of
proximity, pressure, motion, release, eraser, focus/proximity cancellation,
mouse fallback, replay, save/reload, undo, and redo.
