# VibeThinker Bounded Task Catalog

**Status:** Prototype handoff format; experiment required

**Date:** 2026-09-04

This document records candidate tasks for delegating small implementation or
test-writing jobs to the locally served VibeThinker-3B model. The model output
is a proposal only. It must not be applied without an independent diff review
and the relevant project validation commands.

## Handoff format

Every task passed to VibeThinker should use the following structure:

```text
Role: Bounded Rust implementation assistant.

Repository context:
- Repository: /home/lewis/Projects/mmorpg
- Relevant file(s): ...
- Existing APIs and invariants: ...

Scope:
- Allowed output: ...
- Allowed files: ...
- Forbidden changes: no new dependencies, no public API changes, no unsafe
  Rust, and no edits outside the allowed files.

Task:
...

Acceptance cases:
- Given ..., expect ...
- Given ..., expect ...

Required response:
- Return only a unified diff / Rust test code / function body.
- Do not include Markdown fences or explanatory prose.
- Do not claim tests passed; the caller will run them.
```

The caller should keep the task narrow enough that a reviewer can determine
correctness from the prompt, the relevant source, and automated checks. The
caller should request code or a diff, not ask VibeThinker to inspect and edit
the entire repository.

## Acceptance procedure

For each returned implementation:

1. Save the response without applying it to the working tree.
2. Separate the answer from any `<think>...</think>` section.
3. Reject an unterminated thinking block, malformed output, or unrequested
   file path.
4. Review the proposed code against the task's invariants.
5. Require declared `baseline_checks` to fail before spending a VT request.
6. Apply only in a temporary worktree or temporary copy when testing.
7. Run focused formatting, compilation, and tests.
8. For generated tests, apply compiling known-bad mutations and require the
   generated test to kill them.
9. Record the result as accepted, rejected, or requiring revision.

The VibeThinker model card cautions that the model was not trained for
tool-calling or autonomous coding agents. These tasks therefore deliberately
ask for bounded source output and leave repository operations to the caller.

## Candidate task 1: inventory stacking tests

**Why this is a good task:** The behavior is local, deterministic, and
directly testable. It exercises integer capacity reasoning without requiring
architectural context.

**Relevant code:** `Inventory::new`, `used_slots`, `quantity`, private
`can_add`, private `add`, `ItemDefinition`, and `ItemId` in
`crates/mmorpg-core/src/lib.rs`.

**Handoff prompt:**

```text
Role: Bounded Rust test author.

Repository context:
- Tests are inserted inside the existing #[cfg(test)] mod tests in
  crates/mmorpg-core/src/lib.rs, so private Inventory methods are visible.
- Inventory::new(capacity) creates a slot-limited inventory.
- Inventory::used_slots() returns the number of stacks.
- Inventory::quantity(item_id) returns the total quantity for an item.
- Inventory::can_add(definition, quantity) checks whether quantity fits,
  filling partial matching stacks before empty slots.
- Inventory::add(definition, quantity) fills matching partial stacks and then
  creates stacks no larger than definition.max_stack.

Scope:
- Return only compilable #[test] functions.
- Do not include use statements or invent fields or methods.
- Do not modify production code.

Task:
Write focused tests covering partial-stack filling, creation of a second
stack when quantity exceeds max_stack, used slot count, and rejection when the
inventory has insufficient capacity.

Acceptance:
- Construct a local ItemDefinition with a stable ItemId and max_stack 10.
- Use Inventory::new(2).
- Check the expected total quantity and stack count after adding 15 items.
- Check that an additional quantity that cannot fit is rejected.
```

**Observed VibeThinker result:** Rejected. The response invented an incorrect
API: it called `Inventory::can_add(15)` without the required item definition,
used `inventory.add(15)` without the required definition, and compared methods
as fields. This is a useful negative result: even a small task needs exact
signatures in the context packet, and returned code cannot be trusted merely
because the requested behavior is simple.

## Candidate task 2: position distance tests

**Why this is a good task:** It is a three-case deterministic mathematical
test with no repository-wide design choices.

**Relevant code:** `Position::new(x, y)` and private
`Position::distance_squared(other)` in `crates/mmorpg-core/src/lib.rs`.

**Handoff prompt:**

```text
Role: Bounded Rust test author.

Context:
- The response is inserted inside the existing tests module in
  crates/mmorpg-core/src/lib.rs, so Position and its private methods are in
  scope.
- Position::new(x: f32, y: f32) constructs a position.
- Position::distance_squared(self, other: Position) -> f32 returns squared
  Euclidean distance.

Scope:
- Return only three #[test] functions.
- Do not include imports, Markdown fences, or explanation.
- Do not invent APIs.

Task and acceptance cases:
- The distance from a point to itself is 0.0.
- The distance from (0.0, 0.0) to (3.0, 4.0) is 25.0.
- Distance is symmetric.
- Use assert!((actual - expected).abs() < f32::EPSILON).
```

**Observed VibeThinker result:** Not accepted as returned. The model spent a
large amount of output on meta-reasoning and proposed imports and APIs that
were not requested before producing code. A later constrained response was
not captured as a clean final result. The task itself remains a good candidate,
but should use a larger token budget and `--format json`; the caller should
reject the response if the final answer is not cleanly delimited.

## Candidate task 3: development command parser tests

**Why this is a good task:** The parser has a finite line-oriented grammar and
can be tested with exact input/output examples. It also tests the boundary
between user input and authoritative simulation commands.

**Relevant code:** The parser and command conversion functions in
`crates/mmorpg-server/src/main.rs`.

**Handoff prompt:**

```text
Role: Bounded Rust test author.

Repository context:
- The development server accepts line-oriented commands documented in
  crates/mmorpg-server/README.md.
- Relevant command forms include connect <name> <tank|healer|damage>,
  move <dx> <dy>, target <entity-id>, attack, state, snapshot, help, and quit.
- Read the supplied parser source before writing tests; use its actual types,
  visibility, and error behavior.

Scope:
- Modify only the existing parser test module in
  crates/mmorpg-server/src/main.rs.
- Return only #[test] functions or a unified diff for that test module.
- Do not change parser behavior, public APIs, or dependencies.

Task:
Add tests for valid connect parsing, valid movement parsing, malformed numeric
arguments, unknown commands, and the distinction between state/snapshot and
mutating commands.

Acceptance:
- Assert exact parsed values or exact error variants/messages based on the
  supplied source.
- Do not assume a type or helper that is absent from the source.
- The tests must pass with cargo test -p mmorpg-server.
```

**Status:** Recommended next experiment after providing the parser source
excerpt. It is bounded, but the model must see the actual parser definitions;
the README alone is insufficient to write compiling tests.

## Candidate task 4: content-catalog validation test

**Why this is a good task:** The content catalog has stable IDs and validation
rules, making it suitable for a test that verifies a single invalid reference
or duplicate definition.

**Handoff prompt:**

```text
Role: Bounded Rust test author.

Context:
- Read the content catalog definitions and existing tests supplied below.
- Tests must use the catalog's actual constructors and error types.

Scope:
- Modify only the existing test module in the supplied content-catalog file.
- Return only compilable test code.
- Do not alter catalog behavior, schemas, dependencies, or public APIs.

Task:
Add one focused regression test for the specified validation rule: <one exact
rule from the supplied source>.

Acceptance:
- The invalid fixture is minimal.
- The test asserts the relevant failure rather than only asserting that some
  error occurred.
- The valid neighboring fixture remains accepted if the API permits it.
```

**Status:** Good follow-up task once the content crate's exact source and
existing test conventions have been included in the context packet.

## Prototype conclusion

The first experiment suggests that VibeThinker should initially receive
complete signatures, exact insertion location, and explicit output limits.
Descriptions such as “Inventory has can_add and add” are not sufficient when
the argument lists matter. Test-writing is a promising use case because Rust's
compiler and test runner provide an external acceptance oracle, but generated
tests still require review for vacuous assertions, invented APIs, and tests
that encode the wrong behavior.

The recommended first production-like loop is:

```text
agent selects bounded task
  -> agent includes exact source excerpt and acceptance cases
  -> vt --format json --max-tokens 4096
  -> parser extracts answer
  -> reviewer checks allowed scope
  -> temporary compile/test run
  -> record outcome
```

No VibeThinker output recorded here has been applied to the repository.

## Second attempt batch

The following attempts used `./scripts/vt --format json`, an explicit
insertion location, exact signatures, and a 4096-token budget.

### Server parser negative-case test

This attempt succeeded and returned a compilable-looking test without
Markdown or explanatory prose:

```rust
#[test]
fn parser_rejects_unknown_and_malformed_commands() {
    assert_eq!(super::parse_line("bogus", None), Err("unknown command 'bogus'; try help"));
    assert_eq!(super::parse_line("move nope 1", Some(EntityId(7))), Err("entity-id must be an integer"));
    assert_eq!(super::parse_line("buy 1 2 0", Some(EntityId(7))), Err("quantity must be a positive integer"));
}
```

This was not applied or compiled. Before acceptance, it still needs a check
that the test's `super::parse_line` path is appropriate from the existing test
module and that the exact `Result` type permits the string literals through
type inference. The task is a strong candidate because every expected failure
is specified by the implementation and can be checked mechanically.

### Role alias test

The second attempt asked for a single test covering case-insensitive role
aliases and rejection of `mage`. VibeThinker did not return a usable answer
within the command's timeout/output window. This is a useful operational
result: even tiny tasks can spend too long in reasoning, so the caller needs a
timeout, retry policy, and a bounded maximum output budget. It should not wait
indefinitely or treat an empty response as success.

### Position distance test

The earlier position test remains unresolved. VibeThinker understood the
mathematical cases but spent substantial output discussing imports, visibility,
and hypothetical APIs. The improved prompt clarified that the test is inside
the existing tests module, but the response was not captured as a clean final
answer. We should either provide the exact surrounding test-module context or
defer this task until the driver supports retries and response-length
diagnostics.

## Updated recommendation

The server parser negative-case test is currently the best exemplary task for
the prototype. It has:

- A single file and a single test.
- Exact input strings and expected errors.
- No production behavior change.
- No new dependency or API decision.
- A cheap external acceptance check.

The next driver improvement should be a retry-on-empty or retry-on-timeout
policy, with each retry recorded separately. The caller should also record
prompt tokens, completion tokens, elapsed time, and whether the answer passed
basic syntax/scope checks. A model response that looks correct remains a
candidate until the Rust toolchain verifies it.

## Codex feature-plan handoff

The retry and validation primitives are now available through
`scripts/vt-orchestrate`. A Codex agent can represent a higher-level ticket as
a versioned bundle under `experiments/vibethinker/bundles/`. Each task pairs a
bounded description with an existing `vt-task` manifest and declares its
dependencies, target files, context commands, output kind, executable
acceptance checks, and semantic constraints. The orchestrator requires the
machine-checkable declarations to match the manifest before it starts any
VT invocation.

The recommended handoff sequence is:

```text
Codex reads the ticket and repository
  -> Codex writes a feature bundle and leaf manifests
  -> vt-orchestrate --validate-only
  -> vt-orchestrate dispatches independent leaves concurrently
  -> vt-task validates each artifact in a temporary copy
  -> vt-orchestrate integrates accepted artifacts in dependency order
  -> dependent leaves read the cumulative integration state
  -> feature_checks validate the combined candidate
  -> Codex reviews the single feature patch, diagnostics, and run_usage
```

The orchestrator owns a temporary integration repository whose baseline is a
snapshot of the invoking working tree. It passes that repository to `vt-task`
through `--source-root`, applies each accepted leaf artifact there, and emits
one final patch without editing the real working tree. Concurrent leaves must
have disjoint `target_files`; overlapping ownership is allowed only when a
dependency path serializes those tasks. Dependency cycles and unordered target
overlaps are rejected before a VT request is made.

Top-level `acceptance_checks` remain human-readable requirements. Top-level
`feature_checks` are executable commands and run only after every leaf is
integrated. Passing leaf checks therefore does not by itself make a feature
successful; the combined candidate must also pass its feature checks.

The plan's `semantic_constraints` are included in each VT prompt through a
per-task normalized `plan.json`, but they are not a substitute for a semantic
oracle. The model can still produce a compiling test that asserts the wrong
thing, as demonstrated by the A/B record. This infrastructure therefore
supports cumulative decomposition, integration, and cheap rejection; it does
not make VibeThinker an autonomous feature implementer or prove requirements
that are absent from executable feature checks.

Each semantic leaf needs its own oracle. A live decomposed position bundle
initially protected only the symmetry leaf; another leaf compiled with a
tautological assertion and passed the combined suite. Separate Manhattan,
constant-offset, and asymmetric mutants were needed to verify the 3-4-5,
same-point, and symmetry requirements respectively. One strong oracle on a
neighboring task does not confer semantic coverage on the whole feature.

Retries now include bounded validator feedback: the original prompt is
repeated with the failed stage and the final 20 diagnostic lines. Live compiler
errors in same-point and symmetry tests were repaired on the second attempt.
The retry is still subject to the same attempt cap and output contract, and
every attempt remains visible in `run_usage` and the task artifact directory.

Those repairs were not stable across immediate reruns: later attempts repeated
a malformed assertion or invented an extra method argument. The canonical
position leaves consequently provide complete test skeletons and use VT in the
exact-copy mode that passed the earlier controlled trials. This is deliberately
conservative. A planner should expand VT's freedom only when a task family has
repeatedly passed its independent oracle, not merely because one sample worked.
