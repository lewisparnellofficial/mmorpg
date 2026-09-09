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
`VisibleState` compatibility type. UI-node operations remain a deliberately
limited adapter prototype; the full transactional operation application and
manifest/storage lifecycle are still owned only by the contract crate.

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

The standalone test suite completed with **8 passed, 0 failed**. The tests
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
- callback failure disabling only the failing addon.

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
