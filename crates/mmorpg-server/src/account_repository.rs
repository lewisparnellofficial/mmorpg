//! Account and character lookup boundary for the development server.
//!
//! The active world remains in-memory and authoritative. This module owns only
//! the identity-to-character lookup needed before a wire session can bind to a
//! player, so a durable repository can replace the local development catalog
//! without coupling database concerns to socket handling or simulation ticks.

use mmorpg_core::Role;
use mmorpg_wire::{CharacterSummary, RoleCode};

const DEV_AUTH_TOKEN: &str = "dev-local";
const DEV_ACCOUNT_ID: u64 = 1;
const DEV_CHARACTER_ID: u64 = 1;
const DEV_CHARACTER_NAME: &str = "Aria";
const DEV_CHARACTER_ROLE: Role = Role::DamageDealer;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterRecord {
    pub character_id: u64,
    pub account_id: u64,
    pub name: String,
    pub role: Role,
}

impl CharacterRecord {
    pub fn summary(&self) -> CharacterSummary {
        CharacterSummary {
            character_id: self.character_id,
            name: self.name.clone(),
            role: role_code(self.role),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationError {
    DevelopmentAuthenticationDisabled,
    InvalidDevelopmentToken,
}

impl AuthenticationError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::DevelopmentAuthenticationDisabled => {
                "development authentication is only available on loopback"
            }
            Self::InvalidDevelopmentToken => "invalid development token",
        }
    }
}

/// Resolves authenticated accounts and the characters that they may select.
///
/// A production implementation should use durable account and character data,
/// enforce any account/character status policy, and keep credential processing
/// separate from the simulation worker.
pub trait AccountCharacterRepository {
    fn authenticate_development_token(&self, token: &str) -> Result<u64, AuthenticationError>;

    fn list_characters(&self, account_id: u64) -> Vec<CharacterRecord>;

    fn find_character(&self, account_id: u64, character_id: u64) -> Option<CharacterRecord>;
}

/// Local-only catalog used by the current typed wire development path.
pub struct DevelopmentAccountRepository {
    development_authentication_enabled: bool,
}

impl DevelopmentAccountRepository {
    pub const fn new(development_authentication_enabled: bool) -> Self {
        Self {
            development_authentication_enabled,
        }
    }

    fn development_character() -> CharacterRecord {
        CharacterRecord {
            character_id: DEV_CHARACTER_ID,
            account_id: DEV_ACCOUNT_ID,
            name: DEV_CHARACTER_NAME.to_owned(),
            role: DEV_CHARACTER_ROLE,
        }
    }
}

impl AccountCharacterRepository for DevelopmentAccountRepository {
    fn authenticate_development_token(&self, token: &str) -> Result<u64, AuthenticationError> {
        if !self.development_authentication_enabled {
            return Err(AuthenticationError::DevelopmentAuthenticationDisabled);
        }
        if token != DEV_AUTH_TOKEN {
            return Err(AuthenticationError::InvalidDevelopmentToken);
        }
        Ok(DEV_ACCOUNT_ID)
    }

    fn list_characters(&self, account_id: u64) -> Vec<CharacterRecord> {
        let character = Self::development_character();
        (account_id == character.account_id)
            .then_some(character)
            .into_iter()
            .collect()
    }

    fn find_character(&self, account_id: u64, character_id: u64) -> Option<CharacterRecord> {
        let character = Self::development_character();
        (account_id == character.account_id && character_id == character.character_id)
            .then_some(character)
    }
}

fn role_code(role: Role) -> RoleCode {
    match role {
        Role::Tank => RoleCode::Tank,
        Role::Healer => RoleCode::Healer,
        Role::DamageDealer => RoleCode::DamageDealer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_authentication_requires_loopback_and_exact_token() {
        let enabled = DevelopmentAccountRepository::new(true);
        assert_eq!(
            enabled.authenticate_development_token(DEV_AUTH_TOKEN),
            Ok(1)
        );
        assert_eq!(
            enabled.authenticate_development_token("wrong"),
            Err(AuthenticationError::InvalidDevelopmentToken)
        );

        let disabled = DevelopmentAccountRepository::new(false);
        assert_eq!(
            disabled.authenticate_development_token(DEV_AUTH_TOKEN),
            Err(AuthenticationError::DevelopmentAuthenticationDisabled)
        );
    }

    #[test]
    fn development_repository_scopes_characters_to_the_authenticated_account() {
        let repository = DevelopmentAccountRepository::new(true);

        assert_eq!(repository.list_characters(DEV_ACCOUNT_ID).len(), 1);
        assert!(repository.list_characters(2).is_empty());

        let character = repository
            .find_character(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
            .expect("development character should exist");
        assert_eq!(character.name, "Aria");
        assert_eq!(character.role, Role::DamageDealer);
        assert!(repository.find_character(2, DEV_CHARACTER_ID).is_none());
        assert!(
            repository
                .find_character(DEV_ACCOUNT_ID, DEV_CHARACTER_ID + 1)
                .is_none()
        );
    }
}
