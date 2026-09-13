+++
id = "ISSUE-005"
type = "implementation"
title = "Replace local durability prototypes with a production persistence boundary"
status = "candidate"
priority = "P2"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = ["ISSUE-004", "ISSUE-003"]
blocks = []
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["crates/mmorpg-server/**", "crates/mmorpg-core/**", "docs/architecture/persistence.md", "docs/research/persistence-and-recovery.md", "docs/decisions/ADR-009-bounded-durable-operation-boundary.md", "docs/experiments/EXP-005-durable-character-checkpoint.md", "docs/experiments/EXP-012-retryable-commands.md"]
forbidden_paths = ["crates/mmorpg-client/**"]
validation_commands = ["cargo test --workspace", "./scripts/smoke-restart-persistence.sh", "./scripts/validate-all.sh"]
+++

# ISSUE-005 — Replace local durability prototypes with a production persistence boundary

## Objective

Define and implement the production persistence boundary for multi-character durable progress, idempotent business operations, recovery, and safe shutdown without blocking the simulation owner.

## Context

- Requirements: `PLAN.md` Milestones 7 and 12; explicit deferral of PostgreSQL/full journal/outbox.
- Architecture/research: `docs/architecture/persistence.md`, `docs/research/persistence-and-recovery.md`.
- Experiments: EXP-005 and EXP-012.
- Decisions: ADR-009 is accepted only for the bounded local development slice.
- Related issues: starts after `ISSUE-004` and the addon/runtime choice in `ISSUE-003`.

## Deliverables

- Durable repository/schema and migration policy.
- Strongly scoped operation IDs, idempotency, fencing, and crash recovery design.
- Commit-before-publish implementation against the selected store.
- Backup/restore, corruption, retry, shutdown, and failure tests.
- Updated ADR-009 and persistence evidence.

## Acceptance criteria

- [ ] Multiple characters and accounts have isolated, stable namespaces.
- [ ] Retry returns the prior result without duplicate mutation.
- [ ] Store failure, crash boundaries, and shutdown leave no unacknowledged live mutation.
- [ ] Persistence work remains off the simulation owner.
- [ ] Migration, backup, restore, and corruption behavior are tested.

## Scope

### In scope

- Durable identity/progress and retryable economy/quest/loot operations.

### Out of scope

- Transient combat journaling, backups-as-a-service, failover deployment, or a complete production operations platform beyond the agreed boundary.

## Dependencies and parallelism

Requires first-slice acceptance and a coordinated runtime/package decision because both affect the durable boundary. Schema and failure-model investigation can begin independently after refinement.

## Worktree and file ownership

### Shared files requiring integration review

- Core/server command and checkpoint APIs, ADR-009, and persistence architecture docs.

### Read-only context

- Existing checkpoint/journal implementation and EXP-005/012.

## Validation

### Automated commands

Run workspace tests, restart smoke, and aggregate validation, supplemented by store-specific tests when selected.

### Manual or hardware checks

- Validate backup/restore and crash injection in the supported deployment environment.

### Evidence to record

Update the persistence ADR, architecture/research records, and experiment results.

## Integration contract

Gameplay commands retain server-authoritative semantics while storage implementation changes behind the repository/operation boundary.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN Milestones 7 and 12 and NEXT item 5.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
