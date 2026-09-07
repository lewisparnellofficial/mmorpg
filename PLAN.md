# MMORPG First Vertical Slice Execution Plan

**Status:** Reconciled planning baseline. Proposed numeric and gameplay defaults
remain provisional until their named ADR is accepted.

**Scope:** Milestones 0–10 take the current prototype to a repeatable
three-role town-and-field vertical slice. Milestones 11–13 are post-gate
foundations. Full editor, asset, and UI-scripting programs remain in
`docs/implementation-roadmap.md`.

**Relationship to the roadmap:** This expands roadmap Batches 4–6 and adds only
the minimum post-slice content-package boundary. It does not replace the
accepted requirements or roadmap. When implementation changes a status,
command, question, or architectural boundary, update its canonical document in
the same change.

**Vertical-slice target:** Three distinct characters enter one shared outdoor
zone, form a party, complete a server-authoritative tank/healer/damage
encounter, receive correct credit and non-duplicated rewards, return to town to
use a vendor and complete a quest, reconnect or restart without corrupting
acknowledged durable progress, and repeat after the enemy respawns.

## How to use this plan

Each milestone has a runnable outcome, bounded work, required records, and
falsifiable exit tests. Code alone does not complete a milestone: tests,
documentation, migrations, and measured evidence must land with it.

The ordering is a correctness dependency. World scale precedes movement and
perception budgets; compatibility support precedes a protocol bump; identity
and diagnostic access precede private delivery claims; party audiences use the
real party model; and durable results commit before becoming visible.

## Current constraints

- The server runs the untimed step while cast/cooldown behavior uses another
  path.
- Wire and line events are globally broadcast; the line listener can mutate
  the world without identity binding and is not loopback-restricted.
- Join correlation depends on successful-event order.
- One development character exists, with no active-character fence.
- Input, connection count, and displacement are not all bounded per tick.
- Version mismatch fails before a rejection can be decoded, and unknown event
  opcodes terminate decoding.
- One strict v1 checkpoint stores raw coordinates and is synchronously replaced
  on the simulation thread after every active tick.
- Reward identity does not include a spawn generation.
- Party authority, participation, and loot rules do not exist.
- Three non-Bevy client crates are outside the root workspace.
- TCP queues can be prioritized before writes, but bytes already written remain
  ordered and subject to head-of-line blocking.

## Invariants and reconciled policies

An ADR may replace a provisional value, but it must preserve these invariants
or revise this plan and its dependent tests first.

1. **One timed owner.** One world/region owner mutates live player, enemy,
   threat, party, reward, and timer state. Scheduled ticks advance without
   commands.
2. **Intent-only clients.** No client or diagnostic tool supplies authoritative
   damage, healing, threat, movement result, reward, membership, or another
   session's player identity.
3. **No unauthenticated gameplay surface.** Remove the line gameplay listener
   during typed cutover. Replace it with a typed-wire CLI using the same auth,
   character binding, fencing, audiences, and bind restrictions as the client.
4. **Audience is a server contract.** Filter before enqueueing. Multiple
   audiences form a union, but broader/positional audiences never widen private
   fields. Use distinct party-summary and nearby-world-detail events where
   visibility differs. Do not backfill old party events to new members.
5. **Compatibility precedes bump.** Introduce a frozen version-agnostic control
   envelope at the current version. Prove an old client decodes
   `VersionRejected` before changing the gameplay version. Skip and count
   unknown length-delimited event opcodes without dropping the session.
6. **Content compatibility is computed.** Compare a deterministic digest of
   canonical starter-catalog serialization, not hard-coded build constants.
7. **Sequence is not durable identity.** Reserve bounded sequence/request IDs
   for ordering, correlation, and diagnostics. Retryable durable commands use
   stable operation IDs; TCP frames are not deduplicated speculatively.
8. **Commit before apply and publish.** A durable command may become pending,
   but inventory/currency/quest/reward state is applied and success published
   only after an off-thread worker atomically commits it. Failure applies
   nothing and returns a typed result. Retrying a committed operation ID returns
   the prior result without reapplying it.
9. **No blocking persistence on the simulation thread.** Bounded message send
   is allowed; file/database calls and waits are not. Movement checkpoints are
   periodic and safe-logout based.
10. **Checkpoint evolution is staged.** Before an additive writer change,
    readers learn to ignore unknown additive fields while rejecting unsupported
    format versions. Pre-rescale positions migrate to town spawn rather than
    being silently reinterpreted.
11. **TCP claims stop at the application queue.** Coalesce stale state before
    socket write so reliable messages are written promptly; document the
    residual in-flight TCP window.
12. **Disconnect ordering is load-bearing.** Commands accepted before close are
    stepped and final commit/checkpoint work is captured before bindings and
    fences are released. Reverify this in Milestones 1 and 8.

## Provisional defaults and decision gates

Confirm or replace these in ADRs before Milestone 1 implementation. Changing a
value requires updating dependent fixtures and budgets first.

| Topic | Provisional default |
| --- | --- |
| Tick | 20 Hz; durations and deterministic tests remain tick-based. |
| Tick budget | Three-client persistence run: p99 simulation work <= 12.5 ms (25% of a 50 ms tick) on documented hardware. Record p50/p95/p99/max and samples. |
| Scale | 1 unit ~= 1 metre; player 7 units/s, melee 5, heal 30, aggro 20, leash 45, town-to-field >= 80 units. |
| Excess movement | Reject beyond the tick allowance; carry no hidden debt. |
| Connections | A finite cap and accept rate fixed in the scheduling ADR; typed rejection on overflow. |
| Duplicate login | Reject a second in-world session for a character. |
| Party | Invite/accept, maximum five, tested with three roles. |
| Death | Zero health incapacitates; timed release returns to town at full health. Combat state is transient across reconnect. |
| Threat | Damage is proportional; healing is reduced against relevant engaged enemies; taunt moves tank just above the leader without a lock. Clear on leash/death/respawn. |
| Credit | Alive or recently participating members in ADR-defined encounter range at death receive one kill credit. |
| Loot | One bundle per spawn generation; deterministic eligible-member round-robin. |
| Progression gate | Gold, inventory, vendor, loot, and quest state suffice. XP, levels, and equipment stats are deferred. |
| Checkpoint compatibility | Ignore unknown additive fields; reject unknown versions. Non-additive changes require migration/recovery policy. |
| Unreadable checkpoint | Reject entry with typed diagnostic; preserve the original. |
| Transport | Keep TCP through the slice; decide its successor, if any, from Milestone 11 evidence. |

Accept ADRs for scheduling/scale, protocol compatibility, identity/fencing,
audiences/backpressure, combat/death, threat/lifecycle, party/rewards, and
durable operations/recovery before dependent work.

## Milestones

### 0. Planning and aggregate validation

**Outcome:** One command validates all code affected by wire/client changes.

**Status:** Complete (2026-09-07). The aggregate command and its isolated
failure-propagation self-test pass locally.

**Work:**

- Add `mmorpg-client-protocol`, `mmorpg-client-adapter`, and
  `mmorpg-client-transport` to the root workspace. Keep Bevy outside if desktop
  dependencies make it unsuitable for the normal gate.
- Add an aggregate script for the root, remaining standalone crates, tools, and
  relevant experiments. Give it an isolated `--self-test` that proves failures
  propagate.
- Document it in `AGENTS.md` and later CI. Link this plan from canonical
  roadmap/open-question updates without duplicating the active sequence.

**Implementation note:** `scripts/validate-all.sh` is the aggregate command.
It checks the headless workspace and standalone client/protocol, tool, and
experiment manifests; the Bevy client is checked but not launched. Its
`--self-test` proves that a failing child command is observed by the runner.

**Evidence:** `./scripts/validate-all.sh` completed with
`aggregate validation: PASS`; the command covered the root workspace, Bevy
client check, three standalone client crates, two tools, five Rust
experiments, the replication-model compile check, and whitespace validation.
`./scripts/validate-all.sh --self-test` observed the deliberate child status
37 and passed.

**Exit tests:** `aggregate_validation_covers_standalone_client_crates` and
`aggregate_validation_self_test_proves_failure_propagation`.

### 1. Scale, unified timed simulation, and bounded intake

**Outcome:** One deterministic timed path runs against coherent scale, advances
without commands, fairly bounds intake/connections, and correlates results.

**Work:**

- Accept numeric scheduling/scale ADR and rescale town, field, spawns, ranges,
  and fixtures before movement or perception calibration.
- Mark/migrate pre-rescale checkpoints to valid town spawn.
- Collapse both step paths; run the server scheduler through the timed path and
  advance time-owned state on empty ticks.
- Add bounded per-connection queues plus fair local/global tick budgets,
  authoritative movement allowance, connection cap, and accept-rate limit.
- Replace order-based join matching with request/origin identity.
- Record tick/queue/rejection metrics and preserve disconnect ordering.

**Required records:** Scheduling/scale ADR, migration note, networking timing
update, and results separated from estimates.

**Exit tests:**

- `server_advances_ticks_with_no_pending_commands`.
- `cast_submitted_through_the_server_resolves_after_cast_time_ticks`.
- `movement_budget_rejects_burst_commands`.
- `one_client_cannot_starve_other_command_queues`.
- `connection_limit_rejects_with_a_typed_diagnostic`.
- `mixed_valid_and_invalid_joins_bind_correct_clients`.
- `pre_rescale_checkpoint_restores_to_a_valid_position`.
- `disconnect_after_command_preserves_same_tick_reward`.
- `town_position_is_out_of_attack_range_of_field_enemies`.

### 2a. Protocol compatibility groundwork

**Outcome:** The current client survives later evolution and can decode a typed
version rejection.

**Work:**

- Define a bounded, version-agnostic control envelope (for example control
  version 0) accepted regardless of gameplay-envelope version.
- At the current version, teach both sides the frozen
  `VersionRejected { supported_min, supported_max }` control payload.
- Length-delimit event payloads so unknown opcodes can be skipped and counted.
- Canonically serialize the catalog and compute its digest on both sides.
- Reserve bounded sequence/request fields without semantic deduplication.

**Required records:** Protocol ADR, digest specification, and compatibility
policy.

**Exit tests:**

- `old_client_decodes_version_rejection_from_a_newer_server` using a retained
  pre-bump fixture.
- `unknown_event_opcode_is_ignored_without_dropping_the_session`.
- `content_digest_changes_when_canonical_catalog_changes`.
- `wire_cast_ability_has_no_player_amount_or_threat_fields`.

### 2b. Typed cutover and diagnostic closure

**Outcome:** Typed wire is the only gameplay/session path; the new version
fails compatibly for an old client.

**Work:**

- Make launch scripts and graphical client use typed mode by default. Centralize
  connecting/auth/select/enter/reconnect/failure state ownership.
- Bump `PROTOCOL_VERSION` only after 2a passes; compare content digest before
  world entry.
- Delete the line gameplay listener and its mutation/global snapshot surface.
- Add a typed-wire diagnostic CLI using the regular session rules.
- Keep `CastAbility` player-implicit with ability ID, optional target, and
  request ID only. Update `AGENTS.md`, scripts, smoke tools, and examples.

**Required records:** Accepted protocol/diagnostic decisions and updated launch
and networking documentation.

**Exit tests:**

- `graphical_client_enters_world_over_typed_wire_by_default`.
- `old_client_decodes_version_rejection_from_a_newer_server` against the bump.
- `content_digest_mismatch_is_rejected_before_world_entry`.
- `diagnostic_path_cannot_enter_world_without_account_and_character_binding`.
- `diagnostic_path_cannot_observe_another_players_private_events`.
- `non_loopback_bind_disables_every_unauthenticated_path`.

### 3. Multi-character identity, fencing, and bounded checkpoints

**Outcome:** Three roles connect without shared identity, duplicate control,
cross-written storage, or per-tick filesystem I/O.

**Work:**

- Add stable tank/healer/damage characters; distinguish account, character,
  session, runtime entity, request, and operation IDs.
- Add a session-owned active-character fence with exact-once release on exit,
  disconnect, failed entry, or expiry; prevent reconnect cleanup races.
- Replace `--character-store <file>` with a safe per-character namespace, never
  derived from names. Update documented CLI and restart smoke test.
- Checkpoint movement at bounded intervals and safe logout, not every tick.
- Land additive-field reader compatibility before any new writer.

**Required records:** Identity ADR and auth/persistence/CLI documentation.

**Exit tests:**

- `three_role_characters_are_listable_and_selectable`.
- `second_connection_cannot_select_an_active_character`.
- `session_cannot_select_another_accounts_character`.
- `disconnect_releases_character_fence_once`.
- `reconnect_cannot_control_two_entities_for_one_character`.
- `checkpoints_are_namespaced_per_character`.
- `checkpoint_write_rate_stays_within_the_configured_interval_with_three_clients`.
- `reader_ignores_unknown_additive_fields_from_a_newer_writer`.
- `checkpoint_v1_fixture_loads_after_an_additive_writer_change`.
- `unreadable_checkpoint_is_preserved_and_world_entry_is_rejected`.

### 4. Addressed delivery and minimum interest

**Outcome:** Global, player-private, and nearby data reaches only authorized
sessions; bounded application queues prioritize reliable gameplay.

**Work:**

- Add `Global`, `Player(player_id)`, and `Nearby { position, radius }` at the
  core/server boundary. Add `Party` only in Milestone 7.
- Address private events/snapshots to one player; filter world data by
  legitimate perception and define incapacitated perception origin.
- Union matching audiences without widening private fields.
- Split reliable and coalescible application queues, coalesce position by
  entity/tick, bound memory, and diagnose hard-limit disconnects.
- Document the remaining in-flight TCP ordering window.

**Required records:** Audience/backpressure ADR and networking update.

**Exit tests:**

- `purchase_event_is_delivered_only_to_the_buyer`.
- `rejection_event_is_delivered_only_to_its_requesting_player`.
- `private_snapshot_omits_other_players_inventory_currency_and_quests`.
- `nearby_event_is_not_delivered_outside_perception_range`.
- `private_fields_are_not_widened_by_a_nearby_audience`.
- `reliable_event_enqueued_after_n_stale_positions_is_written_within_one_tick`.

### 5. Player combat and recovery state machine

**Outcome:** Authoritative damage, healing, cast, cooldown, incapacitation, and
recovery exist at core level. Enemies exercise damage in Milestone 6.

**Work:**

- Add alive/incapacitated transitions, recovery, cooldown/cast state, and event
  ticks; keep combat state transient.
- Define stable tank strike/taunt, friendly heal, and damage strike IDs.
- Validate ownership, actor, target/friendliness, range, cooldown, liveness,
  and ADR rules before mutation. Failed casts are atomic and addressed.
- Permit damage injection only in `#[cfg(test)]` core code; expose no health
  mutation wire/server command.

**Required records:** Combat/death ADR, abilities, and transient-state note.

**Exit tests:**

- `healer_cannot_heal_an_enemy`.
- `healer_can_heal_self_and_an_eligible_friendly_target`.
- `player_cannot_cast_another_roles_ability`.
- `incapacitated_player_cannot_move_attack_or_cast`.
- `failed_cast_leaves_all_state_unchanged` for every rejection class.
- `cooldown_and_range_use_authoritative_server_ticks`.
- `release_returns_incapacitated_player_to_town_under_documented_rules`.
- `wire_has_no_authoritative_player_damage_command`.

### 6. Threat-driven enemy lifecycle

**Outcome:** Enemies patrol, attack, select targets by threat, leash, die,
expose one reward generation, and respawn deterministically.

**Work:**

- Port validated AI-experiment concepts into core types and ownership.
- Model idle/patrol, engaged, returning, corpse, and respawning states; advance
  them without client input.
- Add deterministic aggro/threat/ties, enemy attacks, leash, corpse/loot expiry,
  healing threat, taunt, and cleanup.
- Give each lifecycle a monotonic generation; key reward/tombstone identity by
  `(enemy_id, spawn_generation)`.
- Mark the experiment superseded only after equivalent core tests land.

**Required records:** Threat/lifecycle ADR and experiment update.

**Exit tests:**

- `enemy_patrol_and_respawn_advance_without_commands`.
- `enemy_attack_exercises_player_incapacitation_state_machine`.
- `taunt_raises_tank_above_current_highest_threat`.
- `damage_dealer_threat_can_overtake_tank_after_taunt`.
- `healing_generates_the_documented_threat`.
- `threat_cannot_be_supplied_by_a_wire_command`.
- `leash_returns_enemy_to_spawn_and_clears_threat`.
- `dead_enemy_rejects_attacks_abilities_and_new_threat`.
- `unclaimed_loot_expires_under_the_documented_corpse_rule`.
- `loot_is_claimable_once_per_spawn_generation` across two generations.
- `respawn_timing_is_deterministic_in_ticks`.

### 7. Parties, party audience, credit, and loot

**Outcome:** Three characters form an authoritative party, receive entitled
party data, and get deterministic credit and loot.

**Work:**

- Add party ID, leader, bounded membership, invite/accept/decline,
  leave/remove/disband, expiry, leadership transfer, and disconnect grace.
- Expose no client-assignable membership mutation.
- Add `Party(party_id)` using the real registry. Publish roster/health/role/grace
  and summaries only to current members; no historical backfill.
- Use separate summary/detail events for out-of-perception members.
- Snapshot eligibility at death. Grant eligible credit once and record one
  round-robin recipient per generation before retryable claim.

**Required records:** Party/reward and audience ADR updates.

**Exit tests:**

- `invite_and_accept_form_a_server_owned_party`.
- `client_cannot_add_an_uninvited_member`.
- `party_membership_has_no_public_client_mutation_api`.
- `healer_can_heal_party_member_but_not_stranger`.
- `party_roster_and_health_are_hidden_from_strangers`.
- `party_member_outside_perception_range_receives_party_summary_but_not_world_detail`.
- `new_member_receives_no_pre_membership_event_backfill`.
- `eligible_party_members_each_receive_one_quest_kill_credit`.
- `ineligible_or_out_of_range_member_receives_no_kill_credit`.
- `loot_recipient_follows_round_robin_across_consecutive_generations` with the
  same first damager.
- `membership_change_after_enemy_death_does_not_change_reward_eligibility`.
- `one_reward_bundle_is_committed_per_spawn_generation`.

### 8. Commit-before-publish persistence safety

**Outcome:** Durable results commit off-thread before becoming authoritative or
visible; retry/restart cannot duplicate acknowledged work, and store failure
cannot create visible divergence.

**Work:**

- Accept the durable ADR first: acknowledgment states, worker bounds,
  timeouts/retries, failure threshold, shutdown, and sequence/operation-ID roles.
- Submit immutable loot/reward/purchase operations to a bounded worker with
  stable scoped IDs.
- Atomically commit ID, result, and data revision; only then apply to the live
  world and publish success. Bound pending dependent actions.
- Queue/full failure returns a typed failure and applies nothing. After the
  specified persistent-failure threshold, disable durable-granting commands and
  cleanly disconnect if consistency cannot be maintained.
- Persist committed results for idempotent retry after restart.
- Add SIGINT/SIGTERM shutdown: stop intake, drain or explicitly fail bounded
  work, checkpoint, release fences, and stop the worker.
- Reverify disconnect ordering; measure queue, latency/failure, checkpoint age,
  writes, and tick duration with three clients.

**Required records:** Durable/schema ADR, persistence update, fixtures,
shutdown behavior, and crash-injection experiment.

**Exit tests:**

- `persistence_io_does_not_run_on_the_simulation_thread`.
- `success_is_not_published_before_durable_commit`.
- `failed_commit_applies_no_durable_state_change`.
- `durable_operation_replay_is_idempotent` for loot, purchase, and quest.
- `queue_full_returns_typed_failure_and_applies_nothing`.
- `persistent_store_failure_stops_granting_rather_than_diverging`.
- `crash_between_submission_and_commit_has_no_acknowledged_loss_or_duplicate`.
- `crash_after_commit_before_delivery_returns_same_result_on_retry`.
- `disconnect_in_a_partial_tick_persists_the_final_committed_reward`.
- `safe_logout_checkpoints_the_latest_committed_position`.
- `restore_resets_transient_combat_state_and_preserves_durable_state`.
- `three_character_loads_and_writes_never_cross_character_ids`.
- `graceful_shutdown_drains_or_explicitly_fails_pending_work`.
- The recorded run meets the predeclared 12.5 ms p99 tick budget; otherwise the
  milestone remains open and the miss is recorded.

### 9. Graphical combat usability

**Outcome:** A Linux user completes the encounter through the graphical client
without the client becoming authority.

**Work:**

- Display health/recovery, target/range, ability/cooldown/cast, rejections,
  party roster, and only server-exposed eligibility/detail.
- Display bounded nearby/party combat, threat target, death, loot, credit, and
  respawn events. Preserve typed inventory/vendor/quest/reconnect flows.
- Use `1`, `Tab`, and `WASD` plus documented interaction keys.
- Never optimistically mutate authoritative state.
- Keep this HUD thin because roadmap Batch 8 must later rebuild the default UI
  on the required public scripting API.

**Required records:** Controls/launch docs and visual/manual evidence.

**Exit tests:** presentation coverage for every event; malformed/oversized
payload rejection; `client_does_not_apply_unconfirmed_damage_healing_or_rewards`;
`client_reconnect_replaces_stale_transient_combat_state`;
`unknown_event_opcode_does_not_break_graphical_session`; and a recorded Linux
interaction smoke.

### 10. Three-client vertical-slice gate

**Outcome:** The full loop passes with three independent graphical clients and
remains repeatable through a headless harness.

**Work:**

- Add a deterministic three-role harness over real typed transport.
- Record at least one three-graphical-client Linux validation. A mixed run does
  not substitute for validating party/private visibility against a stranger.
- Exercise party, encounter, recovery, credit, round-robin loot across
  respawns, town purchase/turn-in, disconnect, restart, and operation retry.
- Include a slow client and assert only the pre-socket queue guarantee.
- Freeze the completed slice as a reproducible experiment fixture.

**Required records:** Exact commit, commands, hardware, client mix, timing,
failures, and limitations.

**Exit tests:**

- `tank_holds_top_threat_for_at_least_n_ticks`, with `n` fixed by ADR.
- `healer_restores_at_least_the_damage_enemy_dealt_to_tank`.
- `damage_dealer_contributes_at_least_p_percent`, with `p` fixed by ADR.
- `three_clients_receive_correct_credit_and_round_robin_loot`.
- `restart_and_retry_preserve_exactly_one_acknowledged_result`.
- `stranger_observes_no_private_or_party_only_state`.
- `reliable_event_after_stale_state_is_written_within_one_tick`.
- The loop repeats after respawn without stale threat, credit, or reward ID.

### 11. Post-gate replication and load foundation

**Outcome:** Spatial indexing, deltas, and measured queue behavior replace
slice-scale scans without premature capacity claims.

Add region-owned spatial indexing; baselines/acks/deltas/resync; per-client
budgets; and representative quiet/hotspot scenarios. Measure joins, loss,
latency, reconnect, AoE, combat, healing, threat, auras, interrupts, and TCP's
residual ordering. Fix thresholds before running. Record distributions,
saturation, hardware, payloads, and misses; decide or further experiment on
transport from this evidence.

**Exit criteria:** AOI/deltas match a full-state oracle; reliable messages enter
the socket by the application deadline after coalescing; unmet predeclared
thresholds remain recorded failures, not redefined successes.

### 12. Post-gate production durable repository

**Outcome:** A database-backed repository can replace the development store
without changing ownership or commit-before-publish semantics.

Define PostgreSQL schema, revisions, migrations, backups/restore, operation
journal/outbox, observability, and bounded unavailability. Inject termination
at each operation boundary and prevent stale writers overwriting new revisions.

**Exit criteria:** Loot/purchase/quest kill-restart yields one atomic operation
or none; stale writers fail; supported migrations and recovery are tested and
documented.

### 13. Post-gate content package and validator

**Outcome:** Slice definitions/layout come from one versioned stable-ID package
with a shared digest.

Define deterministic packaging; move starter NPC/enemy, spawn, patrol, range,
vendor, quest, reward, dialogue, and layout metadata into it; validate IDs,
bounds, ranges, versions, and digest. Do not execute arbitrary authoritative
content logic.

**Exit criteria:** Broken references/ranges/versions fail; client and server
produce the same digest; the package reproduces the completed slice fixture.

Editor GUI, terrain/material authoring, particles/audio, native tablet input,
and the default-UI scripting sandbox remain roadmap Batch 8 work.

## Dependency graph

```text
0 validation -> 1 scale/tick -> 2a compatibility -> 2b typed cutover
 -> 3 identity/checkpoints -> 4 addressed delivery -> 5 player combat
 -> 6 enemy lifecycle -> 7 parties/loot -> 8 durability -> 9 client UI
 -> 10 vertical-slice gate
       |-> 11 replication/load -|
       |-> 12 durable store ----|-> 13 content package
```

Milestones 11 and 12 may run in parallel after Milestone 10 only with disjoint
write scopes. Instances/layers remain outside this plan.

## First vertical-slice definition of done

Milestones 0–10 are complete only when:

- One documented Linux sequence launches typed server and client.
- Three roles authenticate, select, enter one shared world, and have at most one
  live session each.
- No unauthenticated path enters the world or sees another player's private
  state on any listener or bind address.
- A previous-version client gets a decodable rejection; unknown event opcodes
  do not destroy its session; real catalog digest mismatch is rejected.
- One timed simulation advances with bounded connections, fair intake, and
  authoritative movement.
- Role abilities, enemy threat/attacks, recovery, leash, reward generation, and
  respawn follow accepted server rules.
- Party transitions, visibility, credit, and loot follow their ADRs.
- Stale state is discarded before socket write so reliable events meet the
  application deadline; TCP's residual limitation is documented.
- Town purchase and quest turn-in survive disconnect/restart.
- Durable success is published only after atomic commit; retry never duplicates
  it and failure applies no visible durable mutation.
- File/database calls and waits do not run on the simulation thread.
- Aggregate tests pass and a recorded three-graphical-client Linux run verifies
  both visible party state and hidden stranger state.
- Documentation and migrations match behavior; measurements state workload and
  hardware and claim neither 5,000 clients nor 200 active players unless those
  exact requirements were exercised.

## Explicitly deferred

- Production authentication, public exposure, encryption, PostgreSQL, full
  journal/outbox, backups, and production recovery.
- XP/levels/equipment stats, broad classes, chat, resurrection/corpse runs,
  need/greed, trades, mail, marketplace, guilds, matchmaking, and auctions.
- Instances, raids, battlegrounds, and overworld layering.
- Capacity claims before representative 5,000-connected and 200-active tests.
- Editor GUI, terrain/material, particle/audio, tablet, and broad asset work.
- UI scripting, protected input, sandbox testing, and default-HUD rewrite.

Deferral preserves accepted long-term requirements while making the first gate
coherent, secure, and falsifiable.
