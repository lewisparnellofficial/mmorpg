//! Spatial interest filtering and event recipient routing.

use mmorpg_core::{EntityId, Event, World};

pub const INTEREST_RANGE: f32 = 45.0;

/// Returns the specific player recipient if an event is private to that player.
pub fn event_recipient(event: &Event) -> Option<EntityId> {
    match event {
        Event::VendorListed { player_id, .. }
        | Event::ItemPurchased { player_id, .. }
        | Event::LootRewarded { player_id, .. }
        | Event::TransactionRejected { player_id, .. }
        | Event::QuestOffersListed { player_id, .. }
        | Event::QuestAccepted { player_id, .. }
        | Event::QuestProgressed { player_id, .. }
        | Event::QuestCompleted { player_id, .. }
        | Event::QuestRewarded { player_id, .. }
        | Event::QuestRejected { player_id, .. } => Some(*player_id),
        _ => None,
    }
}

/// Determines whether an event is visible to a specific player based on party membership or proximity.
pub fn event_visible_to_player(event: &Event, player_id: Option<EntityId>, world: &World) -> bool {
    let Some(player_id) = player_id else {
        return false;
    };
    let party_members = |party_id| {
        world
            .party(party_id)
            .map(|party| party.member_ids)
            .unwrap_or_default()
    };
    let party_visibility = match event {
        Event::PartyInviteCreated {
            inviter_id,
            invitee_id,
            ..
        } => Some(player_id == *inviter_id || player_id == *invitee_id),
        Event::PartyInviteAccepted { party, .. } => Some(party.member_ids.contains(&player_id)),
        Event::PartyInviteDeclined {
            party_id,
            player_id: invitee_id,
        }
        | Event::PartyInviteExpired {
            party_id,
            player_id: invitee_id,
        } => Some(player_id == *invitee_id || party_members(*party_id).contains(&player_id)),
        Event::PartyMemberLeft {
            party_id,
            player_id: member_id,
        }
        | Event::PartyMemberRemoved {
            party_id,
            player_id: member_id,
            ..
        } => Some(player_id == *member_id || party_members(*party_id).contains(&player_id)),
        Event::PartyLeaderTransferred { party_id, .. } => {
            Some(party_members(*party_id).contains(&player_id))
        }
        Event::PartyDisbanded { member_ids, .. } => Some(member_ids.contains(&player_id)),
        _ => None,
    };
    if let Some(visible) = party_visibility {
        return visible;
    }

    let interest_entity = match event {
        Event::PlayerJoined { player } => Some(player.id),
        Event::PlayerMoved { player_id, .. }
        | Event::PlayerReleasedToTown { player_id, .. }
        | Event::PlayerDefeated { player_id } => Some(*player_id),
        Event::TargetSelected { target_id, .. }
        | Event::AttackResolved { target_id, .. }
        | Event::HealResolved { target_id, .. }
        | Event::TauntResolved { target_id, .. } => Some(*target_id),
        Event::EnemyCorpseExpired { enemy_id, .. }
        | Event::EnemyDefeated { enemy_id }
        | Event::EnemyRespawned { enemy_id, .. } => Some(*enemy_id),
        Event::EnemyAttackResolved { target_id, .. } => Some(*target_id),
        _ => None,
    };
    let Some(interest_entity) = interest_entity else {
        return true;
    };
    if interest_entity == player_id {
        return true;
    }
    let Some(viewer) = world.player(player_id) else {
        return false;
    };
    let target_position = world
        .player(interest_entity)
        .map(|player| player.position)
        .or_else(|| world.npc(interest_entity).map(|npc| npc.position));
    target_position.is_some_and(|position| {
        viewer.position.distance_squared(position) <= INTEREST_RANGE * INTEREST_RANGE
    })
}
