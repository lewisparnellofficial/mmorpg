#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-release-server.XXXXXX.log")
client_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-release-client.XXXXXX.log")
server_pid=''

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    rm -f "$server_log" "$client_log"
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --release --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4830 \
    --wire-address 127.0.0.1:4831 >"$server_log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4831 2>/dev/null; then
        break
    fi
    sleep 0.05
done

set +e
VK_LOADER_LAYERS_DISABLE=VK_LAYER_LSFGVK_frame_generation \
WINIT_UNIX_BACKEND=wayland timeout 10s \
    "$repo_root/crates/mmorpg-client/target/release/mmorpg-client" \
    127.0.0.1:4830 --wire-address 127.0.0.1:4831 --character-id 1 \
    --render-backend vulkan \
    --frame-time-stats \
    >"$client_log" 2>&1
client_status=$?
set -e
if [[ "$client_status" -ne 0 && "$client_status" -ne 124 ]]; then
    echo "release graphical smoke exited with status $client_status" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q "SCRIPTED_UI source=built-in" "$client_log"; then
    echo "release client did not initialize the scripted UI host" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q "loaded zone 'Greenfield'" "$client_log"; then
    echo "release client did not reach the typed world" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q "frame_time_stats samples=[1-9][0-9]* " "$client_log"; then
    echo "release client did not report frame-time samples" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if rg -q "VALIDATION|Failed to find|Skipping layer" "$client_log"; then
    echo "release graphical smoke reported renderer diagnostics" >&2
    rg -n "VALIDATION|Failed to find|Skipping layer" "$client_log" >&2
    exit 1
fi

frame_time_report=$(rg "frame_time_stats samples=" "$client_log" | tail -n 1)
if [[ -z "$frame_time_report" ]]; then
    echo "release graphical smoke did not produce a frame-time report" >&2
    exit 1
fi
p95_ms=$(printf '%s\n' "$frame_time_report" | sed -n 's/.*p95_ms=\([0-9.][0-9.]*\).*/\1/p')
max_ms=$(printf '%s\n' "$frame_time_report" | sed -n 's/.*max_ms=\([0-9.][0-9.]*\).*/\1/p')
if [[ -z "$p95_ms" || -z "$max_ms" ]]; then
    echo "release graphical smoke could not parse frame-time bounds: $frame_time_report" >&2
    exit 1
fi
if ! awk -v p95="$p95_ms" -v max="$max_ms" \
    'BEGIN { exit !(p95 <= 16.7 && max < 50.0) }'; then
    echo "release graphical smoke exceeded frame-time bounds: $frame_time_report" >&2
    exit 1
fi
echo "release graphical smoke: optimized Wayland Vulkan startup had no validation or loader diagnostics; $frame_time_report"
