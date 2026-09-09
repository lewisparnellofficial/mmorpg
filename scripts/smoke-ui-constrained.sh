#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if ! command -v taskset >/dev/null 2>&1; then
    echo "ui constrained smoke: taskset unavailable (modeled evidence pending)"
    exit 0
fi

cargo build --quiet --manifest-path "$repo_root/experiments/ui-scripting/Cargo.toml"
binary="$repo_root/experiments/ui-scripting/target/debug/ui-scripting-spike"
output=$(bash -c 'ulimit -v 1048576; exec taskset -c 0 "$1" --adversarial-gate' -- "$binary")
if [[ "$output" != *"ui adversarial gate: iterations=100"* ]]; then
    echo "ui constrained smoke: adversarial gate did not complete" >&2
    printf '%s\n' "$output" >&2
    exit 1
fi

printf '%s\n' "$output"
echo "ui constrained smoke: one-core/1-GiB modeled profile passed"
