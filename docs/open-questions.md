# Open Questions

These questions remain unresolved. They should be answered through research, prototypes, or explicit project-owner decisions.

## Highest priority

- Which client engine best fits Linux support, asset streaming, UI integration, and editor reuse?
- Which network transport and protocol model should the Rust server use?
- Can one region worker sustain 200 active players in a world-boss scenario within the desired tick and bandwidth budgets?
- What exact layer handoff behavior is acceptable at safe points?
- What should happen when more than 200 players want to participate in the same world-boss event?
- Should a world boss have one canonical encounter or configurable event replicas?
- What movement and combat state rollback is acceptable after worker failure?
- Which UI scripting runtime can enforce protected-action and resource limits reliably?
- Which Linux editor technology provides responsive pen-tablet terrain sculpting?

Research has started on the Rust runtime, network/replication model, and overworld layering. Preliminary findings are recorded in `docs/research/`; none of the related implementation choices are accepted yet.

## Capacity and performance

- What is the desired server tick rate?
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
