# MMORPG First Vertical Slice Execution Plan

**Status:** Synthesized planning baseline; implementation decisions marked
provisional still require the listed ADRs.

**Scope:** The path from the current technical prototype to a repeatable,
three-role, recognizably MMORPG-like town-and-field loop.

**Relationship to the roadmap:** This document is the detailed execution plan
for Batches 4–6 of `docs/implementation-roadmap.md`, followed by selected Batch
8 client/tooling outcomes. Batch 7 instances and layers remain deferred until
the first vertical slice and its replication foundation exist. This plan does
not replace the long-term roadmap or the accepted requirements. When a milestone
changes a roadmap status or an architectural boundary, update the roadmap, open
questions, and relevant architecture documents in the same change.

**Vertical-slice target:** Three distinct characters enter one shared outdoor
zone, form a party, complete a server-authoritative tank/healer/damage encounter,
receive party-correct credit and non-duplicated rewards, return to town to use a
vendor and complete a quest, reconnect or restart without corrupting durable
progress, and repeat the encounter after the enemy respawns.

## Table of contents

- [How to read this plan](#how-to-read-this-plan)
- [Current baseline and constraints](#current-baseline-and-constraints)
- [Execution principles](#execution-principles)
- [Decisions required before gameplay implementation](#decisions-required-before-gameplay-implementation)
- [Milestone sequence](#milestone-sequence)
  - [0. Planning and validation foundation](#0-planning-and-validation-foundation)
  - [1. One authoritative timed simulation path](#1-one-authoritative-timed-simulation-path)
  - [2. Canonical typed protocol and client path](#2-canonical-typed-protocol-and-client-path)
  - [3. Multi-character identity and session fencing](#3-multi-character-identity-and-session-fencing)
  - [4. Addressed delivery and minimum interest filtering](#4-addressed-delivery-and-minimum-interest-filtering)
  - [5. Player combat state and role abilities](#5-player-combat-state-and-role-abilities)
  - [6. Threat-driven enemy lifecycle](#6-threat-driven-enemy-lifecycle)
  - [7. Party rules, credit, and loot](#7-party-rules-credit-and-loot)
  - [8. Persistence safety before the multiplayer gate](#8-persistence-safety-before-the-multiplayer-gate)
  - [9. Graphical combat usability](#9-graphical-combat-usability)
  - [10. Three-client vertical-slice gate](#10-three-client-vertical-slice-gate)
  - [11. Replication and load foundation](#11-replication-and-load-foundation)
  - [12. Production-oriented durable repository](#12-production-oriented-durable-repository)
  - [13. Development content workflow](#13-development-content-workflow)
  - [14. UI scripting and sandboxing](#14-ui-scripting-and-sandboxing)
- [Dependency graph](#dependency-graph)
- [Definition of done](#definition-of-done)
- [Explicitly deferred work](#explicitly-deferred-work)

## How to read this plan

Each milestone has four parts:

- **Outcome** describes the runnable capability left behind.
- **Work** describes the smallest reviewable implementation boundary.
- **Required records** names decisions or documentation that must land with the
  implementation.
- **Exit tests** are falsifiable gates. The exact Rust test-module path can
  change, but the behavior and recognizable test name should remain.

Milestones are ordered by correctness dependencies, not by UI visibility. A
later milestone must not be pulled forward if that would require global event
broadcast, shared-character sessions, blocking persistence in the simulation
step, or client-owned gameplay outcomes.

## Current baseline and constraints

The repository already provides:

- An engine-independent authoritative Rust simulation core and Linux headless
  development server.
- A starter town, field, vendor, enemies, quest, gold, stack-aware inventory,
  combat, exactly-once claims in the current single-spawn model, and a Linux
  Bevy client shell.
- A versioned, bounded typed wire envelope; loopback-only development
  authentication; explicit character selection; bootstrap snapshots; reconnect
  behavior; and an opt-in local checkpoint prototype.
- A renderer-independent presentation model and standalone client protocol,
  adapter, and transport crates.
- A fixed-tick combat-timing path in the core and a separate deterministic
  patrol/aggro/leash/death/respawn experiment.
- Content validation, terrain-editing, presentation-replay, replication-model,
  and layer-management experiments.

The next work must account for these current constraints:

- The running server still calls the untimed `World::step` path, while cast and
  cooldown behavior lives in a second step path.
- Wire gameplay events are broadcast to every connected wire client rather
  than delivered to an explicit audience.
- The development account catalog exposes one hard-coded character, and there
  is no one-session-per-character fence.
- Join-to-connection correlation depends on the order of successful join
  events rather than an explicit request identity.
- Movement is bounded per command but not by an authoritative per-tick movement
  budget; the server drains an unbounded command queue each tick.
- Local checkpoint writes are synchronous and occur on the simulation thread.
- Checkpoints have a single-character path and a strict version-1 parser.
- Loot ownership and quest credit are currently based on first damage and the
  killing blow, which does not define fair party behavior.
- Respawning one entity requires reward identity beyond the entity ID, or a
  later spawn can incorrectly inherit or reopen an earlier claim.
- Several non-Bevy client crates and experiments are outside the root Cargo
  workspace, so `cargo test --workspace` is not a complete compatibility gate.
- The AI experiment is a design reference, not directly portable code: it uses
  different identifiers, positions, timing, and has no threat table.

These are first-order prerequisites for a real three-client test, not cleanup
to postpone until after it.

## Execution principles

1. **One simulation owner and one timed step.** The region/world owner alone
   mutates player combat, enemy AI, threat, parties, rewards, and timers. It
   advances on every scheduled tick, including ticks with no client commands.
2. **Clients submit intent.** A wire command never supplies authoritative
   damage, healing, threat, reward, movement distance per tick, party membership,
   or another connection's player identity.
3. **Audience is part of an event's contract.** Sensitive or local data is not
   globally broadcast and then hidden by the UI. Server-side delivery enforces
   player, party, and perception boundaries.
4. **Durable operations are explicit.** Loot, quest rewards, and purchases have
   stable operation identities and retry semantics. Movement and transient
   combat state are not persisted every frame.
5. **Blocking I/O stays off the simulation thread.** The simulation emits
   durable work; a bounded persistence worker executes it and reports results.
6. **Backpressure is semantic.** Coalescible state can be replaced or dropped;
   reliable gameplay and durable-operation results cannot silently disappear
   behind stale movement updates.
7. **Experiments inform production code but do not become hidden sources of
   truth.** Port validated concepts into project types, test them there, and
   mark superseded experiments accordingly.
8. **Each milestone remains runnable.** Land small changes with focused tests,
   updated documentation, and a Conventional Commit.

## Decisions required before gameplay implementation

The accepted requirements do not settle several gameplay and protocol details.
The following are proposed slice defaults so implementation can be estimated and
tested. They are not accepted requirements until recorded in the named ADRs.

### Proposed slice defaults

| Topic | Proposed default | Reason and boundary |
| --- | --- | --- |
| Tick rate | Keep the current 20 Hz development tick for the slice. | Deterministic tests need one value; later profiling may replace it. |
| World scale | Treat one world unit as approximately one metre; document speed and all ranges in units/second or units. | Makes town/field separation and range tests meaningful. |
| Movement | Authoritative speed budget per player per tick; surplus movement intents are rejected or deferred by a documented rule. | Prevents command bursts from bypassing range, aggro, or leash rules. |
| Duplicate login | Reject the second in-world session for a character with a typed error. | Easier to reason about than silently displacing the first session. |
| Party formation | Implement invite and accept for a maximum five-player party; test with three roles. | Proves a real server-owned party boundary rather than only a test shortcut. |
| Ability resource | Use cooldowns only in the first slice; do not claim role-resource validation until a resource exists. | Avoids an untestable optional requirement. |
| Death and recovery | Health reaching zero incapacitates the player; after a delay the player may release to town at full health. Death and current health remain transient across reconnect. | Provides a complete failure loop without adding resurrection or corpse runs. |
| Threat | Damage adds proportional threat; healing adds reduced threat to engaged enemies; taunt raises the tank just above current highest threat without a permanent lock; leash, death, and respawn clear threat. | Gives all three roles observable interaction with a deterministic model. |
| Group quest credit | Every eligible, alive or recently participating party member in encounter range receives one kill credit. | Prevents healer exclusion and defines a testable participation rule. |
| Group loot | One reward bundle per enemy spawn generation, assigned by deterministic round-robin among eligible party members. | Provides an exactly-once rule without designing need/greed yet. |
| Progression | Gold, inventory, vendor purchases, and quest state satisfy the first gate; XP, levels, and equipment stats are explicitly deferred pending owner approval. | Keeps the gate aligned with implemented town-return value without implying a decided level model. |
| Chat | Defer chat from the automated vertical-slice gate. | Party communication is valuable but not necessary to prove the authoritative encounter; revisit before outside playtesting. |
| Slow clients | Coalesce/drop stale position state first, preserve reliable gameplay and durable-operation results, disconnect only after a documented hard limit. | Matches the networking message classes and makes delay tests meaningful. |
| Unreadable checkpoint | Refuse world entry with a typed diagnostic and preserve the original file unchanged. | Avoids silent character reset or destructive recovery. |

Before Milestone 5 begins, record or amend ADRs for:

- The authoritative combat, death, and recovery model.
- World scale, movement budget, and encounter ranges.
- Protocol version negotiation and command identity.
- Event audience, perception, and backpressure policy.
- Session fencing and duplicate-login behavior.
- Threat and enemy lifecycle rules.
- Party membership, participation, quest credit, and loot distribution.
- Durable schema versioning, operation identity, and recovery behavior.

If the owner rejects a proposed default, revise the dependent exit tests before
implementation rather than allowing code and plan to diverge.

## Milestone sequence

### 0. Planning and validation foundation

**Outcome:** There is one detailed plan, one long-term roadmap, and one command
that actually validates all code affected by wire and client changes.

**Work:**

- Treat this plan as the execution expansion of roadmap Batches 4–6; update the
  roadmap status as each milestone lands rather than maintaining duplicate
  active sequences.
- Add a repository aggregate validation script that runs the root workspace and
  the standalone protocol, adapter, transport, client, editor, tool, and
  relevant experiment checks listed in `AGENTS.md`.
- Prefer adding non-Bevy library crates to the root workspace if doing so does
  not create dependency or build-environment coupling; otherwise keep the
  aggregate script as the authoritative compatibility gate.
- Document the aggregate command in `AGENTS.md` and CI when CI exists.
- Convert unresolved proposed defaults above into owner decisions or accepted
  ADRs before their dependent implementation begins.

**Required records:** Update `docs/implementation-roadmap.md` and
`docs/open-questions.md` to link to this detailed plan and remove questions as
their ADRs are accepted.

**Exit tests:**

- `aggregate_validation_covers_standalone_client_crates` — the documented
  command builds/tests client protocol, adapter, and transport crates and fails
  when a shared wire change breaks their decoder.
- The aggregate command exits nonzero for a deliberately failing standalone
  crate in a local validation of the script.

### 1. One authoritative timed simulation path

**Outcome:** The server drives a single deterministic simulation step on every
scheduled tick, with bounded player input and explicit origin correlation.

**Work:**

- Collapse `World::step` and `World::step_with_combat_timing` into one timed
  authoritative path; retain at most a test convenience wrapper with identical
  semantics.
- Switch the server scheduler to the unified timed path.
- Advance time-owned simulation state even when the command queue is empty.
- Introduce a bounded per-connection inbound command queue and a fair command
  budget per connection and per tick.
- Introduce server-owned movement speed and a per-tick displacement budget.
  Define whether excess valid intents are rejected or carried to a later tick;
  never apply an unbounded burst.
- Replace order-based join correlation with an explicit request/origin ID that
  appears in both success and rejection handling.
- Preserve the existing disconnect ordering regression: commands received
  before close are stepped and checkpointed while the character is still bound.
- Record tick duration and rejected/deferred command counts as development
  metrics.

**Required records:** Combat/scheduling ADR and updates to networking timing
documentation. Record measured results separately from estimates.

**Exit tests:**

- `server_advances_ticks_with_no_pending_commands` — the world tick and a
  time-owned fixture advance with no client commands.
- `server_uses_the_only_timed_step_path` — cast/cooldown behavior is reachable
  through the running server scheduler.
- `movement_budget_rejects_burst_commands` — many moves in one tick cannot move
  farther than the configured tick allowance.
- `one_client_cannot_starve_other_command_queues` — a flooding client does not
  consume another client's minimum command budget.
- `mixed_valid_and_invalid_joins_bind_correct_clients` — an invalid join between
  two valid joins cannot shift either connection's player binding.
- `disconnect_after_command_preserves_same_tick_reward` — the established
  disconnect persistence regression remains covered.

### 2. Canonical typed protocol and client path

**Outcome:** The typed wire route is the only gameplay path used by the
graphical client and all new features; the line protocol remains a local
diagnostic adapter.

**Work:**

- Make root launch commands and convenience scripts start the wire listener and
  graphical client in typed mode by default.
- Keep line commands for terminal debugging but do not add new gameplay
  semantics only to the line path.
- Put the complete connection state machine in one client transport/session
  owner: connecting, authenticating, listing/selecting, entering world,
  in-world, reconnecting, and failed.
- Define a slice protocol version policy and bump `PROTOCOL_VERSION` at the
  start of the breaking combat series.
- Make envelope version mismatch decodable enough to return a typed
  `VersionRejected` response containing the server-supported version or range.
- Add an explicit content/build compatibility field to the handshake, even if
  the development server initially accepts one build ID.
- Add bounded command sequence/request IDs. Durable retryable operations will
  later use operation IDs in addition to transport sequencing.
- Keep wire `CastAbility` player-implicit: the selected session supplies the
  player identity. Use an optional target ID and never accept amount or threat.

**Required records:** Protocol compatibility ADR; update launch instructions,
networking documentation, and stale statements that line mode is the graphical
default.

**Exit tests:**

- `graphical_client_enters_world_over_typed_wire_by_default`.
- `wire_version_mismatch_returns_typed_rejection` — an older version receives a
  typed response rather than an opaque decode failure.
- `content_build_mismatch_is_rejected_before_world_entry`.
- `duplicate_transport_sequence_is_not_applied_twice`.
- `wire_cast_ability_has_no_player_amount_or_threat_fields`.
- The line adapter can still connect, move, inspect state, and quit locally.

### 3. Multi-character identity and session fencing

**Outcome:** Three distinct role characters can connect concurrently without
sharing runtime identity or overwriting one checkpoint.

**Work:**

- Expand the development catalog to at least one tank, healer, and damage
  character with stable character IDs.
- Separate account ID, character ID, session ID, and runtime entity ID in APIs
  and logs.
- Add an active-character lease/fence owned by the session layer.
- Reject selection or world entry when another live session owns the character;
  release the fence on orderly exit, detected disconnect, failed entry, and
  bounded lease expiry.
- Ensure reconnect cannot race the prior connection's cleanup or control two
  runtime entities for one character.
- Replace the single checkpoint path with a safe per-character namespace. Do
  not derive paths directly from character names or other untrusted text.
- Ensure an authenticated account can select only a character it owns.

**Required records:** Session/identity ADR and updates to authentication and
persistence documentation. This remains development identity, not production
authentication.

**Exit tests:**

- `three_role_characters_are_listable_and_selectable`.
- `second_connection_cannot_select_an_active_character`.
- `session_cannot_select_another_accounts_character`.
- `disconnect_releases_character_fence_once`.
- `reconnect_cannot_control_two_entities_for_one_character`.
- `checkpoints_are_namespaced_per_character` — three concurrent characters
  create three distinct records with no cross-write.

### 4. Addressed delivery and minimum interest filtering

**Outcome:** Each wire event has a declared audience, and the server sends it
only to sessions allowed to perceive it. A slow client cannot force reliable
gameplay behind stale state indefinitely.

**Work:**

- Introduce an explicit event audience such as:

  ```text
  Global
  Player(player_id)
  Party(party_id)
  Nearby { position, radius }
  ```

- Keep audience metadata at the core/server boundary without coupling the
  simulation core to sockets.
- Address command rejections, inventory, currency, purchases, quest state,
  private bootstrap data, and session messages to one player/session.
- Address party membership and party combat summaries to the party.
- Send world entity/combat events only to nearby, legitimately perceiving
  clients. A simple bounded scan is acceptable for the first slice; Milestone
  11 replaces it with scalable spatial indexing and deltas.
- Split outbound traffic into at least reliable gameplay and coalescible state
  classes. Coalesce positions by entity/tick and drop stale state before
  reliable messages.
- Apply bounded memory and explicit disconnect diagnostics when the hard limit
  is finally exceeded.
- Filter bootstrap snapshots by the same ownership and perception rules used
  for live events.

**Required records:** Event-addressing/backpressure ADR and updates to
`docs/architecture/networking.md`.

**Exit tests:**

- `purchase_event_is_delivered_only_to_the_buyer`.
- `rejection_event_is_delivered_only_to_its_requesting_player`.
- `private_snapshot_omits_other_players_inventory_currency_and_quests`.
- `nearby_event_is_not_delivered_outside_perception_range`.
- `party_event_is_delivered_to_members_and_not_strangers` using a party fixture.
- `slow_client_drops_or_coalesces_state_before_reliable_gameplay`.
- `reliable_operation_result_survives_position_backpressure`.

### 5. Player combat state and role abilities

**Outcome:** Players can be damaged, healed, incapacitated, recover, and use one
meaningfully distinct server-authoritative role ability.

**Work:**

- Use the existing current/max health fields as the player-damage foundation;
  add alive/incapacitated transitions, recovery timing, ability cooldowns, cast
  state if retained, and authoritative event tick numbers.
- Keep health, target, current casts, cooldown progress, and death state
  transient unless the combat ADR explicitly changes that boundary.
- Define data-driven stable ability IDs and the first three abilities:

  | Role | Ability | Slice behavior |
  | --- | --- | --- |
  | Tank | Shield Strike / Taunt | Deals modest damage and applies the accepted taunt rule. |
  | Healer | Heal | Restores health to self or an eligible friendly target. |
  | Damage | Heavy Strike | Deals the highest sustained single-target damage. |

- Use a wire intent equivalent to `CastAbility { ability_id, target_id:
  Option<EntityId>, request_id }`; bind the player from the session.
- Validate ability ownership, actor state, target kind, friendliness, range,
  cooldown, line/rule constraints selected by the ADR, and target liveness
  before any mutation.
- Return addressed, structured rejection reasons.
- Make failed casts atomic: no health, cooldown, cast, threat, or durable state
  changes.
- Rescale the starter zone and spawn layout so town, field, attack range, heal
  range, aggro radius, and leash radius are spatially distinct.
- If durable player schema changes for any unrelated field, add a version-2
  writer, retain a version-1 reader fixture, and enforce the unreadable-file
  policy before deploying the change.

**Required records:** Combat/death ADR, world-scale decision, ability content
definitions, and persistence-schema note confirming which combat state is
transient.

**Exit tests:**

- `healer_cannot_heal_an_enemy`.
- `healer_can_heal_self_and_an_eligible_friendly_target`.
- `player_cannot_cast_another_roles_ability`.
- `incapacitated_player_cannot_move_attack_or_cast`.
- `failed_cast_leaves_all_state_unchanged` for each rejection class.
- `cooldown_and_range_use_authoritative_server_ticks`.
- `town_position_is_out_of_attack_range_of_field_enemies`.
- `release_returns_incapacitated_player_to_town_under_documented_rules`.
- `checkpoint_v1_fixture_loads_after_any_v2_writer_is_introduced`.
- `unreadable_checkpoint_is_preserved_and_world_entry_is_rejected`.

### 6. Threat-driven enemy lifecycle

**Outcome:** Enemies patrol, acquire targets, attack, leash, die, expose one
generation of rewards, and respawn deterministically in `mmorpg-core`.

**Work:**

- Design and implement a new core threat model. Reuse only the standalone
  AI experiment's state-machine shape and test ideas; translate them into core
  identifiers, floating-point positions, world ticks, and ownership rules.
- Model idle/patrol, engaged, returning, dead/corpse, and respawning states.
- Add server-owned aggro, threat accrual, target selection/tie-breaking, enemy
  basic attacks, attack cadence, leash, return-to-spawn, corpse lifetime,
  unclaimed-loot expiry, and respawn timing.
- Ensure the world advances patrol, attacks, corpse expiry, and respawn without
  client input.
- Define healing threat against enemies engaged with healed players; never take
  threat values from the client.
- Define deterministic taunt behavior and target switching.
- Clear transient threat and combat state on leash, death, and respawn.
- Give every spawn lifecycle a monotonically changing generation identity.
  Key the reward ledger by at least `(enemy_id, spawn_generation)` and retain
  enough tombstone/idempotency state to reject late claims safely.
- Mark the standalone AI experiment superseded once equivalent core behavior
  and tests land; keep its historical experiment record.

**Required records:** Threat/enemy-lifecycle ADR and updated AI experiment
record distinguishing reused concepts from new implementation.

**Exit tests:**

- `enemy_patrol_and_respawn_advance_without_commands`.
- `taunt_raises_tank_above_current_highest_threat`.
- `damage_dealer_threat_can_overtake_tank_after_taunt`.
- `healing_generates_the_documented_threat`.
- `threat_cannot_be_supplied_by_a_wire_command`.
- `leash_returns_enemy_to_spawn_and_clears_threat`.
- `dead_enemy_rejects_attacks_abilities_and_new_threat`.
- `unclaimed_loot_expires_under_the_documented_corpse_rule`.
- `loot_is_claimable_once_per_spawn_generation` — defeat, claim, respawn,
  defeat, and claim again; neither generation can be claimed twice.
- `respawn_timing_is_deterministic_at_twenty_hertz` while 20 Hz remains the
  accepted slice default.

### 7. Party rules, credit, and loot

**Outcome:** Three role characters form a real server-owned party, share the
information needed for the encounter, and receive credit/rewards according to
one explicit policy.

**Work:**

- Add stable party ID, leader, bounded membership, invite, accept, decline,
  leave, removal, and disband transitions.
- Validate all transitions in the server-owned party model. A client cannot
  directly assign itself or another player to a party.
- Define invitation expiry, concurrent invitation behavior, leadership transfer,
  and disconnect grace behavior.
- Use explicit membership for friendly-target validation; never infer a party
  solely from proximity.
- Publish party roster, health, role, connection/grace state, and relevant
  combat summaries only to party members.
- Snapshot eligibility at enemy death using the accepted participation/range
  rule so later membership changes cannot rewrite the outcome.
- Grant quest kill credit to all eligible members exactly once.
- Assign the one per-generation reward bundle using the accepted deterministic
  policy and record its recipient before a loot claim can be retried.
- Keep guilds, matchmaking, raids, master loot, and need/greed out of this
  milestone.

**Required records:** Party/reward ADR and updates to gameplay, networking, and
persistence boundaries.

**Exit tests:**

- `invite_and_accept_form_a_server_owned_party`.
- `client_cannot_add_an_uninvited_member`.
- `healer_can_heal_party_member_but_not_stranger`.
- `party_roster_and_health_are_hidden_from_strangers`.
- `disconnected_member_enters_and_exits_the_documented_grace_state`.
- `eligible_party_members_each_receive_one_quest_kill_credit`.
- `ineligible_or_out_of_range_member_receives_no_kill_credit`.
- `loot_recipient_is_deterministic_and_not_the_first_damager_by_accident`.
- `membership_change_after_enemy_death_does_not_change_reward_eligibility`.
- `one_reward_bundle_is_committed_per_spawn_generation`.

### 8. Persistence safety before the multiplayer gate

**Outcome:** Three players can earn durable rewards and disconnect/retry without
blocking the tick, losing the last accepted operation, or applying an operation
twice.

This milestone hardens the development repository boundary. PostgreSQL,
production credentials, a complete journal, and deployment recovery remain in
Milestone 12.

**Work:**

- Stop checkpointing every active character after every tick.
- Have the simulation owner emit immutable durable operations for loot, quest
  reward, purchase, and other durable changes.
- Give every retryable durable command/operation a stable operation ID scoped
  to the character or account as appropriate.
- Process durable operations through a bounded worker outside the simulation
  thread. Define acknowledgement, timeout, queue-full, retry, and shutdown
  behavior.
- Make operation application idempotent and atomic with inventory/currency/
  quest revision changes in the development store.
- Checkpoint movement at a bounded interval and on safe logout, not per frame.
- Preserve the invariant that a socket close after accepted commands cannot
  clear the player binding before final durable work/checkpoint capture.
- Persist or reconstruct sufficient idempotency state across process restart to
  prevent a retried reward from duplicating.
- Reset transient casts, health/death, target, threat, and party encounter state
  on restore according to their ADRs.
- Add metrics for persistence queue depth, operation latency/failure, checkpoint
  age, and tick duration.
- Measure three-client persistence load and record results in
  `docs/experiments/`; label them as local measurements, not production proof.

**Required records:** Durable-operation/schema ADR, updated persistence
architecture, checkpoint-format migration fixtures, and a measured experiment
record.

**Exit tests:**

- `persistence_io_does_not_run_on_the_simulation_thread` using an injected store
  that records its calling thread.
- `durable_operation_replay_is_idempotent` for loot, purchase, and quest reward.
- `queue_full_does_not_silently_drop_a_durable_operation`.
- `disconnect_in_a_partial_tick_persists_the_final_accepted_reward`.
- `safe_logout_checkpoints_the_latest_committed_position`.
- `restart_then_retry_does_not_duplicate_an_operation`.
- `restore_resets_transient_combat_state_and_preserves_durable_state`.
- `three_character_checkpoint_loads_and_writes_never_cross_character_ids`.
- `tick_duration_with_three_checkpointing_clients_meets_the_recorded_slice_budget`.

### 9. Graphical combat usability

**Outcome:** A Linux user can understand and complete the encounter through the
graphical client without the client becoming an alternate authority.

**Work:**

- Display player health and incapacitated/recovery state.
- Display target name, type, health, range validity, alive/dead/respawning state,
  and current target marker.
- Display the selected character's role, available ability, key binding,
  cooldown/cast state, and addressed rejection reason.
- Display party roster with roles, health, connection/grace state, and current
  eligibility indicators that the server actually exposes.
- Display nearby authoritative damage, healing, threat-target change, death,
  loot-recipient, quest-credit, and respawn events in a bounded combat log.
- Keep existing inventory, vendor, quest, town/field, reconnect, and selection
  UI working on the typed path.
- Use consistent bindings; for the slice, bind the selected role's primary
  ability to `1`, target cycling to `Tab`, movement to `WASD`, and retain
  documented interaction keys for loot/vendor/quest actions.
- Never optimistically mutate authoritative health, cooldown, inventory,
  currency, quest, party, or reward state. Presentation may animate pending
  intent but must reconcile to server results.

**Required records:** Updated client controls and launch documentation, plus
screenshots or a short manual validation record if automated rendering checks
cannot cover the behavior.

**Exit tests:**

- Presentation-model tests project each new combat, party, and rejection event.
- Adapter/transport tests reject malformed and oversized new payloads.
- `client_does_not_apply_unconfirmed_damage_healing_or_rewards`.
- `client_reconnect_replaces_stale_transient_combat_projection`.
- The aggregate validation command covers all non-rendering client crates.
- Manual Linux smoke: the user can identify role, party health, target range,
  cooldown, damage/healing, death, loot recipient, and respawn.

### 10. Three-client vertical-slice gate

**Outcome:** Several small, repeatable tests prove the full first gameplay loop.
No single seventeen-step test is the only source of diagnostic evidence.

**Work:**

- Extend or add a headless typed-wire harness capable of three concurrent
  sessions, deterministic tick advancement, packet delay, disconnect, retry,
  and process restart.
- Use three distinct development characters and the real invite/accept,
  addressed delivery, combat, reward, and persistence paths.
- Assert state throughout the encounter: target/threat ownership over time,
  health changes, cooldowns, credit eligibility, operation IDs, and delivery
  audiences—not merely the final balance.
- Keep graphical completion as a separate manual smoke so headless regression
  tests remain deterministic.
- Extend the restart persistence smoke from one character to all three.

**Required records:** A vertical-slice experiment record with environment,
commands, observed results, limitations, and a clear distinction between
functional evidence and scale evidence.

**Exit tests:**

- `three_clients_authenticate_select_distinct_characters_and_enter_one_world`.
- `party_forms_and_members_receive_only_allowed_roster_state`.
- `trinity_encounter_completes` — the tank holds threat for the required window,
  the healer restores actual enemy damage, and the damage character supplies
  the intended sustained damage.
- `encounter_loot_and_quest_credit_follow_party_policy`.
- `enemy_respawns_and_the_party_can_repeat_the_encounter`.
- `party_returns_to_town_and_completes_vendor_and_quest_flows`.
- `mid_encounter_disconnect_reconnect_restores_only_durable_state`.
- `slow_client_preserves_reliable_gameplay_while_state_is_coalesced`.
- `restart_smoke_preserves_all_three_characters`.
- `retry_across_reconnect_and_restart_applies_no_operation_twice`.
- Manual Linux smoke: three graphical client instances can complete the loop,
  or one graphical client plus two headless clients when hardware limits are
  documented.

Passing this milestone is the gate for calling the repository a basic MMORPG
gameplay loop. It is not evidence for 200-player activity or 5,000-client realm
capacity.

### 11. Replication and load foundation

**Outcome:** The minimum correct audience scan from Milestone 4 evolves into a
measurable replication system without changing gameplay authority.

**Work:**

- Add a spatial index/cell model and incremental interest-set changes.
- Add per-client outbound queues with explicit reliable and coalescible classes,
  priorities, byte/message budgets, and metrics.
- Add baseline snapshots plus delta snapshots with tick/sequence identity,
  stale-delta rejection, and resynchronization.
- Separate combat event channels from high-frequency movement state.
- Benchmark a real Rust serializer and interest scheduler with representative
  party combat before extrapolating.
- Add simulated mostly-idle connections and a 200-active-player world-boss
  workload in stages, measuring tick time, bandwidth, memory, CPU, loss, and
  queue behavior.
- Do not claim the 5,000/200 targets until the architecture requirement's real
  high-load activity and client/gateway conditions are exercised.

**Required records:** Replication experiment plan/results and an ADR only after
the experiments support a production direction.

**Exit tests:**

- Interest enter/leave produces complete, ordered client state.
- A client never receives private or out-of-interest entity fields.
- Lost/stale delta triggers bounded recovery without corrupting projection.
- Reliable combat/economy events are not head-of-line blocked by stale movement.
- Benchmarks report distributions and saturation behavior, not only averages.

### 12. Production-oriented durable repository

**Outcome:** The development store can be replaced by a migration-tested,
transactional repository with explicit crash/retry recovery.

**Work:**

- Design PostgreSQL account, character, inventory, currency, quest, operation,
  checkpoint, and content-version schemas.
- Add forward-only migrations and fixture tests for every durable schema version
  still supported by the repository.
- Commit reward/economy state and operation identity transactionally.
- Add an append-only durable-operation journal or equivalent recovery record,
  snapshots, and compaction/retention rules.
- Fence concurrent writers by character/session revision.
- Define startup recovery, temporary database outage, partial transaction,
  graceful shutdown, and crash-mid-operation behavior.
- Test restore and migration against representative multi-character data.
- Keep database access outside the simulation-critical step.

**Required records:** Accepted persistence ADR, migration/recovery runbook, and
crash-injection experiment records.

**Exit tests:**

- Kill/restart during loot, vendor purchase, and quest reward yields either one
  committed operation or none, never a partial or duplicate result.
- Concurrent stale writers cannot overwrite a newer character revision.
- Every committed migration upgrades supported fixtures and has a documented
  rollback/recovery policy.
- Temporary database unavailability applies bounded backpressure without
  corrupting simulation ownership.

### 13. Development content workflow

**Outcome:** The starter slice can be authored and validated without embedding
its world layout and definitions directly in simulation code.

**Work:**

1. Define a versioned content project/package format with stable IDs.
2. Add tiled terrain source documents and terrain material layers.
3. Add NPC/enemy definitions, spawn placement, patrol paths, and encounter
   ranges matching the combat scale decision.
4. Add vendor inventory, quest, reward, dialogue, and spawn editing.
5. Add particle-effect authoring/preview and placement of existing visual assets.
6. Add import, preview, configuration, and placement of existing sound and music
   assets.
7. Validate cross-references, bounds, content versions, and runtime packaging.
8. Build a Linux GUI shell around the existing editor core.
9. Integrate native pen-tablet pressure/tilt input for terrain sculpting and
   painting.

The editor emits the same validated content structures consumed by server and
client. It does not execute arbitrary authoritative gameplay logic.

**Required records:** Content-package and editor-runtime decisions, updated
tooling documentation, and experiment results for native tablet input.

**Exit tests:**

- The content checker rejects broken stable-ID references and invalid ranges.
- A packaged starter zone produces the same server/client content identity.
- Terrain, spawn, patrol, vendor, quest, particle, visual, and audio references
  round-trip through save/load.
- Native tablet behavior is manually validated on Linux hardware and recorded.

### 14. UI scripting and sandboxing

**Outcome:** The default HUD and player UI customizations use one bounded public
API that cannot automate protected gameplay actions or reveal server-filtered
hidden state.

**Work:**

1. Select an embedded runtime through a Linux feasibility/security experiment.
2. Define a public UI event and widget API over the presentation model.
3. Separate display/query APIs from protected input-to-intent APIs.
4. Add instruction, memory, event, timer, recursion, and widget quotas.
5. Ensure scripts see only data delivered through server-side ownership and
   interest filtering; the sandbox is not a substitute for Milestones 4/11.
6. Require secure, contemporaneous user input for protected gameplay actions.
7. Add adversarial tests for movement/cast injection, hidden-target discovery,
   inventory mutation, event floods, resource exhaustion, and persistence abuse.
8. Rebuild the default HUD using the same supported public API.

**Required records:** Runtime/sandbox ADR, public API documentation, threat
model, and adversarial test record.

**Exit tests:**

- Scripts cannot enumerate data the server did not legitimately deliver.
- Scripts cannot inject movement, targeting, casts, loot, purchases, or quest
  actions outside the protected input path.
- Quota exhaustion terminates or suspends only the offending addon.
- The default HUD runs without private scripting privileges.

## Dependency graph

```text
0 planning + aggregate validation
        |
        v
1 unified tick, bounded input, correlated joins
        |
        v
2 canonical typed protocol + compatibility
        |
        v
3 multi-character identity + session fencing
        |
        v
4 addressed delivery + minimum interest/backpressure
        |
        v
5 player combat + abilities + death
        |
        v
6 threat-driven enemy lifecycle + spawn generations
        |
        v
7 parties + party credit/loot
        |
        v
8 async idempotent persistence safety
        |
        v
9 graphical combat usability
        |
        v
10 three-client vertical-slice gate
       / \
      v   v
11 replication/load foundation    12 production durable repository
      \   /
       v v
13 content workflow
        |
        v
14 UI scripting sandbox
```

Milestones 11 and 12 may proceed in parallel only after Milestone 10 passes and
only with disjoint ownership and validation scopes. Instances/layers depend on
the replication, session, party-cohesion, and durable-operation boundaries but
are intentionally outside this first-slice chain.

## Definition of done

The first vertical slice is complete only when all of the following are true:

- A Linux developer can launch the typed server and graphical client through
  one documented sequence.
- Three distinct role characters authenticate, select, and enter one shared
  world, with at most one live session per character.
- The server advances one deterministic timed simulation path with bounded,
  fairly scheduled client input.
- Tank, healer, and damage abilities have distinct effects validated entirely
  by the authoritative server.
- Enemies attack players, choose targets through server-owned threat, leash,
  die, expose one reward generation, and respawn.
- Players can be incapacitated and recover under documented rules.
- Parties form through server-owned transitions; healing, visibility, quest
  credit, and loot follow the accepted party policy.
- No private or imperceptible event/state is delivered to an unauthorized
  client in the tested slice.
- Slow-client backpressure sheds/coalesces state before reliable gameplay and
  durable-operation results.
- Players can return to town, buy an item, progress and turn in a quest, and
  retain the resulting gold/inventory/quest state.
- Disconnect, retry, and process restart neither lose the last accepted durable
  operation nor apply it twice.
- No file or database I/O runs on the simulation thread.
- Named core, wire, server, standalone client, persistence, and multi-client
  tests pass through the documented aggregate validation command.
- The graphical encounter completes in a recorded Linux smoke test.
- Architecture documents, ADRs, roadmap status, open questions, command
  examples, and experiment records match the implemented behavior.
- Benchmark results are labelled as measurements of their actual workload and
  are not presented as proof of the 5,000-client or 200-player targets unless
  those exact requirements were exercised.

## Explicitly deferred work

The following are not prerequisites for the first vertical-slice gate:

- Production authentication, public-internet exposure, and encrypted production
  transport.
- PostgreSQL deployment, complete crash journal, and operational backups
  beyond the development persistence safety required by Milestone 8.
- XP, levels, equipment statistics, talent trees, and large-scale class design,
  pending an explicit progression decision.
- Chat before outside playtesting.
- Healer resurrection, corpse runs, durability loss, and other advanced death
  penalties.
- Need/greed, master loot, trading, mail, marketplace, guilds, matchmaking, and
  auction house.
- Dungeons, raids, battlegrounds, instance lifecycle, and overworld layering.
- Claims of 5,000-client or 200-player production capacity before Milestone 11's
  representative load evidence.
- Advanced editor collaboration, source asset creation, and a complete asset
  pipeline.
- Full addon API breadth beyond the bounded default-UI-capable sandbox.

Deferral does not remove these accepted long-term requirements. It keeps the
next implementation boundary small enough to validate honestly.
