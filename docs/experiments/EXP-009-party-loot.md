# EXP-009: Party ownership, eligibility, and generation loot

**Status:** Authoritative core and typed adapter slice integrated; development
disconnect grace is integrated; addressed party delivery remains provisional

## Objective

Close the first Milestone 11 boundary without allowing party state or loot
selection to become client-owned. The world owner must validate membership,
bound invitations, leadership changes, death-time eligibility, and
generation-scoped loot selection.

## Implementation

mmorpg-core::World now owns stable PartyId values, membership indexes, leader
authority, bounded five-member parties, pending invitation expiry, and party
lifecycle commands:

- invite and accept/decline;
- leave and leader-authorized removal;
- leader transfer;
- leader disband.

Party events are mapped through typed wire command opcodes 21–27 and server
event opcodes 25–32. The presentation model projects party summaries and
pending invite expiry without creating gameplay authority.

When an enemy dies, the owner snapshots eligible living party members near the
enemy. Quest credit is applied once to that immutable eligibility set, and one
loot recipient is selected by the party's deterministic round-robin cursor.
The selected recipient remains tied to the enemy's spawn generation; a later
respawn resets the reward state and advances the generation. Players who join
after death do not become eligible for that corpse.

Party events are filtered to the inviter/invitee or current/former party
members. Unrelated typed sessions do not receive party membership or invite
details.

## Evidence

- **Measured local result:** 31 root core tests pass, including bounded invite
  expiry and two-generation party loot selection; the server's reconnect-grace
  tests, typed wire, client model, client adapter, workspace, and aggregate
  validation suites pass.
- **Implementation evidence:** authoritative party registry, five-member
  bound, invite expiry, leader checks, death-time eligibility snapshot,
  round-robin cursor, generation reset, typed codec mappings, and client
  projection tests.
- **Project inference:** a per-party monotonic cursor gives deterministic loot
  ordering while allowing eligibility to change between encounters.

## Remaining uncertainty

The grace window is development-session state, not production session resume:
there is no gateway handoff, reconnect token, or cross-process detached-state
store. Party event delivery is currently session-filtered but still uses the
development server's bounded broadcast queue rather than the addressed
delivery/interest manager from Milestone 8. Remote party summaries and nearby
detail will be separated when that delivery boundary is integrated.
