# `mmorpg-server`

The initial server is a Linux headless process for exercising the authoritative
simulation core. It is intentionally a local development server, not a
production gateway or authentication service.

## Intended first-slice behavior

- Bind a configurable local TCP address.
- Advance the authoritative world at a fixed tick rate.
- Accept simple line-oriented development commands.
- Return authoritative events and state summaries.
- Keep sockets and command parsing outside the simulation core.

The development protocol is temporary. It exists to make the first world
simulation observable and testable before a versioned binary protocol is added.

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
state
quit
```

The server defaults to `127.0.0.1:4000`; pass another bind address as the
first argument. For example:

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4400
nc 127.0.0.1 4400
```

The optional explicit player ID on `move` is checked against the player bound
to the connection. It exists for debugging and does not grant authority over
another player.

The starter economy uses vendor entity `1` and these item IDs:

- `1` — Field Wolf Pelt (loot only in the starter slice).
- `2` — Town Ration.
- `3` — Minor Healing Potion.

For example, after connecting in town, `vendor 1` lists the vendor's stock and
`buy 1 2 3` purchases three Town Rations. After defeating a field wolf,
`loot <enemy-id>` claims its reward if the connected player owns the reward.
`inventory` prints the player's authoritative gold and item stacks.

The server must validate every command through the authoritative core. A
successful command is not evidence that a client is trusted.
