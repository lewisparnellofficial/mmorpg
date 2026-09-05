#!/usr/bin/env bash
set -euo pipefail

# Deterministic runner used only by test-vt-orchestrate.sh. It emits real patch
# artifacts, models one rejected leaf, checks dependency visibility, and can
# deliberately return a stale patch to exercise integration failure handling.

manifest=""
output_dir=""
source_root=""
while (($# > 0)); do
    case "$1" in
        --manifest) manifest=$2; shift 2 ;;
        --output-dir) output_dir=$2; shift 2 ;;
        --source-root) source_root=$2; shift 2 ;;
        *) echo "unexpected fake runner argument: $1" >&2; exit 2 ;;
    esac
done

if [[ ! -r "${VT_TASK_PLAN_FILE:-}" ]]; then
    jq -n '{ok:false,stage:"plan-handoff",diagnostic:"VT_TASK_PLAN_FILE was not provided"}'
    exit 1
fi
if [[ -z "$source_root" || ! -d "$source_root/.git" ]]; then
    jq -n '{ok:false,stage:"source-root",diagnostic:"--source-root was not a Git repository"}'
    exit 1
fi

id=$(jq -r '.id' "$VT_TASK_PLAN_FILE")
target=$(jq -r '.target_files[0]' "$VT_TASK_PLAN_FILE")
mkdir -p "$output_dir"
artifact="$output_dir/fake-$id.patch"

make_append_patch() {
    local baseline_root=$1
    local target_file=$2
    local appended_line=$3
    local patch_file=$4
    local candidate
    candidate=$(mktemp -d "${TMPDIR:-/tmp}/fake-vt-task.XXXXXX")
    mkdir -p "$candidate/$(dirname -- "$target_file")"
    cp -- "$baseline_root/$target_file" "$candidate/$target_file"
    (
        cd "$candidate"
        git init -q
        git add "$target_file"
        git -c user.name=fake-vt-task -c user.email=fake-vt-task@localhost \
            commit -qm baseline
        printf '%s\n' "$appended_line" >>"$target_file"
        git diff --binary --no-ext-diff >"$patch_file"
    )
    rm -rf -- "$candidate"
}

make_replace_patch() {
    local baseline_root=$1
    local target_file=$2
    local replacement=$3
    local patch_file=$4
    local candidate
    candidate=$(mktemp -d "${TMPDIR:-/tmp}/fake-vt-task.XXXXXX")
    mkdir -p "$candidate/$(dirname -- "$target_file")"
    cp -- "$baseline_root/$target_file" "$candidate/$target_file"
    (
        cd "$candidate"
        git init -q
        git add "$target_file"
        git -c user.name=fake-vt-task -c user.email=fake-vt-task@localhost \
            commit -qm baseline
        printf '%s\n' "$replacement" >"$target_file"
        git diff --binary --no-ext-diff >"$patch_file"
    )
    rm -rf -- "$candidate"
}

case "$id" in
    reject_b)
        sleep 0.05
        printf '%s\n' '{"ok":false,"task":"fake-rejected","stage":"check","run_usage":{"attempts":1,"prompt_tokens":11,"completion_tokens":7,"total_tokens":18,"elapsed_ms":50}}'
        exit 1
        ;;
    dependency_a) make_append_patch "$source_root" "$target" 'dependency-a' "$artifact" ;;
    dependency_b)
        if ! grep -Fqx 'dependency-a' "$source_root/$target"; then
            jq -n '{ok:false,stage:"dependency-visibility",diagnostic:"dependency A was not visible"}'
            exit 1
        fi
        make_append_patch "$source_root" "$target" 'dependency-b' "$artifact"
        ;;
    fresh_a) make_replace_patch "$source_root" "$target" 'fresh-a' "$artifact" ;;
    stale_b)
        stale_root=$(mktemp -d "${TMPDIR:-/tmp}/fake-vt-stale.XXXXXX")
        mkdir -p "$stale_root/$(dirname -- "$target")"
        printf '%s\n' 'base' >"$stale_root/$target"
        make_replace_patch "$stale_root" "$target" 'stale-b' "$artifact"
        rm -rf -- "$stale_root"
        ;;
    *)
        appended_line=$id
        if [[ "$target" == *.rs ]]; then
            appended_line="// fake-$id"
        fi
        make_append_patch "$source_root" "$target" "$appended_line" "$artifact"
        ;;
esac

sleep 0.05
jq -n --arg task "$id" --arg artifact "$artifact" \
    '{ok:true,task:$task,artifact:$artifact,
      run_usage:{attempts:1,prompt_tokens:13,completion_tokens:9,
                 total_tokens:22,elapsed_ms:50}}'
