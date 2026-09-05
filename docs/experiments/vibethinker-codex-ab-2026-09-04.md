# VibeThinker/Codex A/B trial — 2026-09-04

## Result

The first valid matched trial did **not** demonstrate a token saving. The
direct Codex condition used fewer Codex tokens and less wall time, and it
produced the semantically complete result. The VT-assisted condition produced
compiling tests, but one test did not actually exercise the required reverse
argument order, so it is rejected under the ticket's acceptance criteria.

This is one trial, not a general performance estimate. It is nevertheless a
useful negative result: compilation and passing tests were insufficient to
prove that the delegated artifact implemented the requested behavior.

## Ticket

Both conditions received the same bounded ticket:

> Add exactly three focused Rust unit tests to the existing test module in
> `crates/mmorpg-core/src/lib.rs`, using the existing private
> `Position::distance_squared` method. Cover identical points returning `0.0`,
> `(0,0)` to `(3,4)` returning `25.0`, and symmetry when the arguments are
> reversed. Do not change production methods. Keep the change confined to one
> file. Run formatting and `cargo test -p mmorpg-core`. Do not commit.

The starting repository snapshot was the same for both valid conditions. Each
agent worked in a disposable temporary Git copy. The Codex model and
reasoning setting were the same (`gpt-5.6-luna`, low reasoning effort).

## Matched results

The Codex `turn.completed` usage event reports `output_tokens` with reasoning
as a detail of that output. Therefore the comparable total here is
`input_tokens + output_tokens`; reasoning is reported separately and is not
added a second time.

| Metric | Direct Codex | Codex + VT | Difference |
| --- | ---: | ---: | ---: |
| Codex input tokens | 137,044 | 171,032 | +33,988 |
| Codex cached input | 113,152 | 140,544 | +27,392 |
| Codex output tokens | 1,050 | 1,334 | +284 |
| Codex reasoning-output detail | 69 | 251 | +182 |
| Codex comparable total | **138,094** | **172,366** | **+34,272 / +24.8%** |
| Codex wall time | 33.3s | 44.5s | +11.2s / +33.6% |
| VT prompt tokens | 0 | 394 | +394 local tokens |
| VT completion tokens | 0 | 743 | +743 local tokens |
| VT total tokens | 0 | 1,137 | +1,137 local tokens |
| VT generation time | 0s | 6.2s | +6.2s |
| Files changed | 1 | 1 | equal |
| Compiler/tests | pass | pass | equal |
| Semantic acceptance | **pass** | **reject** | direct wins |

The direct result added the three requested tests and did not change
production code. The VT-assisted artifact added three tests, but its symmetry
test compared a forward call with the expected constant `25.0`; it never
called `distance_squared` with the arguments reversed. The delegated Codex
agent identified this defect in its final report but did not repair it because
the trial explicitly prohibited fallback implementation.

The delegated VT call itself succeeded on its first attempt:

```text
prompt_tokens:     394
completion_tokens: 743
total_tokens:      1137
elapsed_ms:        6161
```

The runner's compile/test validator accepted the artifact because the code was
syntactically valid and the assertions passed. This establishes a concrete
limitation of the current validator: it can enforce shape, scope, formatting,
and executable checks, but it cannot prove that a test asserts the intended
behavior.

## Excluded setup runs

Two runs are excluded from the matched table but retained as engineering
evidence:

1. The first direct prompt accidentally requested a nonexistent
   `Position::distance` method. Codex added that production helper, which was
   a reasonable response to an invalid ticket but not a comparable trial. It
   used 168,803 input tokens and 1,604 output tokens and took 44.2s.
2. The first VT-assisted launch used Codex's `workspace-write` sandbox. The
   sandbox could not reach the explicitly local llama server at
   `127.0.0.1:8081`, so `vt-task` retried twice and stopped before making a VT
   call. That Codex run used 36,722 input tokens and 403 output tokens and
   took 15.1s. The valid assisted run used a network-enabled disposable
   environment.

The second result is operationally important: a production workflow must
allow the Codex process to reach the local VT endpoint, or delegation adds
failure and retry overhead without doing any useful work.

## Interpretation

This trial does not support the claim that “Codex plus VT” is automatically
cheaper in Codex tokens. The assisted agent still spent most of its Codex
budget inspecting the repository, interpreting the ticket, invoking the
runner, applying the patch, and reviewing the result. VT replaced only the
small artifact-generation step.

The assisted condition would need to save more than the orchestration overhead
to win. It also needs a semantic acceptance check strong enough to reject the
kind of false-positive test produced here. A compiler and passing unit tests
are necessary checks, but they are not sufficient when the generated test can
assert the wrong thing.

The result is more encouraging for a different design: have Codex use VT for
literal transformations where a checker can compare exact output, such as a
known source-to-source rewrite or a generated serialization table. For
behavioral test generation, the checker needs explicit structural or semantic
assertion checks in addition to compilation.

## Next experiment

The next useful comparison is not a second arbitrary ticket. It is a small
matrix of at least five tickets in each category:

- exact source-to-source transformations;
- repetitive tests with machine-checkable expected calls;
- tests requiring API or behavior inference.

For each ticket, run a fresh direct and assisted thread, record acceptance,
Codex delta, VT cost, wall time, retries, and human intervention. Report the
median and the rejected-result rate. The hypothesis is that VT may save Codex
tokens only in the first category and perhaps some of the second; this trial
rejects the stronger hypothesis that a generic bounded unit-test ticket is
already a reliable saving.
