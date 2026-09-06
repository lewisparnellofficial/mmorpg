# EXP-004: typed wire gameplay smoke test

## Purpose

Verify that the versioned wire listener and the typed client-facing payloads
work together across a real TCP connection for the complete starter-zone
vertical slice.

## Procedure

1. Start `mmorpg-server` with the line listener and optional wire listener.
2. Run `experiments/wire-gameplay-smoke` against the wire address.
3. The tool verifies that a legacy wire join is rejected before
   authentication, authenticates with the loopback-only development token,
   lists and explicitly selects the development character, requests a typed
   snapshot, buys one vendor item, accepts the starter quest, defeats and
   loots all three field wolves, and turns in the quest.

## Evidence produced

The tool passes only when it decodes and validates:

- welcome, unauthenticated-command rejection, authentication, and connection
  messages;
- a snapshot containing one player, one vendor, and three enemies;
- vendor listings and a successful purchase;
- quest offers and acceptance;
- target selection, repeated authoritative attacks until defeat for each enemy;
- one server-derived loot reward for each enemy; and
- the final quest reward.

## Classification

- Directly measured: the local command completed over TCP without a typed
  protocol, framing, or server-session failure.
- Project-specific inference: the current starter loop is reachable through
  the same typed intent and event boundary that the client uses in wire mode.
- Not demonstrated: thousands of connected clients, 200-player activity,
  production authentication, encryption, interest management, or production
  backpressure. The development authentication path is covered only as a
  loopback smoke-test boundary.

## Reproduction

```bash
cargo run -p mmorpg-server -- 127.0.0.1:4000 \
  --wire-address 127.0.0.1:4001
cargo run --quiet --manifest-path experiments/wire-gameplay-smoke/Cargo.toml
```
