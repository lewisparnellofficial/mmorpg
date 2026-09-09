# `mmorpg-client-session`

Renderer-independent typed-wire session policy. The session owns the
development handshake, character-selection requirement, bounded outgoing
intent queue, bootstrap gate, and disconnect cleanup. Socket workers provide
decoded `mmorpg-wire` messages; Bevy or a diagnostic client consumes the
session's typed outputs.

After the server confirms character selection, the session sends its compiled
catalog digest and waits for `ContentAccepted` before sending `EnterWorld`.
`ContentMismatch` is surfaced as a server rejection and cannot bind gameplay.

This is a policy boundary, not a socket implementation. Production
authentication, reconnect resume, sequencing, and interest management remain
provisional under `PLAN.md`.
