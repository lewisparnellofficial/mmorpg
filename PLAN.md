# MMORPG Integration Gates and First Vertical Slice Plan

**Status:** Active planning baseline. Architecture choices remain provisional
until their named integration tests, experiment records, and ADR gates pass.

This plan closes the editor, typed-wire, and UI-scripting technology-spike
gaps, proves secure input and addon isolation, and then freezes those programs
while development returns to the authoritative three-role town-and-field
vertical slice.

## Baseline and governing decisions

As of 2026-09-07, `./scripts/validate-all.sh` passes from a clean working tree.
Typed wire already covers loopback development authentication, explicit
character selection, world entry, starter-loop commands/events, reconnect, and
atomic snapshot projection, but it remains opt-in beside an unauthenticated
line-gameplay listener. `mmorpg-editor-core` has a bounded tablet bridge,
pressure-aware height editing, deterministic persistence, and stroke-level
undo/redo, but no native shell or hardware evidence. The Luau spike has eight
passing tests but no language-neutral contract, package model, event queue,
saved data, renderer integration, or genuine secure-input provenance.

Owner decisions incorporated here:

- Use Qt 6 Quick/QML, Quick3D, and a narrow CXX-Qt boundary for the editor
  proof. Qt 6.11.2 with Quick/Quick3D is installed on the current host.
- Require a physical pressure-sensitive pen run on the current Wayland Linux
  session. Automated mouse and replay tests supplement but do not replace it.
- Namespace `storage.v1` by stable addon package ID and authenticated account
  ID, not character ID.
- Initially load readable Luau source only. Local unsigned packages require
  manifest validation and an integrity hash; signatures grant no extra
  capability. Bytecode, native modules, and remote code are deferred.
- Reconnect by creating a new authenticated development session and requiring
  explicit character selection. Discard, rather than replay, in-flight and
  deferred gameplay intents.
- Keep TCP through the first vertical slice. This plan changes the application
  protocol and listener surface, not the transport technology.

Non-negotiable invariants:

1. The server owns movement, combat, targets, rewards, inventory, quests,
   currency, parties, and persistence; clients and addons submit intent only.
2. Qt, Bevy, Luau, sockets, databases, and filesystem APIs do not enter
   `mmorpg-core`; Qt types also do not enter `mmorpg-editor-core`.
3. Socket, persistence, addon, and package I/O never block the simulation
   owner. Addon code never runs on a network thread.
4. UI callbacks emit validated, transactional presentation operations. A
   failed callback commits nothing and cannot affect another addon/default UI.
5. Protocol compatibility support and retained fixtures land before any
   `PROTOCOL_VERSION` bump.
6. No unauthenticated gameplay mutation surface remains after typed cutover.
7. Complete snapshots replace presentation state atomically, remove stale
   entities, and cannot be partially applied.
8. Measurements are recorded separately from estimates; a miss remains a
   failed experiment rather than a redefined success.
9. Each milestone includes focused tests, canonical documentation, aggregate
   validation, and focused Conventional Commits.
10. After Milestone 5, editor/addon expansion stops unless a vertical-slice
    gate exposes a defect in the frozen boundary.

## Technology-spike integration milestones

### Milestone 0 — Extend the aggregate validation boundary

**Outcome:** All new headless contract/session crates and the Qt shell build are
covered by one repeatable command without launching a GUI or requiring a pen.

Work and exit criteria:

- Keep the existing root, standalone-crate, Bevy check, and self-test behavior.
- Add headless tests for the editor bridge, client session, UI contract,
  package, and storage boundaries as they appear.
- Add a Qt configure/compile check that does not launch a window. Keep the real
  tablet run as a separately documented manual experiment.
- `./scripts/validate-all.sh` must fail on any new check failure and remain
  usable without a display server or tablet.
- Keep all Cargo/Qt/build output ignored and unstaged.

Suggested commit: `chore(validation): cover integration-gate crates`.

### Milestone 1 — Prove the real Qt Wayland tablet path

**Outcome:** A minimal Linux editor routes genuine Qt tablet events through
`TabletEventBridge`, updates a Quick3D terrain preview, and preserves replay,
cancellation, save/reload, and stroke-level undo/redo.

Editor-core work:

- Add a device-neutral `Pen | Eraser | Mouse` source and monotonic event
  timestamp. Preserve pressure, tilt, rotation, proximity, viewport position,
  and document position; represent unavailable axes explicitly.
- Keep the existing point bound and fail-closed cancellation. Rejected or
  cancelled strokes create neither mutations nor history entries.
- Add a bounded captured-stroke format whose replay enters through the same
  bridge as live input and deterministically reproduces document samples.
- Keep brush/document/history logic independent from Qt and shell ownership.

Create a standalone `tools/mmorpg-editor-qt` proof with:

- A QML window, Quick3D height preview, and narrow value/command CXX-Qt bridge.
- Native handling for proximity, press, motion, release, cancellation, focus
  loss, and proximity loss over the actual viewport.
- Explicit viewport-to-heightmap conversion and mouse fallback at pressure
  `1.0` with neutral optional axes.
- Suppression of Qt compatibility mouse events after a handled tablet event so
  one physical action cannot produce two strokes.
- Raise/lower/smooth selection, radius/strength controls, input diagnostics,
  capture/replay, undo/redo, open, save, and reload.
- Validated atomic saves that leave the last good destination unchanged on
  parse/serialization failure.
- `scripts/run-editor.sh` launching the GUI, while the current deterministic
  CLI remains available explicitly for headless checks.

Automated exit tests cover tablet/eraser/mouse lifecycles, duplicate-event
suppression, focus/proximity cancellation, invalid coordinates and missing
axes, coordinate transforms under resize/pan/zoom, capture/replay equivalence,
one history entry per completed stroke, no entry for cancellation, undo/redo,
bit-preserving save/reload, and malformed-load atomicity.

Add `docs/experiments/EXP-007-qt-tablet-shell.md`. Before measuring, record
these fixed gates:

- No duplicated or unterminated strokes and no visible bridge-created gaps.
- p95 callback-to-preview latency no greater than one 60 Hz frame and maximum
  below 50 ms on the documented host.
- p95 preview frame time at or below 16.7 ms for the small proof document.
- A captured physical stroke replays to byte-identical saved terrain source.

Record exact commit, distribution/kernel, Wayland compositor, Qt/CXX-Qt,
GPU/driver, tablet/tool, display scale, axis availability, event/stroke counts,
sample spacing, latency/frame-time distributions, and peak history memory.
Exercise proximity, pressure, motion, release, eraser if supported,
proximity/focus cancellation, mouse fallback, replay, save/reload, undo, and
redo.

If all gates pass, accept an editor-shell ADR. If Qt drops/duplicates required
events, misses the fixed responsiveness gates, or leaks Qt ownership into the
domain model, keep the decision proposed and run only a bounded SDL3 input
comparison. Terrain materials, tiles, placement, particles, and docking remain
out of scope.

Suggested commits:

- `feat(editor): add device-neutral captured stroke records`
- `feat(editor): add Qt tablet terrain shell`
- `test(editor): record Wayland tablet integration proof`
- `docs(editor): decide Linux editor shell boundary`

### Milestone 2 — Make typed wire compatible, equivalent, and default

**Outcome:** Typed wire becomes the only gameplay/session path. A testable
session state machine owns reconnect/bootstrap; an old client can decode a
version rejection; well-formed unknown additive events do not kill a session.

Compatibility must precede the bump:

- At protocol v1, add a frozen version-independent control profile carrying
  only `VersionRejected { supported_min, supported_max }` and other explicitly
  enumerated non-gameplay control messages.
- Retain encoded v1 fixtures and a previous-client decoder harness.
- Length-delimit server-message/event variants. Decode to `Known`,
  `SkippedUnknown { opcode, length }`, or a fatal malformed result; a malformed
  length is never treated as safely skippable.
- Canonically serialize the shared starter catalog and compute the same
  versioned content digest on client/server. Exchange it before world entry and
  reject a mismatch before binding the character.
- Keep protocol, snapshot schema, content-digest format, application feature,
  and runtime versions distinct.
- Accept the compatibility ADR only after tests prove the old-client
  rejection, unknown-event skipping, malformed-length rejection, stable digest
  ordering, digest sensitivity to semantic content, and pre-entry mismatch
  rejection. Only then bump gameplay protocol to v2.

Move handshake and reconnect policy out of the Bevy binary into a
renderer-independent client session state machine with explicit states:

```text
Disconnected -> Connecting -> Authenticating -> AwaitingCharacterList
 -> AwaitingCharacterSelection -> EnteringWorld -> AwaitingBootstrap -> Ready
```

`Backoff` and `Closed` handle retry and shutdown. The boundary consumes typed
user/session inputs and decoded server messages, and emits typed commands,
character/session status, a world-reset signal, and presentation-eligible
authoritative messages.

Session rules:

- Authenticate, list, wait for explicit selection, enter, then bootstrap.
- Reject gameplay inputs before `Ready` and bound all input, deferred, encoded,
  decoded, and bootstrap queues by count and bytes.
- On disconnect, clear the presented world and discard deferred/in-flight
  gameplay commands; require a new explicit selection after reconnect.
- Add a snapshot baseline sequence. While bootstrapping, buffer only bounded
  later events, apply the complete snapshot atomically, then apply contiguous
  newer events once. A sequence gap/overflow requests a new bootstrap; older
  or duplicate messages are ignored and counted.
- Keep socket ownership on a dedicated worker; Bevy only polls bounded outputs
  and submits bounded intents.

Before deleting line gameplay, drive one deterministic starter-loop transcript
through line and typed adapters and compare normalized authoritative player,
NPC, economy, quest, vendor, combat, loot, error, snapshot, and stale-entity
results. Ignore session IDs, raw text, and transport diagnostics. Any semantic
mismatch blocks closure.

After equivalence passes:

- Make `127.0.0.1:4000` the default typed address for server and graphical
  client; make `run-server.sh` and `run-client.sh` agree without extra flags.
- Remove the graphical line worker/decoder and server line listener, command
  parser mutation path, global line snapshot, and associated flags.
- Add a typed diagnostic CLI using the same authentication, content check,
  character binding, command bounds, and visibility policy as the client.
- Retain line data only as inert compatibility/equivalence fixtures.
- Move gameplay/restart smoke tests to the default typed address and CLI.

Required integration tests prove default typed startup, mandatory auth and
selection, full reconnect with new selection, stale-world clearing, no command
replay, atomic bootstrap replacement, sequenced post-baseline application,
resync on gaps/overflow, unknown-event skipping, old-version rejection,
content mismatch rejection, loopback-only development auth, absence of every
unauthenticated gameplay listener, diagnostic binding, and the complete
starter-loop/restart smokes.

Update the networking architecture/research, system overview, client/server/
wire/transport/smoke READMEs, `AGENTS.md` commands, `EXP-004`, ADR-005, and the
new protocol ADR. Do not claim production auth, encryption, session resume,
unreliable delivery, interest management, or production backpressure.

Suggested commits:

- `feat(protocol): add compatible rejection and event skipping`
- `feat(protocol): validate canonical content compatibility`
- `refactor(client): centralize typed session lifecycle`
- `test(protocol): prove line and wire equivalence`
- `feat(network): make typed wire the only gameplay listener`
- `docs(network): record typed wire cutover evidence`

### Milestone 3 — Freeze the language-neutral `ui.v1` contract

**Outcome:** Rust-owned versioned values and policies define addon behavior;
Luau becomes one adapter rather than the contract itself.

Add a dependency-light `mmorpg-ui-contract` root-workspace crate with no Bevy,
Qt, `mlua`, socket, filesystem, or authoritative-core dependency. It owns
separate API/manifest/runtime/storage versions; stable package, account,
instance, and generation IDs; immutable view records; event delivery classes;
owner/generation-checked opaque handles; renderer-neutral transactional UI
operations; structured errors/diagnostics; quotas; saved values; and validated
manifest records.

Freeze this narrow `ui.v1` surface:

- Create bounded panel/text descriptors; set documented properties; destroy
  owned nodes.
- Subscribe/unsubscribe documented events and create bounded visual timers.
- Read only immutable callback/view records derived from
  `mmorpg-client-model`.
- Get/set/delete account-scoped saved values.
- Declare presentation for allowlisted secure actions without activating them.

Do not expose reflection, engine objects, arbitrary files/assets/textures,
networking, packets, gameplay commands, synthetic clicks, programmatic focus
activation, or protected action execution. Each callback builds an operation
buffer; validate type, size, ownership, generation, depth, and quotas before
atomic renderer commit. Any failure discards the full buffer.

Initial events:

- `ui.ready`, `ui.scale_changed`, `connection.updated`
- `player.updated`, `target.updated`, `inventory.updated`, `quest.updated`
- `vendor.updated`, `combat.received`, `notification.received`

Coalesce scale/connection/player/target/inventory/quest/vendor events by
logical key. Preserve UI-ready, combat, notification, host-error, unload, and
storage-result events in FIFO order. Start with 128 events and 256 KiB per
addon queue: replace same-key state first, evict lower-priority replaceable
state next, then disable only that addon if an ordered event cannot fit. Never
block the producer, network worker, renderer, or another addon. Record every
drop/coalesce.

Use stable errors for unsupported version, invalid package/value, stale or
foreign handle, denied capability, quota, queue, storage, callback failure,
and disabled state. Expected validation returns structured errors. Callback
exceptions/traps discard operations, record sanitized diagnostics, back off,
and disable only the failing addon; no pointer, token, filesystem path, or
private host detail crosses the boundary.

The versioned TOML manifest declares stable ID/name/version, manifest schema,
required UI/runtime ranges, Luau source entry point, deterministic load order,
dependencies, requested capabilities, saved-data request, approved asset IDs,
and integrity hashes. An optional signature has no initial privilege effect.
Validate before VM creation; reject duplicate IDs, cycles, conflicts, path
traversal/absolute paths, undeclared or oversized input, unknown capabilities,
and excessive requested limits. Dependencies never widen capability. Only
Luau source is executable in v1.

`storage.v1` uses `(account_id, package_id, storage_schema_version)` and bounded
null, boolean, integer, finite number, UTF-8 string, list, and string-keyed
record values. Initial limits are 64 KiB total, 128 keys, 128-byte keys, depth
16, 16 KiB per value, and ten committed writes/minute. Reads use an in-memory
snapshot; writes/deletes go through a bounded off-thread adapter and publish
ordered results. Canonical atomic writes preserve the prior value on failure.
Provide deterministic in-memory tests and a local atomic-file restart proof;
do not couple addon data to gameplay persistence.

Version policy: breaking semantics require a new major namespace; minor
versions add optional behavior; patches change no contract semantics.
Manifest, UI API, Luau runtime, host ABI, and storage schema evolve
independently. Unsupported ranges fail before execution; compatibility
adapters must be explicit and fixture-tested.

Exit tests prove default/addon API equality, absence of Luau types in the
contract, immutable records, foreign/stale handle rejection, callback
transactionality, coalescing/FIFO behavior, nonblocking queue pressure,
pre-VM manifest rejection, canonical storage quotas/atomicity/restart, shared
namespace across characters of one account, isolation across accounts/addons,
and no capability widening across upgrades.

Update the UI architecture/research, experiment README, and `EXP-006`; keep
Luau provisional through Milestone 5.

Suggested commits:

- `feat(ui): define versioned language-neutral host contract`
- `feat(ui): add bounded addon package and saved data`
- `refactor(ui): adapt Luau runner to ui v1`
- `docs(ui): record ui v1 compatibility contract`

### Milestone 4 — Prove native secure-input provenance

**Outcome:** Real Bevy input can activate an allowlisted action while scripts
can present controls but cannot mint, retain, replay, or forward trust.

Add a native client registry owning allowlisted `ActionId`s, addon node and
generation, native binding ID, monotonically increasing physical-event ID,
focus generation, and a same-dispatch, single-use token never represented as a
script value.

Eligible v1 interactions are a fresh non-repeat native key press or fresh
primary-pointer press hit-tested to a secure node while focused. Held polling,
key repeat, timers, callbacks, animations, focus traversal, synthetic UI
events, and replay traces are ineligible. During native dispatch, resolve the
binding, mint against current event/focus/node generations, consume once, and
translate the action to an ordinary bounded typed client intent. The server
still validates the intent.

Integrate the minimum host into Bevy: run one default-UI and one ordinary-addon
instance, render one script-described secure action, and activate basic attack
through actual native input over typed wire. Retain the current native HUD;
this is not the full scripted-HUD rewrite.

Tests prove one intent per physical event; denial of reused, stale-focus,
stale-node, cross-addon, post-reload, forged, callback, timer, replay, overlay,
repeat, and addon-to-addon attempts; safe unload during input; ordinary queue
failure; and final server rejection of an invalid but physically sourced
attack. Record the threat model and result, then accept the secure-input ADR.

Suggested commits:

- `feat(client): add native secure input registry`
- `feat(client): connect scripted action presentation`
- `test(ui): prove protected action provenance`
- `docs(ui): decide secure input boundary`

### Milestone 5 — Harden and decide the addon runtime

**Outcome:** The language-neutral policy survives hostile packages, values,
queues, storage, errors, and reloads under measured Linux load.

Fuzz and stress manifest/dependency parsing, source loading, event/value
conversion, UI operations/properties, storage encoding, handle generations,
recursion, infinite startup/event/timer loops, allocation, text/nodes/depth,
host calls, timers, registrations, repeated errors/logging, unload/reload,
mixed event saturation, storage failures, cross-addon access, and attempted
file/process/native/network/clock/engine/packet access. Any crash, deadlock,
unbounded allocation, host panic, protected intent, cross-addon mutation, or
default-UI/client failure is release-blocking. Every fuzz discovery becomes a
deterministic aggregate regression test.

Measure load/compile and callback p50/p95/p99/max, instruction interruption,
host conversion/validation, event throughput/coalescing, memory across
load/failure/unload/reload, renderer frame time, storage latency, and failure
isolation on the documented Wayland host. Change conservative quotas only from
recorded evidence.

Accept Luau only if `mlua` can remove/bound capabilities without
project-written unsafe code; instruction/memory enforcement terminates hostile
scripts; host/queue/node/timer/storage limits cover non-VM cost; unload releases
all owned resources; failures remain isolated; and version pinning is
reproducible. Build a Wasm comparison only if Luau fails a concrete isolation,
interruption, memory, unload, or binding-safety gate. Record the results in
`EXP-006` or a successor and accept the runtime ADR only after passing.

Suggested commits:

- `test(ui): harden addon host adversarially`
- `perf(ui): calibrate addon runtime budgets`
- `docs(ui): decide initial addon runtime`

## Return to the first playable vertical slice

After Milestone 5, freeze editor and addon feature growth. Continue technology
work only to fix a failed vertical-slice gate.

### Milestone 6 — Unified timed simulation and bounded intake

Use one 20 Hz fixed-tick owner, approximately metre-scale coordinates, player
speed 7 units/s, melee 5, heal 30, aggro 20, leash 45, and town-to-field
distance at least 80 units unless an accepted scheduling/scale ADR replaces
them. Target p99 three-client simulation work no greater than 12.5 ms on
documented hardware; reject excess movement without hidden debt.

Unify immediate/timed paths; advance timers and lifecycle on empty ticks; add
bounded per-session and fair global intake, connection/accept limits, explicit
request correlation, and load-bearing command-before-disconnect/checkpoint
ordering. Exit on tests for empty ticks, timed casts, movement bursts,
fairness, typed capacity rejection, join correlation, checkpoint migration,
and same-tick disconnect rewards.

### Milestone 7 — Multi-character identity and bounded checkpoints

Add stable tank/healer/damage characters; distinguish account, character,
session, runtime entity, request, and operation IDs; add exact-once
active-character fencing; namespace checkpoints by stable ID; write on bounded
intervals/safe logout rather than every tick; and teach additive reader
compatibility before writer changes. Tests cover ownership, duplicate login,
reconnect cleanup races, namespaces/write rates, old fixtures, and preserving
corrupt files while rejecting entry.

### Milestone 8 — Addressed delivery and minimum interest

Add `Global`, `Player`, and `Nearby` audiences, with `Party` only after the real
registry exists. Filter before enqueueing; never widen private fields through
audience union; make snapshots player-specific; split reliable/replaceable
queues; coalesce entity positions by tick; bound memory; and document TCP's
in-flight ordering window. Tests cover private purchases/rejections/snapshots,
nearby visibility, union privacy, and reliable admission after stale state.

### Milestone 9 — Three-role combat and recovery

Define stable tank strike/taunt, healer heal, and damage-strike abilities.
Validate ownership, role, target, friendliness, range, cooldown, resource, and
liveness before atomic mutation. Add incapacitation/release-to-town; keep
combat transient; expose no client-supplied damage/healing/threat. Test enemy
heal rejection, friendly/self healing, wrong-role casts, incapacitated denial,
atomic failures, authoritative timing, recovery, and the wire authority
boundary.

### Milestone 10 — Threat-driven enemy lifecycle

Port the AI experiment into the core: idle/patrol, engaged, returning, corpse,
and respawn; deterministic threat/ties, healing threat, taunt, enemy attacks,
leash, corpse expiry, and cleanup; reward identity by
`(enemy_id, spawn_generation)`. Tests cover empty-tick progression, enemy
damage, threat rules, leash/death cleanup, loot expiry/exactly-once generations,
and deterministic respawn. Supersede the spike only after equivalent tests.

### Milestone 11 — Parties, credit, visibility, and loot

Implement invite/accept/decline, leave/remove/disband, leader transfer, expiry,
disconnect grace, and maximum five. Back `Party` delivery by the real registry;
separate remote party summary from nearby detail; snapshot eligibility at
death; grant credit once; and select loot by deterministic round robin per
generation. Test server-owned membership, stranger privacy, no history
backfill, eligible/ineligible credit, immutable post-death eligibility, and
loot across respawns.

### Milestone 12 — Commit-before-publish durability

Accept the durable-operation ADR; add scoped operation IDs and a bounded
off-thread worker; atomically commit ID/result/revision before live apply or
success publication; fail without mutation on queue/store errors; return prior
results on retry; and shut down by stopping intake, draining or explicitly
failing work, checkpointing, and releasing fences. Tests cover thread
ownership, commit-before-publish, failure atomicity, idempotency, queue/store
failure, crash boundaries, disconnect/safe-logout order, transient reset,
cross-character isolation, and shutdown.

### Milestone 13 — Three-client graphical gate

Expose health, casts, cooldowns, recovery, party, threat target, credit, loot,
and respawn without optimistic authority. Add a deterministic three-role
headless typed harness and record a three-client graphical Wayland run plus a
stranger-privacy client. Exercise encounter/recovery, multiple loot
generations, town purchase, quest turn-in, disconnect/restart/retry, and a slow
client.

The first slice is complete only when three distinct roles authenticate and
enter one shared zone with one session per character; no unauthenticated
gameplay path exists; old-version/content/unknown-event behavior follows the
compatibility contract; combat/party/privacy/credit/loot rules pass; durable
town progress survives restart exactly once; persistence never blocks the
owner; the loop repeats after respawn without stale state; and the aggregate
gate plus documented graphical experiment pass.

## Validation and delivery rules

For each milestone:

1. Inspect `git status --short --branch` and preserve unrelated changes.
2. Add unit tests at the lowest owner, loopback integration tests for protocol
   and session behavior, and manual records only for genuine GUI/hardware
   evidence.
3. Update every affected canonical architecture, research, ADR, experiment,
   README, command, roadmap, and open-question document.
4. Keep proposals provisional until their evidence gates pass.
5. Run root and standalone format checks, `cargo check --workspace`,
   `cargo test --workspace`, focused shell/client/UI/session tests, relevant
   smokes, `./scripts/validate-all.sh`, and `git diff --check`.
6. Ensure generated output, credentials, private captures, and logs are not
   staged. Commit focused changes with Conventional Commits and record commit,
   tests, hardware, limitations, and remaining provisional decisions.

Compilation alone never completes a milestone; its named behavior, evidence,
documentation, and aggregate gate must pass.

## Explicitly deferred

- Full terrain materials/tiles, assets, particles, docking, collaboration, and
  production editor packaging.
- Full scripted-HUD replacement or a broad widget API.
- Privileged signing, bytecode, native addons, arbitrary assets, remote package
  repositories, and remote code.
- Wasm unless Luau fails a named gate.
- Public auth, internet exposure, encryption, production session resume, QUIC,
  or custom UDP.
- Production replication and any 5,000-client/200-player capacity claim before
  the exact representative workloads are predeclared and measured.
- PostgreSQL, full journal/outbox, backups/failover, XP/levels/equipment,
  guilds, auctions, professions, battlegrounds, raids, and broad social work.
- Instances and transparent overworld layering beyond preserving their future
  ownership boundary.
