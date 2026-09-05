#!/usr/bin/env bash
set -euo pipefail

# Deterministic regression test for baseline-failure and mutation-kill oracles.
# The mock endpoint returns either a genuine symmetry test or a vacuous test
# that passes normally but survives the asymmetric production mutant.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/../../.." && pwd)
vt_task="$repo_root/scripts/vt-task"
manifest="$repo_root/experiments/vibethinker/tasks/position-distance-symmetry-test.json"
tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/vt-task-oracles.XXXXXX")
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf -- "$tmp_dir"
}
trap cleanup EXIT

port_file="$tmp_dir/port"
request_count_file="$tmp_dir/request-count"
printf '%s\n' 0 >"$request_count_file"
python3 - "$port_file" "$request_count_file" <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port_file, request_count_file = sys.argv[1:]

correct = """#[test]
fn position_distance_squared_is_symmetric() {
    let a = Position::new(1.0, 2.0);
    let b = Position::new(4.0, 6.0);
    let forward = a.distance_squared(b);
    let reverse = b.distance_squared(a);
    assert!((forward - reverse).abs() < f32::EPSILON);
}"""

survives_mutation = """#[test]
fn position_distance_squared_is_symmetric() {
    let forward =
        Position::new(1.0, 2.0).distance_squared(Position::new(4.0, 6.0));
    assert!((forward - 25.0).abs() < f32::EPSILON);
}"""

invalid_return_type = """#[test]
fn position_distance_squared_is_symmetric() -> bool {
    true
}"""


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
        with open(request_count_file, "r+", encoding="ascii") as counter:
            count = int(counter.read()) + 1
            counter.seek(0)
            counter.write(str(count))
            counter.truncate()
        if (
            "RETRY_WITH_DIAGNOSTIC" in prompt
            and "The previous candidate was rejected by the validator." not in prompt
        ):
            answer = invalid_return_type
        elif "SURVIVE_MUTATION" in prompt:
            answer = survives_mutation
        else:
            answer = correct
        self.send_json({
            "model": "mock-vt",
            "choices": [{
                "message": {
                    "content": answer,
                    "reasoning_content": "mock reasoning",
                },
                "finish_reason": "stop",
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 40,
                "total_tokens": 60,
            },
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

accepted=$(
    VT_ENDPOINT="$endpoint" VT_MODEL=mock-vt "$vt_task" \
        --manifest "$manifest" --output-dir "$tmp_dir/accepted"
)
assert_eq true "$(jq -r '.ok' <<<"$accepted")" 'accepted oracle task'
assert_eq 1 "$(jq -r '.baseline_checks | length' <<<"$accepted")" \
    'baseline failure count'
assert_eq true "$(jq -r '.mutations[0].killed' <<<"$accepted")" \
    'mutation killed'

retry_manifest="$tmp_dir/retry.json"
jq '.name = "position-distance-retry-feedback"
    | .task += "\nRETRY_WITH_DIAGNOSTIC"
    | .max_attempts = 2' "$manifest" >"$retry_manifest"
retried=$(
    VT_ENDPOINT="$endpoint" VT_MODEL=mock-vt "$vt_task" \
        --manifest "$retry_manifest" --output-dir "$tmp_dir/retry"
)
assert_eq true "$(jq -r '.ok' <<<"$retried")" 'diagnostic retry task'
assert_eq 2 "$(jq -r '.attempts' <<<"$retried")" 'diagnostic retry attempts'

surviving_manifest="$tmp_dir/surviving.json"
jq '.name = "position-distance-surviving-mutant"
    | .task += "\nSURVIVE_MUTATION"
    | .assertions.required_text = [.assertions.required_text[0]]
    | .max_attempts = 1' "$manifest" >"$surviving_manifest"
if surviving=$(
    VT_ENDPOINT="$endpoint" VT_MODEL=mock-vt "$vt_task" \
        --manifest "$surviving_manifest" --output-dir "$tmp_dir/surviving"
); then
    echo 'FAIL: mutation-surviving test unexpectedly passed' >&2
    exit 1
fi
assert_eq mutation "$(jq -r '.stage' <<<"$surviving")" \
    'surviving mutation failure stage'

invalid_baseline_manifest="$tmp_dir/invalid-baseline.json"
jq '.name = "position-distance-invalid-baseline"
    | .baseline_checks = ["true"]
    | .mutations = []
    | .max_attempts = 1' "$manifest" >"$invalid_baseline_manifest"
if invalid_baseline=$(
    VT_ENDPOINT="$endpoint" VT_MODEL=mock-vt "$vt_task" \
        --manifest "$invalid_baseline_manifest" \
        --output-dir "$tmp_dir/invalid-baseline"
); then
    echo 'FAIL: passing baseline check unexpectedly allowed generation' >&2
    exit 1
fi
assert_eq baseline-check "$(jq -r '.stage' <<<"$invalid_baseline")" \
    'invalid baseline failure stage'
assert_eq 0 "$(jq -r '.run_usage.attempts' <<<"$invalid_baseline")" \
    'invalid baseline VT attempts'

invalid_mutation_manifest="$tmp_dir/invalid-mutation.json"
jq '.name = "position-distance-invalid-mutation"
    | .mutations[0].setup_checks = []' "$manifest" >"$invalid_mutation_manifest"
if VT_ENDPOINT="$endpoint" VT_MODEL=mock-vt "$vt_task" \
    --manifest "$invalid_mutation_manifest" \
    --output-dir "$tmp_dir/invalid-mutation" \
    >"$tmp_dir/invalid-mutation.stdout" 2>"$tmp_dir/invalid-mutation.stderr"; then
    echo 'FAIL: mutation without a setup check unexpectedly passed validation' >&2
    exit 1
fi
if ! grep -Fq 'manifest mutations have an invalid shape' \
    "$tmp_dir/invalid-mutation.stderr"; then
    echo 'FAIL: invalid mutation schema did not report its validation error' >&2
    exit 1
fi
assert_eq 4 "$(<"$request_count_file")" 'mock VT request count'

echo 'PASS: baselines gate generation, mutations reject vacuous tests, and retry feedback repairs output'
