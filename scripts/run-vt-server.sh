#!/usr/bin/env bash
set -euo pipefail

# Reproducible local VibeThinker llama-server launcher. The model is
# intentionally thinking-only: the template always opens <think>, and the
# deepseek reasoning format routes it to reasoning_content.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)

llama_server_bin="${VT_LLAMA_SERVER_BIN:-/mnt/stuff/llama.cpp/build/bin/llama-server}"
model_path="${VT_MODEL_PATH:-/mnt/games/models/VibeThinker-3B.Q8_0.gguf}"
template_path="${VT_TEMPLATE_PATH:-$repo_root/experiments/vibethinker/vibethinker-reasoning.jinja}"
host="${VT_HOST:-127.0.0.1}"
port="${VT_PORT:-8081}"
slots="${VT_SLOTS:-1}"
slot_context="${VT_SLOT_CONTEXT_SIZE:-4096}"
if [[ -n "${VT_CONTEXT_SIZE:-}" ]]; then
    context_size="$VT_CONTEXT_SIZE"
else
    context_size=""
fi
if [[ "$slots" != "1" && -z "$context_size" ]]; then
    context_size=$((slots * slot_context))
fi
fit_context="${VT_FIT_CONTEXT:-100000}"
if [[ "$slots" != "1" && -z "${VT_FIT_CONTEXT:-}" ]]; then
    fit_context="$slot_context"
fi

[[ "$slots" =~ ^[1-9][0-9]*$ ]] || {
    echo "run-vt-server: VT_SLOTS must be a positive integer: $slots" >&2
    exit 2
}
[[ "$slot_context" =~ ^[1-9][0-9]*$ ]] || {
    echo "run-vt-server: VT_SLOT_CONTEXT_SIZE must be a positive integer: $slot_context" >&2
    exit 2
}
[[ "$fit_context" =~ ^[1-9][0-9]*$ ]] || {
    echo "run-vt-server: VT_FIT_CONTEXT must be a positive integer: $fit_context" >&2
    exit 2
}
if [[ -n "$context_size" ]]; then
    [[ "$context_size" =~ ^[1-9][0-9]*$ ]] || {
        echo "run-vt-server: VT_CONTEXT_SIZE must be a positive integer: $context_size" >&2
        exit 2
    }
fi

[[ -x "$llama_server_bin" ]] || {
    echo "run-vt-server: llama-server is not executable: $llama_server_bin" >&2
    exit 2
}
[[ -r "$model_path" ]] || {
    echo "run-vt-server: model is not readable: $model_path" >&2
    exit 2
}
[[ -r "$template_path" ]] || {
    echo "run-vt-server: template is not readable: $template_path" >&2
    exit 2
}

server_args=(
    --model "$model_path" \
    --jinja \
    --chat-template-file "$template_path" \
    --reasoning on \
    --reasoning-format deepseek \
    -fa on \
    --parallel "$slots" \
    --fit-ctx "$fit_context" \
    --fit-target 1024 \
    -ub 512 \
    -b 512 \
    -ctxcp 4 \
    -cram 2048 \
    --mlock \
    --temp 0.2 \
    --top-p 0.95 \
    --top-k 40 \
    --min-p 0.05 \
    --host "$host" \
    --port "$port"
)
if [[ -n "$context_size" ]]; then
    server_args+=(--ctx-size "$context_size")
fi

exec "$llama_server_bin" "${server_args[@]}" "$@"
