#!/usr/bin/env bash
set -euo pipefail

# Deterministic regression tests for the answer/reasoning boundary. The mock
# endpoint exercises both llama-server response shapes without spending local
# model inference time.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../../.." && pwd)
vt="$repo_root/scripts/vt"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/vt-response-test.XXXXXX")
trap 'rm -rf -- "$tmp_dir"' EXIT

port_file="$tmp_dir/port"
python3 - "$port_file" <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port_file = sys.argv[1]


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
        prompt = request["messages"][-1]["content"]
        if "inline" in prompt:
            content = "<think>inline internal</think>visible answer"
            reasoning = ""
        else:
            content = "visible answer"
            reasoning = "hidden reasoning"
        usage = {"prompt_tokens": 7, "completion_tokens": 5, "total_tokens": 12}
        if request.get("stream"):
            chunks = [
                {"model": "mock-vt", "choices": [{"delta": {"reasoning_content": reasoning}}]},
                {"model": "mock-vt", "choices": [{"delta": {"content": content}}]},
                {"model": "mock-vt", "choices": [{"delta": {}, "finish_reason": "stop"}], "usage": usage},
            ]
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            for chunk in chunks:
                self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
                self.wfile.flush()
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        else:
            self.send_json({
                "model": "mock-vt",
                "choices": [{
                    "message": {
                        "content": content,
                        "reasoning_content": reasoning,
                    },
                    "finish_reason": "stop",
                }],
                "usage": usage,
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

assert_eq() {
    local expected=$1
    local actual=$2
    local label=$3
    if [[ "$expected" != "$actual" ]]; then
        printf 'FAIL %s: expected %q, got %q\n' "$label" "$expected" "$actual" >&2
        exit 1
    fi
}

nonstream=$(
    "$vt" --endpoint "$endpoint" --model mock-vt --format json \
        --retries 0 'return the answer'
)
assert_eq 'visible answer' "$(jq -r '.answer' <<<"$nonstream")" 'non-stream answer'
assert_eq 'hidden reasoning' "$(jq -r '.reasoning' <<<"$nonstream")" 'non-stream reasoning'
if jq -e '.answer | contains("<think>") or contains("</think>")' <<<"$nonstream" >/dev/null; then
    echo 'FAIL non-stream answer leaked thinking tags' >&2
    exit 1
fi

fallback=$(
    "$vt" --endpoint "$endpoint" --model mock-vt --format json \
        --retries 0 'return the inline answer'
)
assert_eq 'visible answer' "$(jq -r '.answer' <<<"$fallback")" 'inline fallback answer'
assert_eq 'inline internal' "$(jq -r '.reasoning' <<<"$fallback")" 'inline fallback reasoning'

streamed=$(
    "$vt" --endpoint "$endpoint" --model mock-vt --format json \
        --stream --retries 0 'return the answer'
)
assert_eq 'visible answer' "$(jq -r '.answer' <<<"$streamed")" 'stream answer'
assert_eq 'hidden reasoning' "$(jq -r '.reasoning' <<<"$streamed")" 'stream reasoning'
assert_eq '12' "$(jq -r '.usage.total_tokens' <<<"$streamed")" 'stream usage'

streamed_text=$(
    "$vt" --endpoint "$endpoint" --model mock-vt --format text \
        --stream --retries 0 'return the answer'
)
assert_eq 'visible answer' "$streamed_text" 'stream text output'
if [[ "$streamed_text" == *'<think>'* || "$streamed_text" == *'</think>'* ]]; then
    echo 'FAIL stream text output leaked thinking tags' >&2
    exit 1
fi

kill "$server_pid"
wait "$server_pid" 2>/dev/null || true
echo 'PASS: non-stream, inline fallback, and stream reasoning separation'
