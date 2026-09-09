# `mmorpg-wire-cli`

This headless diagnostic client exercises the same typed development boundary
as the graphical client: bounded `MMOW` frames, loopback authentication,
character listing and explicit selection, canonical starter-content digest
validation, world entry, and an authoritative bootstrap snapshot.

Run it against the default typed listener:

```bash
cargo run --quiet --manifest-path tools/mmorpg-wire-cli/Cargo.toml
```

Use `--character-id <id>` to select a listed character explicitly or
`--token <value>` to test authentication rejection. The command does not
interpret human-readable server diagnostics and never submits gameplay
commands before the selected character has passed the content gate and the
server has returned a bootstrap snapshot.
