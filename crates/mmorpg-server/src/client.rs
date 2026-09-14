//! Wire client connection management, framing, and output queues.

use crate::session::AuthenticatedSession;
use mmorpg_core::EntityId;
use mmorpg_wire::{
    CompatibilityControl, Envelope, MessageKind, SequencedServerMessage, ServerMessage,
};
use std::collections::{BTreeMap, VecDeque};
use std::net::TcpStream;

pub const MAX_WIRE_INPUT_BYTES: usize = mmorpg_wire::MAX_FRAME_SIZE * 2;
pub const MAX_WIRE_OUTPUT_BYTES: usize = 256 * 1024;
pub const MAX_REPLACEABLE_EVENTS: usize = 256;
pub const MAX_WIRE_COMMANDS_PER_CLIENT_POLL: usize = 32;
pub const MAX_WIRE_COMMANDS_PER_POLL: usize = 256;
pub const MAX_WIRE_CLIENTS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientOrigin {
    Wire(u64),
}

pub fn origin_client_id(origin: ClientOrigin) -> u64 {
    match origin {
        ClientOrigin::Wire(client_id) => client_id,
    }
}

#[derive(Debug)]
pub struct WireClient {
    pub id: u64,
    pub stream: TcpStream,
    pub input: Vec<u8>,
    pub output: VecDeque<u8>,
    pub replaceable_events: BTreeMap<u64, ServerMessage>,
    pub authenticated: Option<AuthenticatedSession>,
    pub selected_character_id: Option<u64>,
    pub content_compatible: bool,
    pub next_sequence: u64,
    pub player_id: Option<EntityId>,
    pub pending_request_id: Option<u64>,
    pub closed: bool,
}

impl WireClient {
    pub fn new(id: u64, stream: TcpStream) -> Self {
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
            pending_request_id: None,
            closed: false,
        }
    }

    pub fn queue_event_payload(&mut self, payload: &[u8]) {
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

    pub fn queue_server_message(&mut self, message: &ServerMessage) {
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

    pub fn queue_replaceable_server_message(&mut self, key: u64, message: ServerMessage) {
        if self.replaceable_events.len() >= MAX_REPLACEABLE_EVENTS
            && !self.replaceable_events.contains_key(&key)
        {
            self.closed = true;
            return;
        }
        self.replaceable_events.insert(key, message);
    }

    pub fn materialize_replaceable_events(&mut self) {
        let pending = std::mem::take(&mut self.replaceable_events);
        for message in pending.values() {
            self.queue_server_message(message);
        }
    }

    pub fn queue_compatibility_rejection(&mut self, supported_min: u16, supported_max: u16) {
        let frame =
            mmorpg_wire::encode_compatibility_control(&CompatibilityControl::VersionRejected {
                supported_min,
                supported_max,
            });
        if self.output.len().saturating_add(frame.len()) > MAX_WIRE_OUTPUT_BYTES {
            self.closed = true;
            return;
        }
        self.output.extend(frame);
    }
}
