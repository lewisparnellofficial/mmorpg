+++
id = "ISSUE-008"
type = "coordination"
title = "Plan the next production world and client/tooling expansion"
status = "candidate"
priority = "P3"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = ["ISSUE-003", "ISSUE-004"]
blocks = []
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/architecture/**", "docs/research/**", "docs/decisions/**", "docs/implementation-roadmap.md", "PLAN.md", "NEXT.md"]
forbidden_paths = ["crates/mmorpg-core/**"]
validation_commands = ["./scripts/validate-all.sh"]
+++

# ISSUE-008 — Plan the next production world and client/tooling expansion

## Objective

After the first slice and addon-runtime decision, select and decompose the next authoritative production milestone for instances/layers, client/editor packaging, and other explicitly deferred capabilities.

## Context

- Requirements: `PLAN.md` Milestones 7–8 and Explicitly deferred.
- Architecture/research: `docs/architecture/world-and-layering.md`, `docs/research/instances-and-layers.md`, `docs/architecture/development-tools.md`.
- Experiments: EXP-003 and EXP-007/011.
- Decisions: addon runtime acceptance is tracked by `ISSUE-003`; current layering/editor decisions remain bounded prototypes.
- Related issues: depends on `ISSUE-003` and `ISSUE-004`.

## Deliverables

- A prioritized post-slice roadmap with separate bounded implementation issues.
- Explicit decisions for instance lifecycle, overworld layering, safe-point migration, and ownership.
- Separate scope for production client/editor packaging and deferred terrain/tool features.
- Updated PLAN/NEXT and any required ADR/open questions.

## Acceptance criteria

- [ ] The next production milestone is selected by owner decision.
- [ ] Instances/layers have explicit ownership, migration, lifecycle, and group-cohesion boundaries.
- [ ] Deferred editor/client/runtime features are separated from authoritative server work.
- [ ] Each selected workstream has acceptance criteria, dependencies, and an evidence plan.
- [ ] No deferred feature is silently promoted into the current vertical slice.

## Scope

### In scope

- Planning and architectural decomposition of instances/layers and production tooling/client packaging.

### Out of scope

- Implementing raids, battlegrounds, full terrain materials, broad UI APIs, or production packaging in this issue.

## Dependencies and parallelism

This is a sequencing issue after first-slice acceptance and the addon runtime decision. The research review can be prepared in parallel, but the final ordering is a coordination decision.

## Worktree and file ownership

### Shared files requiring integration review

- `PLAN.md`, `NEXT.md`, implementation roadmap, architecture docs, ADRs, and open questions.

### Read-only context

- Existing layer/editor/client research and experiments.

## Validation

### Automated commands

Run aggregate validation to ensure planning/documentation changes do not disturb the checked boundary.

### Manual or hardware checks

- Owner review of priorities and explicit deferrals.

### Evidence to record

Record decisions in ADRs and implementation issues; do not treat proposals as accepted architecture.

## Integration contract

This issue produces the next bounded backlog rather than broad implementation authority. New work must be represented by follow-up issues with disjoint ownership.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN post-slice direction and NEXT item 5.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
