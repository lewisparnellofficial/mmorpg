#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
package_root=$(mktemp -d "${TMPDIR:-/tmp}/mmorpg-wasm-client-package.XXXXXX")
server_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-addon-process-server.XXXXXX.log")
client_log=$(mktemp "${TMPDIR:-/tmp}/mmorpg-addon-process-client.XXXXXX.log")
server_pid=''

cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$package_root"
    rm -f "$server_log" "$client_log"
}
trap cleanup EXIT INT TERM

printf '%s' '(module (import "ui" "create_panel" (func $create_panel (param i32 i32) (result i32))) (memory (export "memory") 1 4) (data (i32.const 16) "Packaged") (func (export "run") i32.const 16 i32.const 8 call $create_panel drop))' >"$package_root/panel.wat"
package_hash=$(sha256sum "$package_root/panel.wat" | awk '{print $1}')
printf 'package_id = 42\nentry = "panel.wat"\nintegrity_sha256 = "%s"\n' "$package_hash" >"$package_root/manifest.toml"

cargo build --quiet --manifest-path "$repo_root/experiments/ui-wasm-comparison/Cargo.toml"
cargo build --quiet --manifest-path "$repo_root/crates/mmorpg-client/Cargo.toml"
cargo build --quiet --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server

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
timeout 8s "$repo_root/crates/mmorpg-client/target/debug/mmorpg-client" \
    127.0.0.1:4830 --wire-address 127.0.0.1:4831 \
    --addon-process-host "$repo_root/experiments/ui-wasm-comparison/target/debug/ui-wasm-comparison" \
    --addon-process-package-root "$package_root" \
    --character-id 1 >"$client_log" 2>&1
client_status=$?
set -e
if [[ "$client_status" -ne 0 && "$client_status" -ne 124 ]]; then
    echo "client addon process smoke exited with status $client_status" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q 'SCRIPTED_UI source=wasmi-process.*label=Packaged' "$client_log"; then
    echo "client did not report the supervised Wasmi process host" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi
if ! rg -q "loaded zone 'Greenfield'" "$client_log"; then
    echo "process-host client did not reach the typed world" >&2
    sed -n '1,220p' "$client_log" >&2
    exit 1
fi

echo "client addon process smoke: supervised Wasmi lifecycle and typed startup passed"
