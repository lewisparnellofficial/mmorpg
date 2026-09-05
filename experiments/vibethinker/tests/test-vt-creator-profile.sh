#!/usr/bin/env bash
set -euo pipefail

# Regression test for the opt-in creator-style recipe. The mock endpoint
# captures the request so this verifies prompt shape and sampling parameters
# without spending local model inference time.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../../.." && pwd)
vt="$repo_root/scripts/vt"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/vt-creator-profile.XXXXXX")
trap 'rm -rf -- "$tmp_dir"' EXIT

port_file="$tmp_dir/port"
request_file="$tmp_dir/request.json"
python3 - "$port_file" "$request_file" <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port_file, request_file = sys.argv[1:]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, _format, *_args):
        pass

    def send_json(self, payload):
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/v1/models":
            self.send_json({"data": [{"id": "mock-vt"}]})
        else:
            self.send_error(404)

    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with open(request_file, "w", encoding="utf-8") as output:
            json.dump(request, output)
        self.send_json({
            "model": "mock-vt",
            "choices": [{
                "message": {
                    "content": "visible answer",
                    "reasoning_content": "hidden reasoning",
                },
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
        })


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(port_file, "w", encoding="ascii") as output:
    output.write(str(server.server_address[1]))
server.serve_forever()
PY
server_pid=$!
while [[ ! -s "$port_file" ]]; do
    kill -0 "$server_pid" 2>/dev/null || exit 1
    sleep 0.01
done
port=$(<"$port_file")
endpoint="http://127.0.0.1:$port"

prompt_file="$tmp_dir/prompt.txt"
cat >"$prompt_file" <<'EOF'
### Question:
Implement the requested bounded Rust test.

### Format:
Return only the requested code artifact, without explanation or Markdown fences.

```rust
#[test]
fn example() {}
```

### Answer:
Use the provided format with backticks.
EOF

response=$(
    "$vt" --endpoint "$endpoint" --model mock-vt --profile creator-code \
        --format json --file "$prompt_file"
)

assert_eq() {
    local expected=$1
    local actual=$2
    local label=$3
    if [[ "$expected" != "$actual" ]]; then
        printf 'FAIL %s: expected %q, got %q\n' "$label" "$expected" "$actual" >&2
        exit 1
    fi
}

assert_eq 'visible answer' "$(jq -r '.answer' <<<"$response")" 'answer parsing'
assert_eq 1 "$(jq '.messages | length' "$request_file")" 'single user message'
assert_eq user "$(jq -r '.messages[0].role' "$request_file")" 'message role'
assert_eq 40960 "$(jq -r '.max_tokens' "$request_file")" 'max tokens'
assert_eq 0.6 "$(jq -r '.temperature' "$request_file")" 'temperature'
assert_eq 0.95 "$(jq -r '.top_p' "$request_file")" 'top p'
assert_eq -1 "$(jq -r '.top_k' "$request_file")" 'top k'
assert_eq 0 "$(jq -r '.min_p' "$request_file")" 'neutral min p'
assert_eq 1.0 "$(jq -r '.repeat_penalty' "$request_file")" 'neutral repeat penalty'
assert_eq -1 "$(jq -r '.reasoning_budget_tokens' "$request_file")" 'unlimited reasoning budget'
assert_eq false "$(jq -r '.stream' "$request_file")" 'non-streaming request'

jq -e '
    .messages[0].content as $content |
    ($content | startswith("You are an expert Python programmer.")) and
    ($content | contains("### Question:")) and
    ($content | contains("### Format:")) and
    ($content | contains("### Answer:"))
' "$request_file" >/dev/null || {
    echo 'FAIL creator-code prompt was not assembled as expected' >&2
    jq . "$request_file" >&2
    exit 1
}

"$vt" --endpoint "$endpoint" --model mock-vt --profile creator-code \
    --max-tokens 128 --temperature 0.2 --top-k 40 --retries 0 \
    --format json --file "$prompt_file" >/dev/null
assert_eq 128 "$(jq -r '.max_tokens' "$request_file")" 'explicit max tokens override'
assert_eq 0.2 "$(jq -r '.temperature' "$request_file")" 'explicit temperature override'
assert_eq 40 "$(jq -r '.top_k' "$request_file")" 'explicit top k override'

kill "$server_pid"
wait "$server_pid" 2>/dev/null || true
echo 'PASS: creator-code profile uses one user message and published sampler defaults'
