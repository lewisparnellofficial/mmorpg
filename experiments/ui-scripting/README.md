# UI scripting sandbox spike

This standalone experiment embeds Luau through `mlua` and exercises the
proposed `ui.v1` boundary. The client consumes the same runner as a narrow
startup/secure-input integration proof; the experiment remains independent of
authoritative gameplay state and is not a full scripted-HUD implementation.

Each `AddonRunner` owns one sandboxed Luau state and exposes only:

- `ui.create_panel(text)`;
- `ui.set_text(node_id, text)`;
- `ui.set_position(node_id, x, y)`; and
- `ui.on(event_name, callback)`.

The host supplies an immutable, sanitized `mmorpg-ui-contract::ViewRecord`
when dispatching an event (exported locally as the compatibility name
`VisibleState`). `game` remains an empty API namespace, and `storage` exposes
only bounded `get`, `set`, and `delete` operations. There is no script function
for movement, targeting, casting, packets, file
access, process execution, sockets, native modules, or secure input. A native
caller may use `AddonRunner::secure_input` after validating ownership; that
operation is deliberately outside the Lua environment.

The runner enforces source-size, memory, instruction, UI-node, event, and text
limits. A callback is applied as one host-state transaction: if it fails, its
panel mutations are rolled back before the addon is disabled and the failure
is recorded. Runtime errors disable only the failing addon and are recorded in
its diagnostics. Contract `UiEvent`s enter a bounded queue before dispatch;
replaceable state coalesces by key, ordered events retain FIFO order, and an
ordered overflow disables only the affected addon. Separate runners cannot
use each other's node handles.

Panel mutations are staged as the contract crate's `UiOperation` values and
validated with the addon package/generation before atomic host commit.
Subscription registration made during a callback is committed with the same
transaction. The standalone contract crate's package manifest is now used by
the adapter's pre-VM load entry point. `PackageRepository` adds bounded TOML
manifest/source loading from a numeric package directory, rejects path escapes
and directory/manifest ID mismatches, and verifies the declared source hash
before VM creation. `StorageWorker` provides bounded off-thread set/delete
operations for one account/package/schema namespace and atomically replaces a
validated TOML state file. It accepts at most ten successful commits per
rolling 60-second window (sets and deletes share the budget); once exhausted,
it returns a bounded error without touching the last committed file. It is a
local persistence proof, not a production database, journal, or multi-process
rate limiter. The
`load_from_manifest` entry point validates the manifest, capability set,
dependency IDs, source entry path, and SHA-256 source integrity before VM
construction. `PackageRepository::resolve_order` validates a complete package
set and produces a deterministic dependency-first order, rejecting cycles
before VM construction. `storage.get`, `storage.set`, and `storage.delete` use the
contract's bounded account/package/schema namespace; callers can share the
store across runner instances to model character changes within one account.
Storage mutations made during a failed callback are rolled back with the
presentation transaction.

This is evidence for a Luau embedding direction, not a security certification.
The remaining production questions include package/signature policy, exact
runtime pinning and upgrade policy, OS/process isolation, and fuzzing the
manifest/value/host-call boundary.

Run from the repository root:

```bash
cargo fmt --manifest-path experiments/ui-scripting/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-scripting/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-scripting/Cargo.toml
```
