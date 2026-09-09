#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "ui fuzz smoke: cargo-fuzz is not installed (set MMORPG_RUN_FUZZ=1 only on fuzz-capable hosts)"
    exit 0
fi

(cd "$repo_root/fuzz" && RUSTUP_TOOLCHAIN=nightly cargo fuzz run \
    ui-boundaries -- -runs=1000 -max_len=4096 && \
    RUSTUP_TOOLCHAIN=nightly cargo fuzz run \
    luau-source -- -runs=1000 -max_len=4096)
echo "ui fuzz smoke: libFuzzer boundary run passed"
