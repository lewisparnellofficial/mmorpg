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

When a client disconnects, the server first applies and checkpoints any
commands already read during that loop iteration, then queues the player's
authoritative leave. This prevents a same-tick reward from being discarded
before the local checkpoint prototype can record it.

The typed listener accepts versioned `MMOW` command envelopes containing the
typed `mmorpg-wire::ClientCommand` payload. It routes those commands through
the same bound-player checks and authoritative `World` as the former line
listener.
It emits typed server-message payloads for welcome/connect/error responses,
gameplay events, and the bootstrap snapshot. The line parser remains only as
inert compatibility data for equivalence tests.

The typed gameplay vocabulary includes authoritative `BasicAttack` and
`Heal { target_id }` intents. Healing is restricted by the core to healer
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
`TEMP_SNAPSHOT_END`; the records between those markers include the authoritative
world summary and player/NPC state. Each player record carries its explicit
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
