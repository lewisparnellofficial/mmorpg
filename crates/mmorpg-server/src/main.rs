mod account_repository;

use crate::account_repository::{
    AccountCharacterRepository, CheckpointJob, CheckpointWorker, DevelopmentAccountRepository,
    OperationJournalJob, OperationJournalWorker, OperationKey,
};
use mmorpg_content::starter_catalog;
use mmorpg_core::{CombatTiming, Command, EntityId, Event, ItemId, PartyId, QuestId, Role, World};
use mmorpg_wire::{
    ClientCommand as WireCommand, DecodeError as WireDecodeError, Envelope, ItemStackState,
    MessageKind, NpcKindCode, NpcState, PlayerState, QuestOfferState, QuestState, QuestStatusCode,
    SequencedServerMessage, ServerEvent, ServerMessage, VendorListingState, WorldSnapshot,
    ZoneAreaCode, decode_one,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_ADDRESS: &str = "127.0.0.1:4000";
const DEFAULT_TICK_HZ: u64 = 20;
const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
const MAX_WIRE_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_REPLACEABLE_EVENTS: usize = 256;
const INTEREST_RANGE: f32 = 45.0;
const MAX_WIRE_COMMANDS_PER_CLIENT_POLL: usize = 32;
const MAX_WIRE_COMMANDS_PER_POLL: usize = 256;
const MAX_PENDING_COMMANDS: usize = 1024;
const MAX_COMPLETED_OPERATIONS: usize = 256;
const CHECKPOINT_INTERVAL_TICKS: u64 = 20;
const DISCONNECT_GRACE_TICKS: u64 = 100;
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
#[derive(Debug)]
struct Client {
    id: u64,
    stream: TcpStream,
    input: String,
    output: VecDeque<u8>,
    player_id: Option<EntityId>,
    closed: bool,
}

#[cfg(test)]
impl Client {
    fn new(id: u64, stream: TcpStream) -> Self {
        Self {
            id,
            stream,
            input: String::new(),
            output: VecDeque::new(),
            player_id: None,
            closed: false,
        }
    }

    fn queue_line(&mut self, line: impl AsRef<str>) {
        self.output.extend(line.as_ref().as_bytes());
        self.output.push_back(b'\n');
    }
}

#[derive(Debug)]
struct WireClient {
    id: u64,
    stream: TcpStream,
    input: Vec<u8>,
    output: VecDeque<u8>,
    replaceable_events: BTreeMap<u64, ServerMessage>,
    authenticated: Option<AuthenticatedSession>,
    selected_character_id: Option<u64>,
    content_compatible: bool,
    next_sequence: u64,
    player_id: Option<EntityId>,
    closed: bool,
}

impl WireClient {
    fn new(id: u64, stream: TcpStream) -> Self {
        Self {
            id,
            stream,
            input: Vec::new(),
            output: VecDeque::new(),
            replaceable_events: BTreeMap::new(),
            authenticated: None,
            selected_character_id: None,
            content_compatible: false,
            next_sequence: 1,
            player_id: None,
            closed: false,
        }
    }

    fn queue_event_payload(&mut self, payload: &[u8]) {
        let Ok(envelope) = Envelope::new(MessageKind::Event, payload.to_vec()) else {
            self.closed = true;
            return;
        };
        let Ok(frame) = envelope.encode() else {
            self.closed = true;
            return;
        };
        if self.output.len().saturating_add(frame.len()) > MAX_WIRE_OUTPUT_BYTES {
            eprintln!(
                "wire_client_output_saturated id={} queued_bytes={} frame_bytes={} limit={}",
                self.id,
                self.output.len(),
                frame.len(),
                MAX_WIRE_OUTPUT_BYTES
            );
            self.closed = true;
            return;
        }
        self.output.extend(frame);
    }

    fn queue_server_message(&mut self, message: &ServerMessage) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1).max(1);
        let payload = SequencedServerMessage::new(sequence, message.clone())
            .and_then(|message| message.encode_payload());
        match payload {
            Ok(payload) => self.queue_event_payload(&payload),
            Err(error) => {
                eprintln!("wire_message_encode_error id={} error={error}", self.id);
                self.closed = true;
            }
        }
    }

    fn queue_replaceable_server_message(&mut self, key: u64, message: ServerMessage) {
        if self.replaceable_events.len() >= MAX_REPLACEABLE_EVENTS
            && !self.replaceable_events.contains_key(&key)
        {
            self.closed = true;
            return;
        }
        self.replaceable_events.insert(key, message);
    }

    fn materialize_replaceable_events(&mut self) {
        let pending = std::mem::take(&mut self.replaceable_events);
        for message in pending.values() {
            self.queue_server_message(message);
        }
    }

    fn queue_compatibility_rejection(&mut self, supported_min: u16, supported_max: u16) {
        let frame = mmorpg_wire::encode_compatibility_control(
            &mmorpg_wire::CompatibilityControl::VersionRejected {
                supported_min,
                supported_max,
            },
        );
        if self.output.len().saturating_add(frame.len()) > MAX_WIRE_OUTPUT_BYTES {
            self.closed = true;
            return;
        }
        self.output.extend(frame);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AuthenticatedSession {
    account_id: u64,
    session_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DetachedCharacter {
    player_id: EntityId,
    expires_at_tick: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClientOrigin {
    #[cfg(test)]
    Line(u64),
    Wire(u64),
}

struct PendingCommand {
    origin: ClientOrigin,
    command: Command,
    operation: Option<OperationKey>,
}

struct StagedOperationBatch {
    world: World,
    events: Vec<Event>,
    join_origins: VecDeque<ClientOrigin>,
    operations: BTreeMap<OperationKey, u64>,
    operation_events: BTreeMap<OperationKey, Vec<Event>>,
    acknowledged: BTreeSet<OperationKey>,
}

struct Server {
    world: World,
    #[cfg(test)]
    clients: Vec<Client>,
    wire_clients: Vec<WireClient>,
    detached_characters: BTreeMap<(u64, u64), DetachedCharacter>,
    commands: VecDeque<PendingCommand>,
    prepared_operations: BTreeMap<OperationKey, (u64, Command)>,
    staged_operation_batch: Option<StagedOperationBatch>,
    completed_operations: BTreeMap<OperationKey, Vec<ServerMessage>>,
    operation_journal_worker: OperationJournalWorker,
    account_repository: Arc<dyn AccountCharacterRepository>,
    checkpoint_worker: CheckpointWorker,
    next_client_id: u64,
    next_session_id: u64,
    combat_timing: CombatTiming,
    tick_interval: Duration,
    next_tick: Instant,
}

impl Server {
    fn new(tick_hz: u64, dev_auth_enabled: bool, checkpoint_path: Option<PathBuf>) -> Self {
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
        Self::with_account_repository_and_journal(tick_hz, repository, operation_journal_path)
    }

    fn with_account_repository_and_journal(
        tick_hz: u64,
        account_repository: Box<dyn AccountCharacterRepository>,
        operation_journal_path: Option<PathBuf>,
    ) -> Self {
        let tick_hz = tick_hz.max(1);
        let tick_interval = Duration::from_secs_f64(1.0 / tick_hz as f64);
        let combat_timing = CombatTiming::new(tick_hz.min(u32::MAX as u64) as u32, 0, 0)
            .expect("tick_hz is clamped above zero");
        let account_repository: Arc<dyn AccountCharacterRepository> = Arc::from(account_repository);
        let checkpoint_worker = CheckpointWorker::new(Arc::clone(&account_repository));
        let (operation_journal_worker, completed_operations) = operation_journal_path
            .map(|path| {
                OperationJournalWorker::new(path).expect("operation journal should load and start")
            })
            .unwrap_or_else(|| (OperationJournalWorker::disabled(), BTreeMap::new()));
        Self {
            world: World::new_starter_zone(),
            #[cfg(test)]
            clients: Vec::new(),
            wire_clients: Vec::new(),
            detached_characters: BTreeMap::new(),
            commands: VecDeque::new(),
            prepared_operations: BTreeMap::new(),
            staged_operation_batch: None,
            completed_operations: completed_operations
                .into_iter()
                .map(|(key, payloads)| {
                    let messages = payloads
                        .into_iter()
                        .filter_map(|payload| ServerMessage::decode_payload(&payload).ok())
                        .collect();
                    (key, messages)
                })
                .collect(),
            operation_journal_worker,
            checkpoint_worker,
            account_repository,
            next_client_id: 1,
            next_session_id: 1,
            combat_timing,
            tick_interval,
            next_tick: Instant::now() + tick_interval,
        }
    }

    fn add_wire_client(&mut self, stream: TcpStream) {
        let id = self.next_client_id;
        self.next_client_id = self.next_client_id.saturating_add(1);
        let mut client = WireClient::new(id, stream);
        client.queue_server_message(&ServerMessage::Welcome {
            server: "mmorpg-server".to_owned(),
        });
        self.wire_clients.push(client);
        println!("wire_client_connected id={id}");
    }

    fn read_wire_clients(&mut self) {
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

        // A client can fill its own bounded intake window, but it cannot
        // monopolize the simulation owner's command order. Interleave each
        // client's already-decoded commands before applying session checks.
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

    fn handle_wire_command(&mut self, client_id: u64, command: WireCommand) {
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
            self.wire_clients[client_index].queue_server_message(&ServerMessage::Authenticated {
                account_id,
                session_id,
            });
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
            self.wire_clients[client_index].queue_server_message(&ServerMessage::CharacterList {
                account_id,
                characters,
            });
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
            self.wire_clients[client_index].queue_server_message(
                &ServerMessage::CharacterSelected {
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
                self.wire_clients[client_index]
                    .queue_server_message(&ServerMessage::ContentAccepted { digest });
            } else {
                self.wire_clients[client_index].queue_server_message(
                    &ServerMessage::ContentMismatch {
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
            if let Some(detached) = self.detached_characters.remove(&(account_id, character_id)) {
                if self.world.player(detached.player_id).is_some()
                    && detached.expires_at_tick > self.world.tick()
                {
                    let client = &mut self.wire_clients[client_index];
                    client.player_id = Some(detached.player_id);
                    client.queue_server_message(&ServerMessage::Connected {
                        player_id: detached.player_id.0,
                        role: wire_role(character.role),
                    });
                    client.queue_server_message(&ServerMessage::Snapshot(
                        wire_snapshot_for_player(&self.world, detached.player_id),
                    ));
                    return;
                }
            }
            let command = match self
                .account_repository
                .load_checkpoint(account_id, character_id)
            {
                Ok(Some(state)) => Command::RestorePlayer { state },
                Ok(None) => Command::JoinPlayer {
                    name: character.name,
                    role: character.role,
                },
                Err(error) => {
                    self.queue_wire_error(
                        client_id,
                        format!("cannot load character checkpoint: {error}"),
                    );
                    return;
                }
            };
            self.enqueue_pending_wire_command(client_id, command);
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
                self.queue_wire_error(
                    client_id,
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
            Some(key)
        } else {
            None
        };
        let command = match wire_command_to_core(command, player_id) {
            Ok(command) => command,
            Err(error) => {
                self.queue_wire_error(client_id, error);
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

    fn enqueue_pending_wire_command(&mut self, client_id: u64, command: Command) {
        self.enqueue_pending_wire_command_with_operation(client_id, command, None);
    }

    fn enqueue_pending_wire_command_with_operation(
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

    fn character_reserved_by_other(
        &self,
        client_id: u64,
        account_id: u64,
        character_id: u64,
    ) -> bool {
        self.wire_clients.iter().any(|client| {
            client.id != client_id
                && client
                    .authenticated
                    .is_some_and(|session| session.account_id == account_id)
                && client.selected_character_id == Some(character_id)
        })
    }

    fn send_wire_machine_snapshot(&mut self, client_id: u64) {
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
        if let Some(client) = self
            .wire_clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            client.queue_server_message(&ServerMessage::Snapshot(snapshot));
        }
    }

    fn queue_wire_error(&mut self, client_id: u64, error: String) {
        if let Some(client) = self
            .wire_clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            client.queue_server_message(&ServerMessage::Error { message: error });
        }
    }

    #[cfg(test)]
    fn handle_line(&mut self, client_id: u64, line: &str) {
        let Some(client_index) = self
            .clients
            .iter()
            .position(|client| client.id == client_id)
        else {
            return;
        };
        match parse_line(line, self.clients[client_index].player_id) {
            Ok(ParsedLine::Command(command)) => {
                if matches!(command, Command::JoinPlayer { .. }) {
                    let already_pending = self.commands.iter().any(|pending| {
                        pending.origin == ClientOrigin::Line(client_id)
                            && matches!(pending.command, Command::JoinPlayer { .. })
                    });
                    if self.clients[client_index].player_id.is_some() || already_pending {
                        self.clients[client_index].queue_line("ERR already connected");
                        return;
                    }
                }
                self.commands.push_back(PendingCommand {
                    origin: ClientOrigin::Line(client_id),
                    command,
                    operation: None,
                });
            }
            Ok(ParsedLine::State) => self.send_state(client_id),
            Ok(ParsedLine::Snapshot) => self.send_machine_snapshot(client_id),
            Ok(ParsedLine::Inventory) => self.send_inventory(client_id),
            Ok(ParsedLine::Help) => self.send_help(client_id),
            Ok(ParsedLine::Quit) => {
                self.clients[client_index].closed = true;
                self.clients[client_index].queue_line("BYE");
            }
            Err(error) => self.clients[client_index].queue_line(format!("ERR {error}")),
        }
    }

    #[cfg(test)]
    fn send_help(&mut self, client_id: u64) {
        let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        else {
            return;
        };
        client.queue_line("HELP connect <name> <tank|healer|damage>");
        client.queue_line("HELP move <dx> <dy>");
        client.queue_line("HELP target <entity-id>");
        client.queue_line("HELP attack");
        client.queue_line("HELP vendor <vendor-id>");
        client.queue_line("HELP buy <vendor-id> <item-id> <quantity>");
        client.queue_line("HELP loot <enemy-id>");
        client.queue_line("HELP party-invite <player-id>");
        client.queue_line("HELP party-accept <party-id>");
        client.queue_line("HELP party-decline <party-id>");
        client.queue_line("HELP party-leave");
        client.queue_line("HELP party-remove <player-id>");
        client.queue_line("HELP party-leader <player-id>");
        client.queue_line("HELP party-disband");
        client.queue_line("HELP inventory");
        client.queue_line("HELP quest-offers <npc-id>");
        client.queue_line("HELP accept-quest <npc-id> <quest-id>");
        client.queue_line("HELP turn-in-quest <npc-id> <quest-id>");
        client.queue_line("HELP state");
        client.queue_line("HELP snapshot");
        client.queue_line("HELP quit");
    }

    #[cfg(test)]
    fn send_state(&mut self, client_id: u64) {
        if !self.clients.iter().any(|client| client.id == client_id) {
            return;
        }
        let mut lines = Vec::new();
        let summary = self.world.summary();
        lines.push(format!(
            "WORLD tick={} players={} npcs={} enemies={} vendors={}",
            summary.tick,
            summary.player_count,
            summary.npc_count,
            summary.enemy_count,
            summary.vendor_count
        ));
        for player in self.world.players() {
            lines.push(format!(
                "PLAYER id={} name={} role={} pos={:.2},{:.2} hp={}/{} gold={} target={}",
                player.id,
                player.name,
                player.role.as_str(),
                player.position.x,
                player.position.y,
                player.health,
                player.max_health,
                player.gold,
                player
                    .target
                    .map_or_else(|| "none".to_owned(), |target| target.to_string())
            ));
            for stack in player.inventory.stacks() {
                lines.push(format!(
                    "ITEM player={} item={} quantity={}",
                    player.id, stack.item_id, stack.quantity
                ));
            }
            for quest in &player.quests {
                lines.push(format!(
                    "QUEST player={} quest={} progress={}/{} status={:?}",
                    player.id, quest.quest_id, quest.progress, quest.required_count, quest.status
                ));
            }
        }
        for npc in self.world.npcs() {
            lines.push(format!(
                "NPC id={} name={} kind={:?} pos={:.2},{:.2} hp={}/{}",
                npc.id,
                npc.name,
                npc.kind,
                npc.position.x,
                npc.position.y,
                npc.health,
                npc.max_health
            ));
        }
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    #[cfg(test)]
    fn send_inventory(&mut self, client_id: u64) {
        let Some(player_id) = self
            .clients
            .iter()
            .find(|client| client.id == client_id)
            .and_then(|client| client.player_id)
        else {
            if let Some(client) = self
                .clients
                .iter_mut()
                .find(|client| client.id == client_id)
            {
                client.queue_line("ERR connect first");
            }
            return;
        };

        let Some(player) = self.world.players().find(|player| player.id == player_id) else {
            return;
        };
        let lines: Vec<_> = std::iter::once(format!(
            "INVENTORY player={} gold={} slots={}/{}",
            player.id,
            player.gold,
            player.inventory.used_slots(),
            player.inventory.capacity()
        ))
        .chain(player.inventory.stacks().map(|stack| {
            format!(
                "ITEM player={} item={} quantity={}",
                player.id, stack.item_id, stack.quantity
            )
        }))
        .collect();
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    /// Sends the temporary machine-readable bootstrap snapshot. This is kept
    /// separate from `state` so existing terminal users retain the current
    /// human-readable output while the graphical client has a deterministic
    /// response to parse.
    #[cfg(test)]
    fn send_machine_snapshot(&mut self, client_id: u64) {
        let lines = format_machine_snapshot(&self.world);
        if let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.id == client_id)
        {
            for line in lines {
                client.queue_line(line);
            }
        }
    }

    fn advance_if_due(&mut self) {
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
        let pending: Vec<_> = self.commands.drain(..).collect();
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

        let events = self.world.step_with_combat_timing(
            pending.into_iter().map(|pending| pending.command),
            self.combat_timing,
        );

        // A journal-backed operation remains pending until its completed
        // result is durably acknowledged. The no-journal development path
        // retains the bounded in-process fence.
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

        self.schedule_next_tick();
    }

    fn stage_operation_batch(
        &mut self,
        pending: Vec<PendingCommand>,
        join_origins: VecDeque<ClientOrigin>,
        operation_commands: Vec<(OperationKey, EntityId, ClientOrigin)>,
    ) {
        let mut staged_world = self.world.clone();
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

    fn apply_join_origins(&mut self, events: &[Event], join_origins: &mut VecDeque<ClientOrigin>) {
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
                    client.player_id = Some(player.id);
                    client.queue_server_message(&ServerMessage::Connected {
                        player_id: player.id.0,
                        role: wire_role(player.role),
                    });
                }
            }
        }
    }

    fn schedule_next_tick(&mut self) {
        self.next_tick += self.tick_interval;
        if self.next_tick <= Instant::now() {
            self.next_tick = Instant::now() + self.tick_interval;
        }
    }

    fn poll_operation_journal(&mut self) {
        let results: Vec<_> = self.operation_journal_worker.drain_results().collect();
        let mut failed_batch = None;
        for result in results {
            if let Err(error) = result.result {
                self.prepared_operations.remove(&result.key);
                if !result.prepared {
                    failed_batch = Some((result.key, error));
                    break;
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

    fn commit_staged_operation_batch(&mut self) {
        let Some(mut batch) = self.staged_operation_batch.take() else {
            return;
        };
        self.world = batch.world;
        self.apply_join_origins(&batch.events, &mut batch.join_origins);
        for (key, events) in batch.operation_events {
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

    fn insert_completed_operation(&mut self, key: OperationKey, result: Vec<ServerMessage>) {
        if self.completed_operations.len() >= MAX_COMPLETED_OPERATIONS
            && !self.completed_operations.contains_key(&key)
        {
            if let Some(oldest) = self.completed_operations.keys().next().copied() {
                self.completed_operations.remove(&oldest);
            }
        }
        self.completed_operations.insert(key, result);
    }

    fn checkpoint_wire_players(&self) {
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
    /// A production signal coordinator can call this same ordering boundary.
    fn shutdown(&mut self) {
        let deadline = Instant::now() + SHUTDOWN_DRAIN_TIMEOUT;
        while (!self.commands.is_empty()
            || !self.prepared_operations.is_empty()
            || self.staged_operation_batch.is_some())
            && Instant::now() < deadline
        {
            self.next_tick = Instant::now() - Duration::from_millis(1);
            self.advance_if_due();
            thread::sleep(Duration::from_millis(1));
        }
        if !self.commands.is_empty()
            || !self.prepared_operations.is_empty()
            || self.staged_operation_batch.is_some()
        {
            eprintln!(
                "shutdown_drain_timeout commands={} prepared={} staged={}",
                self.commands.len(),
                self.prepared_operations.len(),
                self.staged_operation_batch.is_some()
            );
            self.commands.clear();
            self.prepared_operations.clear();
            self.staged_operation_batch = None;
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

    fn queue_disconnects(&mut self) {
        // A socket close is a safe-logout boundary for the development
        // prototype. Persist the post-command state before removing the live
        // entity, even when the periodic checkpoint interval is not due.
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

    fn expire_detached_characters(&mut self) {
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

    fn flush_clients(&mut self) {
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

    fn remove_closed(&mut self) {
        self.wire_clients
            .retain(|client| !client.closed || !client.output.is_empty());
    }

    fn broadcast_wire_event(&mut self, event: &Event) {
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

fn interleave_wire_commands(mut queues: Vec<Vec<(u64, WireCommand)>>) -> Vec<(u64, WireCommand)> {
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

fn wire_role(role: Role) -> mmorpg_wire::RoleCode {
    match role {
        Role::Tank => mmorpg_wire::RoleCode::Tank,
        Role::Healer => mmorpg_wire::RoleCode::Healer,
        Role::DamageDealer => mmorpg_wire::RoleCode::DamageDealer,
    }
}

fn wire_area(area: mmorpg_core::ZoneArea) -> ZoneAreaCode {
    match area {
        mmorpg_core::ZoneArea::Town => ZoneAreaCode::Town,
        mmorpg_core::ZoneArea::Field => ZoneAreaCode::Field,
    }
}

fn wire_status(status: mmorpg_core::QuestStatus) -> QuestStatusCode {
    match status {
        mmorpg_core::QuestStatus::Accepted => QuestStatusCode::Accepted,
        mmorpg_core::QuestStatus::Completed => QuestStatusCode::Completed,
        mmorpg_core::QuestStatus::Rewarded => QuestStatusCode::Rewarded,
    }
}

fn wire_player(player: &mmorpg_core::PlayerSnapshot) -> PlayerState {
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

fn wire_live_player(player: &mmorpg_core::Player) -> PlayerState {
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

fn wire_npc(npc: &mmorpg_core::Npc) -> NpcState {
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

fn wire_event(event: &Event) -> Option<ServerEvent> {
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

fn wire_snapshot(world: &World) -> WorldSnapshot {
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

fn wire_snapshot_for_player(world: &World, player_id: EntityId) -> WorldSnapshot {
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

fn event_recipient(event: &Event) -> Option<EntityId> {
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

fn event_visible_to_player(event: &Event, player_id: Option<EntityId>, world: &World) -> bool {
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

#[cfg(test)]
fn format_event(event: &Event) -> String {
    match event {
        Event::PlayerJoined { player } => format!(
            "EVENT player_joined id={} name={} role={} pos={:.2},{:.2}",
            player.id,
            player.name,
            player.role.as_str(),
            player.position.x,
            player.position.y
        ),
        Event::PlayerLeft { player_id } => format!("EVENT player_left id={player_id}"),
        Event::PlayerMoved {
            player_id,
            position,
            area,
        } => format!(
            "EVENT player_moved id={} pos={:.2},{:.2} area={area:?}",
            player_id, position.x, position.y
        ),
        Event::TargetSelected {
            player_id,
            target_id,
        } => {
            format!("EVENT target_selected player={player_id} target={target_id}")
        }
        Event::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => format!(
            "EVENT attack player={} target={} damage={} target_hp={}",
            player_id, target_id, damage, target_health
        ),
        Event::CombatCooldownStarted {
            player_id,
            ready_tick,
        } => format!(
            "EVENT cooldown player={} ready_tick={ready_tick}",
            player_id
        ),
        Event::HealResolved {
            player_id,
            target_id,
            amount,
            target_health,
        } => format!(
            "EVENT heal player={} target={} amount={} target_hp={}",
            player_id, target_id, amount, target_health
        ),
        Event::TauntResolved {
            player_id,
            target_id,
        } => format!("EVENT taunt player={} target={}", player_id, target_id),
        Event::PlayerReleasedToTown {
            player_id,
            position,
            health,
        } => format!(
            "EVENT released_to_town player={} pos={:.2},{:.2} health={}",
            player_id, position.x, position.y, health
        ),
        Event::EnemyCorpseExpired {
            enemy_id,
            spawn_generation,
        } => format!(
            "EVENT enemy_corpse_expired id={} generation={}",
            enemy_id, spawn_generation
        ),
        Event::EnemyDefeated { enemy_id } => format!("EVENT enemy_defeated id={enemy_id}"),
        Event::EnemyAttackResolved {
            enemy_id,
            target_id,
            damage,
            target_health,
        } => format!(
            "EVENT enemy_attack enemy={} target={} damage={} target_hp={}",
            enemy_id, target_id, damage, target_health
        ),
        Event::PlayerDefeated { player_id } => {
            format!("EVENT player_defeated id={player_id}")
        }
        Event::EnemyRespawned {
            enemy_id,
            spawn_generation,
        } => format!(
            "EVENT enemy_respawned id={} generation={}",
            enemy_id, spawn_generation
        ),
        Event::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => {
            let listing_text = listings
                .iter()
                .map(|listing| {
                    format!(
                        "item={} name={} price={} stock={} max_stack={}",
                        listing.item_id,
                        listing.name.replace(' ', "_"),
                        listing.unit_price,
                        listing.remaining_quantity,
                        listing.max_stack
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT vendor_listed player={} vendor={} listings={listing_text}",
                player_id, vendor_id
            )
        }
        Event::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => format!(
            "EVENT item_purchased player={} vendor={} item={} quantity={} total_price={} gold={}",
            player_id, vendor_id, item_id, quantity, total_price, gold_remaining
        ),
        Event::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => format!(
            "EVENT loot_rewarded player={} enemy={} item={} quantity={}",
            player_id, enemy_id, item_id, quantity
        ),
        Event::PartyInviteCreated {
            party_id,
            inviter_id,
            invitee_id,
            expires_at_tick,
        } => format!(
            "EVENT party_invite party={} inviter={} invitee={} expires={}",
            party_id, inviter_id, invitee_id, expires_at_tick
        ),
        Event::PartyInviteAccepted { party, player_id } => format!(
            "EVENT party_joined party={} player={} leader={} members={:?}",
            party.id, player_id, party.leader_id, party.member_ids
        ),
        Event::PartyInviteDeclined {
            party_id,
            player_id,
        } => format!(
            "EVENT party_invite_declined party={} player={}",
            party_id, player_id
        ),
        Event::PartyInviteExpired {
            party_id,
            player_id,
        } => format!(
            "EVENT party_invite_expired party={} player={}",
            party_id, player_id
        ),
        Event::PartyMemberLeft {
            party_id,
            player_id,
        } => format!("EVENT party_left party={} player={}", party_id, player_id),
        Event::PartyMemberRemoved {
            party_id,
            player_id,
            removed_by,
        } => format!(
            "EVENT party_removed party={} player={} removed_by={}",
            party_id, player_id, removed_by
        ),
        Event::PartyLeaderTransferred {
            party_id,
            previous_leader_id,
            leader_id,
        } => format!(
            "EVENT party_leader party={} previous={} leader={}",
            party_id, previous_leader_id, leader_id
        ),
        Event::PartyDisbanded {
            party_id,
            member_ids,
        } => format!(
            "EVENT party_disbanded party={} members={:?}",
            party_id, member_ids
        ),
        Event::TransactionRejected { player_id, reason } => format!(
            "EVENT transaction_rejected player={} reason={reason}",
            player_id
        ),
        Event::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => {
            let quest_text = quests
                .iter()
                .map(|quest| {
                    format!(
                        "id={} name={}",
                        quest.quest_id,
                        quest.name.replace(' ', "_")
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT quest_offers player={} npc={} quests={quest_text}",
                player_id, npc_id
            )
        }
        Event::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => format!(
            "EVENT quest_accepted player={} npc={} quest={}",
            player_id, npc_id, quest_id
        ),
        Event::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => format!(
            "EVENT quest_progressed player={} quest={} progress={}/{}",
            player_id, quest_id, progress, required_count
        ),
        Event::QuestCompleted {
            player_id,
            quest_id,
        } => format!(
            "EVENT quest_completed player={} quest={}",
            player_id, quest_id
        ),
        Event::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => format!(
            "EVENT quest_rewarded player={} quest={} gold={} item={} quantity={} gold_remaining={}",
            player_id,
            quest_id,
            gold,
            item_id.map_or_else(|| "none".to_owned(), |item| item.to_string()),
            item_quantity,
            gold_remaining
        ),
        Event::QuestRejected { player_id, reason } => {
            format!("EVENT quest_rejected player={} reason={reason}", player_id)
        }
        Event::CommandRejected { reason } => format!("EVENT rejected reason={reason}"),
    }
}

/// Formats the temporary graphical-client bootstrap response.
///
/// The `TEMP_SNAPSHOT` prefix is deliberately not part of the future wire
/// protocol. Records are whitespace-delimited key/value pairs, and text
/// values use percent encoding so names containing spaces cannot change the
/// record shape. Positions use Rust's shortest round-trippable float format.
#[cfg(test)]
fn format_machine_snapshot(world: &World) -> Vec<String> {
    let summary = world.summary();
    let mut lines = vec!["TEMP_SNAPSHOT_BEGIN version=2".to_owned()];
    lines.push(format!(
        "TEMP_SNAPSHOT WORLD tick={} players={} npcs={} enemies={} vendors={}",
        summary.tick,
        summary.player_count,
        summary.npc_count,
        summary.enemy_count,
        summary.vendor_count
    ));
    for player in world.players() {
        lines.push(format!(
            "TEMP_SNAPSHOT PLAYER id={} name={} role={} position={:?},{:?} health={} max_health={} gold={} capacity={} target={}",
            player.id,
            encode_snapshot_text(&player.name),
            player.role.as_str(),
            player.position.x,
            player.position.y,
            player.health,
            player.max_health,
            player.gold,
            player.inventory.capacity(),
            player
                .target
                .map_or_else(|| "none".to_owned(), |target| target.to_string())
        ));
        for stack in player.inventory.stacks() {
            lines.push(format!(
                "TEMP_SNAPSHOT ITEM player={} item={} quantity={}",
                player.id, stack.item_id, stack.quantity
            ));
        }
        for quest in &player.quests {
            lines.push(format!(
                "TEMP_SNAPSHOT QUEST player={} quest={} progress={}/{} status={:?}",
                player.id, quest.quest_id, quest.progress, quest.required_count, quest.status
            ));
        }
    }
    for npc in world.npcs() {
        lines.push(format!(
            "TEMP_SNAPSHOT NPC id={} template_id={} name={} kind={} position={:?},{:?} health={} max_health={}",
            npc.id,
            npc.template_id,
            encode_snapshot_text(&npc.name),
            machine_npc_kind(npc.kind),
            npc.position.x,
            npc.position.y,
            npc.health,
            npc.max_health,
        ));
    }
    lines.push("TEMP_SNAPSHOT_END".to_owned());
    lines
}

#[cfg(test)]
fn machine_npc_kind(kind: mmorpg_core::NpcKind) -> &'static str {
    match kind {
        mmorpg_core::NpcKind::Vendor => "vendor",
        mmorpg_core::NpcKind::Enemy => "enemy",
    }
}

#[cfg(test)]
fn encode_snapshot_text(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

#[cfg(test)]
enum ParsedLine {
    Command(Command),
    State,
    Snapshot,
    Inventory,
    Help,
    Quit,
}

#[cfg(test)]
fn parse_line(line: &str, bound_player: Option<EntityId>) -> Result<ParsedLine, String> {
    let tokens: Vec<_> = line.split_whitespace().collect();
    let Some(command) = tokens.first().copied() else {
        return Err("empty command".to_owned());
    };
    match command.to_ascii_lowercase().as_str() {
        "connect" => {
            if tokens.len() != 3 {
                return Err("usage: connect <name> <tank|healer|damage>".to_owned());
            }
            let role = tokens[2]
                .parse::<Role>()
                .map_err(|error| error.to_string())?;
            Ok(ParsedLine::Command(Command::JoinPlayer {
                name: tokens[1].to_owned(),
                role,
            }))
        }
        "move" => {
            let (dx, dy) = match tokens.len() {
                3 => (tokens[1], tokens[2]),
                4 => {
                    ensure_bound_id(tokens[1], bound_player)?;
                    (tokens[2], tokens[3])
                }
                _ => return Err("usage: move <dx> <dy>".to_owned()),
            };
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::Move {
                player_id,
                dx: dx.parse().map_err(|_| "dx must be a number".to_owned())?,
                dy: dy.parse().map_err(|_| "dy must be a number".to_owned())?,
            }))
        }
        "target" => {
            if tokens.len() != 2 {
                return Err("usage: target <entity-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::SelectTarget {
                player_id,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "attack" => {
            if tokens.len() != 1 {
                return Err("usage: attack".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BasicAttack { player_id }))
        }
        "vendor" | "list-vendor" => {
            if tokens.len() != 2 {
                return Err("usage: vendor <vendor-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListVendor {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
            }))
        }
        "buy" => {
            if tokens.len() != 4 {
                return Err("usage: buy <vendor-id> <item-id> <quantity>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BuyItem {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
                item_id: parse_item_id(tokens[2])?,
                quantity: parse_positive_quantity(tokens[3])?,
            }))
        }
        "loot" => {
            if tokens.len() != 2 {
                return Err("usage: loot <enemy-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::LootEnemy {
                player_id,
                enemy_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-invite" => {
            if tokens.len() != 2 {
                return Err("usage: party-invite <player-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::InvitePartyMember {
                player_id,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-accept" => {
            if tokens.len() != 2 {
                return Err("usage: party-accept <party-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::AcceptPartyInvite {
                player_id,
                party_id: tokens[1]
                    .parse()
                    .map(mmorpg_core::PartyId)
                    .map_err(|_| "party-id must be an integer".to_owned())?,
            }))
        }
        "party-decline" => {
            if tokens.len() != 2 {
                return Err("usage: party-decline <party-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::DeclinePartyInvite {
                player_id,
                party_id: tokens[1]
                    .parse()
                    .map(mmorpg_core::PartyId)
                    .map_err(|_| "party-id must be an integer".to_owned())?,
            }))
        }
        "party-leave" => {
            if tokens.len() != 1 {
                return Err("usage: party-leave".to_owned());
            }
            Ok(ParsedLine::Command(Command::LeaveParty {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
            }))
        }
        "party-remove" => {
            if tokens.len() != 2 {
                return Err("usage: party-remove <player-id>".to_owned());
            }
            Ok(ParsedLine::Command(Command::RemovePartyMember {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-leader" => {
            if tokens.len() != 2 {
                return Err("usage: party-leader <player-id>".to_owned());
            }
            Ok(ParsedLine::Command(Command::TransferPartyLeader {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-disband" => {
            if tokens.len() != 1 {
                return Err("usage: party-disband".to_owned());
            }
            Ok(ParsedLine::Command(Command::DisbandParty {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
            }))
        }
        "inventory" => {
            if tokens.len() != 1 {
                return Err("usage: inventory".to_owned());
            }
            Ok(ParsedLine::Inventory)
        }
        "quest-offers" | "quests" => {
            if tokens.len() != 2 {
                return Err("usage: quest-offers <npc-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListQuestOffers {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
            }))
        }
        "accept-quest" => {
            if tokens.len() != 3 {
                return Err("usage: accept-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::AcceptQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "turn-in-quest" => {
            if tokens.len() != 3 {
                return Err("usage: turn-in-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::TurnInQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "state" => Ok(ParsedLine::State),
        "snapshot" => Ok(ParsedLine::Snapshot),
        "help" => Ok(ParsedLine::Help),
        "quit" | "exit" => Ok(ParsedLine::Quit),
        _ => Err(format!("unknown command '{command}'; try help")),
    }
}

#[cfg(test)]
fn ensure_bound_id(value: &str, bound_player: Option<EntityId>) -> Result<(), String> {
    let requested = parse_entity_id(value)?;
    if Some(requested) != bound_player {
        return Err("a client may only control its own player".to_owned());
    }
    Ok(())
}

#[cfg(test)]
fn parse_entity_id(value: &str) -> Result<EntityId, String> {
    value
        .parse::<u64>()
        .map(EntityId)
        .map_err(|_| "entity-id must be an integer".to_owned())
}

#[cfg(test)]
fn parse_item_id(value: &str) -> Result<ItemId, String> {
    value
        .parse::<u32>()
        .map(ItemId)
        .map_err(|_| "item-id must be an integer".to_owned())
}

#[cfg(test)]
fn parse_quest_id(value: &str) -> Result<QuestId, String> {
    value
        .parse::<u32>()
        .map(QuestId)
        .map_err(|_| "quest-id must be an integer".to_owned())
}

#[cfg(test)]
fn parse_positive_quantity(value: &str) -> Result<u32, String> {
    let quantity = value
        .parse::<u32>()
        .map_err(|_| "quantity must be a positive integer".to_owned())?;
    if quantity == 0 {
        return Err("quantity must be a positive integer".to_owned());
    }
    Ok(quantity)
}

fn is_retryable_core_command(command: &WireCommand) -> bool {
    matches!(
        command,
        WireCommand::BuyItem { .. }
            | WireCommand::LootEnemy { .. }
            | WireCommand::TurnInQuest { .. }
    )
}

fn command_to_wire_payload(command: &Command) -> Result<Vec<u8>, String> {
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

fn origin_client_id(origin: ClientOrigin) -> u64 {
    match origin {
        #[cfg(test)]
        ClientOrigin::Line(client_id) => client_id,
        ClientOrigin::Wire(client_id) => client_id,
    }
}

fn command_player_id(command: &Command) -> Option<EntityId> {
    match command {
        Command::BuyItem { player_id, .. }
        | Command::LootEnemy { player_id, .. }
        | Command::TurnInQuest { player_id, .. } => Some(*player_id),
        _ => None,
    }
}

fn wire_command_to_core(command: WireCommand, player_id: EntityId) -> Result<Command, String> {
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
        | WireCommand::Snapshot => {
            return Err("command is not valid in a bound session".to_owned());
        }
    })
}

fn parse_server_addresses() -> Result<(String, Option<String>, Option<PathBuf>, Option<u64>), String>
{
    let mut arguments = env::args().skip(1);
    let address = arguments
        .next()
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let mut wire_address = None;
    let mut checkpoint_path = None;
    let mut shutdown_after_ticks = None;
    while let Some(argument) = arguments.next() {
        if argument == "--wire-address" {
            if wire_address.is_some() {
                return Err("--wire-address may only be specified once".to_owned());
            }
            wire_address = Some(
                arguments
                    .next()
                    .ok_or_else(|| "--wire-address requires an address".to_owned())?,
            );
        } else if argument == "--character-store" {
            if checkpoint_path.is_some() {
                return Err("--character-store may only be specified once".to_owned());
            }
            checkpoint_path =
                Some(PathBuf::from(arguments.next().ok_or_else(|| {
                    "--character-store requires a file path".to_owned()
                })?));
        } else if argument == "--shutdown-after-ticks" {
            if shutdown_after_ticks.is_some() {
                return Err("--shutdown-after-ticks may only be specified once".to_owned());
            }
            let ticks = arguments
                .next()
                .ok_or_else(|| "--shutdown-after-ticks requires a tick count".to_owned())?
                .parse::<u64>()
                .map_err(|_| "--shutdown-after-ticks requires an integer".to_owned())?;
            shutdown_after_ticks = Some(ticks);
        } else {
            return Err(format!("unknown argument '{argument}'"));
        }
    }
    Ok((address, wire_address, checkpoint_path, shutdown_after_ticks))
}

fn main() -> io::Result<()> {
    let (address, additional_wire_address, checkpoint_path, shutdown_after_ticks) =
        parse_server_addresses()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    // The primary listener is typed gameplay. The retained optional address
    // is a second typed listener for staged smoke tooling, not a line server.
    let listener = TcpListener::bind(&address)?;
    listener.set_nonblocking(true)?;
    let additional_listener = additional_wire_address
        .as_deref()
        .map(TcpListener::bind)
        .transpose()?;
    if let Some(listener) = &additional_listener {
        listener.set_nonblocking(true)?;
    }
    let dev_auth_enabled = listener
        .local_addr()
        .map(|address| address.ip().is_loopback())
        .unwrap_or(false);
    let mut server = Server::new(DEFAULT_TICK_HZ, dev_auth_enabled, checkpoint_path.clone());
    println!("typed_server_listening address={address} tick_hz={DEFAULT_TICK_HZ}");
    println!("typed_dev_auth_enabled={dev_auth_enabled}");
    if let Some(address) = additional_wire_address {
        println!("typed_server_additional_listener address={address}");
    }
    if let Some(path) = checkpoint_path {
        println!("character_checkpoint_store={}", path.display());
    }
    if let Some(ticks) = shutdown_after_ticks {
        println!("shutdown_after_ticks={ticks}");
    }

    loop {
        if shutdown_after_ticks.is_none_or(|limit| server.world.tick() < limit) {
            loop {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        stream.set_nonblocking(true)?;
                        println!("accepted_typed_peer={peer}");
                        server.add_wire_client(stream);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error),
                }
            }

            if let Some(listener) = &additional_listener {
                loop {
                    match listener.accept() {
                        Ok((stream, peer)) => {
                            stream.set_nonblocking(true)?;
                            println!("accepted_typed_additional_peer={peer}");
                            server.add_wire_client(stream);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) => return Err(error),
                    }
                }
            }
        }

        if shutdown_after_ticks.is_some_and(|limit| server.world.tick() >= limit) {
            println!("graceful_shutdown_begin tick={}", server.world.tick());
            server.shutdown();
            server.flush_clients();
            println!("graceful_shutdown_complete tick={}", server.world.tick());
            break Ok(());
        }

        server.read_wire_clients();
        // Apply commands received before a socket close, then checkpoint the
        // still-bound character in `advance_if_due`. Queue the leave only
        // afterward so a disconnect cannot hide a same-tick reward from the
        // persistence pass.
        server.advance_if_due();
        server.queue_disconnects();
        server.flush_clients();
        server.remove_closed();
        thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_protocol_requires_a_bound_player_for_gameplay() {
        assert!(parse_line("move 1 0", None).is_err());
        assert!(parse_line("attack", None).is_err());
        assert!(parse_line("connect Alice tank", None).is_ok());
    }

    #[test]
    fn development_protocol_only_allows_a_client_to_move_its_own_player() {
        let own = Some(EntityId(7));
        assert!(parse_line("move 7 1 0", own).is_ok());
        assert!(parse_line("move 8 1 0", own).is_err());
    }

    #[test]
    fn development_protocol_parses_economy_commands_for_bound_player() {
        let own = Some(EntityId(7));
        assert_eq!(
            parse_line("vendor 1", own).unwrap_command(),
            Command::ListVendor {
                player_id: EntityId(7),
                vendor_id: EntityId(1),
            }
        );
        assert_eq!(
            parse_line("buy 1 2 3", own).unwrap_command(),
            Command::BuyItem {
                player_id: EntityId(7),
                vendor_id: EntityId(1),
                item_id: ItemId(2),
                quantity: 3,
            }
        );
        assert_eq!(
            parse_line("loot 12", own).unwrap_command(),
            Command::LootEnemy {
                player_id: EntityId(7),
                enemy_id: EntityId(12),
            }
        );
        assert!(parse_line("buy 1 2 0", own).is_err());
        assert!(parse_line("loot 12", None).is_err());
    }

    #[test]
    fn development_protocol_parses_quest_commands_for_bound_player() {
        let own = Some(EntityId(7));
        assert_eq!(
            parse_line("quest-offers 1", own).unwrap_command(),
            Command::ListQuestOffers {
                player_id: EntityId(7),
                npc_id: EntityId(1),
            }
        );
        assert_eq!(
            parse_line("accept-quest 1 1", own).unwrap_command(),
            Command::AcceptQuest {
                player_id: EntityId(7),
                npc_id: EntityId(1),
                quest_id: QuestId(1),
            }
        );
        assert_eq!(
            parse_line("turn-in-quest 1 1", own).unwrap_command(),
            Command::TurnInQuest {
                player_id: EntityId(7),
                npc_id: EntityId(1),
                quest_id: QuestId(1),
            }
        );
        assert!(parse_line("accept-quest 1 nope", own).is_err());
    }

    trait ParsedLineExt {
        fn unwrap_command(self) -> Command;
    }

    impl ParsedLineExt for Result<ParsedLine, String> {
        fn unwrap_command(self) -> Command {
            match self.expect("expected a parsed command") {
                ParsedLine::Command(command) => command,
                ParsedLine::State | ParsedLine::Inventory | ParsedLine::Help | ParsedLine::Quit => {
                    panic!("expected a command")
                }
                ParsedLine::Snapshot => panic!("expected a command"),
            }
        }
    }

    #[test]
    fn event_format_is_human_readable() {
        let event = Event::EnemyDefeated {
            enemy_id: EntityId(9),
        };
        assert_eq!(format_event(&event), "EVENT enemy_defeated id=9");
    }

    #[test]
    fn snapshot_command_is_available_without_a_bound_player() {
        assert!(matches!(
            parse_line("snapshot", None),
            Ok(ParsedLine::Snapshot)
        ));
    }

    #[test]
    fn typed_pending_command_queue_has_a_hard_bound() {
        let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
        for _ in 0..(MAX_PENDING_COMMANDS + 1) {
            server.enqueue_pending_wire_command(
                99,
                Command::Move {
                    player_id: EntityId(5),
                    dx: 1.0,
                    dy: 0.0,
                },
            );
        }
        assert_eq!(server.commands.len(), MAX_PENDING_COMMANDS);
    }

    #[test]
    fn typed_intake_interleaves_clients_before_authoritative_application() {
        let commands = interleave_wire_commands(vec![
            vec![
                (1, WireCommand::Move { dx: 1.0, dy: 0.0 }),
                (1, WireCommand::BasicAttack),
                (1, WireCommand::Snapshot),
            ],
            vec![(2, WireCommand::Move { dx: -1.0, dy: 0.0 })],
        ]);
        let owners: Vec<_> = commands.iter().map(|(owner, _)| *owner).collect();
        assert_eq!(owners, vec![1, 2, 1, 1]);
    }

    #[test]
    fn active_character_fence_is_scoped_to_account_and_character() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
        let mut first = WireClient::new(1, stream);
        first.authenticated = Some(AuthenticatedSession {
            account_id: 7,
            session_id: 1,
        });
        first.selected_character_id = Some(42);
        server.wire_clients.push(first);

        assert!(server.character_reserved_by_other(2, 7, 42));
        assert!(!server.character_reserved_by_other(2, 8, 42));
        assert!(!server.character_reserved_by_other(2, 7, 43));
        assert!(!server.character_reserved_by_other(1, 7, 42));
    }

    #[test]
    fn disconnected_character_rebinds_within_grace_without_joining_again() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
        let player = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        server.detached_characters.insert(
            (1, 1),
            DetachedCharacter {
                player_id: player,
                expires_at_tick: DISCONNECT_GRACE_TICKS,
            },
        );
        let mut client = WireClient::new(1, stream);
        client.authenticated = Some(AuthenticatedSession {
            account_id: 1,
            session_id: 2,
        });
        client.selected_character_id = Some(1);
        client.content_compatible = true;
        server.wire_clients.push(client);

        server.handle_wire_command(1, WireCommand::EnterWorld);

        assert_eq!(server.wire_clients[0].player_id, Some(player));
        assert!(server.detached_characters.is_empty());
        assert!(server.commands.is_empty());
    }

    #[test]
    fn disconnected_character_expires_into_an_authoritative_leave() {
        let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
        let player = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        server.detached_characters.insert(
            (1, 1),
            DetachedCharacter {
                player_id: player,
                expires_at_tick: 0,
            },
        );

        server.expire_detached_characters();

        assert!(server.detached_characters.is_empty());
        assert!(matches!(
            server.commands.front().map(|pending| &pending.command),
            Some(Command::LeavePlayer { player_id }) if *player_id == player
        ));
    }

    #[test]
    fn retryable_durable_command_returns_cached_result_without_reapplying() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, None);
        let player_id = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        let mut client = WireClient::new(1, stream);
        client.authenticated = Some(AuthenticatedSession {
            account_id: 1,
            session_id: 1,
        });
        client.selected_character_id = Some(1);
        client.content_compatible = true;
        client.player_id = Some(player_id);
        server.wire_clients.push(client);
        let key = OperationKey {
            account_id: 1,
            character_id: 1,
            operation_id: 77,
        };
        server.commands.push_back(PendingCommand {
            origin: ClientOrigin::Wire(1),
            command: Command::BuyItem {
                player_id,
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 1,
            },
            operation: Some(key),
        });
        server.next_tick = Instant::now() - Duration::from_millis(1);
        server.advance_if_due();
        let gold_after_first_apply = server.world.player(player_id).unwrap().gold;
        assert!(server.completed_operations.contains_key(&key));

        server.handle_wire_command(
            1,
            WireCommand::Retryable {
                operation_id: 77,
                command: Box::new(WireCommand::BuyItem {
                    vendor_id: 1,
                    item_id: ItemId::TOWN_RATION.0,
                    quantity: 1,
                }),
            },
        );

        assert_eq!(
            server.world.player(player_id).unwrap().gold,
            gold_after_first_apply
        );
        assert!(server.commands.is_empty());
        assert!(!server.wire_clients[0].output.is_empty());
    }

    #[test]
    fn journaled_operation_stages_world_before_commit_and_reloads_after_restart() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let checkpoint_path = std::env::temp_dir().join(format!("mmorpg-staged-{unique}.state"));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
        let player_id = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        let mut client = WireClient::new(1, stream);
        client.authenticated = Some(AuthenticatedSession {
            account_id: 1,
            session_id: 1,
        });
        client.selected_character_id = Some(1);
        client.content_compatible = true;
        client.player_id = Some(player_id);
        server.wire_clients.push(client);
        let initial_gold = server.world.player(player_id).unwrap().gold;
        let key = OperationKey {
            account_id: 1,
            character_id: 1,
            operation_id: 700,
        };

        server.handle_wire_command(
            1,
            WireCommand::Retryable {
                operation_id: key.operation_id,
                command: Box::new(WireCommand::BuyItem {
                    vendor_id: 1,
                    item_id: ItemId::TOWN_RATION.0,
                    quantity: 1,
                }),
            },
        );

        let mut applied = false;
        for _ in 0..200 {
            server.next_tick = Instant::now() - Duration::from_millis(1);
            server.advance_if_due();
            if server.completed_operations.contains_key(&key) {
                applied = true;
                break;
            }
            assert_eq!(server.world.player(player_id).unwrap().gold, initial_gold);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(applied, "journaled operation should eventually commit");
        assert!(server.world.player(player_id).unwrap().gold < initial_gold);
        drop(server);

        let restarted = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
        assert!(restarted.completed_operations.contains_key(&key));
        let _ = std::fs::remove_file(checkpoint_path);
        let _ = std::fs::remove_file(
            std::env::temp_dir().join(format!("mmorpg-staged-{unique}.operations")),
        );
    }

    #[test]
    fn journal_completion_failure_discards_staged_world_without_publishing_success() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let checkpoint_path =
            std::env::temp_dir().join(format!("mmorpg-journal-failure-{unique}.state"));
        let journal_path = checkpoint_path.with_extension("operations");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
        let player_id = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        let mut client = WireClient::new(1, stream);
        client.authenticated = Some(AuthenticatedSession {
            account_id: 1,
            session_id: 1,
        });
        client.selected_character_id = Some(1);
        client.content_compatible = true;
        client.player_id = Some(player_id);
        server.wire_clients.push(client);
        let initial_gold = server.world.player(player_id).unwrap().gold;
        let key = OperationKey {
            account_id: 1,
            character_id: 1,
            operation_id: 701,
        };

        // The journal worker has already opened the path during Server::new.
        // Replacing it with a directory makes the completion append fail
        // deterministically without injecting a test-only branch into the
        // persistence implementation.
        std::fs::create_dir(&journal_path).expect("journal failure directory should be created");
        server.handle_wire_command(
            1,
            WireCommand::Retryable {
                operation_id: key.operation_id,
                command: Box::new(WireCommand::BuyItem {
                    vendor_id: 1,
                    item_id: ItemId::TOWN_RATION.0,
                    quantity: 1,
                }),
            },
        );

        for _ in 0..200 {
            server.next_tick = Instant::now() - Duration::from_millis(1);
            server.advance_if_due();
            if server.staged_operation_batch.is_none()
                && !server.prepared_operations.contains_key(&key)
                && server.commands.is_empty()
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(server.staged_operation_batch.is_none());
        assert!(!server.completed_operations.contains_key(&key));
        assert_eq!(server.world.player(player_id).unwrap().gold, initial_gold);
        drop(server);
        let _ = std::fs::remove_file(checkpoint_path);
        std::fs::remove_dir(journal_path).expect("journal failure directory should be removed");
    }

    #[test]
    fn shutdown_drains_commands_and_checkpoints_before_releasing_players() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let checkpoint_path = std::env::temp_dir().join(format!("mmorpg-shutdown-{unique}.state"));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut server = Server::new(DEFAULT_TICK_HZ, false, Some(checkpoint_path.clone()));
        let player_id = server
            .world
            .step([Command::JoinPlayer {
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
            }])
            .into_iter()
            .find_map(|event| match event {
                Event::PlayerJoined { player } => Some(player.id),
                _ => None,
            })
            .expect("test player should join");
        let mut client = WireClient::new(1, stream);
        client.authenticated = Some(AuthenticatedSession {
            account_id: 1,
            session_id: 1,
        });
        client.selected_character_id = Some(1);
        client.content_compatible = true;
        client.player_id = Some(player_id);
        server.wire_clients.push(client);
        server.commands.push_back(PendingCommand {
            origin: ClientOrigin::Wire(1),
            command: Command::Move {
                player_id,
                dx: 2.0,
                dy: 0.0,
            },
            operation: None,
        });

        server.shutdown();
        assert!(server.commands.is_empty());
        assert!(server.world.player(player_id).is_none());
        drop(server);

        let repository =
            DevelopmentAccountRepository::with_checkpoint_store(true, checkpoint_path.clone());
        assert!(
            repository
                .load_checkpoint(1, 1)
                .expect("shutdown checkpoint should load")
                .is_some()
        );
        let _ = std::fs::remove_file(checkpoint_path);
        let _ = std::fs::remove_file(
            std::env::temp_dir().join(format!("mmorpg-shutdown-{unique}.operations")),
        );
    }

    #[test]
    fn private_events_and_snapshots_are_scoped_to_the_bound_player() {
        let player_id = EntityId(7);
        let other_id = EntityId(8);
        assert_eq!(
            event_recipient(&Event::ItemPurchased {
                player_id,
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 1,
                total_price: 2,
                gold_remaining: 18,
            }),
            Some(player_id)
        );
        assert_eq!(
            event_recipient(&Event::EnemyDefeated { enemy_id: other_id }),
            None
        );

        let mut world = World::new_starter_zone();
        world.step([
            Command::JoinPlayer {
                name: "One".to_owned(),
                role: Role::Tank,
            },
            Command::JoinPlayer {
                name: "Two".to_owned(),
                role: Role::Healer,
            },
        ]);
        let snapshot = wire_snapshot_for_player(&world, EntityId(5));
        assert_eq!(snapshot.players.len(), 1);
        assert_eq!(snapshot.players[0].player_id, 5);
        assert_eq!(snapshot.player_count, 1);
    }

    #[test]
    fn party_events_do_not_leak_to_unrelated_players() {
        let mut world = World::new_starter_zone();
        world.step([
            Command::JoinPlayer {
                name: "One".to_owned(),
                role: Role::Tank,
            },
            Command::JoinPlayer {
                name: "Two".to_owned(),
                role: Role::Healer,
            },
            Command::JoinPlayer {
                name: "Stranger".to_owned(),
                role: Role::DamageDealer,
            },
        ]);
        let invite = world.step([Command::InvitePartyMember {
            player_id: EntityId(5),
            target_id: EntityId(6),
        }]);
        assert!(event_visible_to_player(
            &invite[0],
            Some(EntityId(5)),
            &world
        ));
        assert!(event_visible_to_player(
            &invite[0],
            Some(EntityId(6)),
            &world
        ));
        assert!(!event_visible_to_player(
            &invite[0],
            Some(EntityId(7)),
            &world
        ));

        let accepted = world.step([Command::AcceptPartyInvite {
            player_id: EntityId(6),
            party_id: PartyId(1),
        }]);
        assert!(event_visible_to_player(
            &accepted[0],
            Some(EntityId(5)),
            &world
        ));
        assert!(event_visible_to_player(
            &accepted[0],
            Some(EntityId(6)),
            &world
        ));
        assert!(!event_visible_to_player(
            &accepted[0],
            Some(EntityId(7)),
            &world
        ));
    }

    #[test]
    fn public_events_are_filtered_by_nearby_interest() {
        let mut world = World::new_starter_zone();
        world.step([
            Command::JoinPlayer {
                name: "Near".to_owned(),
                role: Role::Tank,
            },
            Command::JoinPlayer {
                name: "Far".to_owned(),
                role: Role::DamageDealer,
            },
        ]);
        for _ in 0..6 {
            world.step([Command::Move {
                player_id: EntityId(6),
                dx: 10.0,
                dy: 0.0,
            }]);
        }
        let movement = Event::PlayerMoved {
            player_id: EntityId(6),
            position: world.player(EntityId(6)).unwrap().position,
            area: mmorpg_core::ZoneArea::Field,
        };
        assert!(!event_visible_to_player(
            &movement,
            Some(EntityId(5)),
            &world
        ));
        assert!(event_visible_to_player(
            &movement,
            Some(EntityId(6)),
            &world
        ));
    }

    #[test]
    fn replaceable_position_events_coalesce_before_flush() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut client = WireClient::new(1, stream);
        client.queue_replaceable_server_message(
            5,
            ServerMessage::Event(ServerEvent::PlayerMoved {
                player_id: 5,
                position: mmorpg_wire::PositionState { x: 1.0, y: 0.0 },
                area: ZoneAreaCode::Town,
            }),
        );
        client.queue_replaceable_server_message(
            5,
            ServerMessage::Event(ServerEvent::PlayerMoved {
                player_id: 5,
                position: mmorpg_wire::PositionState { x: 2.0, y: 0.0 },
                area: ZoneAreaCode::Town,
            }),
        );
        assert_eq!(client.replaceable_events.len(), 1);
        assert!(matches!(
            client.replaceable_events.get(&5),
            Some(ServerMessage::Event(ServerEvent::PlayerMoved { position, .. }))
                if position.x == 2.0
        ));
    }

    #[test]
    fn slow_client_delivery_is_bounded_and_evicted() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let _peer = TcpStream::connect(address).expect("connect test peer");
        let (stream, _) = listener.accept().expect("accept test peer");
        let mut client = WireClient::new(1, stream);

        for _ in 0..20_000 {
            client.queue_server_message(&ServerMessage::Welcome {
                server: "mmorpg".to_owned(),
            });
            if client.closed {
                break;
            }
        }

        assert!(client.closed, "a saturated client must be evicted");
        assert!(client.output.len() <= MAX_WIRE_OUTPUT_BYTES);

        let replaceable_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind replaceable test listener");
        let replaceable_address = replaceable_listener
            .local_addr()
            .expect("replaceable listener address");
        let _replaceable_peer =
            TcpStream::connect(replaceable_address).expect("connect replaceable test peer");
        let (replaceable_stream, _) = replaceable_listener
            .accept()
            .expect("accept replaceable test peer");
        let mut replaceable_client = WireClient::new(2, replaceable_stream);
        for key in 0..=MAX_REPLACEABLE_EVENTS as u64 {
            replaceable_client.queue_replaceable_server_message(
                key,
                ServerMessage::Event(ServerEvent::PlayerMoved {
                    player_id: key,
                    position: mmorpg_wire::PositionState { x: 0.0, y: 0.0 },
                    area: ZoneAreaCode::Town,
                }),
            );
        }
        assert!(replaceable_client.closed);
        assert_eq!(
            replaceable_client.replaceable_events.len(),
            MAX_REPLACEABLE_EVENTS
        );
    }

    #[test]
    fn machine_snapshot_has_stable_bootstrap_records() {
        let mut world = World::new_starter_zone();
        world.step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }]);

        assert_eq!(
            format_machine_snapshot(&world),
            vec![
                "TEMP_SNAPSHOT_BEGIN version=2",
                "TEMP_SNAPSHOT WORLD tick=1 players=1 npcs=4 enemies=3 vendors=1",
                "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0.0,0.0 health=100 max_health=100 gold=20 capacity=16 target=none",
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0.0,0.0 health=1 max_health=1",
                "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=25.0,0.0 health=100 max_health=100",
                "TEMP_SNAPSHOT NPC id=3 template_id=2 name=Field%20Wolf kind=enemy position=31.0,6.0 health=100 max_health=100",
                "TEMP_SNAPSHOT NPC id=4 template_id=2 name=Field%20Wolf kind=enemy position=31.0,-6.0 health=100 max_health=100",
                "TEMP_SNAPSHOT_END",
            ]
        );
    }

    #[test]
    fn machine_snapshot_includes_player_inventory_and_quest_state() {
        let mut world = World::new_starter_zone();
        world.step([Command::JoinPlayer {
            name: "Aria".to_owned(),
            role: Role::DamageDealer,
        }]);
        let player_id = world.players().next().expect("player joined").id;
        let vendor_id = world
            .npcs()
            .find(|npc| npc.kind == mmorpg_core::NpcKind::Vendor)
            .expect("starter vendor exists")
            .id;

        world.step([Command::BuyItem {
            player_id,
            vendor_id,
            item_id: ItemId::TOWN_RATION,
            quantity: 2,
        }]);
        world.step([Command::AcceptQuest {
            player_id,
            npc_id: vendor_id,
            quest_id: QuestId::CLEAR_THE_FIELD,
        }]);

        let lines = format_machine_snapshot(&world);
        assert!(
            lines
                .iter()
                .any(|line| { line == "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=2" })
        );
        assert!(lines.iter().any(|line| {
            line == "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=0/3 status=Accepted"
        }));
    }

    #[test]
    fn machine_snapshot_text_encoding_preserves_record_boundaries() {
        assert_eq!(
            encode_snapshot_text("Name with\tcontrols%"),
            "Name%20with%09controls%25"
        );
    }
}
