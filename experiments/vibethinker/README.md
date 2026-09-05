# VibeThinker Experiments

This directory contains prompts and captured outputs for the locally served
VibeThinker model. The transport client remains at
[`scripts/vt`](../../scripts/vt); this directory contains the experiment
inputs and records rather than repository-editing automation.

## Directory layout

```text
experiments/vibethinker/
├── README.md
├── vibethinker-reasoning.jinja
├── prompts/
│   ├── core/
│   ├── server/
│   └── benchmark/
├── tasks/
│   ├── position-distance-suite.json
│   ├── position-distance-test.json
│   ├── position-distance-same-point-test.json
│   ├── position-distance-symmetry-test.json
│   ├── parser-empty-command-smoke.json
│   └── parser-empty-command-test.json
├── mutations/
│   ├── position-distance-asymmetric.patch
│   ├── position-distance-manhattan.patch
│   └── position-distance-offset.patch
├── bundles/
│   ├── position-suite.json
│   ├── orchestrator-test.json
│   ├── inline-plan-smoke.json
│   ├── cumulative-integration-test.json
│   └── integration-conflict-test.json
├── tests/
│   ├── test-vt-live-separation.sh
│   ├── test-vt-response-separation.sh
│   ├── test-vt-creator-profile.sh
│   ├── test-vt-task-oracles.sh
│   └── test-vt-orchestrate.sh
└── results/
    └── .gitkeep
```

Prompts use Markdown because it is easy to inspect, diff, and pass directly to
`vt`. Each prompt is self-contained and follows the same handoff structure:

```text
Role
Repository context
Scope and forbidden changes
Task
Acceptance cases
Required response
```

The prompt must state the exact insertion location and include exact API
signatures whenever generated code refers to existing code. The model must
return a proposal only; it must not be allowed to edit the working tree.

## Running a prompt

From the repository root:

```bash
./scripts/vt --format json --max-tokens 4096 --temperature 0.1 \
  --retries 1 --file experiments/vibethinker/prompts/core/position-distance-tests.md
```

To capture a result for later review:

```bash
./scripts/vt --format json --max-tokens 4096 --temperature 0.1 \
  --retries 1 --file experiments/vibethinker/prompts/server/parser-negative-tests.md \
  > experiments/vibethinker/results/parser-negative-tests.json
```

Captured results are evidence of an experiment, not accepted code. A result
record should retain the prompt name, date, model, settings, response, token
usage, elapsed time, and human/automated acceptance outcome.

## Creator-style benchmark profile

The upstream [VibeThinker repository](https://github.com/WeiboAI/VibeThinker)
links to a public [sample-response archive](https://drive.google.com/drive/folders/1qom754QSjujDI98Wv8LIKTaTszPkAN6q?usp=drive_link).
The code records correspond to LiveCodeBench: a generic Python-programming
instruction, followed by `### Question`, `### Format`, starter code, and
`### Answer`. The official adapter puts all of that into one `user` message
and applies the model tokenizer's chat template with a generation prompt. The
[VibeThinker code evaluation guide](https://github.com/WeiboAI/VibeThinker/blob/main/eval/code/README.md)
documents this adapter. A reusable skeleton is in
[`prompts/benchmark/creator-code-template.md`](prompts/benchmark/creator-code-template.md).

`scripts/vt` exposes that recipe as an opt-in profile:

```bash
./scripts/vt \
  --profile creator-code \
  --format code \
  --file experiments/vibethinker/prompts/benchmark/creator-code-template.md
```

The profile sends one user message, prepending the published Python-programmer
instruction to the supplied prompt body, and uses `temperature=0.6`,
`top_p=0.95`, `top_k=-1`, neutral `min_p` and repetition settings, a
40,960-token generation limit, and no artificial reasoning budget. Explicit
CLI options override profile values. `--user-only` can be used independently
when only the message shape is wanted.

This is intentionally not the default for `vt-task`: the published recipe
uses long reasoning and multiple sampled candidates with an external
verifier, while repository tasks need short bounded outputs and local checks.
Use this profile to reproduce or compare against the authors' benchmark-style
prompting, not as a blanket replacement for the low-token task workflow.

## Start the compatible server

Use the repository launcher when the local model and llama.cpp paths match the
defaults:

```bash
./scripts/run-vt-server.sh
```

The launcher passes `--jinja`, the VibeThinker-specific template,
`--reasoning on`, and `--reasoning-format deepseek` to `llama-server`. Override
the local installation paths without editing the script:

```bash
VT_LLAMA_SERVER_BIN=/path/to/llama-server \
VT_MODEL_PATH=/path/to/VibeThinker-3B.gguf \
./scripts/run-vt-server.sh
```

The client sends `reasoning_format: deepseek` on every request as an explicit
per-request setting. This is intentional: on the tested llama.cpp build,
`/props` reports `reasoning_format: none` in its static defaults even though
the explicit request is parsed into separate reasoning and content fields.

Run the deterministic response-boundary regression test without a model:

```bash
./experiments/vibethinker/tests/test-vt-response-separation.sh
```

The creator-style request-shape regression test is also model-free:

```bash
./experiments/vibethinker/tests/test-vt-creator-profile.sh
```

It verifies the single-user-message layout and the published code-generation
sampler defaults without requiring a running server.

It covers non-streaming responses, SSE reasoning/content deltas, and the
defensive fallback for an inline completed `</think>` block. Normal text
output is always the answer; JSON output additionally includes `reasoning`,
`raw_content`, and `raw_reasoning` for explicit inspection.

With the server running, the live template/model smoke test checks the same
contract against VibeThinker itself:

```bash
./experiments/vibethinker/tests/test-vt-live-separation.sh
```

It requires the local endpoint and model, and intentionally fails if
`reasoning_content` is empty or if thinking markers appear in either the
non-streaming content field or the concatenated streaming content deltas.

## Parallel requests

`llama-server` shares the loaded model weights across `--parallel` slots. Each
slot has its own context/KV cache, so the context budget must be chosen with
concurrency in mind. For the short VT tasks in this repository, a measured
starting point on the 12 GB RTX 5070 is:

```bash
VT_SLOTS=8 VT_SLOT_CONTEXT_SIZE=4096 ./scripts/run-vt-server.sh
```

That gives eight 4,096-token slots. Eight simultaneous short requests passed
in the local probe with approximately 5 GiB of VRAM headroom. Thirty-two
4,096-token slots also fit and passed a short-request probe, but left only
approximately 1.8 GiB free and are not a sensible default. See the measured
capacity table in [`results/2026-09-04-driving.md`](results/2026-09-04-driving.md).

The repository launcher keeps the original one-slot long-context defaults
unless `VT_SLOTS` is supplied. For multiple slots it derives the total context
from `VT_SLOTS * VT_SLOT_CONTEXT_SIZE`; use `VT_CONTEXT_SIZE` when an explicit
total is preferred. `VT_FIT_CONTEXT` can override the fit minimum. These
first-class settings avoid duplicate llama.cpp arguments and make the slot
configuration reproducible.

The [`vibethinker-reasoning.jinja`](vibethinker-reasoning.jinja) file is an
experiment-specific template for the GGUF model. It preserves the upstream
Qwen-style role markers and always opens the model's thinking block because
VibeThinker-3B is documented as thinking-only. With llama-server's
`--reasoning-format deepseek`, the trace is separated into
`message.reasoning_content`. The current driving findings are recorded in
[`results/2026-09-04-driving.md`](results/2026-09-04-driving.md).

Token accounting and the direct-vs-VT comparison protocol are recorded in
[`docs/experiments/vibethinker-token-accounting.md`](../../docs/experiments/vibethinker-token-accounting.md).
The read-only [`scripts/codex-usage`](../../scripts/codex-usage) helper reads
the local Codex thread database and rollout telemetry so a task can be
measured without guessing from the visible response length.

## Minimal-context driving

For a small code task, do not send a repository summary when a command can
extract the exact relevant source. The existing stdin interface makes it
possible to construct a task packet from command output without creating an
intermediate file:

```bash
{
  printf '%s\n' 'Relevant source:'
  sed -n '628,674p' crates/mmorpg-server/src/main.rs
  sed -n '760,793p' crates/mmorpg-server/src/main.rs
  printf '%s\n' 'Write one test function for the three stated error cases. Return only Rust code.'
} | ./scripts/vt --format code --reasoning-budget 256 --max-tokens 1024
```

The experiment showed that even this can be too much freedom for VT. The
more reliable pattern was to split the work into one atomic completion, such
as filling a single `TODO` or copying one explicitly specified Rust statement,
and validate each result before assembling it. With a zero reasoning budget,
VT copied three explicitly supplied assertion statements exactly; when asked
to infer the same statements from `TODO`, it left the placeholder, echoed the
template, or invented an unrelated expression. This is a useful distinction:
the local model can be a constrained transformer, but it is not yet reliable
as a small multi-line Rust test author in this setup.

## Naming convention

Use lowercase kebab-case names describing the bounded behavior:

```text
<area>-<behavior>-<output-kind>.md
```

Examples:

```text
inventory-stacking-tests.md
position-distance-tests.md
parser-negative-tests.md
```

## Acceptance record

When a result is reviewed, record one of these outcomes in the associated
result file or experiment record:

```text
accepted       Compiled and passed the relevant tests.
rejected       Failed scope, syntax, behavior, or project checks.
revision       Worth retrying with a changed prompt or setting.
blocked        The task requires missing context or an unresolved design.
```

Do not describe a task as successful based only on a plausible response. The
Rust compiler, focused tests, diff review, and project invariants are the
acceptance authority.

## Running a bounded task

The vt-task prototype keeps routine validation out of the agent context.
Manifests are JSON because the runner already depends on jq through the
scripts/vt client. A manifest declares the exact commands whose output becomes
context, the target insertion point, and the checks to run in a temporary
copy:

    ./scripts/vt-task \
      --manifest experiments/vibethinker/tasks/parser-empty-command-test.json

The command prints compact JSON. On success, artifact points to a patch in
/tmp/vt-task-results; on failure, stage, diagnostic, and run_dir identify the
machine-readable failure without printing VT's reasoning trace. Use
--keep-temp only when inspecting the temporary checkout manually.

`--source-root PATH` selects the exact Git repository state used by context
commands and temporary validation. It defaults to the current repository.
`vt-orchestrate` sets it to the cumulative integration repository so a
dependent leaf can see artifacts that its prerequisites introduced.

Two optional oracle fields let the runner reject plausible but semantically
weak output:

- `baseline_checks` are commands that must fail in the unmodified source
  snapshot. If one passes, the task is rejected before VT is called. This
  establishes that the declared oracle can distinguish the baseline from the
  desired result. A nonzero exit can also represent an infrastructure failure,
  so these commands should be small, deterministic probes whose failure mode
  is unambiguous.
- `mutations` apply a bounded, planner-supplied patch to a copy of an otherwise
  accepted candidate. Each mutation's `setup_checks` must pass, proving the
  mutant still compiles or is otherwise valid, and its `kill_check` must fail.
  A generated test that lets the deliberately faulty behavior pass is rejected
  at the `mutation` stage. Setup checks rule out malformed or uncompilable
  mutants, but the kill check should still be narrowly focused so an unrelated
  failure cannot masquerade as a killed mutant.

Run the deterministic positive and negative oracle cases with:

```bash
./experiments/vibethinker/tests/test-vt-task-oracles.sh
```

Successful and failed results include `run_usage` (the sum of every VT
attempt), `usage` for the final attempt when one succeeded, and
`wall_time_ms`. This makes retries and validation overhead visible instead of
counting only the final answer.

When an attempt fails, the next attempt receives the original prompt plus the
failure stage and the last 20 diagnostic lines. It explicitly prioritizes
literal snippets and original output constraints over conflicting compiler
suggestions. The bounded feedback is saved as `attempt-N/prompt.txt`; it does
not expose an unrestricted shell or ask the model to diagnose the repository.
This repaired compiler errors in a live same-point and symmetry trial, and the
deterministic oracle test verifies that the second prompt contains the
rejection feedback.

When `vt-orchestrate` invokes `vt-task`, it sets `VT_TASK_PLAN_FILE` to the
task's normalized `plan.json`. `vt-task` adds that plan's objective and
semantic constraints to the prompt while continuing to take output shape,
context commands, and executable checks from the leaf manifest.

## Orchestrating a feature's leaf tasks

The [`scripts/vt-orchestrate`](../../scripts/vt-orchestrate) command is the
Codex-to-VT handoff boundary. Codex decomposes a high-level feature into a
small JSON plan; the orchestrator validates that plan against executable
`vt-task` manifests and then dispatches ready leaf tasks concurrently. Each
accepted result is applied to a cumulative temporary Git repository before
dependent tasks are dispatched. A rejected or integration-failed task causes
its dependents to be marked `skipped`, and no generated artifact is applied to
the real worktree.

The bundle format is intentionally explicit and machine-checkable. The
top-level fields are `schema_version`, `name`, `feature`, optional feature
`acceptance_checks`, optional executable `feature_checks`, `max_parallel`, and
`tasks`. `acceptance_checks` record human-readable feature requirements;
`feature_checks` are shell commands run against the completely integrated
candidate. Every task must provide:

```json
{
  "id": "bounded-task-id",
  "description": "One bounded objective with a clear output.",
  "manifest": "experiments/vibethinker/tasks/example.json",
  "depends_on": [],
  "target_files": ["crates/example/src/lib.rs"],
  "context_commands": [
    {"label": "relevant implementation", "command": "sed -n '1,40p' crates/example/src/lib.rs"}
  ],
  "output_kind": "rust_fragment",
  "acceptance_checks": ["cargo test -p example"],
  "baseline_checks": ["cargo test -p example focused_test"],
  "mutations": [
    {
      "name": "focused-behavior-regression",
      "patch_file": "experiments/vibethinker/mutations/example.patch",
      "allowed_files": ["crates/example/src/lib.rs"],
      "setup_checks": ["cargo check -p example"],
      "kill_check": "cargo test -p example focused_test"
    }
  ],
  "semantic_constraints": ["Do not modify production behavior."]
}
```

`manifest` is optional. When present, it points to an existing executable
leaf specification and the orchestrator requires the plan's `target_files`,
`context_commands`, `output_kind`, `acceptance_checks`, `baseline_checks`, and
`mutations` to match it. When
omitted, the task's `task`, `insert_after`, and output fields are materialized
into a temporary manifest inside the execution directory. This lets Codex
emit one self-contained feature plan without manually creating a second
manifest file. In both modes, the machine-checkable declarations cannot
silently drift from the command that will run. `semantic_constraints`
are retained in the normalized plan and audit trail and are included in the VT
prompt as a short handoff preamble; they still require a human or a stronger
checker when their meaning cannot be expressed by a shell command. Each
execution directory contains the exact `plan.json` handoff used for that
task. Optional `assertions` add machine-checkable required/forbidden files and
text snippets, and are enforced after VT output is applied in the temporary
copy. `task_id`
is accepted as an alias for `id` for upstream planners, but both may not
disagree.

Validate a generated plan before spending any VT calls:

```bash
./scripts/vt-orchestrate \
  --bundle experiments/vibethinker/bundles/position-suite.json \
  --validate-only
```

The validation response is normalized JSON containing the feature, all task
metadata, feature checks, and the effective concurrency limit. Validation
rejects dependency cycles and tasks with overlapping target files unless one
task transitively depends on the other. Normal execution uses the same
validation path first:

```bash
./scripts/vt-orchestrate \
  --bundle experiments/vibethinker/bundles/position-suite.json \
  --max-parallel 2
```

The command returns compact JSON with per-task generation and integration
status, feature-check results, a single combined patch in `artifact`,
artifact/result directories, accepted/rejected/skipped/failed counts, and
aggregate `run_usage`. A leaf is `accepted` only after its artifact applies to
the cumulative repository. A valid leaf whose patch no longer applies is
`failed` at the `integration` stage. Feature checks run only after every leaf
has integrated successfully.
`--max-parallel` overrides the bundle's value; otherwise the bundle value,
`VT_ORCHESTRATE_MAX_PARALLEL`, or the default of four is used in that order.
Concurrently running leaves must own disjoint files. Tasks that intentionally
refine the same file must declare a dependency path so that each one is
generated from the state produced by the earlier task. The deterministic
scheduler test uses `--runner` to inject a fake task runner; normal operation
uses `scripts/vt-task`.
The runnable `position-suite.json` is the canonical example of the complete
plan format. Its leaf prompts intentionally use complete exact-copy skeletons
with zero reasoning budget because broader live synthesis remained stochastic
even with diagnostic retries. The baseline, exact-text, mutation, and combined
checks remain independent of that model contract. `inline-plan-smoke.json`
demonstrates a self-contained leaf without a separate manifest;
`orchestrator-test.json` exercises rejection and dependency blocking;
`cumulative-integration-test.json` proves that dependency state is visible;
and `integration-conflict-test.json` proves that stale artifacts are rejected.
These deterministic test bundles do not invoke the model.
