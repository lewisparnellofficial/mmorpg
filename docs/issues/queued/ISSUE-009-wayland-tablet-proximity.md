+++
id = "ISSUE-009"
type = "investigation"
title = "Recover a trustworthy Wayland tablet proximity source"
status = "ready"
priority = "P1"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = "ISSUE-001"

depends_on = []
blocks = ["ISSUE-001"]
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/experiments/EXP-007-qt-tablet-shell.md", "docs/research/**", "tools/mmorpg-editor-qt/**"]
forbidden_paths = ["crates/mmorpg-core/**", "crates/mmorpg-server/**"]
validation_commands = ["cmake -S tools/mmorpg-editor-qt -B /tmp/mmorpg-editor-qt-build -DCMAKE_BUILD_TYPE=Release", "cmake --build /tmp/mmorpg-editor-qt-build --parallel", "./scripts/smoke-editor-bridge.sh"]
+++

# ISSUE-009 — Recover a trustworthy Wayland tablet proximity source

## Objective

Determine and implement, or explicitly rule out, a supported source of
physical tablet-tool proximity enter/leave information for the Qt editor on
Linux Wayland, then provide the evidence and boundary behavior required to
unblock ISSUE-001.

## Context

- Requirements: `docs/architecture/requirements.md` requires Linux pen-tablet
  support for terrain authoring.
- Architecture/research: `docs/architecture/development-tools.md` and
  `docs/research/editor-technology-spike.md`.
- Experiment: `docs/experiments/EXP-007-qt-tablet-shell.md`.
- Blocked issue: `docs/issues/blocked/ISSUE-001-physical-tablet-gate.md`.

The current shell receives real Wacom motion, pressure, tilt, and eraser
identity through Qt, but the tested KDE Wayland session did not deliver
`TabletEnterProximity` or `TabletLeaveProximity`. A filtered monitor of
`/dev/input/event27` also found no `BTN_TOOL_PEN` or `BTN_TOOL_RUBBER`
transitions. Those observations are evidence about the current path, not proof
that every Wayland or Wacom backend lacks the information.

## Handoff facts for future agents

The original physical host and run were:

- Linux distribution: CachyOS.
- Kernel: `7.2.2-1-cachyos`.
- Desktop/compositor: KDE Wayland with `kwin_wayland`.
- Session: `XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-0`.
- Qt: `6.11.2`; Qt Quick, Qt Quick 3D, and Qt Quick Controls 2.
- GPU: NVIDIA GeForce RTX 5070.
- Tablet: Wacom Intuos Pro M / USB vendor `056a`, product `0315`.
- Qt device name: `Wacom Intuos Pro M Pen stylus`.
- Qt system IDs observed: `1443842` for pen and `1443850` for eraser.
- Pen event node: `/dev/input/event27`.
- Pad event node: `/dev/input/event28`.
- Finger event node: `/dev/input/event29`.
- The device capability bitmap for event27 exposes motion/axes but does not
  advertise the legacy tool key codes 320/321.
- The relevant implementation revision is commit `388fd83`; subsequent
  uncommitted changes may add this issue and the latest recorder refinements.

Generated captures were kept outside the repository. Useful paths from the
run were `/tmp/mmorpg-editor-qt-diagnostics-run5.csv`,
`/tmp/mmorpg-editor-qt-diagnostics-proximity-eraser.csv`, and the corresponding
terrain/capture files. They may not exist on another host.

## Deliverables

- A source-backed explanation of the Qt/Wayland/Wacom event path and whether
  proximity should be observable at the chosen boundary.
- A minimal diagnostic or adapter change, if a supported signal exists.
- A deterministic test for `proximity-enter → press → move → proximity-leave`
  that asserts one cancellation and `active=false`.
- A physical or backend-specific capture proving idle proximity enter/leave,
  or a documented owner-approved platform limitation if no supported source is
  available.
- Updated EXP-007 and ISSUE-001 handoff/closure records.

## Acceptance criteria

- [ ] The agent identifies the correct authoritative proximity signal for the
  selected Linux/Wayland backend, without treating a legacy evdev code as the
  sole source of truth.
- [ ] The Qt shell or its explicitly approved native adapter records proximity
  enter and leave with device/tool metadata.
- [ ] The deterministic interrupted-stroke test produces exactly one cancel
  and ends with `active=false`.
- [ ] The result is either physical proximity evidence that unblocks ISSUE-001
  or an owner-visible, reproducible platform limitation with a clear backend
  replacement path.

## Scope

### In scope

- Qt Wayland tablet event delivery.
- Wayland tablet-tool protocol diagnostics.
- Linux input/libinput/Wacom capability inspection.
- The existing Qt-to-Rust editor boundary and its diagnostics.
- Focused lifecycle tests and EXP-007 documentation.

### Out of scope

- Terrain rules or Rust MMORPG simulation.
- Replacing Qt as the editor framework.
- Production tablet support for every Linux compositor or hardware model.
- A proprietary Wacom integration without an explicit project decision.

## Validation

### Automated commands

Run the Qt configure/build and editor bridge smoke commands from the front
matter. Add a focused lifecycle test for the interrupted-stroke sequence.

### Manual or hardware checks

On a documented Wayland host, record a raised-pen hover cycle and, where the
tool supports it, a pen/eraser identity transition. Capture the compositor,
Qt, device, and backend diagnostics together.

### Evidence to record

Record source layers, event counts, timestamps, device/tool metadata, routing,
state transitions, cancellation counts, and the final active-stroke state in
EXP-007. Do not commit private captures or generated build output.

## Integration contract

After this issue closes, ISSUE-001 may be moved to review only if its physical
proximity criterion is evidenced or its limitation is explicitly accepted by
the project owner. Any new native input dependency must remain outside the
device-neutral Rust terrain rules and must document Linux permissions,
compositor support, licensing, and failure behavior.

## Progress log

### 2026-09-12 — coordinator

- Created from the blocked proximity criterion in ISSUE-001.
- Future agents must begin by reading EXP-007 and this handoff rather than
  repeating the broad physical doodle runs.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
