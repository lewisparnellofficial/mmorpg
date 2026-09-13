+++
id = "ISSUE-001"
type = "experiment"
title = "Close the physical Wayland tablet editor gate"
status = "blocked"
priority = "P1"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = []
blocks = ["ISSUE-004"]
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/experiments/EXP-007-qt-tablet-shell.md", "tools/mmorpg-editor-qt/**", "tools/mmorpg-editor-core/**", "scripts/smoke-editor-bridge.sh"]
forbidden_paths = ["crates/mmorpg-core/**", "crates/mmorpg-server/**"]
validation_commands = ["cmake -S tools/mmorpg-editor-qt -B /tmp/mmorpg-editor-qt-build -DCMAKE_BUILD_TYPE=Release", "cmake --build /tmp/mmorpg-editor-qt-build --parallel", "./scripts/smoke-editor-bridge.sh"]
+++

# ISSUE-001 — Close the physical Wayland tablet editor gate

## Objective

Produce the required physical-device evidence that the Qt/Quick3D editor routes a pressure-sensitive Wayland tablet through the device-neutral bridge without duplicate, dropped, or unterminated strokes.

## Context

- Requirements: `docs/architecture/requirements.md`; `PLAN.md` Milestone 1.
- Architecture/research: `docs/architecture/development-tools.md`, `docs/research/editor-technology-spike.md`.
- Experiments: `docs/experiments/EXP-007-qt-tablet-shell.md`.
- Decisions: the Qt shell choice remains provisional until this gate is measured.
- Related issues: blocks `ISSUE-004`.

## Deliverables

- A physical Wayland tablet run using the Qt shell.
- Updated EXP-007 with host, tablet, tool, axis, event, latency, frame-time, memory, and replay evidence.
- Any narrowly scoped fixes needed for lifecycle, duplicate suppression, cancellation, or coordinate handling.
- Screenshots or captures only if they contain no private data and are useful to the record.

## Acceptance criteria

- [ ] Proximity, press, motion, release, cancellation, and focus/proximity loss are observed and recorded.
- [ ] Pressure editing, mouse fallback, capture/replay, save/reload, undo, and redo are exercised.
- [ ] No duplicate or unterminated strokes or bridge-created gaps occur.
- [ ] p95 callback-to-preview latency is at most one 60 Hz frame and maximum latency is below 50 ms.
- [ ] p95 preview frame time is at most 16.7 ms.
- [ ] A captured physical stroke replays to byte-identical saved terrain source.

## Scope

### In scope

- The existing small terrain proof and its Qt-to-Rust bridge.
- Device and compositor diagnostics required by EXP-007.

### Out of scope

- Terrain materials, tiles, placement, particles, docking, or production editor packaging.
- Replacing the device-neutral editor core with Qt types.

## Dependencies and parallelism

The shell and headless bridge already exist, so this can run independently as a manual experiment. It blocks the aggregate first-slice acceptance record because synthetic tests cannot substitute for the physical gate.

## Worktree and file ownership

### Shared files requiring integration review

- `scripts/validate-all.sh` only if a new deterministic check is needed.

### Read-only context

- `PLAN.md`, `NEXT.md`, and the architecture/research/experiment files listed above.

## Validation

### Automated commands

Run the listed configure/build and bridge smoke commands to verify the shell and deterministic process-boundary regression remain healthy.

### Manual or hardware checks

Use an actual pressure-sensitive tablet on the documented Wayland session and follow every fixed gate in EXP-007.

### Evidence to record

Update `docs/experiments/EXP-007-qt-tablet-shell.md`; do not commit private captures or generated build output.

## Integration contract

The issue closes with either accepted physical evidence or a documented hardware blocker. It must not claim success from mouse, replay, or headless evidence alone.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN Milestone 1 and NEXT item 1.

### 2026-09-12 — implementation run

- Confirmed the connected Wacom Intuos Pro M pen, pad, and finger interfaces
  on the KDE Wayland host.
- Fixed the Qt Quick shell root to use an `Item` under `QQuickView` and
  declared the Quick3D index attribute, removing the launch warning and
  allowing the proof shell to run.
- Release shell build and `scripts/smoke-editor-bridge.sh` pass.
- Fixed tablet event routing so pen input outside the terrain viewport is
  delegated to Qt Quick controls; the previous handler consumed pen presses
  on the Save, Reload, Capture, Undo, Redo, and brush controls.
- Physical retest confirmed that the pen can activate the Qt Quick controls;
  this portion of the gate now works.
- The instrumented run recorded real pen pressure/tilt input and one focus-loss
  cancellation, but also exposed compatibility mouse strokes from pen toolbar
  clicks. Mouse fallback is being restricted to the terrain viewport before
  the final gate retest.
- Final isolated pen capture produced 1,258 points and replayed to a
  byte-identical saved terrain source; toolbar pen clicks produced no bridge
  mouse events after the routing fix.
- Explicit proximity enter/leave and eraser evidence remain unavailable from
  this Qt/Wayland run, so the physical gate is measured but not fully closed.
- A filtered raw monitor of `/dev/input/event27` also observed no
  `BTN_TOOL_PEN`/`BTN_TOOL_RUBBER` transitions; the device capability bitmap
  does not advertise those codes. Proximity is therefore a verified
  device/input-stack blocker for the current backend, not merely missing test
  coverage.
- Added a lifecycle recorder that captures event routing, device metadata,
  explicit interaction-state transitions, cancellation reasons, and a
  shutdown active-stroke invariant. A focused proximity run remains to be
  interpreted using this recorder rather than raw evdev codes alone.
- Combined physical hover/eraser run identified the flipped Wacom tool as
  `eraser` and completed a real eraser press/motion/release lifecycle. Raised
  pen hover generated motion without terrain input, but still produced no
  explicit Qt proximity-enter/leave events.
- Moved to `docs/issues/blocked/` after the physical and raw-input audits
  established that this Qt/Wayland setup exposes hover motion and eraser
  identity but no proximity-enter/leave lifecycle signal.

## Completion report

- Result: Blocked. The physical pen path is functional and the eraser,
  pressure, routing, timing, capture, replay, and focus-loss behavior are
  evidenced, but explicit physical proximity enter/leave is unavailable from
  the current Qt/Wayland input path.
- Commit(s): `388fd83`
- Changed files: `tools/mmorpg-editor-qt/main.cpp`,
  `tools/mmorpg-editor-qt/Main.qml`, `tools/mmorpg-editor-qt/README.md`,
  `docs/experiments/EXP-007-qt-tablet-shell.md`, and this issue record.
- Tests and validation run: Qt Release configure/build,
  `./scripts/smoke-editor-bridge.sh`, physical Wacom runs, lifecycle CSV
  capture, and filtered raw monitoring of `/dev/input/event27`.
- Acceptance criteria not met: explicit proximity enter/leave observation.
- Follow-up issues: `ISSUE-009` — investigate a Wayland-native tablet
  proximity source and backend strategy.
- Known limitations: normal pen hover motion is visible and eraser identity is
  available, but the current device/backend does not expose a usable physical
  proximity transition to Qt. The abnormal `press → proximity-leave` path is
  covered conceptually but still needs a deterministic boundary test.
- Integration notes: editor feature work may proceed behind the existing
  device-neutral bridge. Do not claim the complete physical tablet gate or
  accept `ISSUE-004` until ISSUE-009 resolves the proximity question or the
  project owner explicitly accepts the platform limitation.
