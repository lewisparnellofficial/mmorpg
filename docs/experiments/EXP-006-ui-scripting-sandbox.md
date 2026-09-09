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
TOML state file on its own thread. It syncs the temporary state file before
replacement and syncs the parent directory after the rename, so a successful
worker result reflects a completed local file and directory-entry write.

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
./scripts/smoke-ui-constrained.sh
./scripts/smoke-ui-process-isolation.sh
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
enforced on the off-thread persistence owner. Intrinsically invalid keys and
values are rejected before queue admission, while namespace-size and file
commit checks remain on the persistence owner.

The host also bounds retained native secure-intent diagnostics at 64 entries;
the next host submission is rejected rather than growing the addon state
without limit.

These are local test observations, not production capacity measurements or a
security proof.

A modeled constrained run also completed the 100-iteration adversarial gate
with one CPU affinity (`taskset -c 0`) and a 1 GiB virtual-memory limit. It
reported `load_p50_ns=577450`, `load_p95_ns=640237`, `load_max_ns=852989`,
`callback_p50_ns=30884`, `callback_p95_ns=33914`, `callback_max_ns=35911`,
`storage_p95_us=209`, and `storage_reload_us=692`. This is evidence that the
current quotas and failure handling complete under a deliberately constrained
process profile; it is a modeled constraint, not a physical minimum-hardware
calibration or a substitute for the required supported-device run.

### Adversarial and runtime gate

The aggregate gate now runs `scripts/smoke-ui-adversarial.sh`. It repeats a
bounded addon load and callback workload 100 times, exercises the hostile
source corpus and ordered-queue isolation tests, and prints local timing
percentiles. The 2026-09-09 run on CachyOS Linux, kernel
`7.2.2-1-cachyos`, x86_64, AMD Ryzen 7 2700X, reported:

```text
iterations=100
load p50=560036 ns, p95=1186908 ns, max=1668757 ns
callback p50=31534 ns, p95=63767 ns, max=82957 ns
```

The current aggregate run also reported storage-worker timing:

```text
storage_p50_us=280 storage_p95_us=417 storage_max_us=417 storage_reload_us=697
```

The same run sampled Linux process RSS around the repeated addon load/drop
cycle and the storage-worker sample:

```text
rss_baseline_kib=7096 rss_peak_kib=8584 rss_after_load_kib=8892
rss_after_storage_kib=9676
```

RSS is an operating-system observation that includes allocator and runtime
behavior; it is evidence of this process run, not proof that every allocation
was returned to the OS.

A repeat 1,000-run campaign on 2026-09-09 completed both fuzz targets without
creating a new crash artifact. The `ui-boundaries` target retained 827 coverage
features from 952 seeded inputs; the `luau-source` target retained 1,414
coverage features and 1,509 feature combinations from 25 seeded inputs. The
campaign generated additional minimized corpus candidates, which were kept
outside the repository after the run rather than silently staging generated
files. The earlier `[0a, 02]` parser artifact was replayed directly and now
exits cleanly through the control-byte rejection regression.

The storage sample performs ten successful bounded commits, respecting the
per-minute quota, then measures reopening the persisted namespace. The
callback measurement is the average of ten dispatches within each sample; the
load measurement includes VM construction and source execution. These are
repeatable local observations for quota calibration, not a universal
performance guarantee. The gate now has an optional coverage-guided fuzz
target; renderer frame-time data, OS/process isolation, and minimum-hardware
proof remain separate gates.

The repository also contains nightly libFuzzer targets at
`fuzz/fuzz_targets/ui_boundaries.rs` and
`fuzz/fuzz_targets/luau_source.rs`. They exercise bounded arbitrary manifests,
UI operation batches, nested storage values, arbitrary UTF-8 Luau source, and
sanitized event dispatch. The first 1,000-run campaign found a native Luau
parser crash for input `[10, 2]`; the adapter now rejects disallowed control
bytes before VM/parser entry and has a deterministic regression test. A
follow-up 1,000-run smoke completed with 265 contract-boundary coverage
features and 1,414 Luau-adapter coverage features, with no crash or sanitizer
finding. The aggregate run used 64 contract seeds and 20 Luau seeds. Run it
with `./scripts/smoke-ui-fuzz.sh`; set `MMORPG_RUN_FUZZ=1` to
include it in aggregate validation on a host with `cargo-fuzz` and nightly
Rust. The normal aggregate remains independent of that optional tool.

The Wasmi comparison also has a bubblewrap process-wrapper smoke. On a host
with `bwrap`, `scripts/smoke-ui-process-isolation.sh` runs the comparison with
unshared namespaces, read-only system binds, an empty network route, and a
private `/tmp`; a marker created inside the wrapper must not appear in the
host's `/tmp`. This is measured wrapper evidence for the stronger isolation
option, not proof of a production addon supervisor or graphical-client
integration.

An extended local `ui-boundaries` campaign then ran for 21 seconds with
`-max_total_time=20 -max_len=4096`. It completed 746,189 executions without
a crash or sanitizer finding, reached 827 coverage features, and left 952
minimized corpus inputs. This is stronger bounded evidence for the contract
and storage boundary, but it is still one host-local campaign rather than an
exhaustive security guarantee.

Because the Luau campaign found a native parser crash, the repository now also
contains `experiments/ui-wasm-comparison`, a standalone Wasmi proof of the
stronger isolation alternative. Its local run links only one explicit UI
import, rejects every other import before instantiation, rejects WASI by
having no WASI imports, interrupts an infinite loop with fuel, and enforces a
four-page host-side linear-memory limit. The comparison now maps a bounded
`(pointer, length)` guest-memory call to a real
`mmorpg-ui-contract::UiOperation::CreatePanel`, validates the operation with
the shared package/generation rules, and tests an out-of-bounds pointer
returning an error without a host panic. A representative run reported:

```text
wasm comparison: allowlisted_import=pass host_memory_limit=pass fuel=out_of_fuel fuel_trap_us=30666 max_memory_pages=4 wasi_imports=none
```

This is stronger ABI and control-boundary evidence, not a production runtime
selection or a complete scripted-UI implementation. The graphical client and
its process supervisor still do not use this adapter.

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
- There is no full scripted-HUD renderer integration, process-level isolation,
  or minimum-hardware calibration. The fuzz campaigns are bounded local
  evidence rather than exhaustive fuzzing; more varied seeds, sanitizers, and
  host configurations remain required before a production runtime decision.
- The empty `game` namespace is a placeholder, not the final public API.
- Callback rollback covers host state, staged panel operations, and
  registrations made during callbacks; initial script-load registration still
  uses the adapter's direct setup path.
- The adapter consumes the contract event queue and operation validator,
  validates source-only manifests before VM creation, and exposes bounded
  account/package/schema storage. `PackageRepository::load_all` provides a
  bounded local package-discovery path, but the storage backend remains a
  development atomic-file proof rather than a production backend.
- The Bevy client supports the bounded repository path through `--addon-root`
  and requires at least the default and ordinary addon packages there. Built-in
  source remains the fallback when no repository is supplied; package
  signatures, remote repositories, and hot reload remain out of scope.

Next work should vary coverage-guided seeds and sanitizer configurations,
calibrate quotas on the minimum supported Linux client, and use the Wasmi
comparison to decide whether the stronger isolation boundary justifies the
added guest ABI/toolchain cost before accepting an architecture decision.

## Evidence classification

- **Measured local result:** commands and 23 passing tests above.
- **Implementation evidence:** per-addon state, host quotas, native secure
  input boundary, and failure isolation in the standalone crate.
- **Project inference:** Luau is suitable as the first runtime direction.
- **Recommendation:** retain Luau as the lead prototype while keeping Wasm as
  the stronger-isolation comparison.
