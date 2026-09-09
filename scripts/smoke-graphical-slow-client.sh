#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-slow-server.XXXXXX.log")
client_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-slow-client.XXXXXX.log")
server_pid=''
client_pid=''

cleanup() {
    kill "$client_pid" "$server_pid" 2>/dev/null || true
    wait "$client_pid" "$server_pid" 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -f "$server_log" "$client_log"
    else
        echo "graphical slow-client logs retained: server=$server_log client=$client_log" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4900 \
    --wire-address 127.0.0.1:4901 >"$server_log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4901 2>/dev/null; then
        break
    fi
    sleep 0.05
done

# Character 3 leaves characters 1 and 2 available to the real-TCP slow-client
# gate. This checks a live renderer/client worker alongside the saturated peer.
"$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4900 --wire-address 127.0.0.1:4901 --character-id 3 \
    --acceptance-smoke >"$client_log" 2>&1 &
client_pid=$!

gate_output=$(cargo run --quiet --manifest-path \
    "$repo_root/experiments/wire-gameplay-smoke/Cargo.toml" --bin slow-client-gate -- \
    127.0.0.1:4901)
grep -F "slow-client gate: healthy peer remained responsive" <<<"$gate_output" >/dev/null
sleep 3

if ! kill -0 "$client_pid" 2>/dev/null; then
    echo "graphical client exited during slow-peer pressure" >&2
    sed -n '1,260p' "$client_log" >&2
    exit 1
fi
if ! rg -q "loaded zone 'Greenfield'" "$client_log"; then
    echo "graphical client did not load Greenfield during slow-peer pressure" >&2
    sed -n '1,260p' "$client_log" >&2
    exit 1
fi
if ! rg -q "wire_client_output_saturated id=" "$server_log"; then
    echo "server did not record bounded slow-peer saturation" >&2
    sed -n '1,180p' "$server_log" >&2
    exit 1
fi

printf '%s\n' "$gate_output"
echo "graphical slow-client smoke: live Bevy client survived bounded slow-peer pressure"
smoke_status=0
