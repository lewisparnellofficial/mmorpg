# Client presentation replay

This standalone Rust experiment validates the first end-to-end boundary
between the authoritative starter simulation and the renderer-independent
client presentation model.

It constructs `mmorpg_core::World::new_starter_zone`, snapshots every starter
NPC into `mmorpg_client_model::ClientWorld`, then feeds the model every event
produced by a deterministic player journey:

1. Join the starter zone as a damage dealer.
2. List the town vendor's catalog and quest offers.
3. Buy a healing potion.
4. Accept the `Clear the Field` quest.
5. Move from town to the field.
6. Target, defeat, and loot all three field wolves.
7. Return to town and turn in the quest.

The binary reports the resulting projected player, enemy, inventory, quest,
and vendor state. The test suite additionally checks the expected event count
and that two runs produce identical reports.

## Run it

Run these commands from the repository root:

```bash
cargo fmt --manifest-path experiments/client-presentation-replay/Cargo.toml -- --check
cargo test --manifest-path experiments/client-presentation-replay/Cargo.toml
cargo run --quiet --manifest-path experiments/client-presentation-replay/Cargo.toml
```

Expected output includes:

```text
client presentation replay: success
  applied authoritative events=52
  projected position=(0.0, 0.0) area=Town
  defeated enemies=3 quest=1 gold=25
  inventory: pelts=3 rations=5 potions=1
  vendor potion stock remaining=49
```

## Scope and limitations

- This is a deterministic validation replay, not a network client.
- It uses direct in-process calls rather than encoding or decoding a wire
  protocol.
- NPC bootstrap uses `ClientWorld::apply_npc_snapshot` because the current
  core event stream does not yet contain an NPC snapshot event.
- The experiment does not open a Linux window, render a scene, or exercise
  input, networking, layering, persistence, or addon scripting.
- It does not prove server capacity or latency. The replay runs one local
  simulation owner and one projected client model.
- The current client model has no event sequence, tick validation, or world
  identity, so this replay cannot test out-of-order or cross-instance event
  rejection.
- The scenario intentionally follows the current starter-zone rules and
  stable IDs; it is not a general-purpose replay format or content loader.
