#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-graphical-three-role.XXXXXX.log")
client_pids=()
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-graphical-three-role.XXXXXX")
server_pid=''

cleanup() {
    for pid in "${client_pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    if [[ "${smoke_status:-1}" -eq 0 ]]; then
        rm -rf "$work_dir" "$server_log"
    else
        echo "three-role graphical logs retained at $work_dir" >&2
        echo "server log retained at $server_log" >&2
    fi
}
trap cleanup EXIT INT TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4800 \
    --wire-address 127.0.0.1:4801 >"$server_log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4801 2>/dev/null; then
        break
    fi
    sleep 0.05
done

for character_id in 1 2 3; do
    "$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
        127.0.0.1:4800 --wire-address 127.0.0.1:4801 \
        --character-id "$character_id" --acceptance-smoke \
        >"$work_dir/client-$character_id.log" 2>&1 &
    client_pids+=("$!")
    sleep 0.5
done
sleep 20

for character_id in 1 2 3; do
    log="$work_dir/client-$character_id.log"
    if ! rg -q "loaded zone 'Greenfield'" "$log"; then
        echo "graphical role client $character_id did not load Greenfield" >&2
        sed -n '1,240p' "$log" >&2
        exit 1
    fi
done

damage_log="$work_dir/client-1.log"
tank_log="$work_dir/client-2.log"
healer_log="$work_dir/client-3.log"
for expected_event in ItemPurchased QuestAccepted QuestRewarded; do
    if ! rg -q "GRAPHICAL_EVENT $expected_event" "$damage_log"; then
        echo "damage graphical client did not observe $expected_event" >&2
        sed -n '1,360p' "$damage_log" >&2
        exit 1
    fi
done
defeated_count=$(rg -c "GRAPHICAL_EVENT EnemyDefeated" "$damage_log" || true)
loot_count=$(rg -c "GRAPHICAL_EVENT LootRewarded" "$damage_log" || true)
if (( defeated_count < 3 || loot_count < 3 )); then
    echo "damage graphical client observed defeated=$defeated_count loot=$loot_count" >&2
    sed -n '1,420p' "$damage_log" >&2
    exit 1
fi
if ! rg -q "GRAPHICAL_EVENT TauntResolved" "$tank_log"; then
    echo "tank graphical client did not observe taunt resolution" >&2
    sed -n '1,260p' "$tank_log" >&2
    exit 1
fi
if ! rg -q "GRAPHICAL_EVENT PartyInviteAccepted" "$healer_log"; then
    echo "healer graphical client did not observe party acceptance" >&2
    sed -n '1,260p' "$healer_log" >&2
    exit 1
fi
if ! rg -q "GRAPHICAL_EVENT HealResolved" "$healer_log"; then
    echo "healer graphical client did not observe authoritative recovery" >&2
    sed -n '1,320p' "$healer_log" >&2
    exit 1
fi
if rg -q "GRAPHICAL_EVENT PartyInvite" "$damage_log"; then
    echo "stranger damage graphical client observed a private party event" >&2
    rg -n "GRAPHICAL_EVENT PartyInvite" "$damage_log" >&2
    exit 1
fi

echo "graphical three-role smoke: purchase, quest, tank, healer, combat, loot, recovery, and stranger privacy passed"
smoke_status=0
