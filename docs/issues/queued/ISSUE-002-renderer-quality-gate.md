+++
id = "ISSUE-002"
type = "investigation"
title = "Resolve or independently validate the debug Vulkan renderer gate"
status = "ready"
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

allowed_paths = ["docs/experiments/EXP-011-graphical-client-gate.md", "crates/mmorpg-client/**", "scripts/smoke-graphical-*.sh"]
forbidden_paths = ["crates/mmorpg-core/**", "crates/mmorpg-server/**"]
validation_commands = ["./scripts/smoke-graphical-three-client.sh", "./scripts/smoke-graphical-three-role.sh", "./scripts/smoke-graphical-restart-persistence.sh", "./scripts/validate-all.sh"]
+++

# ISSUE-002 — Resolve or independently validate the debug Vulkan renderer gate

## Objective

Close the documented debug-renderer quality gate by fixing the NVIDIA Vulkan swapchain/semaphore validation errors or reproducing the client on a supported renderer/driver where the fixed safety and frame-time criteria pass.

## Context

- Requirements: `docs/architecture/requirements.md`; `PLAN.md` Milestone 13.
- Architecture/research: `docs/research/client-technology-spike.md`.
- Experiments: `docs/experiments/EXP-011-graphical-client-gate.md`.
- Decisions: graphical acceptance must preserve diagnostics rather than suppress them.
- Related issues: blocks `ISSUE-004`.

## Deliverables

- A root-cause fix or independently reproducible supported-renderer validation.
- Updated EXP-011 with exact backend, driver, diagnostics, frame-time, and gameplay results.
- Focused regression coverage for any client/rendering change.

## Acceptance criteria

- [ ] The debug path has no unresolved application-visible swapchain layout or acquire-semaphore validation errors, or the issue is independently validated as a supported host-layer limitation with owner-approved evidence.
- [ ] The fixed frame-time limits remain satisfied.
- [ ] Three-role and restart-persistence graphical smokes still pass.
- [ ] Diagnostics are recorded, not hidden or filtered.

## Scope

### In scope

- Bevy/wgpu presentation configuration and supported renderer/driver validation.
- EXP-011 evidence.

### Out of scope

- New gameplay features or a production renderer rewrite.
- Claiming production hardware coverage from one desktop.

## Dependencies and parallelism

This investigation is independent of the tablet experiment and can proceed in a separate worktree. It blocks the shared graphical acceptance record.

## Worktree and file ownership

### Shared files requiring integration review

- `scripts/validate-all.sh` if the accepted renderer check changes.

### Read-only context

- `PLAN.md`, `NEXT.md`, EXP-011, and client technology research.

## Validation

### Automated commands

Run the listed graphical smokes and aggregate validation after any fix.

### Manual or hardware checks

Inspect the debug renderer output on the documented NVIDIA/Wayland host and, if necessary, a supported alternate renderer/driver.

### Evidence to record

Update EXP-011 with exact host and diagnostic details.

## Integration contract

The result must clearly distinguish a code fix, an accepted development limitation, and unsupported-host evidence. No renderer diagnostic may be silently discarded.

## Progress log

### 2026-09-12 — coordinator

- Queued from NEXT item 2 and PLAN Milestone 13.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
