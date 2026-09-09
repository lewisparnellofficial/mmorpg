#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if ! command -v bwrap >/dev/null 2>&1; then
    echo "ui process isolation smoke: bubblewrap unavailable (capability evidence pending)"
    exit 0
fi

cargo build --quiet --manifest-path "$repo_root/experiments/ui-wasm-comparison/Cargo.toml"
binary="$repo_root/experiments/ui-wasm-comparison/target/debug/ui-wasm-comparison"
marker_name="mmorpg-ui-isolation-$RANDOM-$RANDOM"
host_marker="/tmp/$marker_name"

bwrap \
    --die-with-parent \
    --unshare-all \
    --ro-bind /usr /usr \
    --ro-bind /bin /bin \
    --ro-bind /lib /lib \
    --ro-bind /lib64 /lib64 \
    --ro-bind /etc /etc \
    --proc /proc \
    --dev /dev \
    --tmpfs /tmp \
    --ro-bind "$binary" /opt/ui-wasm-comparison \
    -- /bin/sh -c \
    '/opt/ui-wasm-comparison >/tmp/ui-wasm-output && printf isolated >"/tmp/$0"' \
    "$marker_name"

if [[ -e "$host_marker" ]]; then
    echo "ui process isolation smoke: private /tmp leaked a marker to the host" >&2
    exit 1
fi

echo "ui process isolation smoke: Wasmi comparison ran with unshared namespaces and private /tmp"
