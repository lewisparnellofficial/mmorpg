#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

cargo test --manifest-path "$repo_root/experiments/ui-scripting/Cargo.toml" --quiet
gate_output=$(cargo run --quiet --manifest-path "$repo_root/experiments/ui-scripting/Cargo.toml" -- --adversarial-gate)
case "$gate_output" in
    *"ui adversarial gate: iterations=100"*) ;;
    *)
        echo "adversarial UI gate did not produce its measured summary" >&2
        printf '%s\n' "$gate_output" >&2
        exit 1
        ;;
esac

printf '%s\n' "$gate_output"
printf '%s\n' 'ui adversarial smoke: hostile corpus, queue isolation, and measured callback gate passed'
