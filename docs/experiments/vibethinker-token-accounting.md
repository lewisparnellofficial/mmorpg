# VibeThinker token accounting and comparison protocol

This record defines how to determine whether using the local VibeThinker (VT)
model reduces the amount of Codex work needed for a software task. The goal is
not to make VT's local token count look small. The goal is to measure whether a
Codex agent reaches the same accepted result with fewer Codex tokens and an
acceptable change in latency, throughput, and reliability.

## What can be observed locally

The repository's [`scripts/vt`](../../scripts/vt) client receives the
OpenAI-compatible llama.cpp `usage` object. A VT call therefore exposes its
prompt, completion, cached-input detail when provided, total, and elapsed
time. [`scripts/vt-task`](../../scripts/vt-task) adds the cost of all retry
attempts and the complete task-runner wall time to its compact JSON result.

Codex CLI also persists local telemetry. The read-only
[`scripts/codex-usage`](../../scripts/codex-usage) helper selects the newest
unarchived thread for the current repository by default:

```bash
./scripts/codex-usage --json | jq
```

To monitor the cumulative total while a thread is running:

```bash
./scripts/codex-usage --watch 2
```

To observe a specific thread, use the `thread_id` printed by the first command:

```bash
./scripts/codex-usage --thread THREAD_ID --json | jq
```

The JSON contains two related views:

- `database_tokens_used` is the cumulative total recorded on the Codex thread.
- `usage` is the latest detailed `token_count` snapshot from the rollout,
  including `input_tokens`, `cached_input_tokens`, `output_tokens`,
  `reasoning_output_tokens`, and `total_tokens`.
- `last_usage` is the most recent increment reported by that rollout event.

The values are cumulative within a thread. They must not be described as the
cost of one ticket unless the measurement takes a before snapshot and an after
snapshot with no other work occurring in that thread. A fresh isolated thread
per trial is even cleaner.

Codex's non-interactive JSON event stream can also be archived for a controlled
run:

```bash
codex exec --json --ephemeral -C "$PWD" \
  'Complete the benchmark ticket and run its acceptance checks.' \
  > /tmp/codex-trial.jsonl

jq -c 'select(.type == "event_msg" and .payload.type == "token_count")
  | .payload.info.total_token_usage' /tmp/codex-trial.jsonl
```

The stream is useful for a disposable benchmark because it keeps the usage
record beside the task output. The local database watcher is useful for an
interactive task that is already in progress. Neither mechanism should read or
modify credentials.

## Matched trial design

Run the same ticket under two conditions:

1. **Direct:** Codex receives the ticket, inspects the repository, implements
   the change, and runs the normal checks.
2. **Orchestrated:** Codex receives the same ticket plus a short instruction to
   call `scripts/vt-task` for a bounded subtask. VT receives only the extracted
   context and returns an artifact. The runner formats, scopes, compiles, and
   tests that artifact in a temporary copy. Codex accepts, rejects, or revises
   the artifact and finishes the ticket.

Use fresh or resettable copies of the repository. Keep the ticket, starting
commit, acceptance checks, model settings, and time budget identical. Repeat
each condition several times because a single stochastic VT response is not a
reliable estimate.

Capture at least these fields for every trial:

| Metric | Direct condition | Orchestrated condition |
| --- | --- | --- |
| Accepted result | yes/no and commit/diff | yes/no and commit/diff |
| Codex input tokens | before/after delta | before/after delta |
| Codex cached input | telemetry detail | telemetry detail |
| Codex output/reasoning | telemetry detail | telemetry detail |
| VT prompt/completion/reasoning | zero | `vt-task` `run_usage` |
| VT calls and retries | zero | `attempts` and `run_usage.attempts` |
| Wall time | process or session timestamps | `wall_time_ms` plus Codex time |
| Validation | checks and failures | checks and failures |
| Human intervention | count and description | count and description |

The primary billed-token comparison is the Codex delta, not the sum of Codex
and VT tokens. The local VT tokens still matter for electricity, memory, GPU
occupancy, and throughput, so report them as a separate resource budget.

## Minimal before/after measurement

For an interactive Codex thread, save snapshots around exactly one ticket:

```bash
before=$(./scripts/codex-usage --json)
# Perform exactly one trial here.
after=$(./scripts/codex-usage --json)

jq -n --argjson before "$before" --argjson after "$after" '
  {
    before_total: ($before.usage.total_tokens // $before.database_tokens_used),
    after_total: ($after.usage.total_tokens // $after.database_tokens_used),
    codex_total_delta:
      (($after.usage.total_tokens // $after.database_tokens_used)
       - ($before.usage.total_tokens // $before.database_tokens_used)),
    before_thread: $before.thread_id,
    after_thread: $after.thread_id
  }'
```

If the thread changes between snapshots, or another agent is active, discard
the result. For a multi-turn task, use the explicit `--thread THREAD_ID`
option rather than relying on “newest thread” selection.

## What “saves tokens” means

For an accepted-result trial, define:

```text
codex_savings = direct_codex_tokens - orchestrated_codex_tokens
codex_savings_rate = codex_savings / direct_codex_tokens
```

A positive result is meaningful only if the acceptance rate does not fall and
the extra VT and validation time fit the workflow. Track median and worst-case
values, not only the best run. A failed VT attempt that Codex must diagnose can
consume more Codex tokens than it saves, even when VT itself is free.

There are several likely regimes:

- **Tiny, obvious changes:** direct Codex is likely cheaper and faster because
  the orchestration prompt and process startup are overhead.
- **Bounded repetitive changes:** VT may save Codex output/reasoning tokens if
  it produces a useful artifact from a very small context and the runner can
  reject bad output without returning its reasoning to Codex.
- **Design-heavy or cross-file changes:** direct Codex is likely better. VT's
  current long-context degradation, weak repository-level planning, and retry
  failures can create extra latency and Codex review work.

The current evidence supports the middle case only for narrow unit-test-like
tasks. In this repository, VT produced a valid three-test `Position` suite
with 296 prompt tokens and 772 completion tokens, and the temporary candidate
passed the focused test. A parser test that required more inference failed the
runner's format/compiler checks even after two attempts. These are local
measurements, not a general claim about VT or Codex.

## Current limitation

The token watcher is intentionally read-only and local. It does not change
Codex, intercept a hosted billing system, or attribute tokens to an arbitrary
subtask inside a long-running thread. The reliable path is to run isolated
trials or capture Codex's JSON event stream, then pair that record with the
machine-readable VT result. If the Codex installation changes its state
database schema or rollout event shape, the watcher may need an adapter update.
