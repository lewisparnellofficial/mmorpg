# Development tools

The tools directory contains Linux-friendly authoring and validation
prototypes. Tools are kept as standalone Cargo workspaces until their
dependencies and packaging model are stable enough to join the main runtime
workspace.

## Content catalog check

`mmorpg-content-check` validates the compiled starter catalog shared by the
server and future client/editor. Run it from the repository root:

```bash
cargo fmt --manifest-path tools/mmorpg-content-check/Cargo.toml -- --check
cargo test --manifest-path tools/mmorpg-content-check/Cargo.toml
cargo run --quiet --manifest-path tools/mmorpg-content-check/Cargo.toml
```

Expected output for the current starter catalog:

```text
catalog: items=3 npcs=2 vendor_listings=2 quests=1 zones=1 spawns=3
```

The validator is a smoke tool, not yet the terrain/NPC/quest editor. Future
authoring tools should emit source data that this same validation boundary can
load before runtime packaging.

## Editor core spike

`mmorpg-editor-core` contains the device-neutral terrain-authoring foundation:
normalized tablet samples, bounded heightmaps, pressure-aware raise/lower/
smooth brushes, deterministic source save/load, and stroke-level undo/redo. It
does not access tablet hardware or open a GUI. Run it from the repository root:

```bash
cargo fmt --manifest-path tools/mmorpg-editor-core/Cargo.toml -- --check
cargo test --manifest-path tools/mmorpg-editor-core/Cargo.toml
```

The next editor spike should connect Qt or SDL tablet events to this API and
validate the interaction on a real pen tablet.

From the repository root, the current CLI editor spike can be launched with:

```bash
./scripts/run-editor.sh
./scripts/run-editor.sh --output /tmp/starter-terrain.mmterrain
```
