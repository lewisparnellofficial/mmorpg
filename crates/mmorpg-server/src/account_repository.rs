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
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

const DEV_AUTH_TOKEN: &str = "dev-local";
const DEV_ACCOUNT_ID: u64 = 1;
const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024;
const MAX_CHECKPOINT_ITEMS: usize = 128;
const MAX_CHECKPOINT_QUESTS: usize = 128;
const MAX_OPERATION_JOURNAL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_OPERATION_RESULT_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_OPERATION_RESULT_COUNT: usize = 128;
#[cfg(test)]
const DEV_CHARACTER_ID: u64 = 1;

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
pub trait AccountCharacterRepository: Send + Sync {
    fn authenticate_development_token(&self, token: &str) -> Result<u64, AuthenticationError>;

    fn list_characters(&self, account_id: u64) -> Vec<CharacterRecord>;

    fn find_character(&self, account_id: u64, character_id: u64) -> Option<CharacterRecord>;

    fn load_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<DurablePlayerState>, String>;

    fn load_checkpoint_with_revision(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<(DurablePlayerState, u64)>, String> {
        self.load_checkpoint(account_id, character_id)
            .map(|state| state.map(|state| (state, 0)))
    }

    fn save_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String>;

    fn save_checkpoint_revisioned(
        &self,
        account_id: u64,
        character_id: u64,
        revision: u64,
        operation_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        let _ = (revision, operation_id);
        self.save_checkpoint(account_id, character_id, state)
    }
}

impl<T: AccountCharacterRepository + ?Sized> AccountCharacterRepository for Arc<T> {
    fn authenticate_development_token(&self, token: &str) -> Result<u64, AuthenticationError> {
        (**self).authenticate_development_token(token)
    }

    fn list_characters(&self, account_id: u64) -> Vec<CharacterRecord> {
        (**self).list_characters(account_id)
    }

    fn find_character(&self, account_id: u64, character_id: u64) -> Option<CharacterRecord> {
        (**self).find_character(account_id, character_id)
    }

    fn load_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<DurablePlayerState>, String> {
        (**self).load_checkpoint(account_id, character_id)
    }

    fn load_checkpoint_with_revision(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<(DurablePlayerState, u64)>, String> {
        (**self).load_checkpoint_with_revision(account_id, character_id)
    }

    fn save_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        (**self).save_checkpoint(account_id, character_id, state)
    }

    fn save_checkpoint_revisioned(
        &self,
        account_id: u64,
        character_id: u64,
        revision: u64,
        operation_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        (**self).save_checkpoint_revisioned(account_id, character_id, revision, operation_id, state)
    }
}

#[derive(Debug)]
pub struct CheckpointJob {
    pub account_id: u64,
    pub character_id: u64,
    pub revision: u64,
    pub operation_id: u64,
    pub state: DurablePlayerState,
}

pub struct CheckpointResult {
    pub character_id: u64,
    pub revision: u64,
    pub result: Result<(), String>,
}

/// Development-only cross-process character ownership lease.
///
/// The lease is a create-new marker containing the owning process ID. It is
/// deliberately kept behind the local checkpoint store and is not presented
/// as production fencing; a durable coordinator must replace this boundary.
#[derive(Clone, Debug)]
pub struct CharacterFenceStore {
    base_path: PathBuf,
}

#[derive(Debug)]
pub struct CharacterFence {
    path: PathBuf,
}

impl CharacterFenceStore {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    pub fn acquire(&self, account_id: u64, character_id: u64) -> Result<CharacterFence, String> {
        let parent = self.base_path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create character fence directory: {error}"))?;
        let stem = self
            .base_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("character.state");
        let path = parent.join(format!(
            ".{stem}.account-{account_id}.character-{character_id}.active"
        ));
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    writeln!(file, "pid={}", std::process::id())
                        .and_then(|()| file.sync_all())
                        .map_err(|error| format!("cannot initialize character fence: {error}"))?;
                    return Ok(CharacterFence { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if character_fence_owner_is_alive(&path) {
                        return Err("character is fenced by another server process".to_owned());
                    }
                    fs::remove_file(&path).map_err(|error| {
                        format!("cannot reclaim stale character fence: {error}")
                    })?;
                }
                Err(error) => {
                    return Err(format!("cannot acquire character fence: {error}"));
                }
            }
        }
        Err("character fence acquisition did not complete".to_owned())
    }
}

impl Drop for CharacterFence {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn character_fence_owner_is_alive(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return true;
    };
    let Some(pid) = contents.lines().find_map(|line| {
        line.strip_prefix("pid=")
            .and_then(|pid| pid.parse::<u32>().ok())
    }) else {
        return true;
    };
    Path::new("/proc").join(pid.to_string()).exists()
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OperationKey {
    pub account_id: u64,
    pub character_id: u64,
    pub operation_id: u64,
}

#[derive(Debug)]
pub enum OperationJournalJob {
    Prepare {
        key: OperationKey,
        command_payload: Vec<u8>,
    },
    Complete {
        key: OperationKey,
        revision: u64,
        command_payload: Vec<u8>,
        result_payloads: Vec<Vec<u8>>,
    },
    Failed {
        key: OperationKey,
        reason: String,
    },
}

pub struct OperationJournalResult {
    pub key: OperationKey,
    pub prepared: bool,
    pub failed: bool,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersistedOperation {
    Prepared(Vec<u8>),
    Completed {
        revision: u64,
        payloads: Vec<Vec<u8>>,
        command_payload: Option<Vec<u8>>,
    },
    Failed(String),
}

enum OperationJournalMessage {
    Job(OperationJournalJob),
    Stop,
}

/// Bounded append-only operation-result handoff. The simulation owner only
/// enqueues immutable result bytes; the journal thread performs file I/O.
pub struct OperationJournalWorker {
    sender: Option<SyncSender<OperationJournalMessage>>,
    queued: Option<Arc<AtomicUsize>>,
    results: Receiver<OperationJournalResult>,
    thread: Option<JoinHandle<()>>,
}

impl OperationJournalWorker {
    pub fn disabled() -> Self {
        let (_sender, results) = mpsc::channel();
        Self {
            sender: None,
            queued: None,
            results,
            thread: None,
        }
    }

    pub fn new(
        path: PathBuf,
    ) -> Result<(Self, BTreeMap<OperationKey, PersistedOperation>), String> {
        let completed = load_operation_journal(&path)?;
        let (sender, receiver) = mpsc::sync_channel(128);
        let queued = Arc::new(AtomicUsize::new(0));
        let queued_for_thread = Arc::clone(&queued);
        let (result_sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("mmorpg-operation-journal".to_owned())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    match message {
                        OperationJournalMessage::Job(job) => {
                            queued_for_thread.fetch_sub(1, Ordering::AcqRel);
                            let (key, prepared, failed) = match &job {
                                OperationJournalJob::Prepare { key, .. } => (*key, true, false),
                                OperationJournalJob::Complete { key, .. } => (*key, false, false),
                                OperationJournalJob::Failed { key, .. } => (*key, false, true),
                            };
                            let result = append_operation_journal(&path, &job);
                            let _ = result_sender.send(OperationJournalResult {
                                key,
                                prepared,
                                failed,
                                result,
                            });
                        }
                        OperationJournalMessage::Stop => break,
                    }
                }
            })
            .map_err(|error| format!("cannot start operation journal: {error}"))?;
        Ok((
            Self {
                sender: Some(sender),
                queued: Some(queued),
                results,
                thread: Some(thread),
            },
            completed,
        ))
    }

    pub fn try_enqueue(&self, job: OperationJournalJob) -> Result<(), OperationJournalJob> {
        if !operation_job_within_limits(&job) {
            return Err(job);
        }
        let (Some(sender), Some(queued)) = (&self.sender, &self.queued) else {
            return Err(job);
        };
        if !reserve_journal_slots(queued, 1) {
            return Err(job);
        }
        match sender.try_send(OperationJournalMessage::Job(job)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(OperationJournalMessage::Job(job)))
            | Err(TrySendError::Disconnected(OperationJournalMessage::Job(job))) => {
                queued.fetch_sub(1, Ordering::AcqRel);
                Err(job)
            }
            Err(TrySendError::Full(OperationJournalMessage::Stop))
            | Err(TrySendError::Disconnected(OperationJournalMessage::Stop)) => unreachable!(),
        }
    }

    pub fn try_enqueue_batch(
        &self,
        jobs: Vec<OperationJournalJob>,
    ) -> Result<(), Vec<OperationJournalJob>> {
        let count = jobs.len();
        if count == 0 {
            return Ok(());
        }
        if jobs.iter().any(|job| !operation_job_within_limits(job)) {
            return Err(jobs);
        }
        let (Some(sender), Some(queued)) = (&self.sender, &self.queued) else {
            return Err(jobs);
        };
        if !reserve_journal_slots(queued, count) {
            return Err(jobs);
        }
        for (index, job) in jobs.into_iter().enumerate() {
            match sender.try_send(OperationJournalMessage::Job(job)) {
                Ok(()) => {}
                Err(TrySendError::Full(OperationJournalMessage::Job(job)))
                | Err(TrySendError::Disconnected(OperationJournalMessage::Job(job))) => {
                    let remaining = count.saturating_sub(index + 1);
                    queued.fetch_sub(remaining + 1, Ordering::AcqRel);
                    return Err(vec![job]);
                }
                Err(TrySendError::Full(OperationJournalMessage::Stop))
                | Err(TrySendError::Disconnected(OperationJournalMessage::Stop)) => {
                    unreachable!()
                }
            }
        }
        Ok(())
    }

    pub fn enabled(&self) -> bool {
        self.sender.is_some()
    }

    pub fn drain_results(&self) -> impl Iterator<Item = OperationJournalResult> + '_ {
        std::iter::from_fn(|| self.results.try_recv().ok())
    }
}

fn reserve_journal_slots(queued: &AtomicUsize, count: usize) -> bool {
    const CAPACITY: usize = 128;
    let mut current = queued.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(count) else {
            return false;
        };
        if next > CAPACITY {
            return false;
        }
        match queued.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

fn operation_job_within_limits(job: &OperationJournalJob) -> bool {
    match job {
        OperationJournalJob::Prepare {
            command_payload, ..
        } => command_payload.len() <= MAX_OPERATION_RESULT_PAYLOAD_BYTES,
        OperationJournalJob::Complete {
            command_payload,
            result_payloads,
            ..
        } => {
            command_payload.len() <= MAX_OPERATION_RESULT_PAYLOAD_BYTES
                && result_payloads.len() <= MAX_OPERATION_RESULT_COUNT
                && result_payloads
                    .iter()
                    .all(|payload| payload.len() <= MAX_OPERATION_RESULT_PAYLOAD_BYTES)
        }
        OperationJournalJob::Failed { reason, .. } => {
            reason.len() <= MAX_OPERATION_RESULT_PAYLOAD_BYTES
        }
    }
}

impl Drop for OperationJournalWorker {
    fn drop(&mut self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(OperationJournalMessage::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

enum CheckpointMessage {
    Save(CheckpointJob),
    Stop,
}

/// Bounded persistence handoff. Simulation code only performs `try_send` and
/// polls results; file I/O happens on the dedicated writer thread.
pub struct CheckpointWorker {
    sender: SyncSender<CheckpointMessage>,
    results: Receiver<CheckpointResult>,
    thread: Option<JoinHandle<()>>,
}

impl CheckpointWorker {
    pub fn new(repository: Arc<dyn AccountCharacterRepository>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(128);
        let (result_sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("mmorpg-checkpoint-writer".to_owned())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    match message {
                        CheckpointMessage::Save(job) => {
                            let revision = job.revision;
                            let character_id = job.character_id;
                            let result = repository.save_checkpoint_revisioned(
                                job.account_id,
                                job.character_id,
                                job.revision,
                                job.operation_id,
                                &job.state,
                            );
                            let _ = result_sender.send(CheckpointResult {
                                character_id,
                                revision,
                                result,
                            });
                        }
                        CheckpointMessage::Stop => break,
                    }
                }
            })
            .expect("checkpoint writer thread should start");
        Self {
            sender,
            results,
            thread: Some(thread),
        }
    }

    pub fn try_enqueue(&self, job: CheckpointJob) -> Result<(), CheckpointJob> {
        match self.sender.try_send(CheckpointMessage::Save(job)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(CheckpointMessage::Save(job)))
            | Err(TrySendError::Disconnected(CheckpointMessage::Save(job))) => Err(job),
            Err(TrySendError::Full(CheckpointMessage::Stop))
            | Err(TrySendError::Disconnected(CheckpointMessage::Stop)) => unreachable!(),
        }
    }

    pub fn drain_results(&self) -> impl Iterator<Item = CheckpointResult> + '_ {
        std::iter::from_fn(|| self.results.try_recv().ok())
    }
}

impl Drop for CheckpointWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(CheckpointMessage::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
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

    fn development_characters() -> [CharacterRecord; 3] {
        [
            CharacterRecord {
                character_id: 1,
                account_id: DEV_ACCOUNT_ID,
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            },
            CharacterRecord {
                character_id: 2,
                account_id: DEV_ACCOUNT_ID,
                name: "Borin".to_owned(),
                role: Role::Tank,
            },
            CharacterRecord {
                character_id: 3,
                account_id: DEV_ACCOUNT_ID,
                name: "Celia".to_owned(),
                role: Role::Healer,
            },
        ]
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
        Self::development_characters()
            .into_iter()
            .filter(|character| character.account_id == account_id)
            .collect()
    }

    fn find_character(&self, account_id: u64, character_id: u64) -> Option<CharacterRecord> {
        Self::development_characters()
            .into_iter()
            .find(|character| {
                account_id == character.account_id && character_id == character.character_id
            })
    }

    fn load_checkpoint(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<DurablePlayerState>, String> {
        if self.find_character(account_id, character_id).is_none() {
            return Ok(None);
        }
        self.checkpoint_store.as_ref().map_or(Ok(None), |store| {
            LocalCheckpointStore::new(store.path_for_character(account_id, character_id)).load()
        })
    }

    fn load_checkpoint_with_revision(
        &self,
        account_id: u64,
        character_id: u64,
    ) -> Result<Option<(DurablePlayerState, u64)>, String> {
        if self.find_character(account_id, character_id).is_none() {
            return Ok(None);
        }
        self.checkpoint_store.as_ref().map_or(Ok(None), |store| {
            LocalCheckpointStore::new(store.path_for_character(account_id, character_id))
                .load_record()
                .map(|record| record.map(|record| (record.state, record.revision)))
        })
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
        self.checkpoint_store.as_ref().map_or(Ok(()), |store| {
            LocalCheckpointStore::new(store.path_for_character(account_id, character_id))
                .save(state)
        })
    }

    fn save_checkpoint_revisioned(
        &self,
        account_id: u64,
        character_id: u64,
        revision: u64,
        operation_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        if self.find_character(account_id, character_id).is_none() {
            return Err("unknown character".to_owned());
        }
        self.checkpoint_store.as_ref().map_or(Ok(()), |store| {
            LocalCheckpointStore::new(store.path_for_character(account_id, character_id))
                .save_revisioned(revision, operation_id, state)
        })
    }
}

struct LocalCheckpointStore {
    path: PathBuf,
}

impl LocalCheckpointStore {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path_for_character(&self, account_id: u64, character_id: u64) -> PathBuf {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let stem = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("character.state");
        parent.join(format!(
            "{stem}.account-{account_id}.character-{character_id}"
        ))
    }

    fn load(&self) -> Result<Option<DurablePlayerState>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let text = self.read_bounded()?;
        parse_checkpoint(&text).map(Some)
    }

    fn load_record(&self) -> Result<Option<CheckpointRecord>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let text = self.read_bounded()?;
        parse_checkpoint_record(&text).map(Some)
    }

    fn read_bounded(&self) -> Result<String, String> {
        let length = fs::metadata(&self.path)
            .map_err(|error| format!("cannot inspect checkpoint: {error}"))?
            .len();
        if length > MAX_CHECKPOINT_BYTES {
            return Err("checkpoint exceeds its size limit".to_owned());
        }
        fs::read_to_string(&self.path).map_err(|error| format!("cannot read checkpoint: {error}"))
    }

    fn save(&self, state: &DurablePlayerState) -> Result<(), String> {
        self.save_revisioned(0, 0, state)
    }

    fn save_revisioned(
        &self,
        revision: u64,
        operation_id: u64,
        state: &DurablePlayerState,
    ) -> Result<(), String> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create checkpoint directory: {error}"))?;
        if let Some(existing) = self.load_record()? {
            if existing.revision > revision
                || (existing.revision == revision && existing.operation_id == operation_id)
            {
                return Ok(());
            }
            if existing.revision == revision {
                return Err("checkpoint revision reused with a different operation".to_owned());
            }
        }
        let serialized = format_checkpoint(revision, operation_id, state);
        if serialized.len() > MAX_CHECKPOINT_BYTES as usize {
            return Err("checkpoint exceeds its size limit".to_owned());
        }
        let temporary = self.path.with_extension("tmp");
        let mut file = File::create(&temporary)
            .map_err(|error| format!("cannot create checkpoint: {error}"))?;
        file.write_all(serialized.as_bytes())
            .map_err(|error| format!("cannot write checkpoint: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("cannot sync checkpoint: {error}"))?;
        fs::rename(&temporary, &self.path)
            .map_err(|error| format!("cannot replace checkpoint: {error}"))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("cannot sync checkpoint directory: {error}"))
    }
}

fn load_operation_journal(
    path: &Path,
) -> Result<BTreeMap<OperationKey, PersistedOperation>, String> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let length = fs::metadata(path)
        .map_err(|error| format!("cannot inspect operation journal: {error}"))?
        .len();
    if length > MAX_OPERATION_JOURNAL_BYTES {
        return Err("operation journal exceeds its size limit".to_owned());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read operation journal: {error}"))?;
    let mut operations = BTreeMap::new();
    for (line_index, line) in text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // `append_operation_journal` writes and syncs a newline-terminated
        // record. A process crash can leave only a torn final line; discard
        // that one record while preserving every earlier complete record.
        if line_index + 1 == text.split('\n').count() && !text.ends_with('\n') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 6 && fields.len() != 7 && fields.len() != 8 {
            return Err("malformed operation journal record".to_owned());
        }
        let version = fields[0];
        if version != "version=1" && version != "version=2" {
            return Err("malformed operation journal record".to_owned());
        }
        let account_id = parse_journal_field(fields[2], "account")?;
        let character_id = parse_journal_field(fields[3], "character")?;
        let operation_id = parse_journal_field(fields[4], "operation")?;
        if fields[1] == "prepared" {
            if version != "version=1" || fields.len() != 6 {
                return Err("malformed operation journal prepared record".to_owned());
            }
            let Some(command) = fields[5].strip_prefix("command=") else {
                return Err("malformed operation journal command field".to_owned());
            };
            operations.insert(
                OperationKey {
                    account_id,
                    character_id,
                    operation_id,
                },
                PersistedOperation::Prepared(decode_hex_bounded(
                    command,
                    MAX_OPERATION_RESULT_PAYLOAD_BYTES,
                    "operation journal command payload exceeds its limit",
                )?),
            );
            continue;
        }
        if fields[1] == "failed" {
            if version != "version=1" || fields.len() != 6 {
                return Err("malformed operation journal failed record".to_owned());
            }
            let Some(reason) = fields[5].strip_prefix("reason=") else {
                return Err("malformed operation journal failure field".to_owned());
            };
            operations.insert(
                OperationKey {
                    account_id,
                    character_id,
                    operation_id,
                },
                PersistedOperation::Failed(
                    String::from_utf8(decode_hex_bounded(
                        reason,
                        MAX_OPERATION_RESULT_PAYLOAD_BYTES,
                        "operation journal failure payload exceeds its limit",
                    )?)
                    .map_err(|_| "invalid operation journal failure payload".to_owned())?,
                ),
            );
            continue;
        }
        if fields[1] != "completed" {
            return Err("unknown operation journal record kind".to_owned());
        }
        let (revision, command_payload, results_field) = if version == "version=2" {
            if fields.len() != 7 && fields.len() != 8 {
                return Err("malformed operation journal completed record".to_owned());
            }
            let revision = parse_journal_field(fields[5], "revision")?;
            if fields.len() == 8 {
                let command = fields[6]
                    .strip_prefix("command=")
                    .ok_or_else(|| "malformed operation journal command field".to_owned())?;
                (
                    revision,
                    Some(decode_hex_bounded(
                        command,
                        MAX_OPERATION_RESULT_PAYLOAD_BYTES,
                        "operation journal command payload exceeds its limit",
                    )?),
                    fields[7],
                )
            } else {
                (revision, None, fields[6])
            }
        } else {
            if fields.len() != 6 {
                return Err("malformed operation journal completed record".to_owned());
            }
            (0, None, fields[5])
        };
        let payloads = if let Some(encoded) = results_field.strip_prefix("results=") {
            if encoded.is_empty() {
                Vec::new()
            } else {
                let encoded_payloads = encoded.split(',').collect::<Vec<_>>();
                if encoded_payloads.len() > MAX_OPERATION_RESULT_COUNT {
                    return Err("operation journal result count exceeds its limit".to_owned());
                }
                encoded_payloads
                    .into_iter()
                    .map(|payload| {
                        if payload.len() > MAX_OPERATION_RESULT_PAYLOAD_BYTES * 2 {
                            return Err(
                                "operation journal result payload exceeds its limit".to_owned()
                            );
                        }
                        decode_hex(payload)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
        } else {
            return Err("malformed operation journal result field".to_owned());
        };
        let prior_command_payload = match operations.remove(&OperationKey {
            account_id,
            character_id,
            operation_id,
        }) {
            Some(PersistedOperation::Prepared(command_payload)) => Some(command_payload),
            Some(PersistedOperation::Completed {
                command_payload, ..
            }) => command_payload,
            _ => None,
        };
        let command_payload = command_payload.or(prior_command_payload);
        operations.insert(
            OperationKey {
                account_id,
                character_id,
                operation_id,
            },
            PersistedOperation::Completed {
                revision,
                payloads,
                command_payload,
            },
        );
    }
    Ok(operations)
}

fn append_operation_journal(path: &Path, job: &OperationJournalJob) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create operation journal directory: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open operation journal: {error}"))?;
    let record = match job {
        OperationJournalJob::Prepare {
            key,
            command_payload,
        } => format!(
            "version=1\tprepared\taccount={}\tcharacter={}\toperation={}\tcommand={}",
            key.account_id,
            key.character_id,
            key.operation_id,
            encode_hex(command_payload)
        ),
        OperationJournalJob::Complete {
            key,
            revision,
            command_payload,
            result_payloads,
        } => {
            let results = result_payloads
                .iter()
                .map(|payload| encode_hex(payload))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "version=2\tcompleted\taccount={}\tcharacter={}\toperation={}\trevision={}\tcommand={}\tresults={}",
                key.account_id,
                key.character_id,
                key.operation_id,
                revision,
                encode_hex(command_payload),
                results
            )
        }
        OperationJournalJob::Failed { key, reason } => format!(
            "version=1\tfailed\taccount={}\tcharacter={}\toperation={}\treason={}",
            key.account_id,
            key.character_id,
            key.operation_id,
            encode_hex(reason.as_bytes())
        ),
    };
    writeln!(file, "{record}")
        .map_err(|error| format!("cannot append operation journal: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("cannot sync operation journal: {error}"))
}

fn parse_journal_field(field: &str, name: &str) -> Result<u64, String> {
    field
        .strip_prefix(&format!("{name}="))
        .ok_or_else(|| format!("malformed operation journal {name} field"))?
        .parse()
        .map_err(|_| format!("invalid operation journal {name}"))
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("odd-length operation journal payload".to_owned());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "invalid operation journal payload".to_owned())
        })
        .collect()
}

fn decode_hex_bounded(value: &str, max_bytes: usize, message: &str) -> Result<Vec<u8>, String> {
    if value.len() > max_bytes.saturating_mul(2) {
        return Err(message.to_owned());
    }
    decode_hex(value)
}

struct CheckpointRecord {
    revision: u64,
    operation_id: u64,
    state: DurablePlayerState,
}

fn format_checkpoint(revision: u64, operation_id: u64, state: &DurablePlayerState) -> String {
    let mut text = format!(
        "version=2\nrevision={}\noperation_id={}\nname={}\nrole={}\nx={}\ny={}\ngold={}\ncapacity={}\n",
        revision,
        operation_id,
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
    Ok(parse_checkpoint_record(text)?.state)
}

fn parse_checkpoint_record(text: &str) -> Result<CheckpointRecord, String> {
    let mut version = false;
    let mut revision = None;
    let mut operation_id = None;
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
            "version" if (value == "1" || value == "2") && !version => version = true,
            "version" => return Err("invalid or duplicate checkpoint version".to_owned()),
            "revision" => set_once(
                &mut revision,
                value.parse().map_err(|_| "invalid checkpoint revision")?,
                "revision",
            )?,
            "operation_id" => {
                set_once(
                    &mut operation_id,
                    value
                        .parse()
                        .map_err(|_| "invalid checkpoint operation id")?,
                    "operation_id",
                )?;
            }
            "name" if value.len() <= 24 => set_once(&mut name, value.to_owned(), "name")?,
            "name" => return Err("checkpoint name is too long".to_owned()),
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
                if items.len() >= MAX_CHECKPOINT_ITEMS {
                    return Err("too many checkpoint items".to_owned());
                }
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
                if quests.len() >= MAX_CHECKPOINT_QUESTS {
                    return Err("too many checkpoint quests".to_owned());
                }
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
    Ok(CheckpointRecord {
        revision: revision.unwrap_or(0),
        operation_id: operation_id.unwrap_or(0),
        state: DurablePlayerState {
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
        },
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

        assert_eq!(repository.list_characters(DEV_ACCOUNT_ID).len(), 3);
        assert!(repository.list_characters(2).is_empty());

        let character = repository
            .find_character(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
            .expect("development character should exist");
        assert_eq!(character.name, "Aria");
        assert_eq!(character.role, Role::DamageDealer);
        assert_eq!(
            repository.find_character(DEV_ACCOUNT_ID, 2).unwrap().role,
            Role::Tank
        );
        assert_eq!(
            repository.find_character(DEV_ACCOUNT_ID, 3).unwrap().role,
            Role::Healer
        );
        assert!(repository.find_character(2, DEV_CHARACTER_ID).is_none());
        assert!(repository.find_character(DEV_ACCOUNT_ID, 4).is_none());
    }

    #[test]
    fn character_fence_rejects_live_owner_and_reclaims_stale_marker() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("mmorpg-fence-{unique}.state"));
        let store = CharacterFenceStore::new(base.clone());
        let fence = store
            .acquire(7, 42)
            .expect("first owner should acquire fence");
        assert!(store.acquire(7, 42).is_err());
        drop(fence);
        let fence = store
            .acquire(7, 42)
            .expect("released owner should acquire fence again");
        drop(fence);

        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let stale = parent.join(format!(
            ".{}.account-7.character-42.active",
            base.file_name().unwrap().to_str().unwrap()
        ));
        fs::write(&stale, "pid=4294967294\n").expect("stale marker should write");
        let fence = store
            .acquire(7, 42)
            .expect("stale owner marker should be reclaimed");
        drop(fence);
        let _ = fs::remove_file(stale);
    }

    #[test]
    fn file_checkpoint_round_trips_and_rejects_malformed_data() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-checkpoint-{unique}.state"));
        let repository = DevelopmentAccountRepository::with_checkpoint_store(true, path.clone());
        let character_path = repository
            .checkpoint_store
            .as_ref()
            .unwrap()
            .path_for_character(DEV_ACCOUNT_ID, DEV_CHARACTER_ID);
        let other_character_path = repository
            .checkpoint_store
            .as_ref()
            .unwrap()
            .path_for_character(DEV_ACCOUNT_ID, 2);
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
            Some(state.clone())
        );
        let mut other_character = state.clone();
        other_character.name = "Borin".to_owned();
        other_character.role = Role::Tank;
        repository
            .save_checkpoint(DEV_ACCOUNT_ID, 2, &other_character)
            .expect("second character checkpoint should save");
        assert_eq!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, 2)
                .expect("second character checkpoint should load")
                .expect("second character checkpoint should exist")
                .name,
            "Borin"
        );
        assert_eq!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .expect("first character checkpoint should remain")
                .expect("first character checkpoint should exist")
                .name,
            "Aria"
        );
        let mut oversized_state = state.clone();
        oversized_state.name = "x".repeat(MAX_CHECKPOINT_BYTES as usize + 1);
        assert!(
            repository
                .save_checkpoint_revisioned(
                    DEV_ACCOUNT_ID,
                    DEV_CHARACTER_ID,
                    1,
                    1,
                    &oversized_state,
                )
                .is_err()
        );
        assert_eq!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .expect("prior checkpoint should remain readable")
                .expect("prior checkpoint should remain present")
                .name,
            "Aria"
        );
        fs::write(&character_path, "not a checkpoint\n").expect("malformed fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        fs::write(
            &character_path,
            vec![b'x'; MAX_CHECKPOINT_BYTES as usize + 1],
        )
        .expect("oversized checkpoint fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        fs::write(
            &character_path,
            "name=Aria\nrole=damage\nx=0\ny=0\ngold=0\ncapacity=0\n",
        )
        .expect("unversioned fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        fs::write(
            &character_path,
            "version=1\nname=Aria\nname=Other\nrole=damage\nx=0\ny=0\ngold=0\ncapacity=0\n",
        )
        .expect("duplicate fixture should write");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .is_err()
        );
        let _ = fs::remove_file(character_path);
        let _ = fs::remove_file(other_character_path);
    }

    #[test]
    fn revisioned_checkpoint_rejects_stale_writes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-revision-{unique}.state"));
        let repository = DevelopmentAccountRepository::with_checkpoint_store(true, path);
        let newer = DurablePlayerState {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
            position: Position::new(9.0, 0.0),
            gold: 20,
            inventory: Inventory::from_stacks(8, Vec::new()),
            quests: Vec::new(),
        };
        let mut older = newer.clone();
        older.position = Position::new(1.0, 0.0);
        repository
            .save_checkpoint_revisioned(DEV_ACCOUNT_ID, DEV_CHARACTER_ID, 8, 800, &newer)
            .expect("newer checkpoint should save");
        repository
            .save_checkpoint_revisioned(DEV_ACCOUNT_ID, DEV_CHARACTER_ID, 7, 700, &older)
            .expect("stale retry should be harmless");
        assert_eq!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .expect("checkpoint should load")
                .expect("checkpoint should exist")
                .position,
            newer.position
        );
    }

    #[test]
    fn checkpoint_worker_hands_off_file_io_and_reports_completion() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-worker-{unique}.state"));
        let repository: Arc<dyn AccountCharacterRepository> = Arc::new(
            DevelopmentAccountRepository::with_checkpoint_store(true, path),
        );
        let worker = CheckpointWorker::new(Arc::clone(&repository));
        worker
            .try_enqueue(CheckpointJob {
                account_id: DEV_ACCOUNT_ID,
                character_id: DEV_CHARACTER_ID,
                revision: 4,
                operation_id: 400,
                state: DurablePlayerState {
                    name: "Aria".to_owned(),
                    role: Role::DamageDealer,
                    position: Position::new(4.0, 2.0),
                    gold: 3,
                    inventory: Inventory::from_stacks(4, Vec::new()),
                    quests: Vec::new(),
                },
            })
            .expect("checkpoint should enter bounded queue");
        let mut completed = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(completed, "writer should report checkpoint completion");
        assert!(
            repository
                .load_checkpoint(DEV_ACCOUNT_ID, DEV_CHARACTER_ID)
                .expect("checkpoint should load")
                .is_some()
        );
    }

    #[test]
    fn operation_journal_worker_persists_results_for_restart() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-operations-{unique}.journal"));
        let key = OperationKey {
            account_id: DEV_ACCOUNT_ID,
            character_id: DEV_CHARACTER_ID,
            operation_id: 99,
        };
        let (worker, initially_loaded) =
            OperationJournalWorker::new(path.clone()).expect("journal should start");
        assert!(initially_loaded.is_empty());
        worker
            .try_enqueue(OperationJournalJob::Prepare {
                key,
                command_payload: vec![0x09, 0x08],
            })
            .expect("operation should enter bounded queue");
        let mut completed = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(completed, "journal should report completion");
        worker
            .try_enqueue(OperationJournalJob::Complete {
                key,
                revision: 7,
                command_payload: vec![0x09, 0x08],
                result_payloads: vec![vec![0x01, 0xa5, 0xff]],
            })
            .expect("result should enter bounded queue");
        let mut completed = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(completed, "journal result should report completion");
        worker
            .try_enqueue(OperationJournalJob::Failed {
                key,
                reason: "test failure record".to_owned(),
            })
            .expect("failure record should enter bounded queue");
        let mut failed_recorded = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                failed_recorded = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(failed_recorded, "journal failure record should complete");
        drop(worker);

        use std::io::Write as _;
        let mut journal = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("journal should reopen");
        journal
            .write_all(b"version=1\tcompleted\taccount=1\tcharacter=1")
            .expect("torn record fixture should write");
        journal.sync_all().expect("torn record fixture should sync");
        drop(journal);

        let (_worker, loaded) = OperationJournalWorker::new(path.clone()).expect("journal reload");
        assert_eq!(
            loaded.get(&key),
            Some(&PersistedOperation::Failed(
                "test failure record".to_owned()
            ))
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn operation_journal_rejects_oversized_startup_input() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mmorpg-operation-oversized-{unique}.journal"));
        let oversized = vec![b'x'; MAX_OPERATION_JOURNAL_BYTES as usize + 1];
        fs::write(&path, oversized).expect("oversized journal fixture should write");
        let error = match OperationJournalWorker::new(path.clone()) {
            Ok(_) => panic!("oversized journal should fail before worker startup"),
            Err(error) => error,
        };
        assert!(error.contains("exceeds its size limit"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn operation_journal_rejects_oversized_jobs_before_enqueue() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mmorpg-operation-job-limit-{unique}.journal"));
        let (worker, _) = OperationJournalWorker::new(path.clone()).expect("journal should start");
        let key = OperationKey {
            account_id: DEV_ACCOUNT_ID,
            character_id: DEV_CHARACTER_ID,
            operation_id: 103,
        };
        assert!(
            worker
                .try_enqueue(OperationJournalJob::Prepare {
                    key,
                    command_payload: vec![0; MAX_OPERATION_RESULT_PAYLOAD_BYTES + 1],
                })
                .is_err()
        );
        assert!(
            worker
                .try_enqueue(OperationJournalJob::Complete {
                    key,
                    revision: 1,
                    command_payload: Vec::new(),
                    result_payloads: vec![vec![0; MAX_OPERATION_RESULT_PAYLOAD_BYTES + 1]],
                })
                .is_err()
        );
        drop(worker);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn operation_journal_preserves_completed_revision_metadata() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-operation-revision-{unique}.journal"));
        let key = OperationKey {
            account_id: DEV_ACCOUNT_ID,
            character_id: DEV_CHARACTER_ID,
            operation_id: 100,
        };
        let (worker, _) = OperationJournalWorker::new(path.clone()).expect("journal should start");
        worker
            .try_enqueue(OperationJournalJob::Complete {
                key,
                revision: 42,
                command_payload: Vec::new(),
                result_payloads: vec![vec![0xaa, 0xbb]],
            })
            .expect("completed operation should enter bounded queue");
        let mut completed = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(completed, "journal should report completion");
        drop(worker);

        let (_worker, loaded) = OperationJournalWorker::new(path.clone()).expect("journal reload");
        assert_eq!(
            loaded.get(&key),
            Some(&PersistedOperation::Completed {
                revision: 42,
                payloads: vec![vec![0xaa, 0xbb]],
                command_payload: Some(Vec::new()),
            })
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn operation_journal_reloads_interrupted_prepare_records() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("mmorpg-operation-prepared-{unique}.journal"));
        let key = OperationKey {
            account_id: DEV_ACCOUNT_ID,
            character_id: DEV_CHARACTER_ID,
            operation_id: 101,
        };
        let (worker, _) = OperationJournalWorker::new(path.clone()).expect("journal should start");
        worker
            .try_enqueue(OperationJournalJob::Prepare {
                key,
                command_payload: vec![0x10, 0x20],
            })
            .expect("prepared operation should enter bounded queue");
        let mut prepared = false;
        for _ in 0..100 {
            if worker.drain_results().any(|result| result.result.is_ok()) {
                prepared = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(prepared, "journal should report prepared record");
        drop(worker);

        let (_worker, loaded) = OperationJournalWorker::new(path.clone()).expect("journal reload");
        assert_eq!(
            loaded.get(&key),
            Some(&PersistedOperation::Prepared(vec![0x10, 0x20]))
        );
        let _ = fs::remove_file(path);
    }
}
