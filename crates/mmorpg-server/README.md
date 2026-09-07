# `mmorpg-server`

The initial server is a Linux headless process for exercising the authoritative
simulation core. It is intentionally a local development server, not a
production gateway or authentication service.

## Intended first-slice behavior

- Bind a configurable local TCP address.
- Optionally bind a second TCP address for versioned wire-envelope clients.
- Advance the authoritative world at a fixed tick rate.
- Accept simple line-oriented development commands.
- Return authoritative events and state summaries.
- Keep sockets and command parsing outside the simulation core.

The line development protocol is temporary. It remains available for manual
testing while the optional wire listener exercises versioned binary command
ingress against the same authoritative world.

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
nc 127.0.0.1 4400
```

To enable the opt-in wire listener, provide `--wire-address` after the line
listener address:

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

The wire listener accepts versioned `MMOW` command envelopes containing the
typed `mmorpg-wire::ClientCommand` payload. It routes those commands through
the same bound-player checks and authoritative `World` as the line listener.
It emits typed server-message payloads for welcome/connect/error responses,
gameplay events, and the bootstrap snapshot. The line listener remains
available for terminal debugging and graphical-client migration.

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
The `dev-local` token is accepted only on a loopback-bound wire listener. The
development account currently exposes one static `Aria` damage-dealer
character, but the client must list and explicitly select it before entering
the world. This is intentionally a local protocol smoke-test handshake; it is
not production authentication, authorization, encryption, or account/character
persistence.

The current server resolves the development token and character catalog through
its `AccountCharacterRepository` boundary. `DevelopmentAccountRepository` is
an in-memory local catalog used only by this slice. It does not preserve an
account, character, inventory, quest, or position across process restarts; a
durable implementation must replace that repository before any persistent or
externally reachable deployment.

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
