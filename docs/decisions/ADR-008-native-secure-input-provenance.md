# ADR-008: native provenance for protected client actions

- Status: Accepted for the current development slice
- Date: 2026-09-09
- Scope: `mmorpg-client-secure-input`, the Bevy host integration, and the
  `ui.v1` addon boundary

## Context

Addons may present controls and request allowlisted secure-action bindings,
but a script, timer, callback, animation, replay trace, or another addon must
not be able to mint a trusted gameplay action. The server remains the final
authority, but relying on server rejection alone would still allow a client
addon to automate protected actions and would make the local security
boundary ambiguous.

The first protected action is basic attack. The client needs to distinguish a
fresh native user interaction from script-originated or replayed activity
without exposing a reusable capability as a script value.

## Decision

Native client code owns a dependency-light `SecureInputRegistry`. It stores
allowlisted `(addon, node, action)` bindings with node and focus generations.
The host may dispatch only a fresh non-repeat key press or primary-pointer
press carrying a nonzero physical-event ID. Dispatch rejects unknown or
foreign bindings, stale node generations, stale focus generations, repeated
key presses, and replayed physical-event IDs.

Successful dispatch creates a native-only `TrustedIntent`. Its linear
`consume` operation produces the ordinary typed action tuple exactly once;
the token is never represented in Luau values and cannot be constructed by an
addon, callback, timer, overlay, or replay adapter. Addon unload removes its
bindings, and focus changes invalidate the prior focus generation.

The registry treats physical event IDs as monotonic and retains only the
highest successfully dispatched ID. Duplicate or older IDs are rejected as
replays using this constant-size watermark; it does not accumulate an
unbounded per-event replay set over the lifetime of a client.

The Bevy host owns hit testing and translates a consumed allowlisted action
into a bounded `ClientCommand` intent. The current slice allowlists only
`basic_attack`; adding another protected action requires a separate explicit
binding and validation change. The native HUD and ordinary addon use
the same host binding mechanism. The server still validates the resulting
typed intent for ownership, role, target, range, liveness, cooldown, and all
other gameplay rules; physical provenance does not grant gameplay authority.

The contract intentionally excludes held polling, key repeat, timers,
callbacks, animation completion, focus traversal, synthetic UI events,
programmatic activation, addon-to-addon forwarding, and replay traces. The
registry owns provenance state, while `mmorpg-ui-contract` and Luau own only
presentation values and transactional UI operations.

## Threat model and non-goals

The boundary defends against an untrusted addon trying to:

- call a protected action directly;
- forge or retain another addon's node handle;
- reuse an old physical event or stale focus/node generation;
- activate through a repeat, timer, callback, overlay, or replay;
- forward activation from one addon to another; or
- bypass server validation with a physically sourced but invalid intent.

It does not claim protection against a compromised native client, a hostile
OS/kernel, accessibility tools with equivalent native authority, input-device
firmware compromise, or a production anti-cheat system. It also does not
replace server-side authorization or provide a complete scripted-HUD security
model.

## Consequences

- The default UI and ordinary addons share the same presentation API and
  secure binding path.
- Protected action provenance is concentrated in a small native boundary
  rather than spread through the scripting API.
- A trusted dispatch is single-use and tied to current physical, focus, node,
  and addon generations.
- Native input state is not serializable into addon values or replay fixtures.
- Retained secure-intent diagnostics have a bounded 64-entry host budget;
  exceeding it fails closed rather than growing addon state indefinitely.
- The current implementation supports one real Bevy secure-action
  presentation path and native dispatch integration, not every future widget
  or pointer interaction.
- Luau remains provisional pending the broader Milestone 5 runtime decision;
  this ADR does not accept process-level isolation, exhaustive fuzzing, or a
  production addon packaging policy.

## Validation evidence

The decision is accepted for the current development slice based on:

- `mmorpg-client-secure-input` tests for fresh dispatch, single-use
  consumption, repeat/replay rejection, stale focus and node rejection,
  cross-addon denial, reload invalidation, forged identifiers, safe unload
  behavior, and older-than-watermark rejection.
- Graphical-client secure binding tests covering replay rejection and binding
  refresh after focus changes.
- UI-scripting tests proving protected actions are absent from the script
  API, addon handles cannot cross ownership boundaries, callbacks roll back,
  and the default UI remains isolated from addon failures.
- Aggregate three-role graphical gameplay validation passing through the typed
  wire/server-authority boundary on 2026-09-09.

## Conditions for revisiting

Revisit this decision before exposing additional protected actions, replacing
the native HUD with a fully scripted HUD, permitting richer pointer gestures,
or accepting a production addon runtime. Revisit it immediately if any host
path can mint a trusted intent without a fresh native event, if a token can
cross into script values, or if server validation is bypassed. Physical
tablet-specific evidence remains governed separately by EXP-007 and is not
implied by this decision.
