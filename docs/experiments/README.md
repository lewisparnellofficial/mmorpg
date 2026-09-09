# Experiments and Benchmarks

Measured results should be recorded here rather than mixed into general architecture prose.

The current coordinated run is described in [the local validation plan](validation-plan.md).

Current experiment records:

- [EXP-001: Rust fixed-tick region worker](EXP-001-rust-region-worker.md)
- [EXP-002: Replication workload model](EXP-002-replication-workload.md)
- [EXP-003: Transparent overworld layer manager](EXP-003-layer-manager.md)
- [EXP-004: Typed wire gameplay smoke test](EXP-004-wire-gameplay-smoke.md)
- [EXP-006: Luau UI scripting sandbox](EXP-006-ui-scripting-sandbox.md)
- [EXP-007: Qt Wayland tablet shell](EXP-007-qt-tablet-shell.md)
- [EXP-008: Authoritative enemy respawn generation](EXP-008-enemy-respawn.md)
- [EXP-009: Party ownership, eligibility, and generation loot](EXP-009-party-loot.md)
- [EXP-010: Minimum addressed delivery and interest filtering](EXP-010-addressed-delivery.md)
- [EXP-011: Graphical client runtime gate](EXP-011-graphical-client-gate.md)

## Required experiment categories

- 5,000 connected mostly idle clients.
- 200 active players in one overworld activity.
- World-boss combat with NPCs, effects, and replication.
- Layer creation and safe player migration.
- Worker crash during combat and reward processing.
- Duplicate network commands and transaction retries.
- UI addon CPU, memory, and event budgets.
- Pen-tablet terrain sculpting responsiveness.
- Particle-system runtime cost.

## Experiment record format

Every experiment should record:

- Objective.
- Hypothesis.
- Build and content version.
- Hardware and operating system.
- Configuration.
- Workload.
- Measurements.
- Result.
- Limitations.
- Follow-up work.
