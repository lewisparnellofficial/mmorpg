# EXP-008: Authoritative enemy respawn generation

**Status:** Initial core integration measured; threat/leash slice integrated;
broader AI lifecycle remains provisional

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

The follow-on slice adds a server-owned per-generation threat table. Damage
creates threat, healing creates nearby threat, and a threatened enemy enters
an engaged state, moves at a fixed rate, attacks on a fixed cooldown, and
returns to its spawn when its target is defeated or exceeds the leash. Enemy
damage and player defeat are carried as typed opcodes 20 and 21.

This is intentionally narrower than the standalone AI experiment: patrol
waypoints, explicit taunt, corpse expiry, and
deterministic loot selection across multiple eligible players are not yet
integrated.

## Measured local result

The root workspace core suite completed with **25 passed, 0 failed**, including
`defeated_enemy_respawns_on_a_fixed_tick_and_advances_generation` and
`enemy_threat_drives_attacks_and_leash_return`. Workspace
format/check/test validation and the aggregate validation script remain the
authoritative regression gates.

## Evidence classification

- **Measured local result:** deterministic core test and workspace/aggregate
  validation output.
- **Implementation evidence:** fixed-tick respawn deadline, generation reset,
  reward reset, threat/leash/attack state, typed opcodes, server mappings, and
  client-model application.
- **Project inference:** keeping the entity ID stable while advancing a spawn
  generation is a viable boundary for later threat/loot lifecycle work.
- **Remaining uncertainty:** no proximity patrol/aggro, taunt semantics,
  multi-player loot, corpse retention, or graphical respawn run has been
  measured.
