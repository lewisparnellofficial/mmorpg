#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-graphical-restart.XXXXXX")
checkpoint_path="$work_dir/aria.state"
server_log="$work_dir/server.log"
first_client_log="$work_dir/client-first.log"
second_client_log="$work_dir/client-second.log"
server_pid=''
client_pid=''

cleanup() {
    kill "$client_pid" "$server_pid" 2>/dev/null || true
    wait "$client_pid" "$server_pid" 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -rf "$work_dir"
    else
        echo "graphical restart logs retained: $work_dir" >&2
    fi
}
trap cleanup EXIT INT TERM

start_server() {
    "$repo_root/target/debug/mmorpg-server" 127.0.0.1:4720 \
        --wire-address 127.0.0.1:4721 \
        --character-store "$checkpoint_path" >>"$server_log" 2>&1 &
    server_pid=$!
    for attempt in $(seq 1 100); do
        if nc -z 127.0.0.1 4721 2>/dev/null; then
            return
        fi
        sleep 0.05
    done
    echo "graphical persistence server did not become ready" >&2
    sed -n '1,180p' "$server_log" >&2
    exit 1
}

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"

start_server
"$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4720 --wire-address 127.0.0.1:4721 --character-id 1 \
    --acceptance-smoke >"$first_client_log" 2>&1 &
client_pid=$!
sleep 32

for expected_event in ItemPurchased QuestAccepted QuestRewarded; do
    if ! rg -q "GRAPHICAL_EVENT $expected_event" "$first_client_log"; then
        echo "graphical persistence client did not observe $expected_event" >&2
        sed -n '1,360p' "$first_client_log" >&2
        exit 1
    fi
done

# Closing the graphical client produces the same orderly TCP EOF used by the
# wire persistence smoke. Leave the server running long enough to apply the
# disconnect and checkpoint the final authoritative state.
kill "$client_pid"
wait "$client_pid" 2>/dev/null || true
client_pid=''
sleep 2
kill "$server_pid"
wait "$server_pid" 2>/dev/null || true
server_pid=''

start_server
"$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4720 --wire-address 127.0.0.1:4721 --character-id 1 \
    >"$second_client_log" 2>&1 &
client_pid=$!
sleep 8

if ! rg -q \
    "GRAPHICAL_BOOTSTRAP player=[0-9]+ gold=28 clear_field_progress=3 status=Rewarded" \
    "$second_client_log"; then
    echo "graphical restart did not restore the rewarded town state" >&2
    sed -n '1,240p' "$second_client_log" >&2
    sed -n '1,240p' "$server_log" >&2
    exit 1
fi

echo "graphical restart persistence smoke: purchase and rewarded quest survived restart"
smoke_status=0
