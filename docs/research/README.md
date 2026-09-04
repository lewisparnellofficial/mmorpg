# Research Program

**Status:** In progress — first three tracks started 2026-09-04

## Purpose

Research should reduce architectural uncertainty and identify experiments that must be run locally. It should not silently make irreversible project decisions.

## Suggested research tracks

### 1. Rust server and simulation

Investigate fixed-tick simulation, actor or region-worker models, async boundaries, ECS options, process separation, and Linux deployment.

Primary output: a proposed server runtime model and a small region-worker prototype.

### 2. Networking and replication

Investigate transports, reliable/unreliable delivery, snapshots, delta compression, interest management, client prediction, and 200-player encounter replication.

Primary output: a protocol comparison and a simulated-client benchmark plan.

### 3. Transparent overworld layering

Investigate dynamic sharding/layering, group cohesion, safe handoff, hotspot detection, layer retirement, and world-boss event policies.

Primary output: a layer-assignment design and state-ownership model.

### 4. Persistence and recovery

Investigate relational schemas, operation journals, snapshots, idempotent transactions, migrations, and worker recovery.

Primary output: a persistence design and failure-test matrix.

### 5. Client engine and Linux runtime

Investigate client engines, Linux graphics and input support, asset streaming, UI embedding, editor reuse, and licensing.

Primary output: an engine shortlist and a vertical-slice prototype recommendation.

### 6. Editor, terrain, and tablet workflow

Investigate Linux desktop/editor frameworks, pen-tablet APIs, heightmap storage, tiled terrain, terrain painting, asset placement, and particle authoring.

Primary output: an editor architecture and tablet interaction prototype.

### 7. UI scripting sandbox

Investigate embeddable runtimes, resource quotas, API capability boundaries, protected actions, addon packaging, and sandbox testing.

Primary output: a capability matrix and adversarial sandbox test plan.

### 8. Testing and operations

Investigate simulated clients, load generation, observability, crash dumps, replay, backups, staging, and multi-process deployment.

Primary output: capacity-test scenarios and an operational baseline.

## Current orchestration pass

The first pass is intentionally focused on the three highest-risk, tightly coupled questions:

- [Rust server and simulation](rust-server.md).
- [Networking and replication](networking-and-replication.md).
- [Transparent overworld layering](overworld-layering.md).

These tracks should be read together. The layer model affects the simulation ownership model, and both affect replication and load testing.

Current status:

| Track | Status | Current output |
|---|---|---|
| Rust server and simulation | Preliminary findings recorded | Tokio/service-I/O candidate; region ownership and message passing proposed |
| Networking and replication | Preliminary findings recorded | Reliable/unreliable delivery split; QUIC candidate; spatial replication required |
| Transparent overworld layering | Preliminary findings recorded | Dynamic hotspot partitioning supported in principle; event semantics still open |

No implementation technology has been marked accepted yet. The next step for all three tracks is a local prototype and benchmark.

## Research document format

Each research document should contain:

1. Question.
2. Scope and non-goals.
3. Current project constraints.
4. Candidate approaches.
5. Evidence and primary sources.
6. Comparison criteria.
7. Recommendation.
8. Confidence and assumptions.
9. Risks and rejected alternatives.
10. Prototype or benchmark required.
11. Open questions.

Research should distinguish directly sourced facts from inferences and project-specific recommendations.

## Coordination rules

- One owner per research document.
- Do not edit another track's document without coordination.
- Record sources rather than only conclusions.
- Keep architecture decisions in `docs/decisions/`.
- Keep measured results in `docs/experiments/`.
- Mark decisions as proposed until reviewed.
- Update `docs/open-questions.md` when research discovers a new dependency or unresolved tradeoff.
