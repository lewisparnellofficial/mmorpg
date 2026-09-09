#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
server_address=${1:-127.0.0.1:4500}
wire_address=${2:-127.0.0.1:4501}
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-typed-diagnostic.XXXXXX.log")
server_pid=

cleanup() {
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -f "$server_log"
}
trap cleanup EXIT

cd "$repo_root"
cargo build --quiet -p mmorpg-server
"$repo_root/target/debug/mmorpg-server" "$server_address" --wire-address "$wire_address" >"$server_log" 2>&1 &
server_pid=$!
for _ in {1..100}; do
    if nc -z 127.0.0.1 "${server_address##*:}" 2>/dev/null; then
        break
    fi
    sleep 0.05
done

diagnostic_output=$(cargo run --quiet --manifest-path tools/mmorpg-wire-cli/Cargo.toml -- "$server_address")
grep -F "authenticated account=1" <<<"$diagnostic_output" >/dev/null
grep -F "character id=1" <<<"$diagnostic_output" >/dev/null
grep -F "ready player=" <<<"$diagnostic_output" >/dev/null
printf '%s\n' "$diagnostic_output"
echo "typed diagnostic smoke: success"

three_client_output=$(cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml \
    --bin three-client-gate -- "$wire_address")
grep -F "three-client gate: success" <<<"$three_client_output" >/dev/null
printf '%s\n' "$three_client_output"
