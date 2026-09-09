# UI boundary fuzz target

This target uses nightly Rust's libFuzzer instrumentation to exercise the
public `mmorpg-ui-contract` manifest, operation, and storage validation
boundaries with bounded generated inputs. It is intended to find panics,
sanitizer failures, and unexpected coverage regressions; a successful run is
not a security proof.

The checked-in corpus is deliberately small and is supplemented by the
deterministic adversarial tests in `experiments/ui-scripting`.

Run a bounded local smoke from the repository root:

```bash
./scripts/smoke-ui-fuzz.sh
```

For a longer campaign, use the standard cargo-fuzz options, for example:

```bash
RUSTUP_TOOLCHAIN=nightly cargo fuzz run \
  --manifest-path fuzz/Cargo.toml ui-boundaries -- -max_total_time=300
```

The normal aggregate validator remains usable without cargo-fuzz. Set
`MMORPG_RUN_FUZZ=1` when the nightly tool and libFuzzer are installed to add
the 1,000-run smoke to aggregate validation.
