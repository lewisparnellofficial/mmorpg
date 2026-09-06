//! Account and character lookup boundary for the development server.
//!
//! The active world remains in-memory and authoritative. This module owns only
//! the identity-to-character lookup needed before a wire session can bind to a
//! player, so a durable repository can replace the local development catalog
//! without coupling database concerns to socket handling or simulation ticks.

use mmorpg_core::{
    DurablePlayerState, Inventory, ItemId, ItemStack, Position, QuestId, QuestProgress,
    QuestStatus, Role,
};
use mmorpg_wire::{CharacterSummary, RoleCode};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

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

    fn load_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<DurablePlayerState>, String>;

    fn save_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String>;
}

/// Local-only catalog used by the current typed wire development path.
pub struct DevelopmentAccountRepository {
    development_authentication_enabled: bool,
    checkpoint_store: Option<LocalCheckpointStore>,
}

impl DevelopmentAccountRepository {
    pub const fn new(development_authentication_enabled: bool) -> Self {
        Self {
            development_authentication_enabled,
            checkpoint_store: None,
        }
    }

    pub fn with_checkpoint_store(development_authentication_enabled: bool, path: PathBuf) -> Self {
        Self {
            development_authentication_enabled,
            checkpoint_store: Some(LocalCheckpointStore::new(path)),
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

    fn load_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<DurablePlayerState>, String> {
        if self.find_character(account_id, character_id).is_none() {
            return Ok(None);
        }
        self.checkpoint_store
            .as_ref()
            .map_or(Ok(None), LocalCheckpointStore::load)
    }

    fn save_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        if self.find_character(account_id, character_id).is_none() {
            return Err("unknown character".to_owned());
        }
        self.checkpoint_store
            .as_ref()
            .map_or(Ok(()), |store| store.save(state))
    }
}

struct LocalCheckpointStore {
    path: PathBuf,
}

impl LocalCheckpointStore {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<Option<DurablePlayerState>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&self.path)
            .map_err(|error| format!("cannot read checkpoint: {error}"))?;
        parse_checkpoint(&text).map(Some)
    }

    fn save(&self, state: &DurablePlayerState) -> Result<(), String> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create checkpoint directory: {error}"))?;
        let temporary = self.path.with_extension("tmp");
        let mut file = File::create(&temporary)
            .map_err(|error| format!("cannot create checkpoint: {error}"))?;
        file.write_all(format_checkpoint(state).as_bytes())
            .map_err(|error| format!("cannot write checkpoint: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync checkpoint: {error}"))?;
        fs::rename(&temporary, &self.path)
            .map_err(|error| format!("cannot replace checkpoint: {error}"))
    }
}

fn format_checkpoint(state: &DurablePlayerState) -> String {
    let mut text = format!(
        "version=1\nname={}\nrole={}\nx={}\ny={}\ngold={}\ncapacity={}\n",
        state.name,
        state.role.as_str(),
        state.position.x,
        state.position.y,
        state.gold,
        state.inventory.capacity()
    );
    for stack in state.inventory.stacks() {
        text.push_str(&format!("item={},{}\n", stack.item_id.0, stack.quantity));
    }
    for quest in &state.quests {
        text.push_str(&format!(
            "quest={},{},{},{}\n",
            quest.quest_id.0,
            quest.progress,
            quest.required_count,
            quest_status_name(quest.status)
        ));
    }
    text
}

fn parse_checkpoint(text: &str) -> Result<DurablePlayerState, String> {
    let mut version = false;
    let mut name = None;
    let mut role = None;
    let mut x = None;
    let mut y = None;
    let mut gold = None;
    let mut capacity = None;
    let mut items = Vec::new();
    let mut quests = Vec::new();
    for line in text.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| "malformed checkpoint line".to_owned())?;
        match key {
            "version" if value == "1" && !version => version = true,
            "version" => return Err("invalid or duplicate checkpoint version".to_owned()),
            "name" => set_once(&mut name, value.to_owned(), "name")?,
            "role" => set_once(
                &mut role,
                value
                    .parse::<Role>()
                    .map_err(|_| "invalid checkpoint role")?,
                "role",
            )?,
            "x" => set_once(
                &mut x,
                value.parse::<f32>().map_err(|_| "invalid checkpoint x")?,
                "x",
            )?,
            "y" => set_once(
                &mut y,
                value.parse::<f32>().map_err(|_| "invalid checkpoint y")?,
                "y",
            )?,
            "gold" => set_once(
                &mut gold,
                value
                    .parse::<u32>()
                    .map_err(|_| "invalid checkpoint gold")?,
                "gold",
            )?,
            "capacity" => set_once(
                &mut capacity,
                value
                    .parse::<usize>()
                    .map_err(|_| "invalid checkpoint capacity")?,
                "capacity",
            )?,
            "item" => {
                let (id, quantity) = value
                    .split_once(',')
                    .ok_or_else(|| "invalid checkpoint item".to_owned())?;
                items.push(ItemStack {
                    item_id: ItemId(id.parse().map_err(|_| "invalid checkpoint item id")?),
                    quantity: quantity
                        .parse()
                        .map_err(|_| "invalid checkpoint quantity")?,
                });
            }
            "quest" => {
                let fields: Vec<_> = value.split(',').collect();
                if fields.len() != 4 {
                    return Err("invalid checkpoint quest".to_owned());
                }
                quests.push(QuestProgress {
                    quest_id: QuestId(
                        fields[0]
                            .parse()
                            .map_err(|_| "invalid checkpoint quest id")?,
                    ),
                    progress: fields[1]
                        .parse()
                        .map_err(|_| "invalid checkpoint quest progress")?,
                    required_count: fields[2]
                        .parse()
                        .map_err(|_| "invalid checkpoint quest requirement")?,
                    status: parse_quest_status(fields[3])?,
                });
            }
            _ => return Err("unknown checkpoint field".to_owned()),
        }
    }
    if !version {
        return Err("missing checkpoint version".to_owned());
    }
    Ok(DurablePlayerState {
        name: name.ok_or_else(|| "missing checkpoint name".to_owned())?,
        role: role.ok_or_else(|| "missing checkpoint role".to_owned())?,
        position: Position::new(
            x.ok_or_else(|| "missing checkpoint x".to_owned())?,
            y.ok_or_else(|| "missing checkpoint y".to_owned())?,
        ),
        gold: gold.ok_or_else(|| "missing checkpoint gold".to_owned())?,
        inventory: Inventory::from_stacks(
            capacity.ok_or_else(|| "missing checkpoint capacity".to_owned())?,
            items,
        ),
        quests,
    })
}

fn set_once<T>(field: &mut Option<T>, value: T, name: &str) -> Result<(), String> {
    if field.replace(value).is_some() {
        return Err(format!("duplicate checkpoint {name}"));
    }
    Ok(())
}

fn quest_status_name(status: QuestStatus) -> &'static str {
    match status {
        QuestStatus::Accepted => "accepted",
        QuestStatus::Completed => "completed",
        QuestStatus::Rewarded => "rewarded",
    }
}
fn parse_quest_status(value: &str) -> Result<QuestStatus, String> {
    match value {
        "accepted" => Ok(QuestStatus::Accepted),
        "completed" => Ok(QuestStatus::Completed),
        "rewarded" => Ok(QuestStatus::Rewarded),
        _ => Err("invalid checkpoint quest status".to_owned()),
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
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn file_checkpoint_round_trips_and_rejects_malformed_data() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-checkpoint-{unique}.state"));
        let repository = DevelopmentAccountRepository::with_checkpoint_store(true, path.clone());
        let state = DurablePlayerState {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
            position: Position::new(8.0, -2.0),
            gold: 17,
            inventory: Inventory::from_stacks(
                16,
                vec![ItemStack {
                    item_id: ItemId::TOWN_RATION,
                    quantity: 2,
                }],
            ),
            quests: vec![QuestProgress {
                quest_id: QuestId::CLEAR_THE_FIELD,
                progress: 1,
                required_count: 3,
                status: QuestStatus::Accepted,
            }],
        };
        repository
            .save_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID, &state)
            .expect("checkpoint should save");
        assert_eq!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .expect("checkpoint should load"),
            Some(state)
        );
        fs::write(&path, "not a checkpoint\n").expect("malformed fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        fs::write(
            &path,
            "name=Aria\nrole=damage\nx=0\ny=0\ngold=0\ncapacity=0\n",
        )
        .expect("unversioned fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        fs::write(
            &path,
            "version=1\nname=Aria\nname=Other\nrole=damage\nx=0\ny=0\ngold=0\ncapacity=0\n",
        )
        .expect("duplicate fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        let _ = fs::remove_file(path);
    }
}
