# Open Questions

These questions remain unresolved. They should be answered through research, prototypes, or explicit project-owner decisions.

## Highest priority

- Which client engine best fits long-term Linux support, asset streaming, UI
  integration, and editor reuse beyond the current Bevy/Qt technology proof?
- How should the typed TCP protocol evolve into a production transport and
  gateway model?
- Can one region worker sustain 200 active players in a world-boss scenario within the desired tick and bandwidth budgets?
- What exact layer handoff behavior is acceptable at safe points?
- What should happen when more than 200 players want to participate in the same world-boss event?
- Should a world boss have one canonical encounter or configurable event replicas?
- What movement and combat state rollback is acceptable after worker failure?
- How should addon isolation and package trust evolve beyond the current
  Luau host boundary?
- Does the current Qt/Wayland editor proof remain responsive on the minimum
  supported tablet hardware?

Research continues on the Rust runtime, production networking/replication,
overworld layering, and durable persistence. Prototype boundaries that have
passed their named local gates are recorded in accepted ADRs; production
choices remain open unless explicitly stated otherwise.

## Resolved for the first vertical slice (prototype scope)

These questions have a bounded implementation answer for the current local
vertical slice, but are not claims about production scale:

- The server uses one 20 Hz fixed-tick authoritative owner with bounded
  command intake and timed combat.
- The gameplay/session path uses typed TCP wire envelopes with loopback-only
  development authentication, explicit character selection, compatibility
  rejection, and retained line data only as test/equivalence fixtures.
- The first addon runtime direction is embedded Luau behind the language-
  neutral `ui.v1` contract, with native secure-input provenance and bounded
  storage/package policies.
- The editor proof uses Qt 6 Quick/Quick3D with a narrow CXX-Qt bridge; the
  physical pressure-sensitive tablet gate remains pending on this host.

The current local-validation results are recorded in `docs/experiments/`.
The fixed-tick ownership model, typed serializer, bounded fair intake,
addressed delivery, and layer-assignment invariants have passing prototype
tests; production interest management and a 5,000-connection gateway test
remain open.

## Capacity and performance

- What tick and frame-time budgets should replace the current 20 Hz prototype
  target for production workloads?
- What are the latency targets for movement, ability activation, and durable operations?
- How many NPCs and effects should be included in the 200-player benchmark?
- What client frame-time budget is acceptable during the world-boss event?
- Does a quiet layer containing many idle players have a materially different capacity profile from a combat hotspot?
- What number of connected-but-idle clients should be included in the 5,000-client test?

## World and gameplay

- Are ordinary layers allowed to contain different transient NPC states?
- Are persistent world objects canonical across all layers, layer-local, or configurable by content type?
- Can players manually join friends in another layer, or does the server only migrate them under safe conditions?
- Are normal-world groups allowed to exceed the 200-player activity budget?
- What classes, abilities, and enemy mechanics are required for the first vertical slice?
- What minimum quest and progression model makes the first town/field loop meaningful?

## Tooling and content

- Should the editor share rendering code with the game client?
- Which assets and definitions should be text-based for source control?
- Is collaborative content editing required from the start?
- Which content types require hot reload?
- How should content revisions and persistent player data interact?
- Which particle features are required for the first slice?

## Operations and security

- What is the acceptable downtime for updates?
- How frequently should snapshots and backups be taken?
- Is public internet access required in the first multiplayer test?
- What administrative and moderation features are needed before inviting outside testers?
- Which external bot behaviors are explicitly out of scope initially?
- What content or asset licensing policy should the project adopt?

## Decisions that can safely wait

- Full guild system.
- Auction-house implementation.
- Battleground matchmaking details.
- Multi-region geographic deployment.
- Dynamic migration of active combat between layers.
- Advanced external bot detection.
- Complete server-side gameplay scripting.
