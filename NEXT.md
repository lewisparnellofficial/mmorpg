# Next Steps

## 1. Complete the editor’s physical tablet gate

The Qt/Quick3D shell and the device-neutral bridge are implemented. Run the
shell with an actual pressure-sensitive Wayland tablet and record physical
proximity, press, motion, release, cancellation, pressure-based terrain
editing, mouse fallback, stroke replay, save/reload, and stroke-level undo in
EXP-007. Synthetic and headless tests cannot close this gate.

## 2. Close the renderer-quality gate

The release Wayland Vulkan path now passes the bounded frame-time limits and
has no loader or validation diagnostics. The debug path still reports
application-visible swapchain layout/semaphore validation errors on the
documented NVIDIA host. Resolve or independently validate that behavior on a
supported renderer/driver, then update EXP-011; do not hide the diagnostics.

## 3. Decide the addon runtime boundary

The `ui.v1` contract, package model, bounded storage, secure-input boundary,
adversarial gate, extended fuzz corpus, and Wasmi comparison are implemented.
Use that evidence to accept a runtime ADR only after deciding whether Luau’s
native embedding is sufficient or the stronger Wasmi isolation boundary is
required. Keep the decision provisional until the host/process isolation and
minimum-hardware evidence are available.

## 4. Finish the first vertical-slice acceptance record

The authoritative three-role encounter, party privacy, recovery, loot
generations, typed reconnect, persistence, retry, slow-client, and release
graphical smokes pass. Complete the remaining graphical/physical evidence
required by Milestone 13, including durable progress across restart and the
documented renderer-quality limitation.

## 5. Return to post-slice production work

After the open gates are resolved or explicitly accepted as development
limitations, freeze editor/addon expansion and select the next authoritative
server milestone from PLAN.md. Keep production authentication, capacity,
interest-management, and durable-storage claims out of the development slice.
