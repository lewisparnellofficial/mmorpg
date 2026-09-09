# Wasm runtime comparison spike

This standalone experiment compares the language-neutral `ui.v1` direction
with a small Wasmi-hosted module. It is deliberately not a second UI runtime:
the module has one allowlisted `ui.create_panel` import, no WASI imports, fuel
interruption, and a four-page maximum linear-memory declaration.

Run it from the repository root:

```bash
cargo fmt --manifest-path experiments/ui-wasm-comparison/Cargo.toml -- --check
cargo test --manifest-path experiments/ui-wasm-comparison/Cargo.toml
cargo run --quiet --manifest-path experiments/ui-wasm-comparison/Cargo.toml
```

The output is a local technology comparison, not proof that Wasm solves the
project's addon ABI, package, renderer, or process-isolation requirements.
