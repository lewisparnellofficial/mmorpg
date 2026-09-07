# UI scripting sandbox spike

This standalone experiment embeds Luau through `mlua` and exercises the
proposed `ui.v1` boundary. It is intentionally not connected to the Bevy
client or authoritative server.

Each `AddonRunner` owns one sandboxed Luau state and exposes only:

- `ui.create_panel(text)`;
- `ui.set_text(node_id, text)`;
- `ui.set_position(node_id, x, y)`; and
- `ui.on(event_name, callback)`.

The host supplies an immutable, sanitized visible view model when dispatching
an event. `game` and `storage` are empty API namespaces in this spike, and
there is no script function for movement, targeting, casting, packets, file
access, process execution, sockets, native modules, or secure input. A native
caller may use `AddonRunner::secure_input` after validating ownership; that
operation is deliberately outside the Lua environment.

The runner enforces source-size, memory, instruction, UI-node, event, and text
limits. Runtime errors disable only the failing addon and are recorded in its
diagnostics. Separate runners cannot use each other's node handles.

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
