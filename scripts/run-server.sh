#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

exec cargo run --manifest-path "$repo_root/Cargo.toml" -p mmorpg-server -- "$@"
