# `mmorpg-ui-contract`

This crate defines the dependency-light, language-neutral `ui.v1` boundary
between addons and a renderer. It owns stable package/account/node IDs,
immutable presentation records, bounded/coalescing event delivery,
transaction-validated UI operations, package manifest validation, and the
account/package/schema-scoped `storage.v1` value boundary.

It intentionally has no dependency on Luau, Bevy, Qt, sockets, the
filesystem, or `mmorpg-core`. Runtime adapters must validate one complete
operation buffer before committing any renderer mutation. A validation error
therefore commits no operation from that buffer.

The contract is provisional through Milestone 5 of [`PLAN.md`](../../PLAN.md).
The current tests are deterministic policy tests, not evidence of hostile
runtime isolation or production performance.
