# ADR-010: language-neutral addon runtime boundary

- Status: Proposed
- Date: 2026-09-09
- Scope: `mmorpg-ui-contract`, the Luau adapter, the Wasmi comparison, and
  future addon process supervision

## Context

The project needs a presentation-only addon runtime. Addons must be able to
consume bounded immutable view records, submit transactional UI operations,
and use account/package-scoped storage, but must not obtain filesystem,
network, process, native-module, engine-memory, or gameplay-command access.
Protected actions remain native-input capabilities and all gameplay intents
remain server-authoritative.

The language-neutral `ui.v1` contract is implemented independently of any VM.
The current graphical proof embeds one Luau state per addon through `mlua`.
The repository also contains a Wasmi comparison with explicit import
allowlisting, fuel interruption, host-side memory/instance/table limits, and an
optional bubblewrap process-wrapper smoke.

## Proposed decision

Keep the runtime choice provisional while freezing the host contract and
requiring every adapter to pass the same contract tests. Treat embedded Luau
as the lead development adapter for the current slice, and retain Wasmi as the
stronger-isolation comparison and fallback candidate.

Neither adapter is accepted as the production addon runtime by this ADR. A
future acceptance decision must choose one of these explicit boundaries:

1. Luau may be accepted only if the native embedding remains capability
   reduced without project-written unsafe code, instruction and memory limits
   terminate hostile programs, host/queue/node/timer/storage quotas bound
   non-VM cost, unload releases all owned resources, and the supported minimum
   hardware passes the measured budget.
2. Wasmi may be accepted only if its guest ABI is mapped to the frozen `ui.v1`
   operations, the process or OS wrapper is integrated with the graphical
   client lifecycle, fuel/memory/host quotas are measured on supported minimum
   hardware, and failure/restart semantics are fixture-tested. The repository
   now has an opt-in comparison-host lifecycle, manifest/hash-checked guest
   loading, a graphical startup smoke, and a namespace failure/restart
   fixture; production package/runtime policy and minimum-hardware calibration
   remain open.

The process wrapper is an isolation proof for the alternative, not a claim
that the current graphical client already executes addons out of process.

## Consequences

- Contract versions, package validation, storage namespaces, secure-input
  provenance, and renderer integration cannot depend on Luau-specific types.
- The current client may continue using the embedded Luau proof without
  silently turning it into a production security claim.
- A Wasmi adapter would incur a guest ABI and process-supervision cost, but
  offers a clearer path to resource and host isolation.
- The runtime choice cannot be closed by synthetic fuzzing, a single desktop
  benchmark, or the process-wrapper smoke alone.
- Editor and addon feature expansion remains frozen after the current slice
  unless a failing vertical-slice gate requires boundary work.

## Alternatives considered

### Accept embedded Luau now

Rejected for now. The host contract, quotas, adversarial tests, fuzz corpus,
and secure-input separation are strong local evidence, but the native runtime
has no process-level isolation and minimum-hardware calibration is missing.

### Replace Luau immediately with Wasmi

Rejected for now. Wasmi has stronger measured import, fuel, memory, and process
wrapper evidence, but the project has not implemented the `ui.v1` guest ABI or
integrated the wrapper into the graphical client.

### Permit native addon modules

Rejected. Native modules would bypass the intended capability boundary and are
outside the player-addon threat model.

## Validation evidence

- `mmorpg-ui-contract` defines the renderer- and language-independent values,
  policies, transactional operations, handles, quotas, and storage model.
- The Luau experiment covers package validation, forbidden APIs, bounded
  execution, callback transactionality, queue isolation, storage atomicity,
  secure-input separation, adversarial corpus tests, and repeated fuzz runs.
- The Wasmi comparison proves allowlisted imports, rejection before
  instantiation, fuel interruption, host-side resource limits, no WASI
  imports, and a bounded guest-memory `create_panel` call that maps into the
  shared `ui.v1` operation type with malformed-pointer handling.
- `scripts/smoke-ui-process-isolation.sh` proves on capable hosts that the
  Wasmi comparison can run with unshared namespaces, an empty network route,
  read-only system bindings, and a private `/tmp`; it also drives the
  supervised `--process-host` lifecycle through `READY`, a contract-backed
  `PANEL`, and `BYE`, and verifies that a failed host can be replaced by a
  fresh host that completes the same lifecycle.
- `scripts/smoke-ui-constrained.sh` records a one-core/1-GiB modeled profile;
  it is explicitly not minimum-hardware evidence.
- The normal aggregate validation includes the deterministic adversarial and
  process-isolation checks. The constrained one-core/1-GiB run remains an
  explicit modeled resource-envelope check rather than a default aggregate
  gate.

## Conditions for acceptance or revision

Accept one runtime only after production package/runtime behavior, minimum-hardware,
and production failure-isolation/restart evidence is recorded. Revise this ADR
if the physical/client gate exposes a host cost that invalidates the current
quotas, if a runtime escapes the contract, if process supervision cannot meet
startup/shutdown bounds, or if a supported target lacks the required sandbox
capabilities.
