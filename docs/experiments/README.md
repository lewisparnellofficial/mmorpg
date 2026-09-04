# Experiments and Benchmarks

Measured results should be recorded here rather than mixed into general architecture prose.

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
