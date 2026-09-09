# `mmorpg-server`

The initial server is a Linux headless process for exercising the authoritative
simulation core. It is intentionally a local development server, not a
production gateway or authentication service.

## Intended first-slice behavior

- Bind a configurable local TCP address.
- Bind a versioned typed wire-envelope listener on the primary TCP address.
- Advance the authoritative world at a fixed tick rate.
- Retain line-oriented command data only in inert compatibility fixtures.
- Return authoritative events and state summaries.
- Keep sockets and command parsing outside the simulation core.

The line development protocol is temporary and is no longer bound by the
server. The optional `--wire-address` argument opens a second typed listener
for staged smoke tooling.

## Planned commands

```text
connect <name> <tank|healer|damage>
move <dx> <dy>
move <player-id> <dx> <dy>
target <entity-id>
attack
vendor <vendor-id>
buy <vendor-id> <item-id> <quantity>
loot <enemy-id>
party-invite <player-id>
party-accept <party-id>
party-decline <party-id>
party-leave
party-remove <player-id>
party-leader <player-id>
party-disband
inventory
quest-offers <npc-id>
accept-quest <npc-id> <quest-id>
turn-in-quest <npc-id> <quest-id>
state
snapshot
quit
```

The server defaults to `127.0.0.1:4000`; pass another bind address as the
first argument. For example:

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4400
```

To open a second typed listener for staged smoke tooling, provide
`--wire-address`:

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4400 --wire-address 127.0.0.1:4401
```

To enable the local development character checkpoint, add an explicit file
path. The server writes only after an authoritative world step has completed;
the core simulation performs no file I/O.

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4400 \
  --wire-address 127.0.0.1:4401 \
  --character-store /tmp/mmorpg-dev/aria.state
```

For a deterministic local shutdown smoke, stop intake at a fixed world tick;
the server drains pending operations and commands, checkpoints live players,
and releases runtime entities before exiting:

```bash
cargo run -p mmorpg-server -- 127.0.0.1:0 --shutdown-after-ticks 2
```

When a client disconnects, the server first applies and checkpoints any
commands already read during that loop iteration, then detaches the selected
character for a bounded five-second grace window. A reconnect for the same
account and character rebinds the existing runtime entity and receives a fresh
private snapshot; expiry queues the normal authoritative leave. This is a
development reconnect boundary, not production session resume.

The typed listener accepts versioned MMOW command envelopes containing the
typed mmorpg-wire::ClientCommand payload. It routes those commands through
the same bound-player checks and authoritative `World` as the former line
listener.
It emits typed server-message payloads for welcome/connect/error responses,
gameplay events, and the bootstrap snapshot. The line parser remains only as
inert compatibility data for equivalence tests.

Public combat and movement events are filtered to a 45-unit nearby audience
before enqueueing. Player movement uses a bounded replaceable queue and
coalesces pending updates by entity; transactional, party, and private events
use the reliable ordered queue. This is a minimum-interest development
boundary, not production spatial replication or backpressure.

Purchase, loot, and quest-turn-in commands may use the additive retryable
wrapper with a nonzero operation ID. The server caches up to 256 completed
results per instance, scoped by account and character, and returns the cached
event on a duplicate instead of reapplying the command. When
`--character-store` is enabled, completed result payloads are also appended by
the off-thread operation journal and reloaded on the next process start. The
journal records the typed intent before the command enters the authoritative
queue, but completion is still appended after the live world step. It is a
development boundary, not a complete transaction: it does not yet provide
crash reconciliation for an operation that fails between live mutation and
result publication. Successful journal acknowledgements gate the operation's
success event; a completion-store failure discards the staged world batch and
returns a typed error to the affected clients. Recovery of an operation after
an external process crash remains a later journal milestone. Completion queue
capacity is reserved for the complete staged batch before any completion job
is submitted, and failed operation attempts have an explicit journal record.
On restart, a torn non-newline-terminated final journal record is ignored while
earlier complete records remain available for retry deduplication.

The typed gameplay vocabulary includes authoritative BasicAttack, party
membership, and Heal { target_id } intents. Party invites and membership are
validated by the core and typed party events are filtered to current/former
members. Healing is restricted by the core to healer
players with a living, nearby player target and is capped at the target's
maximum health; rejected intents emit the normal command-rejection event.

Typed intake is bounded before commands reach the simulation owner: each
session contributes at most 32 decoded frames per poll, the server accepts at
most 256 decoded frames per poll, and the pending authoritative command queue
is capped at 1024 entries. Excess input is discarded with a typed error rather
than becoming hidden simulation debt. Decoded commands are interleaved across
active sessions before authoritative application so one session's bounded
burst cannot occupy the entire command order for the tick.

Wire clients must authenticate before sending gameplay commands:

```text
Authenticate { token: "dev-local" }
Authenticated { account_id: 1, session_id: <server-assigned> }
ListCharacters
CharacterList { account_id: 1, characters: [...] }
SelectCharacter { character_id: 1 }
CharacterSelected { character_id: 1, ... }
EnterWorld
Connected { player_id: <server-assigned>, ... }
```

The server rejects unauthenticated commands and the legacy wire `Join` command.
The `dev-local` token is accepted only on a loopback-bound typed listener. The
development account currently exposes three stable starter characters (`Aria`
damage, `Borin` tank, and `Celia` healer), but the client must list and
explicitly select one before entering the world. This is intentionally a local
protocol smoke-test handshake; it is not production authentication,
authorization, encryption, or account/character persistence.

The server fences an account/character selection across active wire sessions;
the same stable character cannot be entered concurrently by two sessions.

The current server resolves the development token and character catalog through
its `AccountCharacterRepository` boundary. `DevelopmentAccountRepository` is
an in-memory local catalog used only by this slice. The optional
`--character-store` prototype preserves validated durable character fields at
bounded intervals and safe logout, but it is not a durable account system or
an externally reachable deployment implementation.

The optional explicit player ID on `move` is checked against the player bound
to the connection. It exists for debugging and does not grant authority over
another player.

`snapshot` returns a temporary machine-readable bootstrap response for the
graphical client. It is framed by `TEMP_SNAPSHOT_BEGIN version=2` and
`TEMP_SNAPSHOT_END`; the typed snapshot carries the bound player's authoritative
player state and the NPC state visible in the starter zone. Player-private
vendor, loot, quest, and transaction events are addressed only to the bound
session. Each player record carries its explicit
inventory `capacity`, alongside inventory and quest records. Text values are
percent-encoded. This development response is not the future production wire
protocol.

The starter economy uses vendor entity `1` and these item IDs:

- `1` — Field Wolf Pelt (loot only in the starter slice).
- `2` — Town Ration.
- `3` — Minor Healing Potion.

For example, after connecting in town, `vendor 1` lists the vendor's stock and
`buy 1 2 3` purchases three Town Rations. After defeating a field wolf,
`loot <enemy-id>` claims its reward if the connected player owns the reward.
`inventory` prints the player's authoritative gold and item stacks.

The starter quest is quest `1`, `Clear the Field`, offered by the starter
town NPC `1`. Use `quest-offers 1`, `accept-quest 1 1`, defeat the three field
wolves, return to town, and use `turn-in-quest 1 1`.

The server must validate every command through the authoritative core. A
successful command is not evidence that a client is trusted.
