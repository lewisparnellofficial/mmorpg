# Client UI Scripting and Addon Sandbox

**Status:** Proposed; language-neutral `ui.v1` contract implemented, runtime
and secure-input evidence still required

## Contract boundary

The dependency-light [`mmorpg-ui-contract`](../../crates/mmorpg-ui-contract/)
crate is the canonical host boundary. It is deliberately independent of the
Luau experiment and of the renderer. The default UI and player addons must
eventually use the same operations and immutable view records through runtime
adapters.

The contract currently defines:

- stable package, account, node, generation, and timer IDs;
- immutable presentation view records;
- replaceable/coalesced state events and ordered FIFO events with bounded
  count/byte queues;
- owner- and generation-checked handles and all-or-nothing operation-batch
  validation;
- manifest checks for source-only packages, dependencies, capabilities, and
  integrity hashes; and
- bounded, finite `storage.v1` values scoped by account, package, and schema.

Native protected-action provenance is kept in the dependency-light
`mmorpg-client-secure-input` crate. It rejects repeats, replayed physical event
IDs, stale focus/node generations, and unloaded addons before yielding a
single-use trusted intent. The graphical client now routes its default Space
attack and a bounded primary-pointer hit region through that registry before
queuing the ordinary typed attack intent, and refreshes the binding across
native window focus transitions. Scripted action presentation and the physical
Wayland proof remain Milestone 4 work; this integration is not evidence that
scripts can activate protected actions.

These are policy tests and a host-adapter foundation. They do not yet prove
that a Luau VM, renderer, filesystem adapter, or native input path enforces
the contract under hostile load.

## Goals

- Allow players to customize the UI.
- Build the default UI using the same public scripting API.
- Permit rich presentation and layout behavior.
- Prevent official addons from automating gameplay.
- Isolate addon failures from the client and default UI.

The language is still open. An embeddable language such as Lua is a candidate, but the host API is the primary security boundary.

## Permitted capabilities

Player scripts may be able to:

- Create and arrange frames.
- Draw text, textures, and approved visual elements.
- Register for UI and permitted game-state events.
- Query state already visible to the client.
- Display combat information.
- Create menus and action bars.
- Save user preferences and addon data within quotas.
- Request presentation changes.

## Protected capabilities

Player scripts should not receive:

- Arbitrary file access.
- Network sockets.
- Process execution.
- Native library loading.
- Engine-memory access.
- Unrestricted reflection.
- Hidden-entity queries.
- Arbitrary gameplay packets.
- Direct movement commands.
- Direct spell-casting commands.
- Automatic target selection.
- Automatic interaction.

Protected gameplay actions should require an explicit user-input context. A script may display or configure an action button, but should not be able to activate protected actions from a timer, combat event, or arbitrary callback.

Every resulting gameplay command is still validated by the server.

## Runtime budgets

Each addon should have limits for:

- Memory.
- CPU or instruction time per frame.
- Event-dispatch frequency.
- Number of registered events.
- Number of created UI objects.
- Saved-variable size.
- Recursion depth.
- Repeated errors.

An addon that exceeds limits should be disabled or throttled without crashing the game client.

## Addon package model

An addon package should contain:

- Manifest.
- API version.
- Namespace.
- Dependency declarations.
- Load order.
- Capability declarations.
- Saved-data declarations.
- Optional signing or trust metadata.

The package format should support compatibility checks and clear failure messages.

## Default UI

The default UI must use the same public UI runtime and APIs available to players. Privileged behavior should be kept narrow and concentrated in secure input-binding primitives rather than in a completely separate default-UI framework.

This ensures that the addon system is exercised continuously and that the public API is not merely a limited imitation of the internal UI.

## Security boundary

The sandbox must be tested as an adversarial boundary. Test cases should include:

- Attempted file access.
- Attempted process execution.
- Native-module loading.
- Unbounded recursion.
- Memory exhaustion.
- Event storms.
- Timer-based action automation.
- Indirect protected-action invocation.
- Hidden-state queries.
- Addon-to-addon privilege escalation.

The project is not initially required to provide a complete external anti-bot system. It is required to keep the official addon API from becoming a gameplay bot API.
