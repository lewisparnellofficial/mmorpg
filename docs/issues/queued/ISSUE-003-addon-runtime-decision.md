+++
id = "ISSUE-003"
type = "decision"
title = "Select and accept the production addon runtime boundary"
status = "ready"
priority = "P1"
owner = ""
created = "2026-09-12"
updated = "2026-09-12"
parent = ""

depends_on = []
blocks = ["ISSUE-005", "ISSUE-008"]
conflicts_with = []

worktree = ""
branch = ""
base_commit = ""

allowed_paths = ["docs/decisions/ADR-010-addon-runtime-boundary.md", "docs/experiments/EXP-006-ui-scripting-sandbox.md", "crates/mmorpg-ui-contract/**", "experiments/ui-scripting/**", "experiments/ui-wasm-comparison/**", "crates/mmorpg-client/**", "scripts/smoke-ui-*.sh"]
forbidden_paths = ["crates/mmorpg-core/**"]
validation_commands = ["cargo test --manifest-path experiments/ui-scripting/Cargo.toml", "cargo run --quiet --manifest-path experiments/ui-wasm-comparison/Cargo.toml", "./scripts/smoke-ui-adversarial.sh", "./scripts/smoke-ui-process-isolation.sh", "./scripts/validate-all.sh"]
+++

# ISSUE-003 — Select and accept the production addon runtime boundary

## Objective

Choose and document one production addon runtime boundary that implements the frozen `ui.v1` contract with bounded execution, resource limits, package validation, failure isolation, and reproducible supported-hardware evidence.

## Context

- Requirements: `docs/architecture/requirements.md`; `PLAN.md` Milestones 3–5.
- Architecture/research: `docs/architecture/ui-scripting.md`, `docs/research/ui-scripting-spike.md`.
- Experiments: `docs/experiments/EXP-006-ui-scripting-sandbox.md`, `experiments/ui-wasm-comparison/README.md`.
- Decisions: `docs/decisions/ADR-010-addon-runtime-boundary.md` is proposed.
- Related issues: blocks post-slice runtime/package work in `ISSUE-005` and `ISSUE-008`.

## Deliverables

- A measured Luau acceptance, a measured Wasmi/process-host acceptance, or a documented decision to keep the boundary provisional.
- Production package-loading and version-policy documentation for the selected runtime.
- Guest ABI and lifecycle implementation if Wasmi is selected.
- Minimum-hardware, startup/shutdown, quota, unload, and failure/restart evidence.
- Accepted or revised ADR-010.

## Acceptance criteria

- [ ] The selected runtime passes the explicit ADR-010 capability, interruption, memory, host-cost, unload, and failure-isolation gates.
- [ ] Package validation, integrity, versioning, and upgrade behavior are defined and fixture-tested.
- [ ] Supported minimum hardware is named and measured.
- [ ] Runtime failure/restart behavior is integrated with the graphical client lifecycle.
- [ ] ADR-010 records the decision, evidence, limitations, and revisit conditions.

## Scope

### In scope

- The existing language-neutral contract and its Luau/Wasmi adapters.
- Runtime/package policy and process supervision needed to make the decision credible.

### Out of scope

- A broad scripted HUD, privileged signing, native modules, remote packages, or arbitrary assets.

## Dependencies and parallelism

The existing contract and comparison evidence permit bounded parallel investigation, but the final decision is a single integration gate. It should be completed before expanding addon packaging or runtime features.

## Worktree and file ownership

### Shared files requiring integration review

- `scripts/validate-all.sh` and graphical client startup if the selected runtime becomes default.

### Read-only context

- PLAN, NEXT, ADR-010, EXP-006, and the `ui.v1` crate.

## Validation

### Automated commands

Run the listed adapter, adversarial, process-isolation, and aggregate checks, plus any new selected-runtime fixture tests.

### Manual or hardware checks

Run the selected runtime on supported minimum hardware and verify addon failure/restart while the default UI and client remain usable.

### Evidence to record

Update EXP-006 or a successor and ADR-010. Do not commit private captures or generated fuzz output.

## Integration contract

The selected runtime must consume the existing language-neutral contract; no Luau, Wasm, Qt, Bevy, socket, filesystem, or authoritative-core types may leak into that contract.

## Progress log

### 2026-09-12 — coordinator

- Queued from PLAN Milestones 3–5 and NEXT item 3.

## Completion report

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
