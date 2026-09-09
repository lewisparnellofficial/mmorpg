# `mmorpg-client-secure-input`

This dependency-light crate owns the native provenance primitive for protected
UI actions. It accepts only registered addon/node/action bindings and fresh
non-repeat native presses. Focus changes, node generations, addon unload, and
physical-event replay invalidate or reject dispatch. A successful dispatch
returns a single-use trusted token whose `consume` method yields the normal
typed action tuple.

The registry does not know about Bevy, Luau, sockets, or gameplay rules. The
Bevy host must perform the actual hit test and translate the consumed action
into a bounded client intent; the server remains authoritative over that
intent.
