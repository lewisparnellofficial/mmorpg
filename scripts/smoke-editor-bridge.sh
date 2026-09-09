#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-editor-bridge.XXXXXX")
cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/tools/mmorpg-editor-core/Cargo.toml"
bridge="$repo_root/tools/mmorpg-editor-core/target/debug/mmorpg-editor-core"
terrain="$work_dir/terrain.mmterrain"
capture="$work_dir/stroke.mmstroke"

output=$(
    printf '%s\n' \
        'event press 16 16 0.75 pen 0 0 0 1' \
        'event move 17 16 0.80 pen 0 0 0 2' \
        'event release 18 16 0.70 pen 0 0 0 3' \
        "save $terrain" \
        "capture $capture" \
        'undo' \
        'redo' \
        "open $terrain" \
        "replay $capture" \
        'state' \
        'quit' | "$bridge" --bridge
)

if ! rg -q 'stroke-finished changed_samples=[1-9][0-9]* undo=true' <<<"$output"; then
    echo "editor bridge did not apply a terrain stroke" >&2
    echo "$output" >&2
    exit 1
fi
if ! rg -q '^saved ' <<<"$output" || ! rg -q '^captured ' <<<"$output"; then
    echo "editor bridge did not save terrain and capture output" >&2
    echo "$output" >&2
    exit 1
fi
if ! rg -q '^undo true$' <<<"$output" || ! rg -q '^redo true$' <<<"$output"; then
    echo "editor bridge did not complete undo/redo" >&2
    echo "$output" >&2
    exit 1
fi
if ! rg -q "^opened $terrain$" <<<"$output" || ! rg -q "^replayed $capture changed_samples=[1-9][0-9]*$" <<<"$output"; then
    echo "editor bridge did not reload an atomic terrain source" >&2
    echo "$output" >&2
    exit 1
fi
if ! rg -q '^state undo=true redo=false ' <<<"$output"; then
    echo "editor bridge did not route captured replay through the editor history" >&2
    echo "$output" >&2
    exit 1
fi
test -s "$terrain"
test -s "$capture"

echo "editor bridge smoke: native lifecycle, terrain mutation, save, capture, replay, reload, undo, and redo passed"
