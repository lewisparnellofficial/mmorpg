#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
if ! command -v bwrap >/dev/null 2>&1; then
    echo "ui process isolation smoke: bubblewrap unavailable (capability evidence pending)"
    exit 0
fi

if ! bwrap --die-with-parent --unshare-all \
    --ro-bind /usr /usr \
    --ro-bind /bin /bin \
    --ro-bind /lib /lib \
    --ro-bind /lib64 /lib64 \
    --ro-bind /etc /etc \
    --proc /proc --dev /dev --tmpfs /tmp \
    -- /bin/true >/dev/null 2>&1; then
    echo "ui process isolation smoke: bubblewrap namespaces unavailable (capability evidence pending)"
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
    'test ! -s /proc/net/route && /opt/ui-wasm-comparison >/tmp/ui-wasm-output && printf "render\nshutdown\n" | /opt/ui-wasm-comparison --process-host >/tmp/ui-wasm-process-output && grep -q "^PANEL[[:space:]]1[[:space:]]Town$" /tmp/ui-wasm-process-output && grep -q "^BYE$" /tmp/ui-wasm-process-output && printf "invalid-command\n" >/tmp/ui-wasm-failure-request && if /opt/ui-wasm-comparison --process-host </tmp/ui-wasm-failure-request >/tmp/ui-wasm-failure-output 2>/tmp/ui-wasm-failure-error; then exit 1; fi && printf "render\nshutdown\n" | /opt/ui-wasm-comparison --process-host >/tmp/ui-wasm-restart-output && grep -q "^PANEL[[:space:]]1[[:space:]]Town$" /tmp/ui-wasm-restart-output && grep -q "^BYE$" /tmp/ui-wasm-restart-output && printf isolated >"/tmp/$0"' \
    "$marker_name"

if [[ -e "$host_marker" ]]; then
    echo "ui process isolation smoke: private /tmp leaked a marker to the host" >&2
    exit 1
fi

echo "ui process isolation smoke: Wasmi comparison, failure/restart, and supervised process-host ABI ran with unshared namespaces, private /tmp, and an empty network route"
