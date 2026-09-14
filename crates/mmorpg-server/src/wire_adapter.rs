//! Translations between authoritative simulation types and wire protocol payloads.

use mmorpg_core::{Command, EntityId, Event, ItemId, PartyId, QuestId, Role, World};
use mmorpg_wire::{
    ClientCommand as WireCommand, ItemStackState, NpcKindCode, NpcState, PlayerState,
    QuestOfferState, QuestState, QuestStatusCode, RoleCode, ServerEvent, VendorListingState,
    WorldSnapshot, ZoneAreaCode,
};

pub fn wire_role(role: Role) -> RoleCode {
    match role {
        Role::Tank => RoleCode::Tank,
        Role::Healer => RoleCode::Healer,
        Role::DamageDealer => RoleCode::DamageDealer,
    }
}

pub fn wire_area(area: mmorpg_core::ZoneArea) -> ZoneAreaCode {
    match area {
        mmorpg_core::ZoneArea::Town => ZoneAreaCode::Town,
        mmorpg_core::ZoneArea::Field => ZoneAreaCode::Field,
    }
}

pub fn wire_status(status: mmorpg_core::QuestStatus) -> QuestStatusCode {
    match status {
        mmorpg_core::QuestStatus::Accepted => QuestStatusCode::Accepted,
        mmorpg_core::QuestStatus::Completed => QuestStatusCode::Completed,
        mmorpg_core::QuestStatus::Rewarded => QuestStatusCode::Rewarded,
    }
}

pub fn wire_player(player: &mmorpg_core::PlayerSnapshot) -> PlayerState {
    PlayerState {
        player_id: player.id.0,
        name: player.name.clone(),
        role: wire_role(player.role),
        position: mmorpg_wire::PositionState {
            x: player.position.x,
            y: player.position.y,
        },
        health: player.health,
        max_health: player.max_health,
        target_id: player.target.map(|target| target.0),
        gold: player.gold,
        inventory_capacity: player.inventory.capacity() as u32,
        inventory: player
            .inventory
            .stacks()
            .map(|stack| ItemStackState {
                item_id: stack.item_id.0,
                quantity: stack.quantity,
            })
            .collect(),
        quests: player
            .quests
            .iter()
            .map(|quest| QuestState {
                quest_id: quest.quest_id.0,
                progress: quest.progress,
                required_count: quest.required_count,
                status: wire_status(quest.status),
            })
            .collect(),
    }
}

pub fn wire_live_player(player: &mmorpg_core::Player) -> PlayerState {
    PlayerState {
        player_id: player.id.0,
        name: player.name.clone(),
        role: wire_role(player.role),
        position: mmorpg_wire::PositionState {
            x: player.position.x,
            y: player.position.y,
        },
        health: player.health,
        max_health: player.max_health,
        target_id: player.target.map(|target| target.0),
        gold: player.gold,
        inventory_capacity: player.inventory.capacity() as u32,
        inventory: player
            .inventory
            .stacks()
            .map(|stack| ItemStackState {
                item_id: stack.item_id.0,
                quantity: stack.quantity,
            })
            .collect(),
        quests: player
            .quests
            .iter()
            .map(|quest| QuestState {
                quest_id: quest.quest_id.0,
                progress: quest.progress,
                required_count: quest.required_count,
                status: wire_status(quest.status),
            })
            .collect(),
    }
}

pub fn wire_npc(npc: &mmorpg_core::Npc) -> NpcState {
    NpcState {
        entity_id: npc.id.0,
        template_id: npc.template_id.0,
        name: npc.name.clone(),
        kind: match npc.kind {
            mmorpg_core::NpcKind::Vendor => NpcKindCode::Vendor,
            mmorpg_core::NpcKind::Enemy => NpcKindCode::Enemy,
        },
        position: mmorpg_wire::PositionState {
            x: npc.position.x,
            y: npc.position.y,
        },
        health: npc.health,
        max_health: npc.max_health,
    }
}

pub fn wire_event(event: &Event) -> Option<ServerEvent> {
    Some(match event {
        Event::PlayerJoined { player } => ServerEvent::PlayerJoined {
            player: wire_player(player),
        },
        Event::PlayerLeft { player_id } => ServerEvent::PlayerLeft {
            player_id: player_id.0,
        },
        Event::PlayerMoved {
            player_id,
            position,
            area,
        } => ServerEvent::PlayerMoved {
            player_id: player_id.0,
            position: mmorpg_wire::PositionState {
                x: position.x,
                y: position.y,
            },
            area: wire_area(*area),
        },
        Event::TargetSelected {
            player_id,
            target_id,
        } => ServerEvent::TargetSelected {
            player_id: player_id.0,
            target_id: target_id.0,
        },
        Event::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => ServerEvent::AttackResolved {
            player_id: player_id.0,
            target_id: target_id.0,
            damage: *damage,
            target_health: *target_health,
        },
        Event::CombatCooldownStarted {
            player_id,
            ready_tick,
        } => ServerEvent::CombatCooldownStarted {
            player_id: player_id.0,
            ready_tick: *ready_tick,
        },
        Event::HealResolved {
            player_id,
            target_id,
            amount,
            target_health,
        } => ServerEvent::HealResolved {
            player_id: player_id.0,
            target_id: target_id.0,
            amount: *amount,
            target_health: *target_health,
        },
        Event::TauntResolved {
            player_id,
            target_id,
        } => ServerEvent::TauntResolved {
            player_id: player_id.0,
            target_id: target_id.0,
        },
        Event::PlayerReleasedToTown {
            player_id,
            position,
            health,
        } => ServerEvent::PlayerReleasedToTown {
            player_id: player_id.0,
            position: mmorpg_wire::PositionState {
                x: position.x,
                y: position.y,
            },
            health: *health,
        },
        Event::EnemyCorpseExpired {
            enemy_id,
            spawn_generation,
        } => ServerEvent::EnemyCorpseExpired {
            enemy_id: enemy_id.0,
            spawn_generation: *spawn_generation,
        },
        Event::EnemyDefeated { enemy_id } => ServerEvent::EnemyDefeated {
            enemy_id: enemy_id.0,
        },
        Event::EnemyAttackResolved {
            enemy_id,
            target_id,
            damage,
            target_health,
        } => ServerEvent::EnemyAttackResolved {
            enemy_id: enemy_id.0,
            target_id: target_id.0,
            damage: *damage,
            target_health: *target_health,
        },
        Event::PlayerDefeated { player_id } => ServerEvent::PlayerDefeated {
            player_id: player_id.0,
        },
        Event::EnemyRespawned {
            enemy_id,
            spawn_generation,
        } => ServerEvent::EnemyRespawned {
            enemy_id: enemy_id.0,
            spawn_generation: *spawn_generation,
        },
        Event::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => ServerEvent::VendorListed {
            player_id: player_id.0,
            vendor_id: vendor_id.0,
            listings: listings
                .iter()
                .map(|listing| VendorListingState {
                    item_id: listing.item_id.0,
                    name: listing.name.to_owned(),
                    unit_price: listing.unit_price,
                    remaining_quantity: listing.remaining_quantity,
                    max_stack: listing.max_stack,
                })
                .collect(),
        },
        Event::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => ServerEvent::ItemPurchased {
            player_id: player_id.0,
            vendor_id: vendor_id.0,
            item_id: item_id.0,
            quantity: *quantity,
            total_price: *total_price,
            gold_remaining: *gold_remaining,
        },
        Event::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => ServerEvent::LootRewarded {
            player_id: player_id.0,
            enemy_id: enemy_id.0,
            item_id: item_id.0,
            quantity: *quantity,
        },
        Event::PartyInviteCreated {
            party_id,
            inviter_id,
            invitee_id,
            expires_at_tick,
        } => ServerEvent::PartyInviteCreated {
            party_id: party_id.0,
            inviter_id: inviter_id.0,
            invitee_id: invitee_id.0,
            expires_at_tick: *expires_at_tick,
        },
        Event::PartyInviteAccepted { party, player_id } => ServerEvent::PartyInviteAccepted {
            party: mmorpg_wire::PartyState {
                party_id: party.id.0,
                leader_id: party.leader_id.0,
                member_ids: party.member_ids.iter().map(|id| id.0).collect(),
            },
            player_id: player_id.0,
        },
        Event::PartyInviteDeclined {
            party_id,
            player_id,
        } => ServerEvent::PartyInviteDeclined {
            party_id: party_id.0,
            player_id: player_id.0,
        },
        Event::PartyInviteExpired {
            party_id,
            player_id,
        } => ServerEvent::PartyInviteExpired {
            party_id: party_id.0,
            player_id: player_id.0,
        },
        Event::PartyMemberLeft {
            party_id,
            player_id,
        } => ServerEvent::PartyMemberLeft {
            party_id: party_id.0,
            player_id: player_id.0,
        },
        Event::PartyMemberRemoved {
            party_id,
            player_id,
            removed_by,
        } => ServerEvent::PartyMemberRemoved {
            party_id: party_id.0,
            player_id: player_id.0,
            removed_by: removed_by.0,
        },
        Event::PartyLeaderTransferred {
            party_id,
            previous_leader_id,
            leader_id,
        } => ServerEvent::PartyLeaderTransferred {
            party_id: party_id.0,
            previous_leader_id: previous_leader_id.0,
            leader_id: leader_id.0,
        },
        Event::PartyDisbanded {
            party_id,
            member_ids,
        } => ServerEvent::PartyDisbanded {
            party_id: party_id.0,
            member_ids: member_ids.iter().map(|id| id.0).collect(),
        },
        Event::TransactionRejected { player_id, reason } => ServerEvent::TransactionRejected {
            player_id: player_id.0,
            reason: reason.clone(),
        },
        Event::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => ServerEvent::QuestOffersListed {
            player_id: player_id.0,
            npc_id: npc_id.0,
            quests: quests
                .iter()
                .map(|quest| QuestOfferState {
                    quest_id: quest.quest_id.0,
                    name: quest.name.to_owned(),
                    description: quest.description.to_owned(),
                })
                .collect(),
        },
        Event::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => ServerEvent::QuestAccepted {
            player_id: player_id.0,
            npc_id: npc_id.0,
            quest_id: quest_id.0,
        },
        Event::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => ServerEvent::QuestProgressed {
            player_id: player_id.0,
            quest_id: quest_id.0,
            progress: *progress,
            required_count: *required_count,
        },
        Event::QuestCompleted {
            player_id,
            quest_id,
        } => ServerEvent::QuestCompleted {
            player_id: player_id.0,
            quest_id: quest_id.0,
        },
        Event::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => ServerEvent::QuestRewarded {
            player_id: player_id.0,
            quest_id: quest_id.0,
            gold: *gold,
            item_id: item_id.map(|item| item.0),
            item_quantity: *item_quantity,
            gold_remaining: *gold_remaining,
        },
        Event::QuestRejected { player_id, reason } => ServerEvent::QuestRejected {
            player_id: player_id.0,
            reason: reason.clone(),
        },
        Event::CommandRejected { reason } => ServerEvent::CommandRejected {
            reason: reason.clone(),
        },
    })
}

pub fn wire_snapshot(world: &World) -> WorldSnapshot {
    let summary = world.summary();
    WorldSnapshot {
        version: mmorpg_wire::SNAPSHOT_SCHEMA_VERSION,
        tick: summary.tick,
        player_count: summary.player_count as u32,
        npc_count: summary.npc_count as u32,
        enemy_count: summary.enemy_count as u32,
        vendor_count: summary.vendor_count as u32,
        players: world.players().map(wire_live_player).collect(),
        npcs: world.npcs().map(wire_npc).collect(),
        party: None,
    }
}

pub fn wire_snapshot_for_player(world: &World, player_id: EntityId) -> WorldSnapshot {
    let mut snapshot = wire_snapshot(world);
    snapshot
        .players
        .retain(|player| player.player_id == player_id.0);
    snapshot.player_count = snapshot.players.len() as u32;
    snapshot.party = world
        .party_for_player(player_id)
        .and_then(|party_id| world.party(party_id))
        .map(|party| mmorpg_wire::PartyState {
            party_id: party.id.0,
            leader_id: party.leader_id.0,
            member_ids: party.member_ids.iter().map(|id| id.0).collect(),
        });
    snapshot
}

pub fn is_retryable_core_command(command: &WireCommand) -> bool {
    matches!(
        command,
        WireCommand::BuyItem { .. }
            | WireCommand::LootEnemy { .. }
            | WireCommand::TurnInQuest { .. }
    )
}

pub fn command_to_wire_payload(command: &Command) -> Result<Vec<u8>, String> {
    let wire_command = match command {
        Command::BuyItem {
            vendor_id,
            item_id,
            quantity,
            ..
        } => WireCommand::BuyItem {
            vendor_id: vendor_id.0,
            item_id: item_id.0,
            quantity: *quantity,
        },
        Command::LootEnemy { enemy_id, .. } => WireCommand::LootEnemy {
            enemy_id: enemy_id.0,
        },
        Command::TurnInQuest {
            npc_id, quest_id, ..
        } => WireCommand::TurnInQuest {
            npc_id: npc_id.0,
            quest_id: quest_id.0,
        },
        _ => return Err("operation is not a durable command".to_owned()),
    };
    wire_command
        .encode_payload()
        .map_err(|error| format!("cannot encode operation journal command: {error}"))
}

pub fn command_player_id(command: &Command) -> Option<EntityId> {
    match command {
        Command::BuyItem { player_id, .. }
        | Command::LootEnemy { player_id, .. }
        | Command::TurnInQuest { player_id, .. } => Some(*player_id),
        _ => None,
    }
}

pub fn wire_command_to_core(command: WireCommand, player_id: EntityId) -> Result<Command, String> {
    Ok(match command {
        WireCommand::Move { dx, dy } => Command::Move { player_id, dx, dy },
        WireCommand::SelectTarget { target_id } => Command::SelectTarget {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::BasicAttack => Command::BasicAttack { player_id },
        WireCommand::Heal { target_id } => Command::Heal {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::Taunt => Command::Taunt { player_id },
        WireCommand::ReleaseToTown => Command::ReleaseToTown { player_id },
        WireCommand::ListVendor { vendor_id } => Command::ListVendor {
            player_id,
            vendor_id: EntityId(vendor_id),
        },
        WireCommand::BuyItem {
            vendor_id,
            item_id,
            quantity,
        } => Command::BuyItem {
            player_id,
            vendor_id: EntityId(vendor_id),
            item_id: ItemId(item_id),
            quantity,
        },
        WireCommand::LootEnemy { enemy_id } => Command::LootEnemy {
            player_id,
            enemy_id: EntityId(enemy_id),
        },
        WireCommand::InvitePartyMember { target_id } => Command::InvitePartyMember {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::AcceptPartyInvite { party_id } => Command::AcceptPartyInvite {
            player_id,
            party_id: PartyId(party_id),
        },
        WireCommand::DeclinePartyInvite { party_id } => Command::DeclinePartyInvite {
            player_id,
            party_id: PartyId(party_id),
        },
        WireCommand::LeaveParty => Command::LeaveParty { player_id },
        WireCommand::RemovePartyMember { target_id } => Command::RemovePartyMember {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::TransferPartyLeader { target_id } => Command::TransferPartyLeader {
            player_id,
            target_id: EntityId(target_id),
        },
        WireCommand::DisbandParty => Command::DisbandParty { player_id },
        WireCommand::ListQuestOffers { npc_id } => Command::ListQuestOffers {
            player_id,
            npc_id: EntityId(npc_id),
        },
        WireCommand::AcceptQuest { npc_id, quest_id } => Command::AcceptQuest {
            player_id,
            npc_id: EntityId(npc_id),
            quest_id: QuestId(quest_id),
        },
        WireCommand::TurnInQuest { npc_id, quest_id } => Command::TurnInQuest {
            player_id,
            npc_id: EntityId(npc_id),
            quest_id: QuestId(quest_id),
        },
        WireCommand::Authenticate { .. }
        | WireCommand::Join { .. }
        | WireCommand::EnterWorld
        | WireCommand::ListCharacters
        | WireCommand::SelectCharacter { .. }
        | WireCommand::ContentDigest { .. }
        | WireCommand::Retryable { .. }
        | WireCommand::Request { .. }
        | WireCommand::Snapshot => {
            return Err("command is not valid in a bound session".to_owned());
        }
    })
}
