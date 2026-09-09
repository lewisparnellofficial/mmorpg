#!/usr/bin/env bash
set -euo pipefail

# Opt-in Linux desktop smoke test. It opens three real Bevy windows, selects
# the three development characters without keyboard input, and checks that
# each client loads the starter zone. It is not part of validate-all.sh because
# it requires a working graphical session.

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_pid=''
client_pids=()
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-graphical-smoke.XXXXXX")

cleanup() {
    for pid in "${client_pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -rf "$work_dir"
    else
        echo "graphical smoke logs retained at $work_dir" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"

"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4400 \
    --wire-address 127.0.0.1:4401 >"$work_dir/server.log" 2>&1 &
server_pid=$!

for attempt in $(seq 1 50); do
    if (echo >/dev/tcp/127.0.0.1/4401) 2>/dev/null; then
        break
    fi
    sleep 0.1
done

for character_id in 1 2 3; do
    "$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
        127.0.0.1:4400 --wire-address 127.0.0.1:4401 \
        --character-id "$character_id" >"$work_dir/client-$character_id.log" 2>&1 &
    client_pids+=("$!")
done

sleep 8

for character_id in 1 2 3; do
    log="$work_dir/client-$character_id.log"
    if ! rg -q "loaded zone 'Greenfield'" "$log"; then
        echo "graphical client $character_id did not load Greenfield" >&2
        sed -n '1,120p' "$log" >&2
        exit 1
    fi
    if ! rg -q "selecting startup character $character_id" "$log"; then
        echo "graphical client $character_id did not select its startup character" >&2
        sed -n '1,120p' "$log" >&2
        exit 1
    fi
done

echo "graphical three-client smoke: startup and role selection passed"
echo "logs: $work_dir"
smoke_status=0
