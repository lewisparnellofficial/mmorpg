# EXP-006: Luau UI scripting sandbox

**Status:** Measured local technology spike plus a provisional language-neutral
host contract; production decision remains provisional

**Date:** 2026-09-09

## Objective

Validate the first narrow implementation of the proposed `ui.v1` addon
boundary with an embedded Luau runtime. The spike must demonstrate bounded UI
capabilities, sanitized visible state, protected-action separation, resource
limits, cross-addon handle isolation, and failure isolation.

## Hypothesis

A per-addon Luau state, exposed through a small host API and bounded by source,
memory, instruction, UI-node, event, and text quotas, can support the default
UI and player addons without exposing OS access, gameplay commands, or another
addon's UI handles.

## Implementation and configuration

The language-neutral portion is implemented in
[`mmorpg-ui-contract`](../../crates/mmorpg-ui-contract/). It has no runtime,
renderer, socket, filesystem, or authoritative-core dependency. Its focused
tests cover bounded event coalescing/FIFO behavior, operation validation,
manifest rejection, and account/package-scoped storage failure atomicity.
The Luau experiment now consumes the contract's immutable `ViewRecord` as its
`VisibleState` compatibility type. Panel mutations are staged as the
contract's `UiOperation` values and validated with package/generation ownership
before atomic host commit. Within the adapter prototype, each callback is a
host-state transaction: if the callback fails, host-owned panel mutations from
that dispatch are rolled back before the addon is disabled and the failure is
recorded. Manifest validation and bounded storage are now adapter entry points;
the adapter now also has a bounded filesystem package repository that reads a
TOML manifest and source entry before VM creation, with path containment and
integrity checks. `StorageWorker` provides bounded queued set/delete operations
for one account/package/schema namespace and atomically replaces a validated
TOML state file on its own thread.

The implementation is in
[`experiments/ui-scripting`](../../experiments/ui-scripting/README.md). It uses
`mlua` 0.12.1 with the vendored Luau runtime. Each `AddonRunner` owns one
Luau state. The exposed functions are limited to `ui.create_panel`,
`ui.set_text`, `ui.set_position`, and `ui.on`; `game` is an empty namespace and
`storage` exposes bounded `get`, `set`, and `delete` operations. Secure input is
a native host method and is not available to script callbacks. The graphical
client now starts one default-UI runner and one ordinary-addon runner, renders
their script-described secure-action labels, and registers their host-owned
action nodes with the native secure-input registry. The default action is
activated only after a fresh native key/pointer dispatch; this is a minimal
renderer integration proof, not a complete scripted-HUD implementation.

Default limits are 64 UI nodes, 16 event registrations, 64 KiB of source,
4 MiB of VM memory, 100,000 interrupt-budget instructions, and 4 KiB of panel
text. Manifest dependency, capability, and asset collections are bounded, as
are their identifying fields; stored records and lists are each capped at 128
entries, with the existing depth and byte limits still applying. The test
policy also exercises lower limits. Node IDs are process-unique so a forged
handle from another addon cannot accidentally refer to a local node.

## Reproduction

Run from the repository root on the measured Linux development environment:

```bash
cargo fmt --manifest-path experiments/ui-scripting/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-scripting/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-scripting/Cargo.toml
./scripts/smoke-ui-adversarial.sh
```

## Measured local results

The standalone test suite completed with **22 passed, 0 failed**. The tests
covered:

- the default UI and an addon using the same public functions;
- immutable/sanitized view-model delivery and absence of gameplay methods;
- cross-addon forged-handle rejection;
- UI-node and event-registration quotas;
- removal of `io`, `os`, `debug`, `package`, loader functions, and gameplay
  methods;
- interruption of an infinite loop while a separate default-UI runner stays
  usable;
- source-size and memory limits;
- callback failure disabling only the failing addon;
- rollback of all host-owned panel mutations from a failed callback;
- contract-queue coalescing before Luau dispatch;
- contract validation rejecting an invalid operation batch atomically;
- rollback of callback registrations made by a failed callback;
- pre-VM manifest and source-integrity rejection;
- manifest collection and field bounds;
- deterministic dependency-first package ordering and cycle rejection;
- account/package-scoped storage sharing and account isolation; and
- rollback of storage writes made by a failed callback;
- storage list-size validation; and
- ordered-event storm isolation from the default UI; and
- a deterministic hostile-source corpus covering loops, recursion, memory
  growth, forbidden libraries, and oversized diagnostics.

The demo process also completed and reported one created panel, one registered
event, zero secure intents, and zero errors after a normal event dispatch.

The storage worker regression now also proves that the eleventh successful
set/delete request in one rolling minute is rejected without changing the
last committed file. The commit budget is shared by sets and deletes and is
enforced on the off-thread persistence owner.

The host also bounds retained native secure-intent diagnostics at 64 entries;
the next host submission is rejected rather than growing the addon state
without limit.

These are local test observations, not production capacity measurements or a
security proof.

### Adversarial and runtime gate

The aggregate gate now runs `scripts/smoke-ui-adversarial.sh`. It repeats a
bounded addon load and callback workload 100 times, exercises the hostile
source corpus and ordered-queue isolation tests, and prints local timing
percentiles. The 2026-09-09 run on CachyOS Linux, kernel
`7.2.2-1-cachyos`, x86_64, AMD Ryzen 7 2700X, reported:

```text
iterations=100
load p50=639026 ns, p95=997021 ns, max=1351470 ns
callback p50=36484 ns, p95=55596 ns, max=67734 ns
```

The callback measurement is the average of ten dispatches within each sample;
the load measurement includes VM construction and source execution. These are
repeatable local observations for quota calibration, not a universal
performance guarantee. The gate does not yet provide coverage-guided fuzzing,
renderer frame-time data, OS/process isolation, or minimum-hardware proof.

## Result

The hypothesis is supported for this narrow host surface. Luau embedding is a
viable first runtime direction, and the host API—not the language alone—keeps
protected actions out of addon code. The prototype also exposed an important
ordering rule: unsafe libraries must be removed before enabling Luau sandbox
mode, because sandbox mode makes the global table read-only.

## Limitations and follow-up

- The spike has no signature verifier or bytecode compatibility policy. Its
  package repository resolves a bounded dependency graph, but the storage
  worker is a local atomic-file proof, not a production database or
  crash-recovery journal.
- It does not provide process-level isolation from a compromised native
  runtime or binding.
- The host integration covers one real Bevy secure-action presentation and
  native dispatch path, but does not yet cover every pointer hit-test,
  reload, overlay, or addon-unload scenario end to end.
- There is no full scripted-HUD renderer integration, coverage-guided fuzzing
  campaign, OS/process isolation, or minimum-hardware calibration. The new
  adversarial gate is deterministic stress/regression coverage rather than a
  claim of exhaustive fuzzing.
- The empty `game` namespace is a placeholder, not the final public API.
- Callback rollback covers host state, staged panel operations, and
  registrations made during callbacks; initial script-load registration still
  uses the adapter's direct setup path.
- The adapter now consumes the contract event queue and operation validator,
  validates source-only manifests before VM creation, and exposes bounded
  account/package/schema storage. It does not yet load packages from a
  repository or provide a production storage backend.
- The Bevy client currently constructs the two integration runners from
  built-in source strings; package repository loading is tested in the
  adapter, but is not yet the client package-discovery path.

Next work should add coverage-guided manifest/value-boundary fuzzing, calibrate
quotas on the minimum supported Linux client, and compare the same
language-neutral contract with a stronger Wasm isolation alternative before
accepting an architecture decision.

## Evidence classification

- **Measured local result:** commands and eight passing tests above.
- **Implementation evidence:** per-addon state, host quotas, native secure
  input boundary, and failure isolation in the standalone crate.
- **Project inference:** Luau is suitable as the first runtime direction.
- **Recommendation:** retain Luau as the lead prototype while keeping Wasm as
  the stronger-isolation comparison.
