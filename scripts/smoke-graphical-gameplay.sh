#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-gameplay.XXXXXX.log")
client_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-gameplay-client.XXXXXX.log")
server_pid=''
client_pid=''

cleanup() {
    kill "$client_pid" "$server_pid" 2>/dev/null || true
    wait "$client_pid" "$server_pid" 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -f "$server_log" "$client_log"
    else
        echo "graphical gameplay logs retained: server=$server_log client=$client_log" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4700 \
    --wire-address 127.0.0.1:4701 >"$server_log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4701 2>/dev/null; then
        break
    fi
    sleep 0.05
done

"$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4700 --wire-address 127.0.0.1:4701 --character-id 1 \
    --acceptance-smoke >"$client_log" 2>&1 &
client_pid=$!
sleep 32

if ! rg -q "loaded zone 'Greenfield'" "$client_log"; then
    echo "graphical acceptance client did not load Greenfield" >&2
    sed -n '1,240p' "$client_log" >&2
    exit 1
fi
if ! rg -q "SCRIPTED_UI default_node=[0-9]+ addon_node=[0-9]+ action=basic_attack" "$client_log"; then
    echo "graphical client did not initialize the scripted UI host proof" >&2
    sed -n '1,240p' "$client_log" >&2
    exit 1
fi
for expected_event in ItemPurchased QuestAccepted QuestRewarded; do
    if ! rg -q "GRAPHICAL_EVENT $expected_event" "$client_log"; then
        echo "graphical acceptance client did not observe $expected_event" >&2
        sed -n '1,320p' "$client_log" >&2
        exit 1
    fi
done
defeated_count=$(rg -c "GRAPHICAL_EVENT EnemyDefeated" "$client_log" || true)
loot_count=$(rg -c "GRAPHICAL_EVENT LootRewarded" "$client_log" || true)
if (( defeated_count < 3 || loot_count < 3 )); then
    echo "graphical acceptance client observed defeated=$defeated_count loot=$loot_count" >&2
    sed -n '1,360p' "$client_log" >&2
    exit 1
fi

echo "graphical gameplay smoke: purchase, quest, combat, loot, and turn-in passed"
smoke_status=0
