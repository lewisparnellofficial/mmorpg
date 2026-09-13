+++
id = "ISSUE-004"
type = "coordination"
title = "Complete the first vertical-slice acceptance record"
status = "ready"
priority = "P1"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = ["ISSUE-001", "ISSUE-002"]
blocks = ["ISSUE-005", "ISSUE-006", "ISSUE-007", "ISSUE-008"]
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/experiments/EXP-011-graphical-client-gate.md", "docs/README.md", "NEXT.md", "PLAN.md"]
forbidden_paths = ["crates/mmorpg-core/**"]
validation_commands = ["./scripts/validate-all.sh", "./scripts/smoke-graphical-three-role.sh", "./scripts/smoke-graphical-restart-persistence.sh", "./scripts/smoke-graphical-slow-client.sh"]
+++

# ISSUE-004 — Complete the first vertical-slice acceptance record

## Objective

Publish one authoritative acceptance record for the three-role town-and-field slice, including the remaining physical-input and renderer evidence, limitations, and explicit owner disposition.

## Context

- Requirements: `docs/architecture/requirements.md`; `PLAN.md` Milestones 6–13.
- Architecture/research: `docs/architecture/system-overview.md`, `docs/architecture/networking.md`, `docs/architecture/persistence.md`.
- Experiments: EXP-004 through EXP-013, especially EXP-011 and EXP-012.
- Decisions: ADR-007, ADR-008, and ADR-009 are accepted for the development slice; ADR-010 remains separate/provisional.
- Related issues: depends on `ISSUE-001` and `ISSUE-002`; blocks post-slice production work.

## Deliverables

- Final cross-check of typed authentication, character binding, combat, party privacy, recovery, loot generations, retry, persistence, and slow-client evidence.
- Updated EXP-011 or a successor acceptance record.
- Updated PLAN/NEXT status and a list of limitations that remain intentionally development-only.

## Acceptance criteria

- [ ] Three distinct roles enter one shared zone with one session per character.
- [ ] No unauthenticated gameplay path exists.
- [ ] Compatibility, content mismatch, unknown-event, reconnect, and stale-state behavior pass.
- [ ] Combat, party/privacy, credit, loot, respawn, retry, and durable town progress pass exactly once.
- [ ] Physical tablet and renderer gates are either passed or explicitly accepted as development limitations by the project owner.
- [ ] Aggregate validation and the documented graphical experiment pass.

## Scope

### In scope

- Evidence synthesis and acceptance documentation for the current development slice.

### Out of scope

- Production authentication, public deployment, 5,000-client capacity claims, or broad new gameplay.

## Dependencies and parallelism

This is the integration gate after the independent tablet and renderer issues. It is the decision point before selecting post-slice production work.

## Worktree and file ownership

### Shared files requiring integration review

- `PLAN.md`, `NEXT.md`, `docs/README.md`, and affected experiment records.

### Read-only context

- All current milestone experiments, ADRs, and validation scripts.

## Validation

### Automated commands

Run the listed graphical and aggregate checks, plus the focused smokes from EXP-004 through EXP-013 as needed.

### Manual or hardware checks

Review ISSUE-001 and ISSUE-002 evidence and obtain explicit disposition for any remaining host-specific limitations.

### Evidence to record

Record the acceptance outcome in the canonical experiment/roadmap documents, including commit, hardware, tests, and limitations.

## Integration contract

After closure, the repository has a clear answer to whether the first development slice is accepted and which production claims remain prohibited.

## Progress log

### 2026-09-12 — coordinator

- Queued from NEXT item 4 and PLAN Milestone 13.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
