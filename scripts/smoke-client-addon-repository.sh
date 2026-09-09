#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
package_root=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-client-addons.XXXXXX")
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-client-addon-server.XXXXXX.log")
client_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-client-addon-client.XXXXXX.log")
server_pid=''

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    rm -rf "$package_root" "$server_log" "$client_log"
}
trap cleanup EXIT INT TERM

mkdir -p "$package_root/1" "$package_root/2"
default_source='local panel = ui.create_panel("Repository default UI")'
addon_source='local panel = ui.create_panel("Repository addon UI")'
printf '%s' "$default_source" >"$package_root/1/main.lua"
printf '%s' "$addon_source" >"$package_root/2/main.lua"
default_hash=$(sha256sum "$package_root/1/main.lua" | awk '{print $1}')
addon_hash=$(sha256sum "$package_root/2/main.lua" | awk '{print $1}')
printf '%s\n' \
    'package_id = 1' \
    'name = "repository-default-ui"' \
    'version = "1.0.0"' \
    'manifest_schema = 1' \
    'api_range = "ui.v1"' \
    'runtime_range = "luau-0.12"' \
    'entry = "main.lua"' \
    'load_order = 0' \
    'integrity_sha256 = '"\"$default_hash\"" >"$package_root/1/manifest.toml"
printf '%s\n' \
    'package_id = 2' \
    'name = "repository-addon-ui"' \
    'version = "1.0.0"' \
    'manifest_schema = 1' \
    'api_range = "ui.v1"' \
    'runtime_range = "luau-0.12"' \
    'entry = "main.lua"' \
    'load_order = 1' \
    'integrity_sha256 = '"\"$addon_hash\"" >"$package_root/2/manifest.toml"

cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
"$repo_root/target/debug/mmorpg-server" 127.0.0.1:4810 \
    --wire-address 127.0.0.1:4811 >"$server_log" 2>&1 &
server_pid=$!
for attempt in $(seq 1 100); do
    if nc -z 127.0.0.1 4811 2>/dev/null; then
        break
    fi
    sleep 0.05
done

set +e
timeout 8s "$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4810 --wire-address 127.0.0.1:4811 \
    --addon-root "$package_root" --character-id 1 >"$client_log" 2>&1
client_status=$?
set -e
if [[ "$client_status" -ne 0 && "$client_status" -ne 124 ]]; then
    echo "client addon repository smoke exited with status $client_status" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q 'SCRIPTED_UI source=repository' "$client_log"; then
    echo "client did not load the addon repository path" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q 'loaded zone '\''Greenfield'\''' "$client_log"; then
    echo "repository-backed client did not reach the typed world" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi

echo "client addon repository smoke: bounded discovery, integrity validation, and typed startup passed"
