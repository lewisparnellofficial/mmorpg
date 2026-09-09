# UI scripting sandbox spike

This standalone experiment embeds Luau through `mlua` and exercises the
proposed `ui.v1` boundary. It is intentionally not connected to the Bevy
client or authoritative server.

Each `AddonRunner` owns one sandboxed Luau state and exposes only:

- `ui.create_panel(text)`;
- `ui.set_text(node_id, text)`;
- `ui.set_position(node_id, x, y)`; and
- `ui.on(event_name, callback)`.

The host supplies an immutable, sanitized `mmorpg-ui-contract::ViewRecord`
when dispatching an event (exported locally as the compatibility name
`VisibleState`). `game` and `storage` are empty API namespaces in this spike, and
there is no script function for movement, targeting, casting, packets, file
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
transaction. The standalone contract crate's package manifest and storage
lifecycle are not yet the implementation behind this Luau adapter.

This is evidence for a Luau embedding direction, not a security certification.
The remaining production questions include package/signature policy, exact
runtime pinning and upgrade policy, OS/process isolation, a real secure-input
provenance implementation, and fuzzing the manifest/value/host-call boundary.

Run from the repository root:

```bash
cargo fmt --manifest-path experiments/ui-scripting/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-scripting/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-scripting/Cargo.toml
```
