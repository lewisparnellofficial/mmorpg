#![forbid(unsafe_code)]

//! Renderer-independent lifecycle for the typed development protocol.
//!
//! Socket workers are intentionally outside this crate. They submit decoded
//! messages and receive bounded commands; the renderer never owns handshake
//! state or decides whether gameplay intent is currently legal.

use std::collections::VecDeque;
use std::fmt;

use mmorpg_wire::{ClientCommand, ServerEvent, ServerMessage, WorldSnapshot};

pub const MAX_PENDING_COMMANDS: usize = 64;
pub const MAX_PENDING_COMMAND_BYTES: usize = 64 * 1024;
pub const MAX_BOOTSTRAP_BUFFERED_EVENTS: usize = 128;
pub const MAX_BOOTSTRAP_BUFFERED_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    Disconnected,
    Connecting,
    Authenticating,
    AwaitingCharacterList,
    AwaitingCharacterSelection,
    EnteringWorld,
    AwaitingBootstrap,
    Ready,
    Backoff,
    Closed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionInput {
    Connect,
    Authenticated,
    Server(ServerMessage),
    Sequenced {
        sequence: u64,
        message: ServerMessage,
    },
    Disconnected,
    SelectCharacter(u64),
    Intent(ClientCommand),
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionOutput {
    Send(ClientCommand),
    StateChanged(SessionState),
    CharacterList {
        account_id: u64,
        characters: Vec<mmorpg_wire::CharacterSummary>,
    },
    WorldReset,
    Bootstrap(WorldSnapshot),
    Event(ServerEvent),
    RequestBootstrap,
    Rejected {
        reason: RejectReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RejectReason {
    NotReady,
    AlreadyAuthenticated,
    CharacterSelectionRequired,
    InvalidTransition,
    QueueFull,
    InvalidCommand,
    Server(String),
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug)]
pub struct Session {
    state: SessionState,
    token: String,
    pending: VecDeque<(ClientCommand, usize)>,
    pending_bytes: usize,
    selected_character: Option<u64>,
    bootstrap: Option<WorldSnapshot>,
    content_digest: [u8; 32],
    last_sequence: Option<u64>,
    buffered_messages: VecDeque<(u64, ServerMessage, usize)>,
    buffered_bytes: usize,
}

impl Session {
    pub fn new(token: impl Into<String>) -> Self {
        Self::with_content_digest(token, [0; 32])
    }
    pub fn with_content_digest(token: impl Into<String>, content_digest: [u8; 32]) -> Self {
        Self {
            state: SessionState::Disconnected,
            token: token.into(),
            pending: VecDeque::new(),
            pending_bytes: 0,
            selected_character: None,
            bootstrap: None,
            content_digest,
            last_sequence: None,
            buffered_messages: VecDeque::new(),
            buffered_bytes: 0,
        }
    }
    pub fn state(&self) -> SessionState {
        self.state
    }
    pub fn selected_character(&self) -> Option<u64> {
        self.selected_character
    }
    pub fn bootstrap(&self) -> Option<&WorldSnapshot> {
        self.bootstrap.as_ref()
    }

    pub fn handle(&mut self, input: SessionInput) -> Vec<SessionOutput> {
        let mut output = Vec::new();
        match input {
            SessionInput::Connect
                if self.state == SessionState::Disconnected
                    || self.state == SessionState::Backoff =>
            {
                self.transition(SessionState::Connecting, &mut output);
                self.transition(SessionState::Authenticating, &mut output);
                self.enqueue(
                    ClientCommand::Authenticate {
                        token: self.token.clone(),
                    },
                    &mut output,
                );
            }
            SessionInput::Authenticated if self.state == SessionState::Authenticating => {
                self.transition(SessionState::AwaitingCharacterList, &mut output);
                self.enqueue(ClientCommand::ListCharacters, &mut output);
            }
            SessionInput::Server(ServerMessage::Authenticated { .. })
                if self.state == SessionState::Authenticating =>
            {
                self.transition(SessionState::AwaitingCharacterList, &mut output);
                self.enqueue(ClientCommand::ListCharacters, &mut output);
            }
            SessionInput::Sequenced { sequence, message } => {
                self.handle_sequenced(sequence, message, &mut output);
            }
            SessionInput::Server(ServerMessage::CharacterList {
                account_id,
                characters,
            }) if self.state == SessionState::AwaitingCharacterList => {
                self.transition(SessionState::AwaitingCharacterSelection, &mut output);
                output.push(SessionOutput::CharacterList {
                    account_id,
                    characters,
                });
            }
            SessionInput::SelectCharacter(character_id)
                if self.state == SessionState::AwaitingCharacterSelection =>
            {
                self.selected_character = Some(character_id);
                self.transition(SessionState::EnteringWorld, &mut output);
                self.enqueue(ClientCommand::SelectCharacter { character_id }, &mut output);
            }
            SessionInput::Server(ServerMessage::CharacterSelected { character_id, .. })
                if self.state == SessionState::EnteringWorld
                    && self.selected_character == Some(character_id) =>
            {
                self.enqueue(
                    ClientCommand::ContentDigest {
                        digest: self.content_digest,
                    },
                    &mut output,
                );
            }
            SessionInput::Server(ServerMessage::ContentAccepted { digest })
                if self.state == SessionState::EnteringWorld && digest == self.content_digest =>
            {
                self.transition(SessionState::AwaitingBootstrap, &mut output);
                self.enqueue(ClientCommand::EnterWorld, &mut output);
            }
            SessionInput::Server(ServerMessage::ContentMismatch { .. })
                if self.state == SessionState::EnteringWorld =>
            {
                output.push(SessionOutput::Rejected {
                    reason: RejectReason::Server("content mismatch".into()),
                });
            }
            SessionInput::Server(ServerMessage::Connected { .. })
                if self.state == SessionState::AwaitingBootstrap =>
            {
                self.enqueue(ClientCommand::Snapshot, &mut output);
            }
            SessionInput::Server(ServerMessage::Snapshot(snapshot))
                if self.state == SessionState::AwaitingBootstrap =>
            {
                self.bootstrap = Some(snapshot.clone());
                self.transition(SessionState::Ready, &mut output);
                output.push(SessionOutput::Bootstrap(snapshot));
                self.flush_pending(&mut output);
            }
            SessionInput::Server(ServerMessage::Event(event))
                if self.state == SessionState::Ready =>
            {
                output.push(SessionOutput::Event(event))
            }
            SessionInput::Server(ServerMessage::Error { message }) => {
                output.push(SessionOutput::Rejected {
                    reason: RejectReason::Server(message),
                })
            }
            SessionInput::Intent(command) if self.state == SessionState::Ready => {
                self.enqueue(command, &mut output)
            }
            SessionInput::Disconnected => {
                self.pending.clear();
                self.pending_bytes = 0;
                self.selected_character = None;
                self.bootstrap = None;
                self.last_sequence = None;
                self.buffered_messages.clear();
                self.buffered_bytes = 0;
                output.push(SessionOutput::WorldReset);
                self.transition(SessionState::Backoff, &mut output);
            }
            SessionInput::Close => self.transition(SessionState::Closed, &mut output),
            SessionInput::Intent(_) => output.push(SessionOutput::Rejected {
                reason: RejectReason::NotReady,
            }),
            SessionInput::Connect
            | SessionInput::Authenticated
            | SessionInput::Server(_)
            | SessionInput::SelectCharacter(_) => output.push(SessionOutput::Rejected {
                reason: RejectReason::InvalidTransition,
            }),
        }
        output
    }

    fn handle_sequenced(
        &mut self,
        sequence: u64,
        message: ServerMessage,
        output: &mut Vec<SessionOutput>,
    ) {
        if sequence == 0 {
            output.push(SessionOutput::RequestBootstrap);
            return;
        }
        if self.state != SessionState::AwaitingBootstrap && self.state != SessionState::Ready {
            output.extend(self.handle(SessionInput::Server(message)));
            return;
        }
        if !matches!(
            message,
            ServerMessage::Snapshot(_) | ServerMessage::Event(_)
        ) {
            output.extend(self.handle(SessionInput::Server(message)));
            return;
        }
        if let ServerMessage::Snapshot(snapshot) = message {
            if self.state != SessionState::AwaitingBootstrap && self.state != SessionState::Ready {
                output.push(SessionOutput::Rejected {
                    reason: RejectReason::InvalidTransition,
                });
                return;
            }
            self.bootstrap = Some(snapshot.clone());
            self.last_sequence = Some(sequence);
            self.transition(SessionState::Ready, output);
            output.push(SessionOutput::Bootstrap(snapshot));
            let mut buffered: Vec<_> = self.buffered_messages.drain(..).collect();
            self.buffered_bytes = 0;
            buffered.sort_by_key(|(queued_sequence, _, _)| *queued_sequence);
            for (queued_sequence, queued_message, _) in buffered {
                self.handle_sequenced(queued_sequence, queued_message, output);
                if self.state != SessionState::Ready {
                    break;
                }
            }
            return;
        }
        if self.state == SessionState::AwaitingBootstrap {
            let bytes = message
                .encode_payload()
                .map(|payload| payload.len())
                .unwrap_or(usize::MAX);
            if bytes == usize::MAX
                || self.buffered_messages.len() >= MAX_BOOTSTRAP_BUFFERED_EVENTS
                || self.buffered_bytes.saturating_add(bytes) > MAX_BOOTSTRAP_BUFFERED_BYTES
            {
                self.buffered_messages.clear();
                self.buffered_bytes = 0;
                output.push(SessionOutput::RequestBootstrap);
                return;
            }
            self.buffered_bytes += bytes;
            self.buffered_messages.push_back((sequence, message, bytes));
            return;
        }
        let Some(last_sequence) = self.last_sequence else {
            output.push(SessionOutput::RequestBootstrap);
            self.state = SessionState::AwaitingBootstrap;
            return;
        };
        if sequence <= last_sequence {
            return;
        }
        if sequence != last_sequence.saturating_add(1) {
            self.buffered_messages.clear();
            self.buffered_bytes = 0;
            self.state = SessionState::AwaitingBootstrap;
            output.push(SessionOutput::StateChanged(SessionState::AwaitingBootstrap));
            output.push(SessionOutput::RequestBootstrap);
            return;
        }
        self.last_sequence = Some(sequence);
        output.extend(self.handle(SessionInput::Server(message)));
    }

    fn transition(&mut self, state: SessionState, output: &mut Vec<SessionOutput>) {
        self.state = state;
        output.push(SessionOutput::StateChanged(state));
    }
    fn enqueue(&mut self, command: ClientCommand, output: &mut Vec<SessionOutput>) {
        let bytes = command
            .encode_payload()
            .map(|payload| payload.len())
            .unwrap_or(usize::MAX);
        if bytes == usize::MAX
            || self.pending.len() >= MAX_PENDING_COMMANDS
            || self.pending_bytes.saturating_add(bytes) > MAX_PENDING_COMMAND_BYTES
        {
            output.push(SessionOutput::Rejected {
                reason: RejectReason::QueueFull,
            });
            return;
        }
        self.pending_bytes += bytes;
        self.pending.push_back((command, bytes));
        self.flush_pending(output);
    }
    fn flush_pending(&mut self, output: &mut Vec<SessionOutput>) {
        while let Some((command, bytes)) = self.pending.pop_front() {
            self.pending_bytes = self.pending_bytes.saturating_sub(bytes);
            output.push(SessionOutput::Send(command));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_wire::{CharacterSummary, RoleCode};

    #[test]
    fn handshake_requires_auth_character_selection_and_bootstrap() {
        let mut session = Session::new("dev-local");
        assert!(
            session
                .handle(SessionInput::Intent(ClientCommand::BasicAttack))
                .iter()
                .any(|item| matches!(
                    item,
                    SessionOutput::Rejected {
                        reason: RejectReason::NotReady
                    }
                ))
        );
        assert_eq!(
            session
                .handle(SessionInput::Connect)
                .iter()
                .find_map(|item| match item {
                    SessionOutput::Send(ClientCommand::Authenticate { token }) =>
                        Some(token.clone()),
                    _ => None,
                }),
            Some("dev-local".into())
        );
        session.handle(SessionInput::Server(ServerMessage::Authenticated {
            account_id: 1,
            session_id: 1,
        }));
        let list = vec![CharacterSummary {
            character_id: 7,
            name: "Aria".into(),
            role: RoleCode::DamageDealer,
        }];
        session.handle(SessionInput::Server(ServerMessage::CharacterList {
            account_id: 1,
            characters: list,
        }));
        assert!(
            session
                .handle(SessionInput::SelectCharacter(7))
                .iter()
                .any(|item| matches!(
                    item,
                    SessionOutput::Send(ClientCommand::SelectCharacter { character_id: 7 })
                ))
        );
        session.handle(SessionInput::Server(ServerMessage::CharacterSelected {
            character_id: 7,
            name: "Aria".into(),
            role: RoleCode::DamageDealer,
        }));
        assert!(
            session
                .handle(SessionInput::Server(ServerMessage::ContentAccepted {
                    digest: [0; 32],
                }))
                .iter()
                .any(|item| matches!(item, SessionOutput::Send(ClientCommand::EnterWorld)))
        );
        let snapshot = WorldSnapshot {
            version: 2,
            tick: 1,
            player_count: 1,
            npc_count: 0,
            enemy_count: 0,
            vendor_count: 0,
            players: vec![],
            npcs: vec![],
        };
        assert!(
            session
                .handle(SessionInput::Server(ServerMessage::Snapshot(snapshot)))
                .iter()
                .any(|item| matches!(item, SessionOutput::Bootstrap(_)))
        );
        assert_eq!(session.state(), SessionState::Ready);
    }

    #[test]
    fn disconnect_clears_world_selection_and_pending_intents() {
        let mut session = Session::new("token");
        session.handle(SessionInput::Connect);
        session.handle(SessionInput::Disconnected);
        assert_eq!(session.state(), SessionState::Backoff);
        assert_eq!(session.selected_character(), None);
        assert!(session.bootstrap().is_none());
        assert!(
            session
                .handle(SessionInput::Intent(ClientCommand::BasicAttack))
                .iter()
                .any(|item| matches!(
                    item,
                    SessionOutput::Rejected {
                        reason: RejectReason::NotReady
                    }
                ))
        );
    }

    #[test]
    fn duplicate_or_wrong_character_selection_cannot_enter_world() {
        let mut session = Session::new("token");
        session.handle(SessionInput::Connect);
        session.handle(SessionInput::Server(ServerMessage::Authenticated {
            account_id: 1,
            session_id: 2,
        }));
        session.handle(SessionInput::Server(ServerMessage::CharacterList {
            account_id: 1,
            characters: vec![],
        }));
        session.handle(SessionInput::SelectCharacter(9));
        assert!(
            session
                .handle(SessionInput::Server(ServerMessage::CharacterSelected {
                    character_id: 8,
                    name: "Other".into(),
                    role: RoleCode::Tank
                }))
                .iter()
                .any(|item| matches!(
                    item,
                    SessionOutput::Rejected {
                        reason: RejectReason::InvalidTransition
                    }
                ))
        );
        assert_eq!(session.state(), SessionState::EnteringWorld);
    }

    #[test]
    fn sequenced_bootstrap_applies_contiguous_events_and_requests_resync_on_gap() {
        let mut session = Session::new("token");
        session.handle(SessionInput::Connect);
        session.handle(SessionInput::Server(ServerMessage::Authenticated {
            account_id: 1,
            session_id: 1,
        }));
        session.handle(SessionInput::Server(ServerMessage::CharacterList {
            account_id: 1,
            characters: vec![CharacterSummary {
                character_id: 7,
                name: "Aria".into(),
                role: RoleCode::DamageDealer,
            }],
        }));
        session.handle(SessionInput::SelectCharacter(7));
        session.handle(SessionInput::Server(ServerMessage::CharacterSelected {
            character_id: 7,
            name: "Aria".into(),
            role: RoleCode::DamageDealer,
        }));
        session.handle(SessionInput::Server(ServerMessage::ContentAccepted {
            digest: [0; 32],
        }));
        session.handle(SessionInput::Server(ServerMessage::Connected {
            player_id: 7,
            role: RoleCode::DamageDealer,
        }));
        let snapshot = WorldSnapshot {
            version: 2,
            tick: 1,
            player_count: 0,
            npc_count: 0,
            enemy_count: 0,
            vendor_count: 0,
            players: vec![],
            npcs: vec![],
        };
        session.handle(SessionInput::Sequenced {
            sequence: 10,
            message: ServerMessage::Snapshot(snapshot),
        });
        assert_eq!(session.state(), SessionState::Ready);
        let applied = session.handle(SessionInput::Sequenced {
            sequence: 11,
            message: ServerMessage::Event(ServerEvent::EnemyDefeated { enemy_id: 9 }),
        });
        assert!(
            applied
                .iter()
                .any(|item| matches!(item, SessionOutput::Event(_)))
        );
        assert!(
            session
                .handle(SessionInput::Sequenced {
                    sequence: 11,
                    message: ServerMessage::Event(ServerEvent::EnemyDefeated { enemy_id: 9 }),
                })
                .is_empty()
        );
        let gap = session.handle(SessionInput::Sequenced {
            sequence: 13,
            message: ServerMessage::Event(ServerEvent::EnemyDefeated { enemy_id: 10 }),
        });
        assert!(
            gap.iter()
                .any(|item| matches!(item, SessionOutput::RequestBootstrap))
        );
        assert_eq!(session.state(), SessionState::AwaitingBootstrap);
    }

    #[test]
    fn bootstrap_event_buffer_overflow_requests_a_new_bootstrap() {
        let mut session = Session::new("token");
        session.handle(SessionInput::Connect);
        session.handle(SessionInput::Server(ServerMessage::Authenticated {
            account_id: 1,
            session_id: 1,
        }));
        session.handle(SessionInput::Server(ServerMessage::CharacterList {
            account_id: 1,
            characters: vec![],
        }));
        session.state = SessionState::AwaitingBootstrap;
        for sequence in 1..=MAX_BOOTSTRAP_BUFFERED_EVENTS {
            assert!(
                session
                    .handle(SessionInput::Sequenced {
                        sequence: sequence as u64,
                        message: ServerMessage::Event(ServerEvent::EnemyDefeated {
                            enemy_id: sequence as u64,
                        }),
                    })
                    .is_empty()
            );
        }
        let overflow = session.handle(SessionInput::Sequenced {
            sequence: (MAX_BOOTSTRAP_BUFFERED_EVENTS + 1) as u64,
            message: ServerMessage::Event(ServerEvent::EnemyDefeated {
                enemy_id: (MAX_BOOTSTRAP_BUFFERED_EVENTS + 1) as u64,
            }),
        });
        assert!(
            overflow
                .iter()
                .any(|item| matches!(item, SessionOutput::RequestBootstrap))
        );
    }
}
