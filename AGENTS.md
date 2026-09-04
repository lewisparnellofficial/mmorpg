# MMORPG Project Agent Guide

This file is the working agreement for agents and contributors operating in
this repository.

## Table of contents

- [Project orientation](#project-orientation)
- [Repository layout](#repository-layout)
- [Required workflow](#required-workflow)
- [Subagent and API cost guardrails](#subagent-and-api-cost-guardrails)
- [Commands](#commands)
- [Development server](#development-server)
- [Experiments and research](#experiments-and-research)
- [Code and documentation rules](#code-and-documentation-rules)
- [Git and commit rules](#git-and-commit-rules)
- [Validation checklist](#validation-checklist)

## Project orientation

This is a Linux-first hobby MMORPG project inspired by classic real-time,
tab-targeted MMORPGs. The long-term target is one logical persistent realm
with approximately 5,000 connected clients and approximately 200 players able
to participate in one shared overworld activity on one layer.

The current implementation is intentionally small:

- `mmorpg-core` is an engine-independent authoritative simulation library.
- `mmorpg-server` is a Linux headless development server using a temporary
  line-oriented TCP protocol.
- `experiments/` contains local validation prototypes and workload models.
- `docs/` contains requirements, architecture, research, decisions, and
  experiment records.

Read [`docs/architecture/requirements.md`](docs/architecture/requirements.md)
before making architectural or gameplay assumptions. Read the relevant
research and experiment documents before changing a proposed boundary.

## Repository layout

```text
AGENTS.md                         This contribution guide
Cargo.toml                        Rust workspace manifest
crates/mmorpg-core/               Authoritative simulation primitives
crates/mmorpg-server/             Linux headless development server
experiments/                      Benchmarks and behavioral prototypes
docs/                             Architecture and research dossier
  architecture/                   Requirements and proposed architecture
  decisions/                      Architecture Decision Records
  experiments/                    Experiment plans and measured results
  research/                       Research tracks and source-backed findings
```

## Required workflow

1. Inspect the working tree before editing with `git status --short --branch`.
2. Preserve unrelated user changes.
3. Read the relevant requirements and architecture documents.
4. Keep production code, experiments, and documentation in their designated
   locations.
5. Prefer small, reviewable changes with focused tests.
6. Run formatting, tests, and relevant smoke tests before committing.
7. Update documentation when behavior, architecture, commands, or decisions
   change.
8. Record measured benchmark results separately from estimates or research
   conclusions.
9. Use a Conventional Commit message for every commit.

## Subagent and API cost guardrails

This project is Codex-only for delegated work. These are hard constraints,
not suggestions:

- Never invoke Pi, Paseo-hosted non-Codex agents, or an unknown agent provider
  for this repository.
- If delegation is useful, use only the Codex subagent mechanism exposed by
  the current session (`multi_agent_v1__spawn_agent`) and leave the model
  override unset unless the user explicitly chooses a Codex model.
- Do not use generic agent-creation or scheduling integrations for project
  work. In particular, do not call `mcp__paseo__create_agent`,
  `mcp__paseo__create_schedule`, or any provider whose identity is not
  explicitly Codex.
- Before delegating, state the selected Codex mechanism and bounded write
  scope in the working update. If the provider cannot be verified as Codex,
  do the work locally instead.
- Do not spend external API tokens on parallel research unless the user has
  asked for that specific delegation. Prefer local inspection and validation
  when they are sufficient.

The repository cannot revoke tools supplied by the host application, so these
rules are reinforced by process: use the explicit Codex-only tool allowlist
above, never infer a provider from a nickname, and stop rather than guessing
when a delegation surface is ambiguous.

## Commands

Run these commands from the repository root.

### Rust formatting and tests

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

### Run the development server

```bash
cargo run -p mmorpg-server
cargo run -p mmorpg-server -- 127.0.0.1:4400
```

Connect to the default server from another terminal:

```bash
nc 127.0.0.1 4000
```

The temporary development protocol supports:

```text
connect <name> <tank|healer|damage>
move <dx> <dy>
move <player-id> <dx> <dy>
target <entity-id>
attack
state
snapshot
help
quit
```

The explicit player ID form of `move` is restricted to the player bound to the
connection. The protocol is for local development and is not suitable for
internet deployment.

### Run validation experiments

Rust region-worker benchmark:

```bash
cargo run --manifest-path experiments/rust-region-bench/Cargo.toml --release -- \
  --players 200 \
  --npcs 1000 \
  --ticks 1000 \
  --warmup 100 \
  --commands-per-tick 800 \
  --command-budget 800
```

Layer-manager simulation:

```bash
cargo fmt --manifest-path experiments/layer-manager/Cargo.toml -- --check
cargo test --manifest-path experiments/layer-manager/Cargo.toml
cargo run --manifest-path experiments/layer-manager/Cargo.toml --release --quiet
```

Replication workload model:

```bash
python3 -m py_compile experiments/replication-model/model.py
python3 experiments/replication-model/model.py \
  --players 200 \
  --seconds 1 \
  --tick-hz 20 \
  --budget-kib 64
```

The experiment README files and records under `docs/experiments/` define the
meaning and limitations of each result. Do not present a synthetic benchmark
as proof of production capacity.

Content catalog smoke check:

```bash
cargo fmt --manifest-path tools/mmorpg-content-check/Cargo.toml -- --check
cargo test --manifest-path tools/mmorpg-content-check/Cargo.toml
cargo run --quiet --manifest-path tools/mmorpg-content-check/Cargo.toml
```

Client and editor technology spikes:

```bash
cargo check --manifest-path crates/mmorpg-client/Cargo.toml
cargo run --manifest-path crates/mmorpg-client/Cargo.toml
cargo test --manifest-path crates/mmorpg-client-protocol/Cargo.toml
cargo test --manifest-path crates/mmorpg-client-transport/Cargo.toml
cargo test --manifest-path crates/mmorpg-wire/Cargo.toml
cargo test --manifest-path experiments/ai-respawn/Cargo.toml
cargo run --quiet --manifest-path experiments/ai-respawn/Cargo.toml
cargo test --manifest-path tools/mmorpg-editor-core/Cargo.toml
cargo test --manifest-path experiments/client-presentation-replay/Cargo.toml
cargo run --quiet --manifest-path experiments/client-presentation-replay/Cargo.toml
```

The Bevy client requires a Linux desktop session and a working graphics stack.
The protocol, editor-core, and replay crates are standalone technology spikes
and are intentionally not part of the root workspace yet.

Convenience launch commands from the repository root:

```bash
./scripts/run-server.sh [127.0.0.1:4000]
./scripts/run-client.sh
./scripts/run-editor.sh [--output /tmp/starter-terrain.mmterrain]
```

The editor launcher currently runs the editor-core CLI spike and does not open
a desktop window. The client launcher requires a Linux desktop session and
working graphics stack.

## Development server

The initial server is a development process, not a production MMO server. It
currently has no authentication, durable persistence, binary protocol,
interest-managed replication, client prediction, layer manager, or instance
manager.

The authoritative simulation lives in `mmorpg-core`. Network parsing and
socket management belong in `mmorpg-server` or a future network adapter; do
not put socket, database, or rendering dependencies into the core merely to
make a feature convenient.

Gameplay rules should be server-authoritative. Clients and future addons may
submit intent, but they must not determine movement, combat results, rewards,
inventory, quest completion, or currency changes.

## Experiments and research

Research and experiments are part of the architecture process, not disposable
notes.

- Requirements belong in `docs/architecture/requirements.md`.
- Proposed system boundaries belong in `docs/architecture/`.
- Source-backed investigation belongs in `docs/research/`.
- Architectural choices belong in `docs/decisions/` as ADRs.
- Measurements and prototypes belong in `docs/experiments/`.
- Unresolved questions belong in `docs/open-questions.md`.

Every research or experiment record should distinguish:

- Directly sourced facts.
- Project-specific inferences.
- Measured local results.
- Modeled estimates.
- Recommendations.
- Remaining uncertainty.

When parallel agents are used, give each agent a disjoint write scope and a
bounded deliverable. A synthesis pass must review conflicting findings before
an architecture decision is accepted.

## Code and documentation rules

- Use `apply_patch` for repository file edits.
- Do not commit generated build output, `target/`, Python caches, logs, or
  local credentials.
- Keep unsafe Rust disabled; do not introduce `unsafe` without an explicit
  architecture decision and review.
- Keep simulation ownership explicit. One region/layer owner should mutate a
  live entity at a time.
- Keep blocking I/O out of the simulation-critical path.
- Prefer stable IDs over names for content and persistent references.
- Keep static content definitions separate from mutable player/world state.
- Add focused tests for authority, validation, transactions, ownership, and
  failure behavior.
- Mark proposals as proposed until they are validated and accepted in an ADR.
- Update command examples whenever CLI behavior changes.

## Git and commit rules

All commits must use [Conventional Commits](https://www.conventionalcommits.org/).

Use the form:

```text
<type>(<optional scope>): <imperative description>
```

Examples:

```text
feat(server): add authoritative movement commands
fix(core): reject attacks against vendors
docs(architecture): record layer handoff policy
test(simulation): cover starter-zone target validation
refactor(protocol): isolate development command parsing
perf(replication): cache spatial interest candidates
chore: update Rust workspace metadata
```

Common types:

- `feat` — new user-visible or architectural capability.
- `fix` — bug fix.
- `docs` — documentation-only change.
- `test` — tests or validation experiments.
- `refactor` — behavior-preserving code restructuring.
- `perf` — performance improvement.
- `chore` — maintenance, tooling, or repository housekeeping.
- `build` — build-system or dependency changes.
- `ci` — continuous-integration changes.

Use an imperative subject, keep it concise, and avoid vague messages such as
`changes`, `work`, or `updates`. Do not combine unrelated concerns in one
commit when they can be reviewed independently.

## Validation checklist

Before handing off work:

- [ ] `git status --short --branch` was checked before editing.
- [ ] Requirements and relevant architecture documents were read.
- [ ] Existing unrelated changes were preserved.
- [ ] `cargo fmt --all -- --check` passes for Rust changes.
- [ ] `cargo test --workspace` passes for Rust changes.
- [ ] Relevant experiment or runtime smoke tests were run.
- [ ] `git diff --check` passes.
- [ ] Documentation and open questions were updated where needed.
- [ ] Generated files are ignored and not staged.
- [ ] Commit message follows Conventional Commits.
- [ ] Final handoff names changed files, tests run, limitations, and commit ID.
