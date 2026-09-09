#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
build_dir=${MMORPG_EDITOR_QT_BUILD_DIR:-/tmp/mmorpg-editor-qt-build}

cmake -S "$repo_root/tools/mmorpg-editor-qt" -B "$build_dir" \
  -DCMAKE_BUILD_TYPE=Debug
cmake --build "$build_dir" --parallel
exec "$build_dir/mmorpg-editor-qt" "$@"
