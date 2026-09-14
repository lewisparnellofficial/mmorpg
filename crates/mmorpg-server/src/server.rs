//! Authoritative server coordinator, network intake loop, command dispatch, and shutdown.

use crate::account_repository::{
    AccountCharacterRepository, CharacterFence, CharacterFenceStore, CheckpointJob,
    CheckpointWorker, DevelopmentAccountRepository, OperationJournalJob, OperationJournalWorker,
    OperationKey,
};
use crate::client::{
    ClientOrigin, MAX_WIRE_CLIENTS, MAX_WIRE_COMMANDS_PER_CLIENT_POLL, MAX_WIRE_COMMANDS_PER_POLL,
    MAX_WIRE_INPUT_BYTES, WireClient, origin_client_id,
};
use crate::interest::{event_recipient, event_visible_to_player};
use crate::journal_batch::{
    MAX_COMPLETED_OPERATIONS, StagedOperationBatch, load_persisted_operations, trim_operation_maps,
};
use crate::session::{
    AuthenticatedSession, DISCONNECT_GRACE_TICKS, DetachedCharacter, PendingCommand,
    character_reserved_by_other as is_character_reserved,
};
use crate::wire_adapter::{
    command_player_id, command_to_wire_payload, is_retryable_core_command, wire_command_to_core,
    wire_event, wire_role, wire_snapshot_for_player,
};
use mmorpg_content::starter_catalog;
use mmorpg_core::{CombatTiming, Command, EntityId, Event, World};
use mmorpg_wire::{
    ClientCommand as WireCommand, DecodeError as WireDecodeError, MessageKind, ServerMessage,
    decode_one,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_ADDRESS: &str = "127.0.0.1:4000";
pub const DEFAULT_TICK_HZ: u64 = 20;
pub const DEFAULT_CAST_TIME_TICKS: u64 = 2;
pub const DEFAULT_COMBAT_COOLDOWN_TICKS: u64 = 2;
pub const MAX_PENDING_COMMANDS: usize = 1024;
pub const MAX_COMMANDS_PER_TICK: usize = 256;
pub const CHECKPOINT_INTERVAL_TICKS: u64 = 20;
pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimulationTimingStats {
    pub ticks: u64,
    pub deadline_misses: u64,
    pub max_duration: Duration,
}

impl SimulationTimingStats {
    pub fn record(&mut self, duration: Duration, deadline: Duration) {
        self.ticks = self.ticks.saturating_add(1);
        self.max_duration = self.max_duration.max(duration);
        if duration > deadline {
            self.deadline_misses = self.deadline_misses.saturating_add(1);
        }
    }
}

pub fn interleave_wire_commands(
    mut queues: Vec<Vec<(u64, WireCommand)>>,
) -> Vec<(u64, WireCommand)> {
    let total = queues.iter().map(Vec::len).sum();
    let mut interleaved = Vec::with_capacity(total);
    while queues.iter().any(|queue| !queue.is_empty()) {
        for queue in &mut queues {
            if !queue.is_empty() {
                interleaved.push(queue.remove(0));
            }
        }
    }
    interleaved
}

pub struct Server {
    pub world: World,
    pub wire_clients: Vec<WireClient>,
    pub detached_characters: BTreeMap<(u64, u64), DetachedCharacter>,
    pub character_fences: BTreeMap<(u64, u64), CharacterFence>,
    pub character_fence_store: Option<CharacterFenceStore>,
    pub commands: VecDeque<PendingCommand>,
    pub prepared_operations: BTreeMap<OperationKey, (u64, Command)>,
    pub staged_operation_batch: Option<StagedOperationBatch>,
    pub completed_operations: BTreeMap<OperationKey, Vec<ServerMessage>>,
    pub recovery_operations: BTreeMap<OperationKey, (u64, WireCommand)>,
    pub pending_recovery_operations: BTreeMap<(u64, u64), Vec<(OperationKey, WireCommand)>>,
    pub recovering_operations: BTreeSet<OperationKey>,
    pub failed_operations: BTreeMap<OperationKey, String>,
    pub pending_failed_operations: BTreeMap<OperationKey, (u64, String)>,
    pub operation_journal_worker: OperationJournalWorker,
    pub account_repository: Arc<dyn AccountCharacterRepository>,
    pub checkpoint_worker: CheckpointWorker,
    pub next_client_id: u64,
    pub next_session_id: u64,
    pub active_request_ids: BTreeMap<u64, u64>,
    pub combat_timing: CombatTiming,
    pub tick_interval: Duration,
    pub next_tick: Instant,
    pub timing_stats: SimulationTimingStats,
}

impl Server {
    pub fn new(tick_hz: u64, dev_auth_enabled: bool, checkpoint_path: Option<PathBuf>) -> Self {
        let character_fence_store = checkpoint_path
            .as_ref()
            .map(|path| CharacterFenceStore::new(path.clone()));
        let operation_journal_path = checkpoint_path
            .as_ref()
            .map(|path| path.with_extension("operations"));
        let repository: Box<dyn AccountCharacterRepository> = match checkpoint_path {
            Some(path) => Box::new(DevelopmentAccountRepository::with_checkpoint_store(
                dev_auth_enabled,
                path,
            )),
            None => Box::new(DevelopmentAccountRepository::new(dev_auth_enabled)),
        };
        let mut server =
            Self::with_account_repository_and_journal(tick_hz, repository, operation_journal_path);
        server.character_fence_store = character_fence_store;
        server
    }

    pub fn with_account_repository_and_journal(
        tick_hz: u64,
        account_repository: Box<dyn AccountCharacterRepository>,
        operation_journal_path: Option<PathBuf>,
    ) -> Self {
        let tick_hz = tick_hz.max(1);
        let tick_interval = Duration::from_secs_f64(1.0 / tick_hz as f64);
        let combat_timing = CombatTiming::new(
            tick_hz.min(u32::MAX as u64) as u32,
            DEFAULT_CAST_TIME_TICKS,
            DEFAULT_COMBAT_COOLDOWN_TICKS,
        )
        .expect("tick_hz is clamped above zero");
        let account_repository: Arc<dyn AccountCharacterRepository> = Arc::from(account_repository);
        let checkpoint_worker = CheckpointWorker::new(Arc::clone(&account_repository));
        let (operation_journal_worker, persisted_operations) = operation_journal_path
            .map(|path| {
                OperationJournalWorker::new(path).expect("operation journal should load and start")
            })
            .unwrap_or_else(|| (OperationJournalWorker::disabled(), BTreeMap::new()));
        let loaded = load_persisted_operations(persisted_operations);
        for key in loaded.interrupted {
            let _ = operation_journal_worker.try_enqueue(OperationJournalJob::Failed {
                key,
                reason: "operation was interrupted before durable completion".to_owned(),
            });
        }
        Self {
            world: World::new_starter_zone(),
            wire_clients: Vec::new(),
            detached_characters: BTreeMap::new(),
            character_fences: BTreeMap::new(),
            character_fence_store: None,
            commands: VecDeque::new(),
            prepared_operations: BTreeMap::new(),
            staged_operation_batch: None,
            completed_operations: loaded.completed,
            recovery_operations: loaded.recovery,
            pending_recovery_operations: BTreeMap::new(),
            recovering_operations: BTreeSet::new(),
            failed_operations: loaded.failed,
            pending_failed_operations: BTreeMap::new(),
            operation_journal_worker,
            checkpoint_worker,
            account_repository,
            next_client_id: 1,
            next_session_id: 1,
            active_request_ids: BTreeMap::new(),
            combat_timing,
            tick_interval,
            next_tick: Instant::now() + tick_interval,
            timing_stats: SimulationTimingStats::default(),
        }
    }

    pub fn accept_from(&mut self, listener: &TcpListener, log_prefix: &str) -> io::Result<()> {
        loop {
            match listener.accept() {
                Ok((stream, peer)) => {
                    stream.set_nonblocking(true)?;
                    println!("{log_prefix}={peer}");
                    let _ = self.add_wire_client(stream);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }

    pub fn add_wire_client(&mut self, stream: TcpStream) -> bool {
        if self.wire_clients.len() >= MAX_WIRE_CLIENTS {
            eprintln!(
                "wire_client_capacity_reached active={} limit={MAX_WIRE_CLIENTS}",
                self.wire_clients.len()
            );
            return false;
        }
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.saturating_add(1);
        let mut client = WireClient::new(id, stream);
        client.queue_server_message(&ServerMessage::Welcome {
            server: "mmorpg-server".to_owned(),
        });
        self.wire_clients.push(client);
        println!("wire_client_connected id={id}");
        true
    }

    pub fn read_wire_clients(&mut self) {
        let mut command_queues = Vec::new();
        let mut errors = Vec::new();
        let mut compatibility_rejections = Vec::new();
        let mut decoded_frames = 0;
        for client in &mut self.wire_clients {
            let mut client_commands = Vec::new();
            if client.closed {
                command_queues.push(client_commands);
                continue;
            }
            let mut buffer = [0_u8; 4096];
            loop {
                match client.stream.read(&mut buffer) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_read) => {
                        client.input.extend_from_slice(&buffer[..bytes_read]);
                        if client.input.len() > MAX_WIRE_INPUT_BYTES {
                            errors.push((
                                client.id,
                                "wire input buffer exceeded its limit".to_owned(),
                            ));
                            client.input.clear();
                            client.closed = true;
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("wire_client_read_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }

            let mut client_frames = 0;
            loop {
                if client_frames >= MAX_WIRE_COMMANDS_PER_CLIENT_POLL {
                    errors.push((
                        client.id,
                        "typed command intake limit reached for this poll".to_owned(),
                    ));
                    client.input.clear();
                    break;
                }
                if decoded_frames >= MAX_WIRE_COMMANDS_PER_POLL {
                    errors.push((
                        client.id,
                        "global typed command intake limit reached for this poll".to_owned(),
                    ));
                    client.input.clear();
                    break;
                }
                let decoded = match decode_one(&client.input) {
                    Ok(decoded) => decoded,
                    Err(WireDecodeError::Truncated { .. }) => break,
                    Err(WireDecodeError::UnsupportedVersion { .. }) => {
                        compatibility_rejections.push((
                            client.id,
                            mmorpg_wire::PROTOCOL_VERSION,
                            mmorpg_wire::PROTOCOL_VERSION,
                        ));
                        client.input.clear();
                        client.closed = true;
                        break;
                    }
                    Err(error) => {
                        errors.push((client.id, format!("invalid wire frame: {error}")));
                        client.input.clear();
                        client.closed = true;
                        break;
                    }
                };
                let consumed = decoded.consumed;
                let kind = decoded.envelope.kind;
                let payload = decoded.envelope.payload;
                client.input.drain(..consumed);
                client_frames += 1;
                decoded_frames += 1;
                if kind != MessageKind::Command {
                    errors.push((
                        client.id,
                        format!("expected command envelope, received {kind:?}"),
                    ));
                    continue;
                }
                match WireCommand::decode_payload(&payload) {
                    Ok(command) => {
                        client_commands.push((client.id, command));
                    }
                    Err(error) => errors.push((client.id, format!("invalid command: {error}"))),
                }
            }
            command_queues.push(client_commands);
        }

        let commands = interleave_wire_commands(command_queues);

        for (client_id, error) in errors {
            self.queue_wire_error(client_id, error);
        }
        for (client_id, supported_min, supported_max) in compatibility_rejections {
            if let Some(client) = self
                .wire_clients
                .iter_mut()
                .find(|client| client.id == client_id)
            {
                client.queue_compatibility_rejection(supported_min, supported_max);
            }
        }
        for (client_id, command) in commands {
            self.handle_wire_command(client_id, command);
        }
    }

    pub fn handle_wire_command(&mut self, client_id: u64, command: WireCommand) {
        let (request_id, command) = match command {
            WireCommand::Request {
                request_id,
                command,
            } => (Some(request_id), *command),
            command => (None, command),
        };
        if let Some(request_id) = request_id {
            self.active_request_ids.insert(client_id, request_id);
        }
        self.handle_wire_command_inner(client_id, command);
        self.active_request_ids.remove(&client_id);
    }

    pub fn handle_wire_command_inner(&mut self, client_id: u64, command: WireCommand) {
        let Some(client_index) = self
            .wire_clients
            .iter()
            .position(|client| client.id == client_id)
        else {
            return;
        };
        if let WireCommand::Authenticate { token } = command {
            if self.wire_clients[client_index].authenticated.is_some() {
                self.queue_wire_error(client_id, "already authenticated".to_owned());
                return;
            }
            let account_id = match self
                .account_repository
                .authenticate_development_token(&token)
            {
                Ok(account_id) => account_id,
                Err(error) => {
                    self.queue_wire_error(client_id, error.message().to_owned());
                    return;
                }
            };
            let session_id = self.next_session_id;
            self.next_session_id = self.next_session_id.saturating_add(1);
            self.wire_clients[client_index].authenticated = Some(AuthenticatedSession {
                account_id,
                session_id,
            });
            self.queue_wire_message(
                client_id,
                ServerMessage::Authenticated {
                    account_id,
                    session_id,
                },
            );
            return;
        }
        if self.wire_clients[client_index].authenticated.is_none() {
            self.queue_wire_error(
                client_id,
                "authenticate before sending wire commands".to_owned(),
            );
            return;
        }
        let (requested_operation_id, command) = match command {
            WireCommand::Retryable {
                operation_id,
                command,
            } => (Some(operation_id), *command),
            command => (None, command),
        };
        let bound_player = self.wire_clients[client_index].player_id;
        if matches!(command, WireCommand::Join { .. }) {
            self.queue_wire_error(
                client_id,
                "wire join is disabled; use enter-world after authentication".to_owned(),
            );
            return;
        }
        if matches!(command, WireCommand::ListCharacters) {
            if bound_player.is_some() {
                self.queue_wire_error(client_id, "already connected".to_owned());
                return;
            }
            let account_id = self.wire_clients[client_index]
                .authenticated
                .expect("authenticated state checked above")
                .account_id;
            let characters = self
                .account_repository
                .list_characters(account_id)
                .into_iter()
                .map(|character| character.summary())
                .collect();
            self.queue_wire_message(
                client_id,
                ServerMessage::CharacterList {
                    account_id,
                    characters,
                },
            );
            return;
        }
        if let WireCommand::SelectCharacter { character_id } = command {
            if bound_player.is_some() {
                self.queue_wire_error(client_id, "already connected".to_owned());
                return;
            }
            let account_id = self.wire_clients[client_index]
                .authenticated
                .expect("authenticated state checked above")
                .account_id;
            let Some(character) = self
                .account_repository
                .find_character(account_id, character_id)
            else {
                self.queue_wire_error(client_id, "unknown character".to_owned());
                return;
            };
            if self.character_reserved_by_other(client_id, account_id, character_id) {
                self.queue_wire_error(client_id, "character is already active".to_owned());
                return;
            }
            self.wire_clients[client_index].selected_character_id = Some(character_id);
            let summary = character.summary();
            self.queue_wire_message(
                client_id,
                ServerMessage::CharacterSelected {
                    character_id: summary.character_id,
                    name: summary.name,
                    role: summary.role,
                },
            );
            return;
        }
        if let WireCommand::ContentDigest { digest } = command {
            if self.wire_clients[client_index]
                .selected_character_id
                .is_none()
            {
                self.queue_wire_error(
                    client_id,
                    "select a character before checking content".to_owned(),
                );
                return;
            }
            let expected = starter_catalog().content_digest();
            if digest == expected {
                self.wire_clients[client_index].content_compatible = true;
                self.queue_wire_message(client_id, ServerMessage::ContentAccepted { digest });
            } else {
                self.queue_wire_message(
                    client_id,
                    ServerMessage::ContentMismatch {
                        expected,
                        received: digest,
                    },
                );
            }
            return;
        }
        if matches!(command, WireCommand::EnterWorld) {
            let already_pending = self.commands.iter().any(|pending| {
                pending.origin == ClientOrigin::Wire(client_id)
                    && matches!(pending.command, Command::JoinPlayer { .. })
            });
            if bound_player.is_some() || already_pending {
                self.queue_wire_error(client_id, "already connected".to_owned());
                return;
            }
            let Some(character_id) = self.wire_clients[client_index].selected_character_id else {
                self.queue_wire_error(
                    client_id,
                    "select a character before entering world".to_owned(),
                );
                return;
            };
            if !self.wire_clients[client_index].content_compatible {
                self.queue_wire_error(
                    client_id,
                    "content compatibility check required before entering world".to_owned(),
                );
                return;
            }
            let account_id = self.wire_clients[client_index]
                .authenticated
                .expect("authenticated state checked above")
                .account_id;
            if self.character_reserved_by_other(client_id, account_id, character_id) {
                self.queue_wire_error(client_id, "character is already active".to_owned());
                return;
            }
            let Some(character) = self
                .account_repository
                .find_character(account_id, character_id)
            else {
                self.queue_wire_error(
                    client_id,
                    "selected character is no longer available".to_owned(),
                );
                return;
            };
            if self.commands.len() >= MAX_PENDING_COMMANDS {
                self.queue_wire_error(client_id, "typed command queue is full".to_owned());
                return;
            }
            if !self
                .character_fences
                .contains_key(&(account_id, character_id))
                && let Some(store) = &self.character_fence_store
            {
                match store.acquire(account_id, character_id) {
                    Ok(fence) => {
                        self.character_fences
                            .insert((account_id, character_id), fence);
                    }
                    Err(error) => {
                        self.queue_wire_error(client_id, error);
                        return;
                    }
                }
            }
            if let Some(detached) = self.detached_characters.remove(&(account_id, character_id)) {
                if self.world.player(detached.player_id).is_some()
                    && detached.expires_at_tick > self.world.tick()
                {
                    self.wire_clients[client_index].player_id = Some(detached.player_id);
                    let request_id = self.active_request_ids.get(&client_id).copied();
                    self.queue_wire_message_with_request(
                        client_id,
                        request_id,
                        ServerMessage::Connected {
                            player_id: detached.player_id.0,
                            role: wire_role(character.role),
                        },
                    );
                    self.queue_wire_message_with_request(
                        client_id,
                        request_id,
                        ServerMessage::Snapshot(wire_snapshot_for_player(
                            &self.world,
                            detached.player_id,
                        )),
                    );
                    return;
                }
            }
            let (command, checkpoint_revision) = match self
                .account_repository
                .load_checkpoint_with_revision(account_id, character_id)
            {
                Ok(Some((state, revision))) => (Command::RestorePlayer { state }, revision),
                Ok(None) => (
                    Command::JoinPlayer {
                        name: character.name,
                        role: character.role,
                    },
                    0,
                ),
                Err(error) => {
                    self.queue_wire_error(
                        client_id,
                        format!("cannot load character checkpoint: {error}"),
                    );
                    return;
                }
            };
            if let Some(request_id) = self.active_request_ids.get(&client_id).copied() {
                self.wire_clients[client_index].pending_request_id = Some(request_id);
            }
            self.enqueue_pending_wire_command(client_id, command);
            let recoveries: Vec<_> = self
                .recovery_operations
                .iter()
                .filter_map(|(key, (revision, command))| {
                    (key.account_id == account_id
                        && key.character_id == character_id
                        && *revision > checkpoint_revision)
                        .then_some((*key, command.clone()))
                })
                .collect();
            if !recoveries.is_empty() {
                self.pending_recovery_operations
                    .insert((account_id, character_id), recoveries);
            }
            return;
        }
        if matches!(command, WireCommand::Snapshot) {
            if bound_player.is_none() {
                self.queue_wire_error(
                    client_id,
                    "enter-world before requesting snapshot".to_owned(),
                );
                return;
            }
            self.send_wire_machine_snapshot(client_id);
            return;
        }
        let Some(player_id) = bound_player else {
            self.queue_wire_error(client_id, "connect first".to_owned());
            return;
        };
        let operation = if let Some(operation_id) = requested_operation_id {
            if !is_retryable_core_command(&command) {
                let client = &self.wire_clients[client_index];
                let session = client
                    .authenticated
                    .expect("authenticated state checked above");
                let character_id = client
                    .selected_character_id
                    .expect("bound gameplay client has selected character");
                self.record_failed_operation(
                    client_id,
                    OperationKey {
                        account_id: session.account_id,
                        character_id,
                        operation_id,
                    },
                    "operation wrapper is only valid for durable commands".to_owned(),
                );
                return;
            }
            let client = &self.wire_clients[client_index];
            let session = client
                .authenticated
                .expect("authenticated state checked above");
            let character_id = client
                .selected_character_id
                .expect("bound gameplay client has selected character");
            let key = OperationKey {
                account_id: session.account_id,
                character_id,
                operation_id,
            };
            if self.recovering_operations.contains(&key) {
                return;
            }
            if let Some(events) = self.completed_operations.get(&key).cloned() {
                if let Some(client) = self
                    .wire_clients
                    .iter_mut()
                    .find(|client| client.id == client_id)
                {
                    for message in events {
                        client.queue_server_message(&message);
                    }
                }
                return;
            }
            if let Some(reason) = self.failed_operations.get(&key).cloned() {
                self.queue_wire_error(client_id, reason);
                return;
            }
            if self.pending_failed_operations.contains_key(&key) {
                return;
            }
            Some(key)
        } else {
            None
        };
        let command = match wire_command_to_core(command, player_id) {
            Ok(command) => command,
            Err(error) => {
                if let Some(key) = operation {
                    self.record_failed_operation(client_id, key, error);
                } else {
                    self.queue_wire_error(client_id, error);
                }
                return;
            }
        };
        if let Some(key) = operation
            && self.operation_journal_worker.enabled()
        {
            if self.prepared_operations.contains_key(&key) {
                return;
            }
            let command_payload = match command_to_wire_payload(&command) {
                Ok(payload) => payload,
                Err(error) => {
                    self.queue_wire_error(client_id, error);
                    return;
                }
            };
            self.prepared_operations.insert(key, (client_id, command));
            if self
                .operation_journal_worker
                .try_enqueue(OperationJournalJob::Prepare {
                    key,
                    command_payload,
                })
                .is_err()
            {
                self.prepared_operations.remove(&key);
                self.queue_wire_error(client_id, "operation journal queue is full".to_owned());
            }
            return;
        }
        self.enqueue_pending_wire_command_with_operation(client_id, command, operation);
    }

    pub fn enqueue_pending_wire_command(&mut self, client_id: u64, command: Command) {
        self.enqueue_pending_wire_command_with_operation(client_id, command, None);
    }

    pub fn enqueue_pending_wire_command_with_operation(
        &mut self,
        client_id: u64,
        command: Command,
        operation: Option<OperationKey>,
    ) {
        if self.commands.len() >= MAX_PENDING_COMMANDS {
            self.queue_wire_error(client_id, "typed command queue is full".to_owned());
            return;
        }
        self.commands.push_back(PendingCommand {
            origin: ClientOrigin::Wire(client_id),
            command,
            operation,
        });
    }

    pub fn record_failed_operation(&mut self, client_id: u64, key: OperationKey, reason: String) {
        if self.pending_failed_operations.contains_key(&key) {
            return;
        }
        if !self.operation_journal_worker.enabled() {
            self.queue_wire_error(client_id, reason);
            return;
        }
        self.pending_failed_operations
            .insert(key, (client_id, reason.clone()));
        if self
            .operation_journal_worker
            .try_enqueue(OperationJournalJob::Failed { key, reason })
            .is_err()
        {
            let (_, reason) = self
                .pending_failed_operations
                .remove(&key)
                .expect("pending failed operation was inserted above");
            self.queue_wire_error(
                client_id,
                format!("operation journal queue is full: {reason}"),
            );
        }
    }

    pub fn character_reserved_by_other(
        &self,
        client_id: u64,
        account_id: u64,
        character_id: u64,
    ) -> bool {
        is_character_reserved(&self.wire_clients, client_id, account_id, character_id)
    }

    pub fn send_wire_machine_snapshot(&mut self, client_id: u64) {
        let Some(player_id) = self
            .wire_clients
            .iter()
            .find(|client| client.id == client_id)
            .and_then(|client| client.player_id)
        else {
            self.queue_wire_error(client_id, "connect first".to_owned());
            return;
        };
        let snapshot = wire_snapshot_for_player(&self.world, player_id);
        self.queue_wire_message(client_id, ServerMessage::Snapshot(snapshot));
    }

    pub fn queue_wire_error(&mut self, client_id: u64, error: String) {
        self.queue_wire_message(client_id, ServerMessage::Error { message: error });
    }

    pub fn queue_wire_message(&mut self, client_id: u64, message: ServerMessage) {
        let request_id = self.active_request_ids.get(&client_id).copied();
        self.queue_wire_message_with_request(client_id, request_id, message);
    }

    pub fn queue_wire_message_with_request(
        &mut self,
        client_id: u64,
        request_id: Option<u64>,
        message: ServerMessage,
    ) {
        let message = request_id.map_or(message.clone(), |request_id| ServerMessage::Response {
            request_id,
            message: Box::new(message),
        });
        if let Some(client) = self
            .wire_clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            client.queue_server_message(&message);
        }
    }

    pub fn advance_if_due(&mut self) {
        if Instant::now() < self.next_tick {
            return;
        }

        if self.staged_operation_batch.is_some() {
            self.poll_operation_journal();
            self.schedule_next_tick();
            return;
        }
        self.poll_operation_journal();
        self.expire_detached_characters();
        let pending = self.take_tick_commands();
        let operation_commands: Vec<_> = pending
            .iter()
            .filter_map(|pending| {
                pending
                    .operation
                    .zip(command_player_id(&pending.command))
                    .map(|(key, player_id)| (key, player_id, pending.origin))
            })
            .collect();
        let mut join_origins: VecDeque<ClientOrigin> = pending
            .iter()
            .filter_map(|pending| {
                matches!(
                    pending.command,
                    Command::JoinPlayer { .. } | Command::RestorePlayer { .. }
                )
                .then_some(pending.origin)
            })
            .collect();

        if self.operation_journal_worker.enabled() && !operation_commands.is_empty() {
            self.stage_operation_batch(pending, join_origins, operation_commands);
            self.schedule_next_tick();
            return;
        }

        let simulation_started = Instant::now();
        let events = self.world.step_with_combat_timing(
            pending.into_iter().map(|pending| pending.command),
            self.combat_timing,
        );

        for (key, player_id, _) in operation_commands {
            let result = events
                .iter()
                .filter(|event| event_recipient(event) == Some(player_id))
                .cloned()
                .collect::<Vec<_>>();
            let messages = result
                .iter()
                .filter_map(|event| wire_event(event).map(ServerMessage::Event))
                .collect();
            self.insert_completed_operation(key, messages);
        }

        self.apply_join_origins(&events, &mut join_origins);

        for event in events {
            self.broadcast_wire_event(&event);
        }

        if self.world.tick().is_multiple_of(CHECKPOINT_INTERVAL_TICKS) {
            self.checkpoint_wire_players();
        }

        let duration = simulation_started.elapsed();
        self.timing_stats.record(duration, self.tick_interval);
        if duration > self.tick_interval {
            eprintln!(
                "simulation_tick_deadline_missed tick={} duration_ms={:.3} budget_ms={:.3}",
                self.world.tick(),
                duration.as_secs_f64() * 1000.0,
                self.tick_interval.as_secs_f64() * 1000.0
            );
        }

        self.schedule_next_tick();
    }

    pub fn take_tick_commands(&mut self) -> Vec<PendingCommand> {
        let mut pending = Vec::with_capacity(self.commands.len().min(MAX_COMMANDS_PER_TICK));
        while let Some(command) = self.commands.pop_front() {
            if pending.len() < MAX_COMMANDS_PER_TICK {
                pending.push(command);
                continue;
            }

            let client_id = origin_client_id(command.origin);
            if let Some(key) = command.operation {
                self.record_failed_operation(
                    client_id,
                    key,
                    "simulation tick command budget exceeded".to_owned(),
                );
            } else {
                self.queue_wire_error(
                    client_id,
                    "simulation tick command budget exceeded".to_owned(),
                );
            }
        }
        pending
    }

    pub fn stage_operation_batch(
        &mut self,
        pending: Vec<PendingCommand>,
        join_origins: VecDeque<ClientOrigin>,
        operation_commands: Vec<(OperationKey, EntityId, ClientOrigin)>,
    ) {
        let mut staged_world = self.world.clone();
        let operation_payloads: BTreeMap<OperationKey, Vec<u8>> = pending
            .iter()
            .filter_map(|pending| {
                pending.operation.and_then(|key| {
                    command_to_wire_payload(&pending.command)
                        .ok()
                        .map(|payload| (key, payload))
                })
            })
            .collect();
        let events = staged_world.step_with_combat_timing(
            pending.into_iter().map(|pending| pending.command),
            self.combat_timing,
        );
        let mut operations = BTreeMap::new();
        let mut operation_events = BTreeMap::new();
        let mut journal_jobs = Vec::new();
        for (key, player_id, origin) in operation_commands {
            let result = events
                .iter()
                .filter(|event| event_recipient(event) == Some(player_id))
                .cloned()
                .collect::<Vec<_>>();
            let payloads = result
                .iter()
                .filter_map(|event| wire_event(event).map(ServerMessage::Event))
                .filter_map(|message| message.encode_payload().ok())
                .collect();
            journal_jobs.push(OperationJournalJob::Complete {
                key,
                revision: staged_world.tick(),
                command_payload: operation_payloads.get(&key).cloned().unwrap_or_default(),
                result_payloads: payloads,
            });
            operations.insert(key, origin_client_id(origin));
            operation_events.insert(key, result);
        }
        if self
            .operation_journal_worker
            .try_enqueue_batch(journal_jobs)
            .is_err()
        {
            for client_id in operations.values().copied() {
                self.queue_wire_error(client_id, "operation journal queue is full".to_owned());
            }
            return;
        }
        self.staged_operation_batch = Some(StagedOperationBatch {
            world: staged_world,
            events,
            join_origins,
            operations,
            operation_events,
            acknowledged: BTreeSet::new(),
        });
    }

    pub fn apply_join_origins(
        &mut self,
        events: &[Event],
        join_origins: &mut VecDeque<ClientOrigin>,
    ) {
        for event in events {
            if let Event::PlayerJoined { player } = event
                && let Some(origin) = join_origins.pop_front()
            {
                if let ClientOrigin::Wire(origin) = origin
                    && let Some(client) = self
                        .wire_clients
                        .iter_mut()
                        .find(|client| client.id == origin)
                {
                    let identity = client
                        .authenticated
                        .zip(client.selected_character_id)
                        .map(|(session, character_id)| (session.account_id, character_id));
                    let request_id = client.pending_request_id.take();
                    client.player_id = Some(player.id);
                    let _ = client;
                    self.queue_wire_message_with_request(
                        origin,
                        request_id,
                        ServerMessage::Connected {
                            player_id: player.id.0,
                            role: wire_role(player.role),
                        },
                    );
                    if let Some(identity) = identity
                        && let Some(recoveries) = self.pending_recovery_operations.remove(&identity)
                    {
                        for (key, command) in recoveries {
                            let Some(command) = wire_command_to_core(command, player.id).ok()
                            else {
                                self.failed_operations.insert(
                                    key,
                                    "recovered operation command was invalid".to_owned(),
                                );
                                self.trim_operation_results();
                                continue;
                            };
                            self.recovering_operations.insert(key);
                            self.commands.push_back(PendingCommand {
                                origin: ClientOrigin::Wire(origin),
                                command,
                                operation: Some(key),
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn schedule_next_tick(&mut self) {
        self.next_tick += self.tick_interval;
        if self.next_tick <= Instant::now() {
            self.next_tick = Instant::now() + self.tick_interval;
        }
    }

    pub fn poll_operation_journal(&mut self) {
        let results: Vec<_> = self.operation_journal_worker.drain_results().collect();
        let mut failed_batch = None;
        for result in results {
            if let Err(error) = result.result {
                self.prepared_operations.remove(&result.key);
                if !result.prepared && !result.failed {
                    failed_batch = Some((result.key, error));
                    break;
                }
                if result.failed
                    && let Some((client_id, _)) = self.pending_failed_operations.remove(&result.key)
                {
                    self.queue_wire_error(
                        client_id,
                        format!("could not persist operation failure: {error}"),
                    );
                }
                eprintln!(
                    "operation_journal_error account={} character={} operation={} error={error}",
                    result.key.account_id, result.key.character_id, result.key.operation_id
                );
                continue;
            }
            if result.prepared
                && let Some((client_id, command)) = self.prepared_operations.remove(&result.key)
            {
                self.enqueue_pending_wire_command_with_operation(
                    client_id,
                    command,
                    Some(result.key),
                );
            } else if result.failed {
                if let Some((client_id, reason)) =
                    self.pending_failed_operations.remove(&result.key)
                {
                    self.failed_operations.insert(result.key, reason.clone());
                    self.trim_operation_results();
                    self.queue_wire_error(client_id, reason);
                }
            } else if !result.prepared
                && let Some(batch) = self.staged_operation_batch.as_mut()
                && batch.operations.contains_key(&result.key)
            {
                batch.acknowledged.insert(result.key);
            }
        }

        if let Some((failed_key, error)) = failed_batch {
            if let Some(batch) = self.staged_operation_batch.take() {
                let failure_reason = error;
                for client_id in batch.operations.values().copied() {
                    self.queue_wire_error(
                        client_id,
                        format!("operation journal failed: {failure_reason}"),
                    );
                }
                eprintln!(
                    "operation_journal_error account={} character={} operation={} error={failure_reason}",
                    failed_key.account_id, failed_key.character_id, failed_key.operation_id
                );
                let _ = self
                    .operation_journal_worker
                    .try_enqueue(OperationJournalJob::Failed {
                        key: failed_key,
                        reason: failure_reason,
                    });
            }
        } else if self
            .staged_operation_batch
            .as_ref()
            .is_some_and(|batch| batch.acknowledged.len() == batch.operations.len())
        {
            self.commit_staged_operation_batch();
        }
    }

    pub fn commit_staged_operation_batch(&mut self) {
        let Some(mut batch) = self.staged_operation_batch.take() else {
            return;
        };
        self.world = batch.world;
        self.apply_join_origins(&batch.events, &mut batch.join_origins);
        for (key, events) in batch.operation_events {
            self.recovery_operations.remove(&key);
            self.recovering_operations.remove(&key);
            let messages = events
                .iter()
                .filter_map(|event| wire_event(event).map(ServerMessage::Event))
                .collect();
            self.insert_completed_operation(key, messages);
        }
        for event in batch.events {
            self.broadcast_wire_event(&event);
        }
        if self.world.tick().is_multiple_of(CHECKPOINT_INTERVAL_TICKS) {
            self.checkpoint_wire_players();
        }
    }

    pub fn insert_completed_operation(&mut self, key: OperationKey, result: Vec<ServerMessage>) {
        if self.completed_operations.len() >= MAX_COMPLETED_OPERATIONS
            && !self.completed_operations.contains_key(&key)
        {
            if let Some(oldest) = self.completed_operations.keys().next().copied() {
                self.completed_operations.remove(&oldest);
            }
        }
        self.completed_operations.insert(key, result);
        self.trim_operation_results();
    }

    pub fn trim_operation_results(&mut self) {
        trim_operation_maps(&mut self.completed_operations, &mut self.failed_operations);
    }

    pub fn checkpoint_wire_players(&self) {
        for result in self.checkpoint_worker.drain_results() {
            if let Err(error) = result.result {
                eprintln!(
                    "checkpoint_save_error character={} revision={} error={error}",
                    result.character_id, result.revision
                );
            }
        }
        for client in &self.wire_clients {
            let (Some(session), Some(character_id), Some(player_id)) = (
                client.authenticated,
                client.selected_character_id,
                client.player_id,
            ) else {
                continue;
            };
            let Some(player) = self.world.player(player_id) else {
                continue;
            };
            let revision = self.world.tick();
            let job = CheckpointJob {
                account_id: session.account_id,
                character_id,
                revision,
                operation_id: (revision << 32) | player_id.0,
                state: player.durable_state(),
            };
            if self.checkpoint_worker.try_enqueue(job).is_err() {
                eprintln!("checkpoint_queue_full player={player_id} revision={revision}");
            }
        }
    }

    /// Stops accepting gameplay work, drains the bounded operation/command
    /// pipeline, checkpoints live characters, and applies final leave events.
    pub fn shutdown(&mut self) {
        let deadline = Instant::now() + SHUTDOWN_DRAIN_TIMEOUT;
        while (!self.commands.is_empty()
            || !self.prepared_operations.is_empty()
            || self.staged_operation_batch.is_some()
            || !self.pending_failed_operations.is_empty())
            && Instant::now() < deadline
        {
            self.next_tick = Instant::now() - Duration::from_millis(1);
            self.advance_if_due();
            thread::sleep(Duration::from_millis(1));
        }
        if !self.commands.is_empty()
            || !self.prepared_operations.is_empty()
            || self.staged_operation_batch.is_some()
            || !self.pending_failed_operations.is_empty()
        {
            eprintln!(
                "shutdown_drain_timeout commands={} prepared={} staged={} pending_failed={}",
                self.commands.len(),
                self.prepared_operations.len(),
                self.staged_operation_batch.is_some(),
                self.pending_failed_operations.len()
            );
            self.commands.clear();
            self.prepared_operations.clear();
            self.staged_operation_batch = None;
            for (_, (client_id, reason)) in std::mem::take(&mut self.pending_failed_operations) {
                self.queue_wire_error(
                    client_id,
                    format!("operation failure abandoned during shutdown: {reason}"),
                );
            }
        }

        self.checkpoint_wire_players();
        let leaving: Vec<_> = self
            .wire_clients
            .iter()
            .filter_map(|client| client.player_id)
            .collect();
        for player_id in leaving {
            self.commands.push_back(PendingCommand {
                origin: ClientOrigin::Wire(0),
                command: Command::LeavePlayer { player_id },
                operation: None,
            });
        }
        if !self.commands.is_empty() {
            self.next_tick = Instant::now() - Duration::from_millis(1);
            self.advance_if_due();
        }
    }

    pub fn queue_disconnects(&mut self) {
        if self.wire_clients.iter().any(|client| client.closed) {
            self.checkpoint_wire_players();
        }
        for client in &mut self.wire_clients {
            if !client.closed {
                continue;
            }
            let (Some(session), Some(character_id), Some(player_id)) = (
                client.authenticated,
                client.selected_character_id,
                client.player_id,
            ) else {
                continue;
            };
            self.detached_characters.insert(
                (session.account_id, character_id),
                DetachedCharacter {
                    player_id,
                    expires_at_tick: self.world.tick().saturating_add(DISCONNECT_GRACE_TICKS),
                },
            );
            client.player_id = None;
        }
    }

    pub fn expire_detached_characters(&mut self) {
        let current_tick = self.world.tick();
        let expired: Vec<_> = self
            .detached_characters
            .iter()
            .filter_map(|(identity, detached)| {
                (detached.expires_at_tick <= current_tick).then_some((*identity, *detached))
            })
            .collect();
        for (identity, detached) in expired {
            self.detached_characters.remove(&identity);
            self.character_fences.remove(&identity);
            if self.world.player(detached.player_id).is_some() {
                self.commands.push_back(PendingCommand {
                    origin: ClientOrigin::Wire(0),
                    command: Command::LeavePlayer {
                        player_id: detached.player_id,
                    },
                    operation: None,
                });
            }
        }
    }

    pub fn flush_clients(&mut self) {
        for client in &mut self.wire_clients {
            client.materialize_replaceable_events();
            while !client.output.is_empty() {
                let chunk: Vec<u8> = client.output.iter().copied().take(8192).collect();
                match client.stream.write(&chunk) {
                    Ok(0) => {
                        client.closed = true;
                        break;
                    }
                    Ok(bytes_written) => {
                        client.output.drain(..bytes_written);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("wire_client_write_error id={} error={error}", client.id);
                        client.closed = true;
                        break;
                    }
                }
            }
        }
    }

    pub fn remove_closed(&mut self) {
        self.wire_clients
            .retain(|client| !client.closed || !client.output.is_empty());
    }

    pub fn step_frame(&mut self) {
        self.read_wire_clients();
        self.advance_if_due();
        self.queue_disconnects();
        self.flush_clients();
        self.remove_closed();
    }

    pub fn broadcast_wire_event(&mut self, event: &Event) {
        let recipient = event_recipient(event);
        let Some(wire_event) = wire_event(event) else {
            return;
        };
        for client in &mut self.wire_clients {
            let visible = recipient.is_none_or(|player_id| client.player_id == Some(player_id))
                && event_visible_to_player(event, client.player_id, &self.world);
            if !client.closed && visible {
                if let Event::PlayerMoved { player_id, .. } = event {
                    client.queue_replaceable_server_message(
                        player_id.0,
                        ServerMessage::Event(wire_event.clone()),
                    );
                } else {
                    client.queue_server_message(&ServerMessage::Event(wire_event.clone()));
                }
            }
        }
    }
}
