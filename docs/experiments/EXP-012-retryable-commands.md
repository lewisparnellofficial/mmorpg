# EXP-012: Retryable durable-command prototype

## Objective

Exercise the first bounded implementation slice for duplicate purchase, loot,
and quest-turn-in commands. The slice should let a reconnecting client repeat
an operation identifier without applying the authoritative mutation twice.

## Hypothesis

An additive typed command wrapper carrying a nonzero operation identifier,
scoped to an authenticated account and selected character, can provide
process-local duplicate suppression while the project continues toward a
durable operation journal.

## Build and content version

- Repository working tree after commit `ae484a8` plus the retryable-command
  changes recorded with this experiment.
- Starter content catalog and development `dev-local` authentication.
- Date: 2026-09-09.

## Configuration and workload

- Linux development server with the typed wire listener on loopback.
- `experiments/wire-gameplay-smoke` sends wrapped purchase, loot, and quest
  turn-in commands with distinct operation identifiers.
- The server retains at most 256 completed or failed operation outcomes in
  process memory; with `--character-store`, result payloads and failed reasons
  are also appended to an operation journal and loaded on restart.
- The focused server tests submit the same purchase operation twice and check
  that the second submission returns the cached event without changing gold or
  queueing another authoritative command. They also submit a retryable wrapper
  around a non-durable command, verify that the rejection is appended as a
  failed operation record, and verify that the same rejection is loaded after
  restart.
- The aggregate validator runs the typed diagnostic and the three-client gate
  in addition to workspace, standalone-crate, editor, and experiment checks.

## Measurements

- `cargo test --workspace`: passed, including 31 core tests, 33 server tests,
  18 wire tests, and the workspace compatibility fixtures.
- Retry-specific server test: passed.
- Rejected-retry restart test: passed; the failed operation reason survived a
  server restart and was available for duplicate-retry rejection.
- Completed-operation revision test: passed; a version-2 journal record
  preserved the staged world revision, while legacy version-1 records remain
  readable with revision zero.
- Interrupted-prepare restart test: passed; a journal ending in `prepared` is
  converted to a durable rejection before the operation key can be retried.
- Completed-record recovery test: passed; a completed durable command with an
  older checkpoint revision is replayed once through the staged world path,
  while the cached result remains the duplicate fence.
- Typed gameplay smoke: passed with purchase, loot, and quest completion.
- Three-client gate: passed; the tank and healer shared one party summary while
  the unrelated damage client received no private party summary.
- `./scripts/validate-all.sh`: passed with `aggregate validation: PASS`.
- `--shutdown-after-ticks 2`: passed; the server reported a graceful drain
  and completion at the requested tick, including the pending failed-operation
  drain regression test.

## Result

The hypothesis is supported for the bounded prototype. With the opt-in
character store, the journal records the typed intent before it enters the
  authoritative command queue and persists the completed result for restart
loading. The wire codec accepts an additive `Retryable` wrapper, and duplicate operation
keys for the same account and character are served from the bounded result
cache without reapplying the core command.

## Limitations

- Without `--character-store`, the result cache is process-local and is lost on
  restart.
- The prepared intent is journaled before live apply, but completion insertion
  follows successful world-step application; the pair is not yet one atomic
  commit-before-live-apply transaction.
- Completion queue capacity is reserved for the full staged batch before any
  completion record is submitted, and journal parsing preserves explicit
  failed-operation records without treating them as successful results.
- A failure-injection test replaces the journal path with a directory and
  verifies that completion-store failure discards the staged world, leaves the
  live economy unchanged, and does not populate the completed-operation cache.
- The journal restart test appends a torn final record and confirms earlier
  complete results remain loadable.
- The restart smoke repeats the same wrapped operation IDs after process
  restart and passes; the loaded result cache prevents the second run from
  reapplying those operations. Completion-store failure recovery remains an
  explicitly unproven cross-process path; within one live process the
  staged world batch is discarded and the affected clients receive an error.
- Legacy version-2 completion records written before command-payload capture
  remain deduplication-only and cannot be replayed after a checkpoint gap.
  Current records compare operation and checkpoint revisions before replaying
  a completed command. Cross-process fencing and database-level transaction
  semantics remain outside the prototype. Prepared-but-not-completed records
  have an explicit no-replay failure policy. The deterministic shutdown path
  covers orderly local exit and now drains pending failed-operation records;
  the bounded timeout path explicitly reports abandoned failures.
- Failed validation outcomes now carry a durable failed-operation record and
  are replayed as the same error for a duplicate key after restart. Core
  gameplay rejections and a general typed error-result schema remain outside
  this narrow validation-level path.
- The graphical client runtime gate remains partial because Vulkan validation
  errors were observed during the bounded client run; see EXP-011.

## Follow-up work

- Extend the durable operation record toward explicit pending, committed, and
  failed states for core gameplay outcomes, not only validation-level
  rejections.
- Add crash-injection coverage for checkpoint/journal races and replace the
  development replay policy with a database-backed transaction before
  production operation.
- Add crash-oriented restart and failure-injection tests for the commit-before-
  live-apply boundary, including duplicate retries after an interrupted
  process restart.
