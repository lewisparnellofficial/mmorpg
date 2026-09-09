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
- The server retains at most 256 completed operation results in process memory.
- The focused server test submits the same purchase operation twice and checks
  that the second submission returns the cached event without changing gold or
  queueing another authoritative command.
- The aggregate validator runs the typed diagnostic and the three-client gate
  in addition to workspace, standalone-crate, editor, and experiment checks.

## Measurements

- `cargo test --workspace`: passed (31 core, 24 server, 18 wire tests, plus
  workspace compatibility fixtures and other crate tests).
- Retry-specific server test: passed.
- Typed gameplay smoke: passed with purchase, loot, and quest completion.
- Three-client gate: passed; the tank and healer shared one party summary while
  the unrelated damage client received no private party summary.
- `./scripts/validate-all.sh`: passed with `aggregate validation: PASS`.

## Result

The hypothesis is supported for the bounded process-local prototype. The
wire codec accepts an additive `Retryable` wrapper, and duplicate operation
keys for the same account and character are served from the bounded result
cache without reapplying the core command.

## Limitations

- The result cache is process-local and is lost on restart.
- Cache insertion currently follows successful world-step application; it is
  not a commit-before-live-apply transaction boundary.
- The prototype does not provide a durable operation journal, fsync policy,
  cross-process fencing, or recovery of in-flight operations.
- Rejected operations do not yet carry a durable typed result record suitable
  for replay; this slice is primarily a successful-operation duplicate fence.
- The graphical client runtime gate remains partial because Vulkan validation
  errors were observed during the bounded client run; see EXP-011.

## Follow-up work

- Design and implement a durable operation record with explicit pending,
  committed, and failed states.
- Define commit ordering and recovery behavior for crashes between the journal
  write and live simulation application.
- Add restart and failure-injection tests, including duplicate retries after a
  process restart and retries of rejected operations.
