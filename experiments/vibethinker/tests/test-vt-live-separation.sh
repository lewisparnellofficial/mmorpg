#!/usr/bin/env bash
set -euo pipefail

# Live integration smoke test. Unlike test-vt-response-separation.sh, this
# intentionally requires the configured llama-server and VibeThinker GGUF.

endpoint="${VT_ENDPOINT:-http://127.0.0.1:8081}"
model="${VT_MODEL:-}"
timeout="${VT_TIMEOUT:-30}"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/vt-live-test.XXXXXX")
trap 'rm -rf -- "$tmp_dir"' EXIT

command -v curl >/dev/null 2>&1 || { echo 'FAIL: curl is required' >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo 'FAIL: jq is required' >&2; exit 2; }

if [[ -z "$model" ]]; then
    model=$(curl -fsS --max-time "$timeout" "$endpoint/v1/models" |
        jq -r '(.data // .models)[0].id // (.data // .models)[0].name // empty')
fi
[[ -n "$model" ]] || { echo "FAIL: no model exposed by $endpoint" >&2; exit 2; }

request=$(jq -n \
    --arg model "$model" \
    '{model:$model,
      messages:[
        {role:"system",content:"You are a careful concise assistant."},
        {role:"user",content:"Return exactly READY."}
      ],
      max_tokens:256,
      temperature:0.2,
      top_p:0.95,
      top_k:40,
      min_p:0.05,
      top_n_sigma:-1,
      repeat_penalty:1.05,
      reasoning_format:"deepseek",
      reasoning_budget_tokens:64,
      reasoning_budget_message:"Stop reasoning now and return the final answer.",
      stream:false}')

nonstream=$(curl -fsS --max-time "$timeout" "$endpoint/v1/chat/completions" \
    -H 'Content-Type: application/json' -d "$request")
jq -e '
    (.choices[0].message.reasoning_content // "") | length > 0
' <<<"$nonstream" >/dev/null || {
    echo 'FAIL: non-stream response did not expose reasoning_content' >&2
    jq . <<<"$nonstream" >&2
    exit 1
}
jq -e '
    (.choices[0].message.content // "") as $content |
    (($content | contains("<think>")) or ($content | contains("</think>"))) | not
' <<<"$nonstream" >/dev/null || {
    echo 'FAIL: non-stream content exposed thinking tags' >&2
    jq . <<<"$nonstream" >&2
    exit 1
}

stream_request=$(jq '.stream = true | .stream_options = {include_usage:true}' <<<"$request")
stream_file="$tmp_dir/stream.sse"
curl -fsSN --max-time "$timeout" "$endpoint/v1/chat/completions" \
    -H 'Content-Type: application/json' -d "$stream_request" >"$stream_file"
streamed=$(awk '
    /^data: / {
        sub(/^data: /, "")
        if ($0 != "[DONE]") print
    }
' "$stream_file" | jq -s '
    (map(.choices[0].delta.reasoning_content // "") | join("")) as $reasoning |
    (map(.choices[0].delta.content // "") | join("")) as $content |
    {reasoning:$reasoning,content:$content}
')
jq -e '.reasoning | length > 0' <<<"$streamed" >/dev/null || {
    echo 'FAIL: stream did not expose reasoning deltas' >&2
    jq . <<<"$streamed" >&2
    exit 1
}
jq -e '
    .content as $content |
    (($content | contains("<think>")) or ($content | contains("</think>"))) | not
' <<<"$streamed" >/dev/null || {
    echo 'FAIL: stream content deltas exposed thinking tags' >&2
    jq . <<<"$streamed" >&2
    exit 1
}

jq -n \
    --argjson nonstream "$nonstream" \
    --argjson streamed "$streamed" \
    '{ok:true,
      nonstream_reasoning_chars:($nonstream.choices[0].message.reasoning_content|length),
      nonstream_content:$nonstream.choices[0].message.content,
      stream_reasoning_chars:($streamed.reasoning|length),
      stream_content:$streamed.content}'
