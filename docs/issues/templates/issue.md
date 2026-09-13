+++
# Copy this file to docs/issues/{queued,active,blocked}/ISSUE-NNN-short-title.md.
# Keep the metadata valid TOML; the issue manager can parse it with Python's
# standard-library tomllib module.

id = "ISSUE-NNN"
type = "implementation" # implementation | experiment | investigation | decision | coordination | maintenance
title = ""
status = "candidate" # candidate | refined | ready | claimed | active | blocked | review | integrated | closed
priority = "P1" # P0 | P1 | P2 | P3
owner = ""
created = "YYYY-MM-DD"
updated = "YYYY-MM-DD"
parent = ""

depends_on = []
blocks = []
conflicts_with = []

# Fill these when the issue is claimed for parallel work.
worktree = ""
branch = ""
base_commit = ""

# Keep implementation and experiment worktrees bounded. Use repository-relative
# paths and explicit globs. Shared files should be listed in integration notes.
allowed_paths = []
forbidden_paths = []

# Use existing commands where possible. Mark manual or not-yet-existing checks
# in the Validation section below rather than inventing executable commands.
validation_commands = []
+++

# ISSUE-NNN — Title

## Objective

State the single outcome that should exist when this issue closes. Describe the
problem and desired result, not an uncommitted implementation assumption.

## Context

Explain why the issue matters and link the relevant requirements, architecture
documents, research, experiments, decisions, and related issues.

- Requirements:
- Architecture/research:
- Experiments:
- Decisions:
- Related issues:

## Deliverables

List the code, tests, documentation, measurements, or decisions that this
issue must produce.

-

## Acceptance criteria

Every criterion should be observable through a test, command, documented
experiment, review, or explicit project-owner decision.

- [ ]

## Scope

### In scope

-

### Out of scope

-

## Dependencies and parallelism

Explain why the declared dependencies and conflicts exist. If this issue can
run independently in a worktree, say what makes it independent. If it cannot,
state the prerequisite or shared boundary that prevents parallel execution.

## Worktree and file ownership

The front matter declares the intended exclusive file scope. Record any shared
files that may need coordinator review here.

### Shared files requiring integration review

-

### Read-only context

-

## Validation

### Automated commands

Commands from `validation_commands` are listed in the front matter. Add the
purpose of each command here and identify any focused tests or smoke tests.

### Manual or hardware checks

Describe manual, GUI, hardware, or measurement steps that cannot be captured
by an automated command.

### Evidence to record

State where logs, measurements, screenshots, experiment results, or decision
records belong. Do not commit private captures, credentials, or generated build
output.

## Integration contract

Describe what other workers may rely on after this issue is integrated. Note
API, schema, content, documentation, or migration changes that require a
coordinated integration pass.

If newly discovered work changes the objective, scope, dependencies, or
acceptance criteria, create a follow-up issue instead of silently expanding
this one.

## Progress log

Record short dated updates, blockers, and decisions. Keep implementation detail
in commits and the relevant code or experiment documents.

### YYYY-MM-DD — owner

-

## Completion report

Fill this section before moving the issue to `review`.

- Result:
- Commit(s):
- Changed files:
- Tests and validation run:
- Acceptance criteria not met:
- Follow-up issues:
- Known limitations:
- Integration notes:
