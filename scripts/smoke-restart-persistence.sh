#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
server_address=${1:-127.0.0.1:4400}
wire_address=${2:-127.0.0.1:4401}
scratch_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-restart-smoke.XXXXXX")
checkpoint_path="$scratch_dir/aria.state"
server_log="$scratch_dir/server.log"
server_pid=

cleanup() {
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$scratch_dir"
}
trap cleanup EXIT

start_server() {
    "$repo_root/target/debug/mmorpg-server" "$server_address" \
        --wire-address "$wire_address" \
        --character-store "$checkpoint_path" >"$server_log" 2>&1 &
    server_pid=$!
    for _ in {1..100}; do
        if nc -z 127.0.0.1 "${wire_address##*:}" 2>/dev/null; then
            return
        fi
        sleep 0.05
    done
    echo "server did not start; log follows:" >&2
    sed -n '1,160p' "$server_log" >&2 || true
    return 1
}

stop_server() {
    kill "$server_pid"
    wait "$server_pid" || true
    server_pid=
}

cd "$repo_root"
cargo build --quiet -p mmorpg-server

start_server
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml -- "$wire_address"
stop_server

start_server
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml -- \
    "$wire_address" --expect-restored
echo "restart persistence smoke: success"
