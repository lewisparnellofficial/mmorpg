//! Session lifecycle, detached characters, and character reservation guards.

use crate::account_repository::OperationKey;
use crate::client::{ClientOrigin, WireClient};
use mmorpg_core::{Command, EntityId};

pub const DISCONNECT_GRACE_TICKS: u64 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedSession {
    pub account_id: u64,
    pub session_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DetachedCharacter {
    pub player_id: EntityId,
    pub expires_at_tick: u64,
}

pub struct PendingCommand {
    pub origin: ClientOrigin,
    pub command: Command,
    pub operation: Option<OperationKey>,
}

/// Checks whether an account and character combination is currently selected by another active client.
pub fn character_reserved_by_other(
    clients: &[WireClient],
    client_id: u64,
    account_id: u64,
    character_id: u64,
) -> bool {
    clients.iter().any(|client| {
        client.id != client_id
            && client
                .authenticated
                .is_some_and(|session| session.account_id == account_id)
            && client.selected_character_id == Some(character_id)
    })
}
