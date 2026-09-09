#!/usr/bin/env bash
set -euo pipefail

# Aggregate the repository's normal validation boundaries. The root workspace
# owns the engine/server crates; the remaining crates intentionally retain
# their own manifests so the Bevy client and experiments do not become part of
# the default headless build.

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

usage() {
    cat <<'EOF'
Usage:
  scripts/validate-all.sh
  scripts/validate-all.sh --self-test

The normal run formats and tests the root workspace, checks the graphical
client, and formats/tests/runs the standalone crates, tools, and experiments
that make up the current validation boundary.

--self-test runs an isolated failing child check and succeeds only when this
script observes and propagates that failure. It does not touch the repository.
EOF
}

run_step() {
    local label=$1
    shift
    printf '==> %s\n' "$label"
    "$@"
}

run_manifest_checks() {
    local manifest=$1
    local label=$2
    run_step "$label: fmt" cargo fmt --manifest-path "$repo_root/$manifest" -- --check
    run_step "$label: test" cargo test --manifest-path "$repo_root/$manifest"
}

self_test() {
    local child_status
    set +e
    run_step "self-test: deliberate child failure" bash -c 'exit 37'
    child_status=$?
    set -e
    if [[ "$child_status" -eq 0 ]]; then
        echo "self-test failed: deliberate child unexpectedly passed" >&2
        return 1
    fi
    printf 'self-test: observed child status %s and propagated failure\n' "$child_status"
}

case "${1:-}" in
    "") ;;
    --self-test)
        [[ "$#" -eq 1 ]] || { echo "--self-test does not accept extra arguments" >&2; exit 2; }
        self_test
        exit 0
        ;;
    --help|-h)
        usage
        exit 0
        ;;
    *)
        echo "unknown argument '$1'; use --help" >&2
        exit 2
        ;;
esac

cd "$repo_root"

run_step "root workspace: fmt" cargo fmt --all -- --check
run_step "root workspace: check" cargo check --workspace
run_step "root workspace: test" cargo test --workspace

# The language-neutral addon host is part of the root workspace. Keep an
# explicit focused invocation here so the aggregate output names this new
# integration gate and its contract tests remain visible in CI logs.
run_step "UI contract: test" cargo test --manifest-path "$repo_root/crates/mmorpg-ui-contract/Cargo.toml"
run_step "secure input registry: test" cargo test --manifest-path "$repo_root/crates/mmorpg-client-secure-input/Cargo.toml"
run_step "client session: test" cargo test --manifest-path "$repo_root/crates/mmorpg-client-session/Cargo.toml"

# The Bevy client is intentionally checked rather than run: it requires a
# Linux desktop session and graphics stack, while compilation still verifies
# its protocol/model integration in the aggregate gate.
run_step "graphical client: fmt" cargo fmt --manifest-path crates/mmorpg-client/Cargo.toml -- --check
run_step "graphical client: check" cargo check --manifest-path crates/mmorpg-client/Cargo.toml

run_manifest_checks crates/mmorpg-client-protocol/Cargo.toml "client protocol"
run_manifest_checks crates/mmorpg-client-adapter/Cargo.toml "client adapter"
run_manifest_checks crates/mmorpg-client-transport/Cargo.toml "client transport"

run_manifest_checks tools/mmorpg-editor-core/Cargo.toml "editor core"
run_manifest_checks tools/mmorpg-content-check/Cargo.toml "content check"
run_manifest_checks tools/mmorpg-wire-cli/Cargo.toml "typed diagnostic CLI"
run_manifest_checks experiments/ui-scripting/Cargo.toml "UI scripting"

run_manifest_checks experiments/wire-gameplay-smoke/Cargo.toml "wire gameplay smoke"
run_manifest_checks experiments/client-presentation-replay/Cargo.toml "presentation replay"
run_manifest_checks experiments/ai-respawn/Cargo.toml "AI respawn"
run_manifest_checks experiments/layer-manager/Cargo.toml "layer manager"
run_manifest_checks experiments/rust-region-bench/Cargo.toml "region benchmark"

run_step "replication model: compile" python3 -m py_compile experiments/replication-model/model.py
run_step "diff: whitespace" git diff --check

printf '%s\n' 'aggregate validation: PASS'
