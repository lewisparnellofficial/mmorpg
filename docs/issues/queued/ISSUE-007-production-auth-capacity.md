+++
id = "ISSUE-007"
type = "investigation"
title = "Define production authentication, deployment, and capacity gates"
status = "candidate"
priority = "P2"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = ["ISSUE-004", "ISSUE-005", "ISSUE-006"]
blocks = []
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/architecture/**", "docs/research/**", "docs/decisions/**", "docs/experiments/**", "crates/mmorpg-server/**", "scripts/**"]
forbidden_paths = ["crates/mmorpg-core/**"]
validation_commands = ["cargo test --workspace", "./scripts/validate-all.sh"]
+++

# ISSUE-007 — Define production authentication, deployment, and capacity gates

## Objective

Replace the loopback-only development boundary with an explicit, measured production security and capacity plan before any public or large-scale deployment claim.

## Context

- Requirements: `PLAN.md` Milestones 6–8 and explicit deferrals for public auth, encryption, and capacity claims.
- Architecture/research: `docs/architecture/networking.md`, `docs/research/networking-and-replication.md`, `docs/research/rust-server.md`.
- Experiments: EXP-001, EXP-002, EXP-010, and EXP-013.
- Decisions: ADR-005 and ADR-007 are development-only boundaries.
- Related issues: depends on durable persistence and production replication issues `ISSUE-005` and `ISSUE-006`.

## Deliverables

- Production authentication/session/security architecture and threat model.
- Encryption, credential, abuse, deployment, observability, and upgrade policy.
- Predeclared representative 5,000-client/200-player workloads.
- Capacity measurements, bottleneck findings, and an accepted go/no-go decision.

## Acceptance criteria

- [ ] Public authentication and encrypted transport are designed, implemented, and tested in the chosen deployment boundary.
- [ ] Session, accept, command, and resource limits are explicit and load-tested.
- [ ] Representative capacity workloads are measured on documented hardware.
- [ ] Results distinguish measured capacity from estimates and do not redefine failed gates.
- [ ] Production readiness decision and remaining risks are recorded in ADRs/experiments.

## Scope

### In scope

- Security and capacity prerequisites for public deployment.

### Out of scope

- Claiming production readiness from the current loopback development server or synthetic microbenchmarks.

## Dependencies and parallelism

The workload design can begin during earlier issues, but final capacity evidence depends on durable storage and production replication boundaries.

## Worktree and file ownership

### Shared files requiring integration review

- Server listener/session code, networking architecture, security decisions, and experiment records.

### Read-only context

- Existing development authentication and benchmark evidence.

## Validation

### Automated commands

Run workspace and aggregate validation plus the new load/security test suite.

### Manual or hardware checks

- Deployment, key/credential handling, TLS/encryption, and representative hardware/load environment review.

### Evidence to record

Use new architecture/decision/experiment records; preserve raw measurements and limitations.

## Integration contract

No public listener or production-capacity claim is enabled until security and representative workload gates are accepted.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN explicit deferrals and NEXT item 5.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
