# EXP-006: Luau UI scripting sandbox

**Status:** Measured local technology spike plus a provisional language-neutral
host contract; production decision remains provisional

**Date:** 2026-09-07

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
recorded. Manifest and storage lifecycle integration remain separate work.

The implementation is in
[`experiments/ui-scripting`](../../experiments/ui-scripting/README.md). It uses
`mlua` 0.12.1 with the vendored Luau runtime. Each `AddonRunner` owns one
Luau state. The exposed functions are limited to `ui.create_panel`,
`ui.set_text`, `ui.set_position`, and `ui.on`; `game` and `storage` are empty
namespaces in this spike. Secure input is a native host method and is not
available to script callbacks.

Default limits are 64 UI nodes, 16 event registrations, 64 KiB of source,
4 MiB of VM memory, 100,000 interrupt-budget instructions, and 4 KiB of panel
text. The test policy also exercises lower limits. Node IDs are process-unique
so a forged handle from another addon cannot accidentally refer to a local
node.

## Reproduction

Run from the repository root on the measured Linux development environment:

```bash
cargo fmt --manifest-path experiments/ui-scripting/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-scripting/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-scripting/Cargo.toml
```

## Measured local results

The standalone test suite completed with **12 passed, 0 failed**. The tests
covered:

- the default UI and an addon using the same public functions;
- immutable/sanitized view-model delivery and absence of gameplay methods;
- cross-addon forged-handle rejection;
- UI-node and event-registration quotas;
- removal of `io`, `os`, `debug`, `package`, loader functions, and empty
  gameplay/storage namespaces;
- interruption of an infinite loop while a separate default-UI runner stays
  usable;
- source-size and memory limits; and
- callback failure disabling only the failing addon; and
- rollback of all host-owned panel mutations from a failed callback; and
- contract-queue coalescing before Luau dispatch; and
- contract validation rejecting an invalid operation batch atomically; and
- rollback of callback registrations made by a failed callback.

The demo process also completed and reported one created panel, one registered
event, zero secure intents, and zero errors after a normal event dispatch.

These are local test observations, not production capacity measurements or a
security proof.

## Result

The hypothesis is supported for this narrow host surface. Luau embedding is a
viable first runtime direction, and the host API—not the language alone—keeps
protected actions out of addon code. The prototype also exposed an important
ordering rule: unsafe libraries must be removed before enabling Luau sandbox
mode, because sandbox mode makes the global table read-only.

## Limitations and follow-up

- The spike has no package manifest/signature verifier, persistent saved-data
  store, dependency resolver, or bytecode compatibility policy.
- It does not provide process-level isolation from a compromised native
  runtime or binding.
- Secure-input provenance is represented only by a native method and ownership
  check; it is not connected to a real window/input system.
- There is no renderer integration, event-queue backpressure measurement,
  wall-clock benchmark, fuzzing campaign, or minimum-hardware calibration.
- The empty `game` and `storage` namespaces are placeholders, not the final
  public API.
- Callback rollback covers host state, staged panel operations, and
  registrations made during callbacks; initial script-load registration still
  uses the adapter's direct setup path.
- The adapter now consumes the contract event queue and operation validator, but
  does not yet expose manifest validation or the storage API to Luau.

Next work should add manifest/value-boundary fuzzing, calibrate quotas on the
minimum supported Linux client, and compare the same language-neutral contract
with a stronger Wasm isolation alternative before accepting an architecture
decision.

## Evidence classification

- **Measured local result:** commands and eight passing tests above.
- **Implementation evidence:** per-addon state, host quotas, native secure
  input boundary, and failure isolation in the standalone crate.
- **Project inference:** Luau is suitable as the first runtime direction.
- **Recommendation:** retain Luau as the lead prototype while keeping Wasm as
  the stronger-isolation comparison.
