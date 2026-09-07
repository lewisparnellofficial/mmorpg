# Player UI Scripting Spike

**Status:** Research complete; Luau implementation spike measured; production
recommendation remains provisional

**Date:** 2026-09-04

**Scope:** The runtime, host API, packaging, scheduling, and adversarial
validation requirements for player-authored UI addons. This record compares
Luau, Rhai, WebAssembly hosted by Wasmtime, Lua 5.4 through a Rust binding,
Rune, and Wasmi as a possible WebAssembly interpreter. It is specifically
about untrusted client-side presentation code. It does not authorize scripts
to run on the server, modify the authoritative simulation, or issue gameplay
commands.

This document was prepared after reading the accepted
[requirements baseline](../architecture/requirements.md), the proposed
[UI-scripting architecture](../architecture/ui-scripting.md), the proposed
[system overview](../architecture/system-overview.md), and the
[client-technology spike](client-technology-spike.md).

## Executive result

The leading implementation direction is:

1. Use **Luau** for the first UI scripting prototype, embedded from Rust
   through a narrow adapter such as `mlua`'s Luau feature or a project-owned
   C-API wrapper.
2. Treat the **host capability API as the security boundary**. The script
   language is only one layer of defense. No script receives a command
   constructor, raw engine object, filesystem handle, socket, process API,
   native module loader, or arbitrary reflection surface.
3. Run the default UI and each player addon through the **same versioned public
   UI API and view-model schema**. The default UI may be installed by the
   client distribution, but it must not receive gameplay capabilities hidden
   from addons.
4. Represent gameplay-affecting actions as **user-originated secure intents**.
   A script may create an action-button presentation and may ask the host to
   show that button as available. Only trusted native input handling may turn
   a physical input gesture into a protected intent. The server remains the
   final authority.
5. Use a **cooperative, event-driven scheduler** with per-addon budgets,
   bounded queues, and a host-side wall-clock watchdog. A script callback must
   never run directly on the render thread without a budget and an error
   boundary.
6. Keep **Wasmtime/Wasm as the strongest isolation alternative** if the
   project later decides that supporting independently authored or more
   adversarial modules is worth the language/toolchain/ABI cost. Wasmi is a
   smaller interpreter-based alternative for the same model.

This is not a security proof or a final language selection. The first local
spike should implement the same minimal API twice: once with Luau and once
with a deliberately hostile test suite. Acceptance depends on the host API
and test results, not on a hello-world script.

### Local implementation result

The first Luau host spike now exists in
[`experiments/ui-scripting`](../../experiments/ui-scripting/README.md), with
the measured results recorded in
[EXP-006](../experiments/EXP-006-ui-scripting-sandbox.md). It uses one embedded
state per addon, a small `ui.v1`-shaped API, explicit source/memory/
instruction/node/event/text limits, globally unique opaque node IDs, and a
native-only secure-input method. Eight adversarial tests pass locally,
including OS/module API absence, infinite-loop interruption, cross-addon handle
forgery, resource limits, and addon-only failure disablement.

This validates the direction of the first prototype only. It does not select
Luau as the final runtime, establish process-level isolation, measure render
thread latency, or connect secure input to a real window system.

## Project constraints

### Sourced facts from project documents

The repository requirements establish that:

- The game client runs on Linux.
- Players can customize the UI with scripts.
- The default UI uses the same scripting system and public UI API as player
  addons.
- Scripts must be sandboxed and must not automate gameplay through the
  official addon API.
- The client is an untrusted presentation and input device; the server owns
  authoritative movement, combat, progression, economy, and persistence.
- Addons may query state already visible to the client, create presentation
  elements, register for permitted UI/game-state events, save preferences
  within quotas, and request presentation changes.
- Addons must not have arbitrary file access, network sockets, process
  execution, native library loading, engine-memory access, unrestricted
  reflection, hidden-entity queries, arbitrary gameplay packets, direct
  movement, direct spell casting, automatic targeting, or automatic
  interaction.
- Each addon needs limits for memory, CPU/instruction time, event frequency,
  event registrations, UI objects, saved data, recursion, and repeated
  errors.
- The sandbox must be tested against file/process access, native-module
  loading, recursion, memory exhaustion, event storms, timer-based action
  automation, indirect protected-action invocation, hidden-state queries, and
  addon-to-addon privilege escalation.

These are requirements from the project, not claims about any third-party
runtime. See the [requirements baseline](../architecture/requirements.md) and
[UI-scripting architecture](../architecture/ui-scripting.md).

### Project inferences

- The addon system is a **client presentation subsystem**, not a second
  gameplay API. Its outputs should be UI mutations, local preferences, and
  telemetry/diagnostics—not server commands or authoritative state changes.
- “Same API” should mean the same documented host functions, event names,
  view-model types, capability checks, and error semantics. It does not
  necessarily mean the same package, source files, or load order.
- The safest default UI is one that uses the public API continuously, while a
  small native bootstrap layer owns only capabilities that must not be exposed
  to scripts, such as secure input capture, renderer integration, package
  verification, and addon lifecycle management.
- A script can automate gameplay indirectly even when no function is named
  `cast_spell`: for example, it might synthesize a click, invoke a callback
  retained by a secure button, abuse focus/navigation events, or infer hidden
  state from timing. The design must therefore classify **effects and
  provenance**, not only function names.
- Since the client can be modified by a user, this sandbox is primarily a
  protection boundary against accidental or malicious addon code affecting
  the local client, leaking local data, degrading the client, or turning the
  official API into an automation convenience. It is not a substitute for
  server validation or a complete external anti-bot system.

## What “sandbox” must mean here

There are several different properties that are often conflated:

| Property | Meaning for this project | Required? |
| --- | --- | --- |
| Language safety | Script cannot directly perform native memory or arbitrary host operations through normal language facilities | Yes |
| Capability isolation | Only explicitly registered host capabilities exist, and handles cannot be forged or widened by script code | Yes |
| Resource bounds | CPU, memory, stack, queue, timers, UI nodes, and saved data have enforceable limits | Yes |
| Failure isolation | A syntax/runtime/resource error disables one addon without taking down the default UI or client | Yes |
| Gameplay non-automation | No script-originated path can cause a protected gameplay intent without secure user provenance | Yes |
| Deterministic replay | The same script receives a stable event/value order when replayed under a fixed API contract | Desirable for tests; not a gameplay authority property |
| OS isolation | A compromised runtime or native dependency cannot access the host process/OS | Desirable defense in depth; not supplied by an addon API alone |
| Formal verification | A machine-checked proof that the complete runtime and host bindings are secure | Not an initial requirement; no candidate provides this for the whole stack |

**Recommendation.** Define the project sandbox as the composition of:

```text
package verifier
    -> language/runtime policy
        -> isolated script instance
            -> read-only view models + UI capability handles
                -> scheduler and quota accounting
                    -> secure native input boundary
```

Every arrow is part of the security review. A runtime that prevents file I/O
but exposes a `click()` function connected to an ability button is still an
automation API. Conversely, a simple language can be acceptable for cosmetic
UI if its host surface is narrow and its quotas are enforced.

## Candidate comparison

The ratings are project assessments based on the cited documentation and the
current requirements. They are not benchmark results or security proofs.

| Candidate | Linux/Rust fit | Isolation and limits | UI ergonomics | Packaging/deployment | Main concern | Initial position |
| --- | --- | --- | --- | --- | --- | --- |
| **Luau** | Linux-capable C++ VM; Rust embedding needs a binding or wrapper | Official sandbox helpers, isolated environments, interrupts, and memory accounting; host API still decisive | Lua-family syntax, dynamic tables, coroutines, approachable addon model | Source or bytecode can be shipped; package policy is project-owned | C++ runtime/FFI boundary and the need to audit every binding | **Lead prototype** |
| **Rhai** | Rust-native embedding and straightforward Cargo integration | Explicit limits for operations, call depth, variables, functions, strings, arrays, maps, and modules; immutable engine pattern | Rust-like scripting and easy native function registration | Text/AST policy is project-owned; language/runtime version compatibility must be managed | Host registrations and feature flags can accidentally remove protections; smaller addon ecosystem | **Strong Rust-native comparison** |
| **Wasm + Wasmtime** | Rust-native host; guest can be compiled from many languages | Linear-memory isolation, explicit imports, fuel/epoch interruption, resource limiters, and capability-oriented WASI when used | Depends on SDK/ABI; less immediately approachable for casual UI authors | Strong module boundary; ABI/component versioning and toolchain become project work | Larger runtime/toolchain and host ABI; async integration complexity | **Isolation fallback / serious second spike** |
| **Lua 5.4 via mlua** | Mature C VM exposed through a Rust binding | Standard library is modular, but the host must omit unsafe libraries and install hooks/memory limits; `mlua` documents sandbox support only for Luau | Familiar Lua ecosystem and syntax | Easy source packaging; bytecode trust and version policy are project-owned | Lua 5.4 is not Luau; default APIs and sandbox guarantees differ | **Fallback if Luau integration fails** |
| **Rune** | Rust-native dynamic language and VM | Provides controlled native modules and a stack-isolated VM, but no project-specific security claim comparable to Luau or Wasm | Rich Rust-like language, modules, async, macros; likely more than UI authors need | Own source/compiled-unit policy and API stability burden | Larger language surface and less established untrusted-addon evidence | **Research-only alternative** |
| **Wasmi + Wasm** | Rust-native interpreter; useful where JIT/native compilation is undesirable | Fuel, resumable execution, stack/resource limits, and enforced module limits are documented | Same guest-ABI burden as Wasmtime | Compact interpreter option; toolchain and component policy still needed | Interpreter performance and maturity/feature tradeoffs require local measurement | **Wasm interpreter comparison** |
| **Native Rust/plugin DLL or shared object** | Excellent Rust integration and raw performance | No acceptable sandbox for untrusted player addons; native code has process authority | Any API can be built | Compile/install/toolchain burden | A loaded native module can bypass the intended boundary | **Reject for player addons** |

### Luau

**Sourced facts.** Luau's official embedding documentation describes Luau as
safe to embed against untrusted code under its documented assumptions, while
also explicitly stating that the complete C++ stack is not formally proven.
The sandbox guidance describes removing unsafe Lua 5.x facilities, isolating
global environments, making built-ins/read-only tables immutable, interrupting
running code, and tracking memory. It warns that memory limits do not happen
automatically and that a single long-running host C function can still evade a
VM-level prompt interrupt. [Luau sandbox embedding guide](https://luau.org/sandbox/),
[Luau C API](https://luau.org/api/), and
[Luau project overview](https://luau.org/)

`mlua` is a Rust binding whose API documents a Luau-only `sandbox` operation.
The documented operation makes libraries, built-in metatables, and globals
read-only, establishes a local environment, and restricts `collectgarbage`.
The same API documents memory limits and hooks/interruption facilities for
execution. [mlua `Lua` API](https://docs.rs/mlua/latest/mlua/struct.Lua.html)

**Project inference.** Luau maps well to the project's goals because the
language documentation addresses isolation primitives directly and the Lua
programming model is well suited to declarative UI construction, callbacks,
tables, and lightweight local state. The C++ implementation is not by itself
a blocker for an otherwise Rust project, but it adds a native dependency and
requires a narrow, audited Rust binding boundary.

**Recommendation.** Prototype Luau first, preferably through a binding that
exposes only the subset required by the project. Do not enable general module
loading, file access, process APIs, arbitrary userdata methods, or a general
debug API. Build one immutable standard environment and a fresh per-addon
environment. Treat `mlua`'s sandbox helper as a baseline, not as the complete
project policy.

**Unresolved risks.** The project still needs to validate the exact Luau
version, Linux build/link behavior, interrupt behavior while callbacks are
running, memory accounting for host-created UI objects, bytecode/source
compatibility, and whether a particular Rust binding exposes all controls
without making unsafe or overly broad abstractions convenient.

### Rhai

**Sourced facts.** Rhai's documentation describes an embedded Rust scripting
engine and a sandboxing model in which scripts cannot read outside their own
environment. It recommends an immutable engine and explains that external
state should be exposed through deliberately registered control-layer APIs.
[Rhai sandboxing](https://rhai.rs/book/safety/sandbox.html)

Rhai documents an `on_progress` callback invoked once per operation, which can
terminate an evaluation, and documents configurable limits for expression
depth, call depth, operations, variables, functions, modules, strings, arrays,
maps, and interned strings. Its safety documentation warns that enabling the
`unchecked` feature disables important protections and can allow a malicious
script to bring down the host. [Rhai runtime limits](https://rhai.rs/book/safety/progress.html),
[Rhai engine options](https://rhai.rs/book/engine/options.html),
[Rhai safety guidance](https://rhai.rs/book/safety/index.html), and
[Rhai feature flags](https://rhai.rs/book/start/features.html)

**Project inference.** Rhai offers the clearest Rust-native route to a small
prototype and has useful interpreter-level quotas. Its safety story remains
configuration- and host-dependent: an innocent-looking registered function
can expose arbitrary Rust state or block for an unbounded time. Its operation
counter is a budget mechanism, not wall-clock accounting, and operation count
does not represent a stable amount of real work across all host functions.

**Recommendation.** Keep Rhai as the first Rust-native comparison. If tested,
compile without `unchecked`, disable modules and custom syntax unless needed,
use an immutable engine after registration, register only project-owned value
types, and wrap every host function with its own time/size policy. Never
interpret an `on_progress` success as proof that a host call is bounded.

**Unresolved risks.** The project needs to determine whether Rhai's language
and object model are pleasant enough for UI authors, whether its dynamic value
conversion causes unacceptable allocation churn, how module/AST caching should
be versioned, and whether error/panic behavior remains isolated under the
chosen feature set and binding patterns.

### WebAssembly hosted by Wasmtime

**Sourced facts.** WebAssembly describes a portable, sandboxed stack-machine
execution environment. Wasmtime documents fuel consumption for halting or
yielding execution, epoch-based interruption, maximum Wasm stack settings,
and per-store resource limiters for memories, tables, and instances.
Wasmtime's documentation distinguishes fuel as the deterministic mechanism
and epochs as the lower-overhead, non-deterministic interruption mechanism.
It also warns that neither fuel nor epochs can interrupt a blocking host call;
the host must use bounded, asynchronous integration and external timeouts for
that case. [WebAssembly overview](https://webassembly.org/),
[Wasmtime `Config`](https://docs.wasmtime.dev/api/wasmtime/struct.Config.html),
[Wasmtime `Store`](https://docs.wasmtime.dev/api/wasmtime/struct.Store.html),
[Wasmtime interruption example](https://docs.wasmtime.dev/examples-interrupting-wasm.html),
and [Wasmtime limits](https://docs.wasmtime.dev/api/src/wasmtime/runtime/limits.rs.html)

WASI's design principles describe capability-based security with unforgeable
resource handles and no ambient authorities. Wasmtime's WASI context builder
documents that there are no preopened directories by default and that access
must be explicitly configured; it also documents that network access is denied
by default at the address-check layer. The project should expose no WASI
imports at all for UI addons unless a future requirement proves one necessary.
[WASI design principles](https://github.com/WebAssembly/WASI/blob/main/docs/DesignPrinciples.md),
[Wasmtime WASI context](https://docs.rs/wasmtime-wasi/latest/wasmtime_wasi/struct.WasiCtxBuilder.html)

**Project inference.** Wasm gives a stronger structural boundary than an
embedded dynamic language: guest linear memory and imports are explicit, and
the runtime can reject modules with excessive memory, tables, functions, or
other resources before execution. However, Wasm does not define this project's
UI API, package trust model, host object graph, or protected-input semantics.
An unrestricted or blocking host import can recreate the same vulnerability
under a different name.

**Recommendation.** Keep Wasmtime as the serious alternative if the project
expects third parties to author more complex or independently compiled
modules, wants a language-neutral ABI, or later needs stronger runtime
compartmentalization. For UI-only addons, expose a tiny custom import module
or a versioned component interface and omit WASI. Start each addon in its own
store/context, cap linear memory and table growth, set fuel, use host-side
timeouts, and make the ABI return data-oriented UI operations rather than
references to engine objects.

Use fuel for deterministic test budgets and reproducible “over budget”
behavior. Use epochs or async fuel yielding only when the scheduler needs to
cooperatively return to the client event loop. Any imported function must be
nonblocking or explicitly asynchronous with a bounded future.

**Unresolved risks.** A Wasm SDK for casual UI authors, API ergonomics, module
signing, component-model stability, binary size, startup time, hot reload,
debugging, and the performance cost of serializing view models all need local
measurement. Wasmtime's store lifetime and resource ownership also need to be
aligned with addon unload/reload so discarded addons do not retain memory.

### Lua 5.4 through `mlua`

**Sourced facts.** The official Lua 5.4 manual lists the standard libraries as
separate modules, including I/O, operating-system, package, and debug
facilities. It documents functions such as `dofile`, `loadfile`, `io`, and
`os`, and warns that maliciously crafted binary chunks can crash the
interpreter. [Lua 5.4 reference manual](https://www.lua.org/manual/5.4/)

`mlua` documents support for Lua 5.4 and Luau, including loading selected
standard libraries, sandboxing for Luau, memory limits, execution hooks, and
interrupts. The documented `sandbox` helper is specifically marked as
available with the Luau feature, so it should not be assumed to provide the
same turnkey policy for stock Lua 5.4. [mlua `Lua` API](https://docs.rs/mlua/latest/mlua/struct.Lua.html),
[mlua thread hooks](https://docs.rs/mlua/latest/mlua/thread/struct.Thread.html)

**Project inference.** Stock Lua is technically viable, familiar, and likely
to have the largest body of UI examples. It is not equivalent to Luau's
documented untrusted-embedding profile. The project would need to construct a
minimal library set, prohibit binary chunks or validate them, install hooks,
limit memory, and carefully audit every userdata/metamethod exposed to scripts.

**Recommendation.** Use Lua 5.4 only as a fallback if Luau's native
integration or language compatibility becomes a blocker. If selected, ship a
project-owned “safe Lua” profile with no `io`, `os`, `package`, `debug`,
`dofile`, `loadfile`, or arbitrary `load`; do not load the full standard
library; and retain the same host API and secure-input rules as the Luau
prototype.

**Unresolved risks.** The team must decide whether ecosystem familiarity is
worth maintaining a custom hardening profile, how to handle bytecode/source
distribution, and whether Lua's C ABI is an acceptable long-term dependency
for a Rust/Linux client.

### Rune

**Sourced facts.** Rune's Rust API describes it as an embeddable dynamic
language with a stack-based VM, Rust module registration, hot reloading,
multithreaded execution, async support, and stack isolation between function
calls. Its module API exposes a way for the host to register native functions
and types. [Rune crate documentation](https://docs.rs/rune/latest/rune/),
[Rune module API](https://docs.rs/rune/latest/rune/struct.Module.html)

**Project inference.** Rune is attractive for a Rust-only toolchain and can
model structured UI code, but its rich language surface is a liability for a
first untrusted-addon boundary. The reviewed primary documentation does not
establish the same dedicated hostile-embedding guidance as Luau, the same
fuel/resource control model as Wasmtime, or a project-ready prohibition on
blocking/native capabilities. “Memory safe through reference counting” does
not mean that registered host functions are harmless or bounded.

**Recommendation.** Do not select Rune for the first addon prototype. It can
remain a research candidate for trusted development-time UI tooling or a
future internal scripting layer, provided a separate security review defines
limits, module policy, and blocking-call behavior.

### Wasmi as an interpreter alternative

**Sourced facts.** Wasmi documents itself as a Rust WebAssembly interpreter.
Its configuration supports fuel consumption, maximum stack settings, and
enforced limits for module parsing/translation. It documents an out-of-fuel
trap and resumable calls, allowing an embedder to deterministically halt or
resume execution. [Wasmi crate documentation](https://docs.rs/wasmi/latest/wasmi/),
[Wasmi configuration](https://docs.rs/wasmi/latest/wasmi/struct.Config.html),
[Wasmi traps](https://docs.rs/wasmi/latest/wasmi/enum.TrapCode.html), and
[Wasmi enforced limits](https://docs.rs/wasmi/latest/src/wasmi/engine/limits/engine.rs.html)

**Project inference.** Wasmi is relevant if a smaller interpreter footprint,
predictable execution, or avoiding JIT/native code generation is more
important than peak addon execution speed. It does not eliminate the need for
a project-owned UI ABI or host capability policy; it is an alternative
execution substrate, not an addon design.

**Recommendation.** Compare Wasmi only after the project has a concrete Wasm
ABI. Measure startup, memory, event throughput, and frame impact against
Wasmtime using identical modules. Do not broaden the API merely because the
interpreter appears easier to reason about.

## Capability matrix

The following matrix is the proposed **project policy**, not a claim that a
runtime enforces every row automatically. “Host only” means the capability is
implemented by trusted native client code and is never exposed as a script
function or import. “UI-scoped” means it operates only on handles owned by the
current addon and on approved presentation data.

| Capability | Default UI | Player addon | Script mechanism | Enforcement |
| --- | --- | --- | --- | --- |
| Read visible player/target/party data | Yes | Yes | Immutable view-model snapshot | Host constructs a filtered snapshot; no live engine references |
| Read inventory/quest/combat presentation data | Yes | Yes | Immutable view-model snapshot | Exclude hidden entities, secrets, server-only fields, and unobserved data |
| Subscribe to whitelisted UI/state events | Yes | Yes | Subscription token | Per-addon event count, queue size, frequency, and payload limits |
| Create UI nodes | Yes | Yes | `ui.create` returning opaque node handle | Per-addon node quota, depth/child quota, text/texture size limits |
| Mutate own UI nodes | Yes | Yes | UI-scoped handle methods | Ownership check; no cross-addon handles; bounded property sizes |
| Read approved theme/style constants | Yes | Yes | Immutable value tables | Copy values; do not expose renderer objects or reflection |
| Use approved textures/fonts | Yes | Yes | Content-ID lookup | Allowlist, package ownership, memory/decode limits |
| Local animation/tween | Yes | Yes | Bounded declarative animation | No callback that invokes protected input; cap active animations |
| Create menus/action-bar presentation | Yes | Yes | UI operations and local callbacks | Presentation only; secure action binding remains native |
| Save preferences/addon data | Yes | Yes | Key/value storage | Namespace, serialized-size, key-count, and write-frequency quota |
| Timers for visual refresh | Yes | Yes | Budgeted scheduler timer | Timer callback may update UI only; rate and count quotas |
| Receive raw keyboard/mouse/stylus events | Narrow native path | Restricted | Sanitized local UI events | No raw event injection into secure controls; focus and provenance checked |
| Bind a physical input to a protected gameplay action | Host only | No | None | Native secure-input registry; server validates resulting intent |
| Ask the host to activate a gameplay action | Host only | No | None | No command constructor or action activation import |
| Target, move, cast, interact, purchase, trade, loot | Host/server only | No | None | Intent must originate from explicit user input; server authority remains final |
| Query hidden entities or world state | No | No | None | View-model projection filters before script delivery |
| Read files or environment variables | No | No | None | No OS/WASI imports; runtime process policy is defense in depth |
| Open sockets or make network requests | No | No | None | No socket/WASI imports; network adapter is native only |
| Spawn processes or load native modules | No | No | None | No host imports; package verifier rejects native payloads |
| Access engine ECS/renderer/database objects | No | No | None | Data copies and opaque UI handles only |
| Access another addon’s globals or storage | No | No | None | Separate VM/environment and namespace checks |
| Install new host functions at runtime | Host bootstrap only | No | None | Immutable host module after initialization |

### Capability design rule

Prefer APIs that return **data** or **opaque, capability-scoped handles** over
APIs that return engine objects. For example:

```text
good:  ui.create_panel(parent_handle, descriptor) -> own_node_handle
good:  state.player() -> immutable_player_view
bad:   engine.find_entity(entity_id) -> live_entity_reference
bad:   input.click(widget_handle)
bad:   client.send(command_bytes)
```

Handles must be unforgeable within the scripting model, checked for addon
ownership, invalidated on unload, and invalid after their underlying UI node
is destroyed. Numeric IDs are not sufficient by themselves if an addon can
guess another addon’s ID; the host must associate each handle with an owner
and generation.

## One API for the default UI and addons

### Layered architecture

The proposed architecture is:

```text
authoritative server events/snapshots
              |
      client presentation model
              |
  filtered read-only view models
              |
       versioned UI host API
          /             \
 default UI instance   addon instances
          \             /
        UI operation sink
              |
    native renderer/UI backend
```

The native client owns the presentation model, package verifier, script
loader, scheduler, quota meter, and renderer adapter. The script layer sees
only a stable API facade. Both default UI and addons receive equivalent
view-model schemas and event types. They differ only in package metadata,
load ordering, update policy, and whether an addon is enabled by the player.

### Shared public contract

The public API should have explicit versioned namespaces rather than exposing
the host language's global namespace directly:

```text
ui.v1.create_panel(descriptor)
ui.v1.set_property(node, property, value)
ui.v1.destroy(node)
ui.v1.subscribe(event_name, callback)
ui.v1.timer.after(delay_ms, callback)
ui.v1.timer.every(period_ms, callback)
game.v1.player_view()
game.v1.target_view()
game.v1.inventory_view()
game.v1.quest_view()
game.v1.combat_feed()
storage.v1.get(key)
storage.v1.set(key, value)
```

This is illustrative API shape, not an accepted public API. The important
properties are:

- `ui` operations affect only presentation.
- `game` operations return filtered copies or immutable views.
- `storage` is scoped to the package and character/account policy selected by
  the host; it is not filesystem access.
- No `input.activate`, `send_command`, `cast`, `move`, `target`, `interact`,
  or similarly indirect function exists in the script namespace.
- API functions return structured success/error values or raise a bounded
  script-visible error; they do not leak Rust pointers or engine exceptions.
- New APIs are additive where possible, and every package declares the API
  range it was authored against.

### Default UI privilege policy

The default UI should not be a privileged exception to the public API. It may
have a small native bootstrap that creates the initial root surface and
installs secure input bindings, but the layout, panels, bars, tooltips, combat
feed, quest windows, and vendor windows should call the same public functions
available to an addon.

If a feature genuinely requires privilege, put it in a narrow native service
with an explicit threat-model justification. Do not silently add the function
to the default UI's script environment. The difference between “default UI”
and “addon” should be visible in the capability manifest and logs.

### Event and view-model contract

The host should publish immutable, bounded event payloads such as:

- `ui.ready`, `ui.scale_changed`, `ui.focus_changed`.
- `player.updated`, `target.updated`, `party.updated`.
- `inventory.updated`, `quest.updated`, `combat_message.received`.
- `vendor.opened`, `vendor.closed`, `notification.received`.

The event payload must contain only information already visible to the client
and must have a documented maximum size. Events should carry a monotonic
client sequence number for diagnostics, but scripts must not be able to use
sequence/timing behavior to access hidden state. If the game later exposes
combat timing, it should be intentionally coarsened or classified as public
presentation data rather than leaking server scheduling details.

**Recommendation.** Treat the existing renderer-independent
`mmorpg-client-model` as the source for script-facing view models, with a
separate adapter that converts model changes into immutable script values. Do
not let scripts access core simulation structs directly.

## Preventing gameplay automation

### Explicit user-input provenance

Every protected action must carry a provenance record in the native client:

```text
ProtectedIntent {
    action: ActionId,
    source: SecureInputToken,
    physical_event_id: PhysicalEventId,
    focus_generation: FocusGeneration,
    created_at: MonotonicTimestamp,
}
```

This is conceptual, not a committed Rust type. The token should be minted by
the native input layer only while processing an eligible physical input event
and should be single-use or short-lived. Script callbacks, timers, game-state
events, animation completions, promises, and synthetic UI events must not be
able to mint or forward it.

The client may use a script to render an action button and to update its
enabled/disabled appearance. When the user physically activates a secure
button, the native input layer resolves the binding to an action ID and emits
an ordinary client intent. The script is not called as the authority for that
activation. The server then checks session, actor, target, cooldown, range,
resource, state, and rate limits as appropriate.

### Prohibited indirect paths

The security review must reject all of these paths, not only an obvious
`cast_spell()` function:

- A timer callback that calls a UI method which activates a secure button.
- A combat event callback that synthesizes a click or key event.
- A script-created node that overlays a secure control and captures its
  activation.
- Programmatic focus traversal that activates the focused action.
- A callback passed to a native button that is invoked without a physical
  input token.
- A script reading a protected control's internal action binding and replaying
  it through another path.
- A script generating raw network bytes or calling a generic “dispatch input”
  function.
- Two addons cooperating through shared globals, storage, or event payloads to
  create a hidden action queue.
- A script inferring hidden entities or cooldown state from a host response
  difference and using that information to schedule automated actions.

**Recommendation.** Make the script-side event system incapable of returning
or accepting `ProtectedIntent`. Test the invariant at the type/adapter
boundary and again in the native secure-input registry. Server validation is
the final backstop, not the only control.

## Runtime lifecycle and event-loop integration

### Scheduling model

Scripts should not execute arbitrary code on every render call. Use a host
scheduler with these stages:

```text
native frame/input collection
    -> authoritative event ingestion
        -> immutable view-model update
            -> bounded event coalescing
                -> per-addon ready queues
                    -> budgeted script dispatch
                        -> validated UI operation buffer
                            -> renderer commit
```

The operation buffer prevents a script from mutating the renderer or UI tree
halfway through a callback. The host validates each operation, applies the
addon ownership check, charges quotas, and commits accepted operations at a
safe point. A failed callback discards its uncommitted operation buffer.

### Per-addon execution context

Each addon should have:

- A stable package ID and instance generation.
- Its own VM/environment/store and host state.
- A ready queue with a maximum event count and byte size.
- A timer wheel or host timer entries owned by that addon.
- A UI-node ownership set.
- A storage namespace and quota meter.
- A cumulative error counter and disable state.
- A cancellation token used during unload, reload, and shutdown.

The default UI may use one or more instances for compositional reasons, but
it should not share mutable globals or live engine objects with addons.

### Frame budget policy

The project should choose initial values through measurement rather than
pretending that a fixed number is portable across hardware. The policy should
nevertheless define all dimensions:

| Budget | Unit | Enforcement point | Initial policy shape |
| --- | --- | --- | --- |
| Callback CPU | Monotonic microseconds and runtime units | Before/while dispatching callback | Hard per-callback ceiling; defer remaining work |
| Frame CPU | Microseconds per addon per frame | Scheduler | Soft target plus hard burst ceiling |
| Event rate | Events/second and queue bytes | Event enqueue | Coalesce replaceable state; drop or disable on storms |
| Timers | Active timers and minimum interval | Timer registration/dispatch | No zero-delay unbounded loops; minimum period and count quota |
| VM memory | Runtime bytes/pages | Allocator or store limiter | Hard cap; disable on exceeded allocation |
| UI nodes | Count, depth, children, text/texture bytes | Operation validation | Hard cap and per-frame creation quota |
| Saved data | Bytes, keys, writes/minute | Storage adapter | Namespaced, serialized-size and write-frequency limits |
| Stack/recursion | Call depth/stack bytes | Runtime and host | Runtime limit plus script-level disable on repeat |
| Host calls | Calls/frame and serialized argument bytes | Binding wrapper | Per-function and aggregate quotas |
| Errors | Errors/minute and consecutive failures | Error boundary | Backoff, then disable only that addon |

An execution budget must include time spent in host functions and data
conversion. A runtime instruction counter alone cannot bound a Rust function
that scans a large table, decodes a texture, allocates UI nodes, or blocks on
I/O.

### Yielding and backpressure

Scripts should process one bounded callback at a time. Long work should be
split by the script through explicit host-supported continuation/yield points,
not by allowing arbitrary blocking coroutines to run on the render thread.

- Coalesce replaceable events such as `target.updated` and `ui.scale_changed`.
- Preserve ordered delivery for small, semantically important events such as a
  server rejection or a quest reward notification.
- Drop low-priority visual updates before dropping lifecycle or security
  events.
- Stop delivering events to a disabled addon and release its queue and owned
  UI nodes.
- Apply backpressure to addon storage writes and logging.
- Never run addon code on the network I/O thread.

**Runtime-specific note.** Rhai's operation progress callback and Luau's
interrupt callback are useful for terminating a runaway synchronous callback.
Wasmtime fuel can provide deterministic exhaustion and can be configured for
cooperative async yielding; Wasmtime epochs are lower-overhead but not
deterministic. Regardless of runtime, the host still needs a wall-clock
watchdog, because a host call may dominate the callback.

## Packaging, trust, and versioning

### Package contents

An addon package should contain at least:

```text
manifest.toml/json
script source or validated runtime artifact
optional precompiled artifact/cache
optional UI assets referenced by content ID
optional localization data
optional schema/API declarations
integrity hash
```

The manifest should declare:

- Stable package ID and human-readable name.
- Package version using a documented SemVer policy or project equivalent.
- Required UI API range and runtime ABI/engine version.
- Entry points and load order.
- Dependencies by stable package ID and compatible version range.
- Requested capabilities, which must be a subset of the allowed addon policy.
- Declared storage namespaces and maximum data size.
- Asset references and size/type limits.
- Source/artifact format and compiler/runtime version.
- Optional signature/trust metadata.

The manifest is a request and description, not a grant. The host computes the
effective capability set as the intersection of package declaration, user
policy, client policy, and security profile.

### Source versus bytecode/module artifacts

**Project inference.** Source distribution is easier to inspect and debug but
has parse/compile cost and may expose authorship. Bytecode/module distribution
can reduce startup work but is not automatically trustworthy: a bytecode file
still has to be validated by the selected runtime and matched to the exact
language/runtime/API version. Lua's official manual explicitly warns about
maliciously crafted binary chunks, so “compiled” must not be treated as
“safe.” [Lua binary-chunk warning](https://www.lua.org/manual/5.4/manual.html)

**Recommendation.** For the first prototype, accept source or a project-owned
validated artifact but always retain the source hash, compiler/runtime
version, API version, and package hash in logs. Do not support arbitrary native
plugins. If bytecode is later shipped, validate it offline and again at load
time, reject incompatible versions, and keep a source-mode debug path.

### API compatibility

Use semantic versioning for the public UI contract only if the project defines
what compatibility means. A practical rule is:

- Major API version: removes or changes semantics of a function, event,
  property, or security rule.
- Minor API version: adds optional capabilities/events that older addons can
  ignore.
- Patch API version: documentation or bug fixes with no contract change.

The runtime version, language version, host ABI version, and UI API version
should be recorded separately. A runtime upgrade must not silently widen
capabilities or change event ordering without an API/security review.

The host should fail closed with a useful diagnostic when an addon asks for an
unsupported API. It should not load an incompatible addon with partial
semantics unless an explicit compatibility adapter exists and is tested.

### Dependency and load isolation

Dependencies should be declarative and resolved before execution. The host
should reject dependency cycles, duplicate package IDs, version conflicts,
unapproved assets, and dependency packages requesting broader capabilities.
Addons should not dynamically fetch code or execute code from arbitrary asset
files.

Use separate environments/stores and storage namespaces per package. If a
shared utility library is eventually needed, load it as a dependency into the
dependent addon’s environment or expose a read-only data-only API. Do not
create a process-wide mutable global table that all addons can alter.

## Determinism and reproducibility

### Scope of determinism

The client addon runtime does not participate in authoritative simulation, so
its exact execution cannot determine whether a spell hits, loot is granted,
or a quest completes. Determinism is still valuable for:

- Reproducing UI bugs.
- Testing quota behavior.
- Comparing default UI and addon output.
- Replaying a captured presentation event stream.
- Diagnosing event-order and package-version regressions.

### Sources of nondeterminism to control

- Wall-clock time and timer scheduling.
- Random numbers and random iteration order.
- Hash-map traversal order.
- Thread scheduling and async host calls.
- Floating-point calculations in layout/animation.
- Arrival order of network-derived events.
- Locale, font fallback, and text shaping.
- Runtime/compiler optimization and bytecode version.
- Dropped/coalesced event policy.

**Recommendation.** Expose a host-provided monotonic frame/time value and a
documented event sequence, not arbitrary OS clock access. If random numbers
are needed for cosmetic effects, expose a per-addon seeded PRNG or a host
random stream and label it non-authoritative. Use stable ordering for maps and
event delivery. Record the package/API/runtime versions with captured event
traces.

Do not promise cross-runtime or cross-platform pixel-identical output. Promise
that a fixed input/event trace under one API/runtime profile produces stable
host decisions, quota outcomes, and ordered UI operations to the extent the
renderer/platform allows.

### Runtime-specific determinism

- Luau and Lua instruction counts are useful local limits, but host call cost,
  garbage collection, and OS scheduling are not made deterministic by a count.
- Rhai operation counts can make evaluation termination reproducible, but
  `on_progress` callbacks that inspect wall time are intentionally
  nondeterministic.
- Wasmtime fuel is documented as deterministic for execution exhaustion under
  a fixed module/configuration; epoch interruption is explicitly lower
  overhead but nondeterministic. [Wasmtime interruption policy](https://docs.wasmtime.dev/api/wasmtime/struct.Config.html)
- Wasmi documents fuel-based out-of-fuel traps and resumable calls, making it a
  useful deterministic comparison for a Wasm implementation.

## Error isolation and recovery

Every script entry point must be called inside a host-owned error boundary.
The boundary should catch:

- Parse/compile errors.
- Missing or incompatible API versions.
- Runtime exceptions/traps.
- Stack overflow/recursion exhaustion.
- Memory/resource-limit failures.
- Host API validation failures.
- Queue overflow and timer storms.
- Callback cancellation during unload.
- Unexpected panic/exception crossing a binding boundary, where the binding
  permits such a boundary.

On failure:

1. Discard the current callback's uncommitted UI operations.
2. Record package ID, entry point, API/runtime versions, error class, source
   location if available, and quota state.
3. Increment the addon error counter without exposing sensitive host details
   to the script.
4. Apply per-addon backoff for repeated failures.
5. Disable the addon when the consecutive/repeated-error policy is exceeded.
6. Cancel its timers and queued events.
7. Destroy or quarantine its owned UI nodes.
8. Keep the default UI, network adapter, renderer, and other addons alive.

The UI should expose a local diagnostic panel showing that an addon was
disabled and why, without allowing the failed addon to intercept or spoof the
diagnostic. A reload must create a fresh environment/store and generation;
stale callbacks and handles from the old generation must fail closed.

**Recommendation.** Make addon failure a normal state transition with a
machine-readable reason, not an exceptional client-wide path. Test unloading
while callbacks, timers, event delivery, and storage writes are pending.

## Adversarial validation plan

### Test harness architecture

Build a host-only test harness that runs each candidate runtime behind the same
abstract `AddonRunner` contract:

```text
load(package, policy) -> instance
deliver(event) -> bounded result
advance(frame_clock) -> bounded result
apply_ui_operations() -> validated operations
unload(instance)
diagnostics() -> structured records
```

The harness should replace OS services, clocks, random sources, renderer
objects, network adapters, and secure-input tokens with test doubles. Tests
must assert both the returned error and the absence of side effects.

### Required negative tests

| Attack family | Example | Expected result |
| --- | --- | --- |
| File access | Open/read/write known sentinel file; inspect environment/current directory | API unavailable or denied; sentinel unchanged |
| Process execution | Spawn shell, execute command, access process APIs | Rejected; no child process created |
| Native loading | Load shared library, FFI, dynamic module, or arbitrary host symbol | Rejected before execution or trapped |
| Network access | Open socket, DNS, HTTP, raw packet, or generic client send | Rejected; no network call |
| Engine escape | Reflect over host objects, pointer conversion, userdata/metatable abuse | No live engine references; operation fails |
| Memory exhaustion | Grow strings/tables/arrays or repeatedly allocate UI nodes | Runtime/host quota trips; client stays alive |
| Deep recursion | Recursive function with no base case | Stack/runtime limit; addon disabled only |
| Infinite loop | Tight loop in startup, event, timer, and coroutine | Interrupt/fuel/wall-clock policy terminates it |
| Host-call stall | Call a host function with huge input or simulated blocking work | Host rejects/limits; render/network threads remain responsive |
| Event storm | Register maximum+1 events; enqueue millions of updates | Registration/queue limit; coalescing/backpressure; no frame starvation |
| Timer storm | Zero-delay self-rescheduling callback or many timers | Minimum interval/count quota; no automatic gameplay path |
| UI exhaustion | Excessive depth, nodes, text, textures, animation callbacks | Operation rejected or addon disabled; other UI survives |
| Protected action | Timer/combat callback invokes secure button, click, key, cast, move, target | No protected intent/token; no gameplay command emitted |
| Indirect action | Overlay, focus traversal, callback chaining, animation completion | No secure input provenance; action denied |
| Hidden-state query | Ask for unobserved NPC, server-only field, or other player private data | Filtered/absent; no distinguishable privileged response |
| Addon escalation | Access another addon globals, handles, storage, or callbacks | Isolation/ownership check denies access |
| Package attack | Dependency cycle, duplicate ID, path traversal, oversized manifest, incompatible ABI | Package rejected before VM execution |
| Stale lifecycle | Invoke callback or handle after unload/reload | Generation check rejects; no mutation |
| Error cascade | One addon throws repeatedly or floods logs | Only that addon is disabled/backed off |

### Positive tests

The harness also needs to prove that the policy is useful:

- Default UI and one addon create equivalent panels through the same API.
- An addon reads only the fields included in a visible view model.
- An addon updates its own node and cannot update another addon’s node.
- A replaceable event is coalesced while an ordered event is preserved.
- A bounded timer updates a local visual element and is cancelled on unload.
- Saved data persists within its package namespace and quota.
- A secure physical input produces one client intent, while the equivalent
  script callback produces none.
- A server rejection reaches both default UI and addons as a presentation
  event without exposing a command-retry primitive.
- A malformed event/value does not panic the host or partially apply an
  invalid UI operation.
- The default UI continues after an addon reaches every failure mode.

### Fuzzing and differential tests

**Recommendation.** Fuzz:

- Manifest parsing and dependency resolution.
- Script source/module parsing and load limits.
- Event payload decoding and bounded value conversion.
- UI operation descriptors and property values.
- Storage serialization/deserialization.
- Handle IDs, generations, and cross-addon ownership checks.
- Package archives, compression limits, and asset references.

Where feasible, run the same abstract test corpus against Luau, Rhai, and a
Wasm runner. The expected result is not identical error text; it is identical
policy: denied capabilities, bounded resource use, no protected intent, and
isolated failure.

Use sanitizers/fuzzers appropriate to native bindings and treat any runtime
crash, memory safety report, deadlock, unbounded allocation, or client-wide
failure as a release-blocking defect for player addons.

## Recommended first implementation spike

The first spike should stay deliberately narrow:

1. Define a language-neutral host contract for `ui.v1`, `game.v1`, and
   `storage.v1` using immutable data values and opaque UI-node handles.
2. Implement an in-memory fake renderer and secure-input registry.
3. Run the default UI and one test addon through the same contract.
4. Add Luau execution with a per-instance environment, allocator limit,
   interrupt callback, callback budget, and host-call wrappers.
5. Implement the negative tests in the previous section before adding rich
   UI features.
6. Implement the same minimal contract with Rhai, using its safe feature
   profile and `on_progress`, to measure ergonomics and resource behavior.
7. Build a small Wasmtime proof of concept that imports only UI operations and
   view-model reads, with no WASI, fuel, memory limiter, and a fixed ABI.
8. Measure startup, callback latency, allocation behavior, event throughput,
   unload/reload correctness, and impact on a Linux render loop.
9. Record the results as measurements, separate from this research record,
   before accepting a runtime in an ADR.

The vertical slice needs only cosmetic UI for this spike: player frame,
target frame, inventory/quest panel, combat feed, and a configurable action
bar whose protected activations are handled by native secure input. No script
should be able to influence authoritative core state.

## Recommendation summary

| Concern | Provisional direction | Confidence |
| --- | --- | --- |
| Public API boundary | Versioned, data-oriented, UI-only host API | High |
| Default UI | Same public script API as addons; narrow native bootstrap only | High |
| Protected actions | Native secure-input provenance plus server validation | High |
| First language | Luau | Medium |
| Rust-native comparison | Rhai | Medium |
| Strong isolation alternative | Wasmtime/Wasm with no WASI imports | Medium-high |
| Stock Lua fallback | Lua 5.4 via mlua, custom safe-library profile | Medium-low |
| Wasm interpreter comparison | Wasmi | Medium-low |
| Rune | Defer; research-only | Low |
| Scheduling | Per-addon event queue and budgeted cooperative dispatch | High |
| Quotas | Runtime + host-call + UI/storage/event quotas | High |
| Persistence | Package-scoped versioned saved data, never arbitrary files | High |
| Determinism | Stable host event/value ordering for tests; no pixel-identical promise | Medium-high |
| Error handling | Per-addon disable/backoff with fresh-instance reload | High |

## Unresolved risks and questions

1. **Luau binding choice:** Should the project use `mlua` with Luau enabled,
   maintain a small C++/C API wrapper, or adopt another Rust binding so that
   the required allocator, interrupt, environment, and debug controls remain
   explicit?
2. **Runtime version policy:** How will the project pin and upgrade Luau/Rhai/
   Wasmtime versions without silently changing bytecode semantics, limits, or
   security behavior?
3. **UI ABI shape:** Should the host API use language-neutral tables/records,
   a generated schema, WebAssembly components/WIT, or a project-owned value
   encoding?
4. **Default UI source:** Is the default UI shipped as readable source for
   iteration, validated bytecode for startup, or both?
5. **Secure input semantics:** Which physical interactions count as explicit
   user input, how are key repeats handled, and how is a secure token prevented
   from crossing an addon callback boundary?
6. **View-model privacy:** Which visible state is safe to expose with exact
   values, and which timing/detail fields could become an indirect hidden-state
   or automation signal?
7. **Budget calibration:** What per-addon CPU, memory, node, queue, and storage
   budgets preserve a responsive client on the minimum supported Linux GPU/CPU?
8. **Host-call bounds:** Which API operations have input-dependent work, and
   what explicit size limits make their time and allocation behavior safe?
9. **Event semantics:** Which events are coalescible, ordered, replayable, or
   intentionally lossy? How are sequence gaps communicated without revealing
   hidden server state?
10. **Persistence scope:** Are saved variables per addon, character, account,
    or device, and how are migrations and deletion handled?
11. **Package trust:** Is signing needed for the hobby project's initial local
    addons, and if so, are signatures for authorship, integrity, or a trusted
    capability tier?
12. **Wasm viability:** Does the language-neutral ABI justify Wasmtime's added
    toolchain and serialization work compared with Luau/Rhai?
13. **Threading:** Should all addon execution remain on one client scheduler
    thread, or can isolated workers be worthwhile without creating nondetermin-
    istic UI races and larger synchronization surfaces?
14. **OS defense in depth:** Should a future hardened mode run the addon host
    in a separate process or OS sandbox, especially if native runtime
    vulnerabilities become a concern?
15. **Editor integration:** Should the future UI editor preview run the same
    addon host and quotas as the game client, with a mock view model and no
    network authority?

## Source classification

The document uses these labels:

- **Sourced fact:** A behavior or capability documented by the language,
  runtime, standards body, or project repository linked directly.
- **Project inference:** A conclusion drawn from those facts and this
  repository's requirements.
- **Recommendation:** A proposed project choice not yet accepted in an ADR or
  proven by a local experiment.
- **Unresolved risk:** A question documentation cannot settle and that needs a
  local prototype, benchmark, security review, or owner decision.

## Primary sources consulted

- [Project requirements baseline](../architecture/requirements.md)
- [Project UI-scripting architecture](../architecture/ui-scripting.md)
- [Project system overview](../architecture/system-overview.md)
- [Project client-technology spike](client-technology-spike.md)
- [Luau project overview](https://luau.org/)
- [Luau sandbox embedding guide](https://luau.org/sandbox/)
- [Luau C API](https://luau.org/api/)
- [mlua `Lua` API](https://docs.rs/mlua/latest/mlua/struct.Lua.html)
- [mlua thread hooks](https://docs.rs/mlua/latest/mlua/thread/struct.Thread.html)
- [Rhai sandboxing](https://rhai.rs/book/safety/sandbox.html)
- [Rhai runtime progress/termination](https://rhai.rs/book/safety/progress.html)
- [Rhai engine limits](https://rhai.rs/book/engine/options.html)
- [Rhai safety guidance](https://rhai.rs/book/safety/index.html)
- [Rhai feature flags](https://rhai.rs/book/start/features.html)
- [WebAssembly overview](https://webassembly.org/)
- [Wasmtime configuration](https://docs.wasmtime.dev/api/wasmtime/struct.Config.html)
- [Wasmtime store/resource controls](https://docs.wasmtime.dev/api/wasmtime/struct.Store.html)
- [Wasmtime interruption example](https://docs.wasmtime.dev/examples-interrupting-wasm.html)
- [Wasmtime resource limits](https://docs.wasmtime.dev/api/src/wasmtime/runtime/limits.rs.html)
- [WASI design principles](https://github.com/WebAssembly/WASI/blob/main/docs/DesignPrinciples.md)
- [Wasmtime WASI context builder](https://docs.rs/wasmtime-wasi/latest/wasmtime_wasi/struct.WasiCtxBuilder.html)
- [Lua 5.4 reference manual](https://www.lua.org/manual/5.4/)
- [Rune crate documentation](https://docs.rs/rune/latest/rune/)
- [Rune module API](https://docs.rs/rune/latest/rune/struct.Module.html)
- [Wasmi crate documentation](https://docs.rs/wasmi/latest/wasmi/)
- [Wasmi configuration](https://docs.rs/wasmi/latest/wasmi/struct.Config.html)
- [Wasmi trap codes](https://docs.rs/wasmi/latest/wasmi/enum.TrapCode.html)
- [Wasmi enforced limits](https://docs.rs/wasmi/latest/src/wasmi/engine/limits/engine.rs.html)

Runtime and dependency facts are volatile. Revisit the source links and pin
exact versions before implementing the production addon host.
