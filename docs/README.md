# MMORPG Architecture Dossier

This directory is the shared architecture and research record for the MMORPG project.

The project is a Linux-first hobby MMORPG inspired by the design of classic tab-targeted MMORPGs. It is intended to be technically capable of supporting a persistent player community, while remaining an original project with original content and assets.

## Document map

- [Requirements baseline](architecture/requirements.md) — current project goals and non-functional requirements.
- [System overview](architecture/system-overview.md) — proposed client, server, persistence, and tooling boundaries.
- [World and layering](architecture/world-and-layering.md) — persistent overworld, transparent dynamic layers, and explicit instances.
- [Networking](architecture/networking.md) — authoritative simulation, replication, interest management, and capacity scenarios.
- [Persistence](architecture/persistence.md) — durable state, transactions, snapshots, journals, and recovery.
- [UI scripting](architecture/ui-scripting.md) — player addons, the default UI, sandbox boundaries, and protected actions.
- [Development tools](architecture/development-tools.md) — terrain, tablet, particle, world, NPC, quest, and content workflows.
- [UI scripting spike](research/ui-scripting-spike.md) — runtime candidates, shared UI API, quotas, and anti-automation tests.
- [Research plan](research/README.md) — research tracks, deliverables, and coordination rules.
- [Rust server research](research/rust-server.md) — preliminary findings for Rust runtime, ownership, and region workers.
- [Networking research](research/networking-and-replication.md) — preliminary findings for transport and replication.
- [Overworld layering research](research/overworld-layering.md) — preliminary findings for hotspot scaling and transparent layers.
- [Open questions](open-questions.md) — unresolved decisions and questions requiring experiments or owner input.
- [Implementation roadmap](implementation-roadmap.md) — ordered vertical-slice implementation batches.
- [Shared content schema decision](decisions/ADR-003-shared-content-schema.md) — static definitions shared by tools, client, and server.
- [Client technology spike](research/client-technology-spike.md) — Linux/Rust runtime, rendering, input, asset, networking, and scripting candidates.
- [Editor technology spike](research/editor-technology-spike.md) — Linux editor, pen-tablet, terrain, particle, and source-data workflow candidates.
- [Client presentation model](../crates/mmorpg-client-model/README.md) — event/snapshot-only client state projection used by future rendering and UI adapters.
- [Client shell spike](../crates/mmorpg-client/README.md) — Bevy Linux window and starter-zone scene prototype.
- [Client protocol adapter](../crates/mmorpg-client-protocol/README.md) — typed helpers for the temporary development command line.
- [Content validation tool](../tools/README.md) — current starter-catalog validation command and future authoring-tool boundary.

## Status vocabulary

- **Requirement** — explicitly stated project behavior or constraint.
- **Proposed** — an architectural recommendation that has not yet been validated.
- **Experiment required** — documentation is insufficient; a prototype or benchmark is needed.
- **Accepted** — chosen for implementation.
- **Superseded** — replaced by a later decision.

The requirements baseline is authoritative for the current project goals. Architectural recommendations remain provisional until an Architecture Decision Record marks them as accepted.

## Working rules for future agents

1. Read `architecture/requirements.md` before researching or changing the architecture.
2. Record evidence and links in the relevant `research/` document.
3. Clearly label assumptions, inferences, recommendations, and open questions.
4. Do not silently turn a recommendation into an accepted decision.
5. Record benchmarks and prototypes in `experiments/`, separate from general research.
6. Avoid editing another agent's research document without coordinating the change.
