# EXP-011: Graphical client runtime gate

**Status:** Repeatable gameplay/reconnect evidence; renderer-quality and
physical-input gates remain open

## Objective

Exercise the real Bevy client against the typed development server on the
Linux desktop session, and distinguish window/renderer startup from the full
three-client gameplay acceptance gate.

## Run

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4000
printf '\n' | timeout 8s crates/mmorpg-client/target/debug/mmorpg-client \
  127.0.0.1:4000
```

The client binary was rebuilt through its standalone manifest before the run.
The development server was bound to loopback and the client used the default
typed wire path.

For the opt-in three-window startup smoke, use:

```bash
./scripts/smoke-graphical-three-client.sh
./scripts/smoke-graphical-restart-persistence.sh
```

The script launches three real clients with `--character-id 1`, `2`, and `3`,
then checks that each process loads `Greenfield` and reaches the typed startup
character-selection path. It also checks the server-confirmed role and world
player binding for each client, plus at least three accepted typed peers. While
the clients remain alive, the script restarts the typed server once and checks
that each client re-authenticates, re-selects its preferred character, and
re-enters the world with the same role. It is a bounded startup/reconnect
check, not a substitute for the full encounter, privacy, durable-persistence,
or renderer-quality acceptance gate.

## Evidence

- **Measured local result:** Bevy created the window named
  `MMORPG Client — interactive slice (65v0)`, selected the Vulkan backend, and
  detected an NVIDIA GeForce RTX 5070. The client loaded the `Greenfield`
  starter zone and remained alive until the eight-second bounded observation
  ended.
- **Host:** Linux CachyOS rolling, kernel `7.2.2-1-cachyos`, AMD Ryzen 7
  2700X, 8 cores, 31.2 GiB memory, NVIDIA driver `610.57.04`, Wayland/X
  desktop session.
- **Runtime limitation:** Vulkan validation output reported swapchain image
  layout and acquire-semaphore errors, and the run did not provide evidence of
  three simultaneous graphical clients, role interaction, stranger privacy,
  restart/retry, or the full encounter loop.
- **Implementation update:** The graphical client now accepts an optional
  `--character-id <id>` argument. The worker selects that character only after
  receiving the authoritative character list; without the option, the
  existing Enter-key flow remains unchanged.
- **Backend diagnostic update:** The client now accepts
  `--render-backend auto|vulkan|gl` and reports the requested backend before
  Bevy initialization. This prevents a fallback run from being mistaken for
  an explicit backend result.
- **Measured backend result:** On the documented host, an explicit
  `--render-backend gl` request reported `render_backend_request=gl` and then
  failed during Bevy adapter creation with `Unable to find a GPU`. OpenGL/GLES
  is therefore not a validated fallback on this host; the existing successful
  graphical runs use the automatic Vulkan path and retain its validation
  warnings.
- **Presentation update:** The graphical HUD now displays the authoritative
  player role, health, target health, and private party membership summary in
  addition to inventory, quest, vendor, server notification state, and the
  server-published combat cooldown-ready tick. After an enemy attack, the HUD
  also shows the last authoritative threat target for the selected enemy and
  clears it when that enemy is defeated or respawned.
- **Measured local result:** The three-window smoke passed startup, role
  selection, one typed-server restart, and graphical client reconnect/re-entry
  for all three requested characters. This verifies the bounded worker
  reconnect path while the clients remain alive; it does not verify durable
  world state across the process restart.
- **Aggregate gate:** `scripts/validate-all.sh` invokes the three-role gameplay
  smoke and the three-window reconnect smoke as the repeatable graphical
  gameplay/startup validation boundary.
- **Optimized release smoke:** `scripts/validate-all.sh` also runs
  `scripts/smoke-graphical-release.sh`. It builds the release client, forces
  the Wayland backend, disables the known-broken optional Lossless Scaling
  layer, and requires typed startup with no Vulkan validation or loader
  diagnostics. This is an operational renderer smoke, not the fixed
  frame-time or physical-input acceptance gate.
- **Headless slow-client result:** A real TCP gate sent 12,800 snapshot
  requests from a non-reading client while a second client remained able to
  receive a snapshot. The server recorded bounded output saturation and
  isolated the slow session. This is network/queue evidence, not graphical
  slow-client evidence or a production backpressure measurement.
- **Measured graphical slow-client result:**
  `scripts/smoke-graphical-slow-client.sh` launched a real Bevy acceptance
  client for character 3 alongside the raw non-reading/healthy pair, observed
  the healthy peer remain responsive, saw bounded server saturation, and
  verified that the graphical client remained alive after loading `Greenfield`.
  This is a client-survival smoke, not a renderer frame-time or production
  backpressure measurement.
- **Measured renderer-active result:** The graphical three-role smoke launched
  real Bevy windows with `--acceptance-smoke` and observed authoritative vendor
  purchase, quest acceptance, movement into range, three enemy defeats, three
  loot rewards, and quest turn-in through the normal typed worker and
  presentation adapter. Vulkan validation errors were still present on this
  host; this is gameplay/presentation evidence, not renderer-quality acceptance
  evidence.
- **Measured renderer-active role result:** The graphical three-role smoke
  launched three real Bevy clients concurrently and observed the damage
  character's purchase, quest, combat, loot, and turn-in flow, plus the tank's
  authoritative taunt and the healer's party-invite acceptance and recovery
  event. It also verified that the damage client, which remained outside the
  tank/healer party, received no private party event. The run passed on the
  same host, but it retains the Vulkan validation limitation and uses fixed
  local smoke IDs.
- **Controlled Wayland backend result (2026-09-09):** Running the graphical
  gameplay smoke with `WINIT_UNIX_BACKEND=wayland` reached the same typed
  gameplay result and created a real Wayland-backed window. A direct seven-
  second capture isolated four validation reports: the host's broken
  Lossless Scaling implicit layer produced two loader-chain errors, while the
  remaining reports were application-visible Vulkan swapchain issues
  (`VK_IMAGE_LAYOUT_UNDEFINED` at present and already-signaled acquire
  semaphores). The client requests explicit mailbox presentation; this keeps
  the same four debug reports on this host but does not eliminate the
  warnings. Repeating with
  `VK_LOADER_LAYERS_DISABLE=VK_LAYER_LSFGVK_frame_generation` removed the
  loader-chain errors but retained the swapchain reports. This identifies a
  host-layer contributor without proving that the remaining wgpu/driver
  behavior is safe; the renderer-quality gate therefore remains open.
- **Optimized Wayland capture (2026-09-09):** After building the standalone
  client with `cargo build --release`, a seven-second run with
  `WINIT_UNIX_BACKEND=wayland` and the broken optional Lossless Scaling layer
  disabled created the Vulkan window and selected the NVIDIA RTX 5070 adapter
  with zero `VALIDATION`, `Failed to find`, or `Skipping layer` reports. This
  is evidence that the optimized path is operational on this host, but it is
  not a frame-time distribution and does not establish correctness for the
  debug validation path; the fixed renderer-quality gates remain open.
- **Release frame-time capture (2026-09-09):** The optimized smoke now accepts
  `--frame-time-stats` and samples the real Bevy frame delta after a bounded
  two-second renderer warm-up followed by a five-second observation. The
  sampler is capped at 6,000 records and reports
  p50, p95, p99, and maximum values, so it cannot grow without bound. The
  The warm-up-aware FIFO runs reported `samples=718 p50_ms=6.545 p95_ms=16.742
  p99_ms=17.269 max_ms=18.659` and then `samples=719 p50_ms=6.392
  p95_ms=16.866 p99_ms=17.470 max_ms=18.222`. Maximum time remained within
  the 50 ms limit, but both p95 values missed the strict 16.7 ms limit. These
  are release-path observations, not universal performance guarantees, and
  the latest repeated results mean the frame-time gate is not passing on this
  host. A subsequent mailbox run reported `samples=1596 p50_ms=3.088
  p95_ms=3.883 p99_ms=4.418 max_ms=7.095`, passing both fixed frame-time
  bounds with substantial margin. These are release-path observations, not
  universal performance guarantees; debug-path Vulkan validation warnings and
  the physical-input gate remain unresolved.
- **Present-mode retest (2026-09-09):** The client now requests Bevy's
  `PresentMode::AutoNoVsync`, which prefers a low-latency supported mode while
  retaining platform fallback behavior. A controlled eight-second Wayland
  debug capture with the Lossless Scaling layer disabled still reported the
  same application-visible `VK_IMAGE_LAYOUT_UNDEFINED` present reports and
  already-signaled acquire-semaphore reports. A second capture using explicit
  `PresentMode::Fifo`, and a third using explicit `PresentMode::Immediate`,
  reproduced those reports. The warnings therefore are not resolved by
  present-mode selection; this change improves fallback behavior but does not
  close the debug renderer gate.
- **Upstream correlation:** wgpu issue [#9213](https://github.com/gfx-rs/wgpu/issues/9213)
  reports the same `VK_IMAGE_LAYOUT_UNDEFINED` presentation validation on a
  Bevy/wgpu Vulkan path, while wgpu issue
  [#7957](https://github.com/gfx-rs/wgpu/issues/7957) documents the related
  swapchain-semaphore reuse class. These reports support treating the current
  result as a wgpu/driver integration limitation rather than evidence that the
  project should suppress validation output; they do not independently clear
  this host's renderer gate.
- **Graphical restart-persistence result (2026-09-09):** The new bounded smoke
  ran the real Bevy client through the purchase, quest acceptance, three-kill
  loot, and quest-reward flow, closed the client, allowed the server's
  off-thread checkpoint to commit, restarted the server with the same
  character store, and entered the world again. The second authoritative
  bootstrap reported `gold=28`, `clear_field_progress=3`, and
  `status=Rewarded`. This verifies the graphical presentation path observes
  durable town progress across restart; the wire-level retry and exactly-once
  operation evidence remains covered by the separate restart smoke.
- **Bound enforcement:** `smoke-graphical-release.sh` now parses the measured
  p95 and maximum values and fails aggregate validation if p95 exceeds 16.7 ms
  or maximum reaches 50 ms. A report is no longer treated as passing merely
  because sampling succeeded.
- **Headless companion result:** The typed three-role gate subsequently passed
  with tank player 5, healer player 6, damage player 7, and party 1. It
  verified own-player-only detailed snapshots, member-only party summaries,
  stranger privacy, a town purchase and quest acceptance, tank taunt, an
  authoritative healer recovery of the tank, three quest kills with
  retryable loot, quest turn-in, and two enemy respawn generations. This does
  not substitute for the graphical run.
- **Explicit debug renderer gate (2026-09-12):** The new focused renderer
  smoke requested `--render-backend vulkan` on the documented Wayland host
  with `VK_LAYER_LSFGVK_frame_generation` disabled. Bevy selected an NVIDIA
  GeForce RTX 5070 through Vulkan; the current host reported driver
  `615.71.09`, kernel `7.2.4-3-cachyos`, and the client loaded `Greenfield`.
  The complete unfiltered client log remains available when the smoke fails.
  The run reproduced three `VK_IMAGE_LAYOUT_UNDEFINED` present reports and
  three `vkAcquireNextImageKHR` already-signaled semaphore reports. Its
  bounded debug frame-time report was `samples=165 p50_ms=29.737
  p95_ms=35.810 p99_ms=40.942 max_ms=61.282`, exceeding both fixed limits.
  This is a reproducible failure of the debug renderer gate, not an accepted
  limitation or a passing renderer result.
- **Renderer-smoke tightening (2026-09-12):** The optimized release smoke
  now requests explicit Vulkan instead of automatic backend selection. It
  passed on the same host with `samples=1625 p50_ms=3.035 p95_ms=3.692
  p99_ms=4.126 max_ms=4.920` and no matching renderer or loader diagnostics.
  This confirms a clean optimized startup path but does not clear the debug
  Vulkan result above.
- **Post-change graphical regression (2026-09-12):** The graphical
  three-role smoke passed purchase, quest, tank, healer, combat, loot,
  recovery, and stranger-privacy checks. The graphical restart-persistence
  smoke passed the purchase and rewarded-quest restore check. These are
  presentation/integration regressions; neither test is used as evidence
  that the debug Vulkan renderer is correct.
- **Root-cause correlation (2026-09-13):** The focused failure exactly matches
  [Bevy issue #22733](https://github.com/bevyengine/bevy/issues/22733): its
  reports contain the same `VUID-VkPresentInfoKHR-pImageIndices-01430`
  (`VK_IMAGE_LAYOUT_UNDEFINED`) followed by
  `VUID-vkAcquireNextImageKHR-semaphore-01286` (acquire semaphore already
  signaled). Bevy maintainers closed that issue as fixed by wgpu v30, and
  explicitly stated that it will be part of Bevy 0.20 rather than Bevy 0.19.
  The corresponding wgpu correction is
  [PR #9361](https://github.com/gfx-rs/wgpu/pull/9361), merged 2026-04-14. It
  prepares a surface texture for presentation by clearing an uninitialized
  texture and transitioning it to `PRESENT`, so a frame with no render work
  cannot be presented in `UNDEFINED` layout. The PR adds a reproduction test.
  The exact locked `wgpu-hal 29.0.4` source still uses the predecessor path:
  its Vulkan `discard_texture` is a no-op and its `present` call does not
  perform that clear/transition. This supplies a concrete lower-stack owner
  and mechanism for the two errors: an unrendered acquired image is presented
  in `UNDEFINED`; because no submission waited on its acquisition semaphore,
  reusing that now-signaled binary semaphore violates the Vulkan acquire
  precondition. It is therefore a single presentation-lifetime defect, not
  evidence of project-owned image-index, resize, or frame-submission code.
  The host’s disabled Lossless Scaling layer remains a separate loader-chain
  concern. A fresh run of `./scripts/smoke-graphical-renderer-debug.sh` used
  `VK_LOADER_LAYERS_DISABLE=VK_LAYER_LSFGVK_frame_generation` and
  `WINIT_UNIX_BACKEND=wayland`, selected the RTX 5070 / NVIDIA 615.71.09
  Vulkan adapter, reached `Greenfield`, and again produced three layout VUIDs
  followed by three acquire-semaphore VUIDs. Its five-second post-warm-up
  measurement was `samples=160 p50_ms=31.126 p95_ms=38.015 p99_ms=41.594
  max_ms=57.380`; it failed both debug frame-time limits. No renderer
  diagnostic was suppressed by this investigation.
- **Upgrade feasibility check (2026-09-13):** A direct `cargo update -p wgpu
  --precise 30.0.1` was rejected because Bevy 0.19.1's `bevy_render` requires
  `wgpu ^29.0.3`. Bevy 0.20 is not currently published in the crates.io
  registry or in the upstream release tags; v0.19.1 is the latest released
  Bevy tag. Therefore a clean supported dependency upgrade cannot be made in
  this branch today without moving the entire client to an unreleased Bevy
  git revision (or maintaining a local wgpu backport). Neither a partial
  wgpu override nor a claim of an upgrade fix is valid evidence.

## Interpretation

The client shell, bounded graphical role encounter, three-window restart
reconnect, and graphical slow-peer survival now have repeatable local
evidence. The focused explicit debug renderer gate makes the remaining
failure concrete: the current NVIDIA/Wayland host still reports Vulkan
swapchain layout and acquire-semaphore validation errors and misses the
debug frame-time bounds. The optimized release path passes its bounded
frame-time check, but that is not a substitute for debug renderer evidence.
This is still not a passing Milestone 13 result, and no host limitation has
been owner-approved. The headless harness still provides the stronger
privacy, retry, persistence, and respawn evidence; it does not substitute
for the remaining renderer-quality disposition.
