#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-graphical-renderer-debug.XXXXXX")
server_pid=''
client_status=0

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -rf "$work_dir"
    else
        echo "debug renderer logs retained at $work_dir" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"

"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4840 \
    --wire-address 127.0.0.1:4841 >"$work_dir/server.log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4841 2>/dev/null; then
        break
    fi
    sleep 0.05
done

set +e
VK_LOADER_LAYERS_DISABLE=VK_LAYER_LSFGVK_frame_generation \
WINIT_UNIX_BACKEND=wayland timeout 10s \
    "$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4840 --wire-address 127.0.0.1:4841 --character-id 1 \
    --render-backend vulkan --frame-time-stats \
    >"$work_dir/client.log" 2>&1
client_status=$?
set -e

if [[ "$client_status" -ne 0 && "$client_status" -ne 124 ]]; then
    echo "debug renderer smoke exited with status $client_status" >&2
    sed -n '1,260p' "$work_dir/client.log" >&2
    exit 1
fi
if ! rg -q '^render_backend_request=vulkan$' "$work_dir/client.log"; then
    echo "debug renderer smoke did not request explicit Vulkan" >&2
    sed -n '1,220p' "$work_dir/client.log" >&2
    exit 1
fi
if ! rg -q "loaded zone 'Greenfield'" "$work_dir/client.log"; then
    echo "debug renderer smoke did not reach the typed world" >&2
    sed -n '1,260p' "$work_dir/client.log" >&2
    exit 1
fi

diagnostic_pattern='VALIDATION|Failed to find|Skipping layer|VK_IMAGE_LAYOUT_UNDEFINED|already-signaled|already signaled'
if rg -n "$diagnostic_pattern" "$work_dir/client.log"; then
    echo "debug renderer smoke reported renderer diagnostics" >&2
    exit 1
fi

frame_time_report=$(rg "frame_time_stats samples=" "$work_dir/client.log" | tail -n 1 || true)
if [[ -z "$frame_time_report" ]]; then
    echo "debug renderer smoke did not produce a frame-time report" >&2
    sed -n '1,260p' "$work_dir/client.log" >&2
    exit 1
fi
p95_ms=$(printf '%s\n' "$frame_time_report" | sed -n 's/.*p95_ms=\([0-9.][0-9.]*\).*/\1/p')
max_ms=$(printf '%s\n' "$frame_time_report" | sed -n 's/.*max_ms=\([0-9.][0-9.]*\).*/\1/p')
if [[ -z "$p95_ms" || -z "$max_ms" ]]; then
    echo "debug renderer smoke could not parse frame-time bounds: $frame_time_report" >&2
    exit 1
fi
if ! awk -v p95="$p95_ms" -v max="$max_ms" \
    'BEGIN { exit !(p95 <= 16.7 && max < 50.0) }'; then
    echo "debug renderer smoke exceeded frame-time bounds: $frame_time_report" >&2
    exit 1
fi

echo "debug renderer smoke: explicit Wayland Vulkan startup had no renderer diagnostics; $frame_time_report"
smoke_status=0
