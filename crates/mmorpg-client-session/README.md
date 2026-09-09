# `mmorpg-client-session`

Renderer-independent typed-wire session policy. The session owns the
development handshake, character-selection requirement, bounded outgoing
intent queue, bootstrap gate, and disconnect cleanup. Socket workers provide
decoded `mmorpg-wire` messages; Bevy or a diagnostic client consumes the
session's typed outputs.

After the server confirms character selection, the session sends its compiled
catalog digest and waits for `ContentAccepted` before sending `EnterWorld`.
`ContentMismatch` is surfaced as a server rejection and cannot bind gameplay.

Sequenced inputs provide the bootstrap/event boundary: a complete snapshot
installs a baseline sequence atomically; later events apply only when their
sequence is contiguous. Older or duplicate messages are ignored, while a gap
or bounded-buffer overflow emits `RequestBootstrap` and waits for a fresh
snapshot. The typed development listener now carries per-session sequences
into this policy; legacy transport callers may still unwrap them for
compatibility.

This is a policy boundary, not a socket implementation. Production
authentication, reconnect resume, sequencing, and interest management remain
provisional under `PLAN.md`.
