# EXP-008: Authoritative enemy respawn generation

**Status:** Initial core integration measured; broader AI lifecycle remains
provisional

## Objective

Move the first bounded enemy-lifecycle rule from the standalone AI experiment
into the authoritative world: defeated starter enemies must return on a fixed
simulation tick, reset their reward ownership, and publish an additive event
that typed clients can apply without replacing the world snapshot.

## Implementation

`mmorpg-core::Npc` now retains its spawn position, respawn deadline, and
spawn-generation counter. A defeated enemy schedules a 100-tick respawn. The
fixed-tick world owner restores health and position, increments the generation,
clears the prior reward claimant/claimed flag, and emits
`Event::EnemyRespawned`. The event is carried as typed wire opcode 19 and is
applied by the client model as an authoritative health/defeat transition.

This is intentionally narrower than the standalone AI experiment: patrol,
aggro, threat, enemy attacks, leash state, corpse expiry, and deterministic
loot selection across multiple eligible players are not yet integrated.

## Measured local result

The root workspace core suite completed with **23 passed, 0 failed**, including
`defeated_enemy_respawns_on_a_fixed_tick_and_advances_generation`. Workspace
format/check/test validation and the aggregate validation script remain the
authoritative regression gates.

## Evidence classification

- **Measured local result:** deterministic core test and workspace/aggregate
  validation output.
- **Implementation evidence:** fixed-tick respawn deadline, generation reset,
  reward reset, typed opcode, server mapping, and client-model application.
- **Project inference:** keeping the entity ID stable while advancing a spawn
  generation is a viable boundary for later threat/loot lifecycle work.
- **Remaining uncertainty:** no real AI navigation, enemy attack cadence,
  multi-player threat, corpse retention, or graphical respawn run has been
  measured.
