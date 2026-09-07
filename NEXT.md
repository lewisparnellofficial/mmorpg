# Next Steps

## 1. Prove the editor with a real native tablet shell

Build the smallest Linux editor shell that connects an actual Qt or SDL tablet callback to `TabletEventBridge`. Demonstrate physical proximity, press, motion, release, cancellation, pressure-based terrain editing, mouse fallback, stroke replay, save/reload, and stroke-level undo; record hardware and responsiveness results in a new experiment instead of treating synthetic unit tests as hardware evidence.

## 2. Make typed wire the default client path

Move the graphical client and launch scripts to typed-wire mode by default, covering authentication, character selection, world entry, snapshot bootstrap, events, reconnect, and stale-entity replacement. Keep the line protocol only as a diagnostic fallback until equivalence tests pass, then close or remove its unauthenticated gameplay surface according to the protocol plan.

## 3. Stabilize the versioned UI host contract

Turn the current Luau API into an explicit language-neutral `ui.v1` contract with immutable view-model records, event names, bounded queues, coalescing rules, error semantics, and version compatibility behavior. Define bounded saved-data storage and package-manifest responsibilities before adding a larger widget or gameplay-facing API.

## 4. Implement real secure-input provenance

Replace the prototype’s native-only `secure_input` placeholder with a design and testable token flow tied to trusted physical input context. Prove that scripts can render and configure action presentation but cannot activate protected actions from timers, event callbacks, replayed tokens, forged handles, or addon-to-addon calls; retain server authority as the final validation layer.

## 5. Harden the addon boundary adversarially

Extend the eight passing Luau tests with fuzzing and stress coverage for event payloads, value conversion, handle ownership, recursion, allocation, event storms, repeated failures, unload/reload, and disabled-addon behavior. Calibrate instruction, memory, queue, node, and storage budgets on the minimum supported Linux client, and compare against Wasm only if Luau exposes a concrete isolation or resource-control limitation.

## 6. Return to the main vertical slice

Once the editor and addon experiments have real integration evidence, stop expanding technology spikes and resume the main gameplay sequence. Select the next authoritative-server milestone from the generated `PLAN.md`, preserving typed identity, audience filtering, bounded intake, durable commit-before-publish behavior, and the repository’s existing validation/documentation workflow.
