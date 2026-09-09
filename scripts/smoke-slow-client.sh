#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-slow-client.XXXXXX.log")
server_pid=''

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -f "$server_log"
    else
        echo "slow-client server log retained at $server_log" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4500 \
    --wire-address 127.0.0.1:4501 >"$server_log" 2>&1 &
server_pid=$!

for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4501 2>/dev/null; then
        break
    fi
    sleep 0.05
done

gate_output=$(cargo run --quiet --manifest-path \
    "$repo_root/experiments/wire-gameplay-smoke/Cargo.toml" --bin slow-client-gate -- \
    127.0.0.1:4501)
grep -F "slow-client gate: healthy peer remained responsive" <<<"$gate_output" >/dev/null
grep -F "wire_client_output_saturated id=" "$server_log" >/dev/null
printf '%s\n' "$gate_output"
echo "slow-client smoke: bounded eviction passed"
smoke_status=0
