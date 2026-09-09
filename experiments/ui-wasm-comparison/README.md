# Wasm runtime comparison spike

This standalone experiment compares the language-neutral `ui.v1` direction
with a small Wasmi-hosted module. It is deliberately not a second UI runtime:
the module has one allowlisted `ui.create_panel` import, rejects non-allowlisted
imports before instantiation, exposes no WASI imports, interrupts execution
with fuel, and enforces a four-page host-side linear-memory limit. The import
uses a bounded `(pointer, length)` guest-memory ABI and converts valid UTF-8
panel text into a `mmorpg-ui-contract::UiOperation`; malformed pointers and
invalid UTF-8 return error codes without panicking the host.

Run it from the repository root:

```bash
cargo fmt --manifest-path experiments/ui-wasm-comparison/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-wasm-comparison/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-wasm-comparison/Cargo.toml
./scripts/smoke-ui-process-isolation.sh
```

The output is a local technology comparison, not proof that Wasm solves the
project's addon ABI, package, renderer, or production process-isolation
requirements. The optional process smoke uses bubblewrap when available to
prove that this comparison can run behind an unshared namespace/private-temp
wrapper; it does not yet integrate that wrapper into the graphical client.
