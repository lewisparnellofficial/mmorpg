#!/usr/bin/env bash
set -euo pipefail

# Deterministic scheduler test. It verifies independent dispatch, cumulative
# dependency state, artifact integration, conflict rejection, feature checks,
# overlap validation, dependency cycles, and usage aggregation.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../../.." && pwd)
orchestrator="$repo_root/scripts/vt-orchestrate"
bundle="$repo_root/experiments/vibethinker/bundles/orchestrator-test.json"
fake_runner="$script_dir/fake-vt-task.sh"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/vt-orchestrate-test.XXXXXX")
trap 'rm -rf -- "$tmp_dir"' EXIT

if output=$("$orchestrator" --bundle "$bundle" --runner "$fake_runner" \
    --output-dir "$tmp_dir/results"); then
    echo 'FAIL: bundle with a rejected task unexpectedly succeeded' >&2
    exit 1
fi

assert_eq() {
    local expected=$1
    local actual=$2
    local label=$3
    if [[ "$expected" != "$actual" ]]; then
        printf 'FAIL %s: expected %q, got %q\n' "$label" "$expected" "$actual" >&2
        exit 1
    fi
}

assert_eq false "$(jq -r '.ok' <<<"$output")" 'bundle status'
assert_eq accepted "$(jq -r '.tasks.accept_a.status' <<<"$output")" 'accepted task'
assert_eq true "$(jq -r '.tasks.accept_a.result.integration.ok' <<<"$output")" 'accepted task integration'
assert_eq rejected "$(jq -r '.tasks.reject_b.status' <<<"$output")" 'rejected task'
assert_eq skipped "$(jq -r '.tasks.blocked_c.status' <<<"$output")" 'blocked dependent'
assert_eq accepted "$(jq -r '.tasks.independent_d.status' <<<"$output")" 'independent task'
assert_eq reject_b "$(jq -r '.tasks.blocked_c.blocked_by[0]' <<<"$output")" 'blocked dependency id'
assert_eq 3 "$(jq -r '.run_usage.attempts' <<<"$output")" 'usage attempts'
assert_eq 62 "$(jq -r '.run_usage.total_tokens' <<<"$output")" 'usage total'
assert_eq 2 "$(jq -r '.summary.accepted' <<<"$output")" 'accepted count'
assert_eq 1 "$(jq -r '.summary.rejected' <<<"$output")" 'rejected count'
assert_eq 1 "$(jq -r '.summary.skipped' <<<"$output")" 'skipped count'
if ! grep -Fq '+accept_a' "$(jq -r '.artifact' <<<"$output")"; then
    echo 'FAIL: integrated artifact omitted accept_a' >&2
    exit 1
fi

validated=$(
    "$orchestrator" --bundle "$bundle" --runner "$fake_runner" --validate-only
)
assert_eq true "$(jq -r '.ok' <<<"$validated")" 'plan validation status'
assert_eq validate-only "$(jq -r '.mode' <<<"$validated")" 'plan validation mode'
assert_eq 4 "$(jq -r '.tasks | length' <<<"$validated")" 'normalized task count'
assert_eq 'experiments/vibethinker/tests/fixtures/scheduler-a.txt' \
    "$(jq -r '.tasks[0].target_files[0]' <<<"$validated")" \
    'normalized target file'
assert_eq 2 "$(jq -r '.feature_checks | length' <<<"$validated")" \
    'normalized feature checks'

inline_bundle="$repo_root/experiments/vibethinker/bundles/inline-plan-smoke.json"
if inline_output=$(
    "$orchestrator" --bundle "$inline_bundle" --runner "$fake_runner" \
        --output-dir "$tmp_dir/inline-results"
); then
    assert_eq true "$(jq -r '.ok' <<<"$inline_output")" 'inline plan status'
else
    echo 'FAIL: inline plan unexpectedly failed' >&2
    exit 1
fi
inline_manifest=$(jq -r '.tasks["parser-inline"].manifest_file' <<<"$inline_output")
assert_eq parser-inline "$(jq -r '.name' "$inline_manifest")" 'generated manifest name'
assert_eq rust_fragment "$(jq -r '.output_kind' "$inline_manifest")" 'generated manifest output kind'
assert_eq 1 "$(jq -r '.assertions.required_text | length' "$inline_manifest")" \
    'generated machine assertion count'

cumulative_bundle="$repo_root/experiments/vibethinker/bundles/cumulative-integration-test.json"
if cumulative_output=$(
    "$orchestrator" --bundle "$cumulative_bundle" --runner "$fake_runner" \
        --output-dir "$tmp_dir/cumulative-results"
); then
    assert_eq true "$(jq -r '.ok' <<<"$cumulative_output")" 'cumulative plan status'
else
    echo 'FAIL: cumulative integration plan unexpectedly failed' >&2
    exit 1
fi
assert_eq accepted "$(jq -r '.tasks.dependency_a.status' <<<"$cumulative_output")" \
    'cumulative prerequisite status'
assert_eq accepted "$(jq -r '.tasks.dependency_b.status' <<<"$cumulative_output")" \
    'cumulative dependent status'
assert_eq true "$(jq -r '.feature_validation.ok' <<<"$cumulative_output")" \
    'cumulative feature validation'
cumulative_artifact=$(jq -r '.artifact' <<<"$cumulative_output")
for expected_line in dependency-a dependency-b; do
    if ! grep -Fq "+$expected_line" "$cumulative_artifact"; then
        echo "FAIL: cumulative artifact omitted $expected_line" >&2
        exit 1
    fi
done

conflict_bundle="$repo_root/experiments/vibethinker/bundles/integration-conflict-test.json"
if conflict_output=$(
    "$orchestrator" --bundle "$conflict_bundle" --runner "$fake_runner" \
        --output-dir "$tmp_dir/conflict-results"
); then
    echo 'FAIL: stale integration bundle unexpectedly succeeded' >&2
    exit 1
fi
assert_eq accepted "$(jq -r '.tasks.fresh_a.status' <<<"$conflict_output")" \
    'conflict prerequisite status'
assert_eq failed "$(jq -r '.tasks.stale_b.status' <<<"$conflict_output")" \
    'stale artifact status'
assert_eq integration "$(jq -r '.tasks.stale_b.result.stage' <<<"$conflict_output")" \
    'stale artifact failure stage'

feature_failure_bundle="$tmp_dir/feature-failure.json"
jq '.name = "feature-failure-test" | .feature_checks = ["false"]' \
    "$cumulative_bundle" >"$feature_failure_bundle"
if feature_failure_output=$(
    "$orchestrator" --bundle "$feature_failure_bundle" --runner "$fake_runner" \
        --output-dir "$tmp_dir/feature-failure-results"
); then
    echo 'FAIL: failing feature check unexpectedly succeeded' >&2
    exit 1
fi
assert_eq 2 "$(jq -r '.summary.accepted' <<<"$feature_failure_output")" \
    'feature failure integrated leaf count'
assert_eq false "$(jq -r '.feature_validation.ok' <<<"$feature_failure_output")" \
    'feature failure validation status'
assert_eq 1 "$(jq -r '.feature_validation.failed_check' <<<"$feature_failure_output")" \
    'feature failure check index'

overlap_bundle="$tmp_dir/overlap-plan.json"
jq '.name = "overlap-test" | .tasks[3].target_files = .tasks[0].target_files' \
    "$bundle" >"$overlap_bundle"
if "$orchestrator" --bundle "$overlap_bundle" --runner "$fake_runner" \
    --output-dir "$tmp_dir/overlap-results" --validate-only \
    >"$tmp_dir/overlap.stdout" 2>"$tmp_dir/overlap.stderr"; then
    echo 'FAIL: unordered overlapping targets unexpectedly passed validation' >&2
    exit 1
fi

cycle_bundle="$tmp_dir/cycle-plan.json"
jq '.name = "cycle-test" | .tasks[0].depends_on = ["dependency_b"]' \
    "$cumulative_bundle" >"$cycle_bundle"
if "$orchestrator" --bundle "$cycle_bundle" --runner "$fake_runner" \
    --output-dir "$tmp_dir/cycle-results" --validate-only \
    >"$tmp_dir/cycle.stdout" 2>"$tmp_dir/cycle.stderr"; then
    echo 'FAIL: dependency cycle unexpectedly passed validation' >&2
    exit 1
fi

invalid_bundle="$tmp_dir/invalid-plan.json"
jq 'del(.tasks[0].semantic_constraints)' "$bundle" >"$invalid_bundle"
if "$orchestrator" --bundle "$invalid_bundle" --runner "$fake_runner" \
    --output-dir "$tmp_dir/invalid-results" --validate-only \
    >"$tmp_dir/invalid.stdout" 2>"$tmp_dir/invalid.stderr"; then
    echo 'FAIL: invalid plan unexpectedly passed validation' >&2
    exit 1
fi
if [[ -e "$tmp_dir/invalid-results" ]]; then
    echo 'FAIL: invalid plan created an execution directory' >&2
    exit 1
fi

echo 'PASS: scheduling, cumulative integration, conflicts, and feature validation'
