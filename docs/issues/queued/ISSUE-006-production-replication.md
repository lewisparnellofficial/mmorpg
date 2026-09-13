+++
id = "ISSUE-006"
type = "implementation"
title = "Build production addressed replication and interest management"
status = "candidate"
priority = "P2"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = ["ISSUE-004"]
blocks = []
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["crates/mmorpg-server/**", "crates/mmorpg-wire/**", "crates/mmorpg-client-session/**", "crates/mmorpg-client-model/**", "docs/architecture/networking.md", "docs/experiments/EXP-002-replication-workload.md", "docs/experiments/EXP-010-addressed-delivery.md"]
forbidden_paths = ["crates/mmorpg-core/**"]
validation_commands = ["cargo test --workspace", "cargo run --manifest-path experiments/replication-model/Cargo.toml -- --players 200 --seconds 1 --tick-hz 20 --budget-kib 64", "./scripts/validate-all.sh"]
+++

# ISSUE-006 — Build production addressed replication and interest management

## Objective

Replace the current minimum addressed-delivery prototype with production-ready audience filtering, interest management, bounded reliable/replaceable queues, and representative workload evidence.

## Context

- Requirements: `PLAN.md` Milestone 8 and explicit deferral of production replication.
- Architecture/research: `docs/architecture/networking.md`, `docs/research/networking-and-replication.md`.
- Experiments: EXP-002 and EXP-010.
- Decisions: current delivery decisions are development-slice boundaries only.
- Related issues: follows `ISSUE-004`; must coordinate with production durability in `ISSUE-005`.

## Deliverables

- Global, player, nearby, and real-registry party audience implementation.
- Player-specific snapshots and privacy-preserving audience unions.
- Reliable and replaceable queue policy with bounded memory and TCP ordering documentation.
- Spatial interest implementation and representative benchmark results.

## Acceptance criteria

- [ ] Private purchases, rejections, snapshots, and party data never reach unauthorized clients.
- [ ] Nearby visibility and movement coalescing are correct under queue pressure.
- [ ] Reliable events are admitted without stale replaceable state consuming their budget.
- [ ] Representative predeclared workloads meet documented bounds or produce a failed, unmasked experiment result.
- [ ] No 5,000-client/200-player claim is made without exact workload evidence.

## Scope

### In scope

- Server replication and client delivery/session boundaries.

### Out of scope

- QUIC, custom UDP, public deployment, or broad protocol redesign without a new decision.

## Dependencies and parallelism

Can be designed independently after first-slice acceptance, but implementation must coordinate with persistence revisions and any simulation-worker ownership changes.

## Worktree and file ownership

### Shared files requiring integration review

- Wire schemas, server enqueue paths, session queues, and networking docs.

### Read-only context

- EXP-002/010 and the current addressed-delivery implementation.

## Validation

### Automated commands

Run workspace tests, replication model, aggregate validation, and new privacy/backpressure/integration workloads.

### Manual or hardware checks

- None required unless the selected renderer/client path adds a presentation-specific gate.

### Evidence to record

Record exact workload, hardware, distributions, queue bounds, and failed cases in EXP-002 or a successor.

## Integration contract

Audience filtering occurs before enqueueing and cannot widen private fields through union or snapshot projection.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN Milestone 8 and NEXT item 5.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
