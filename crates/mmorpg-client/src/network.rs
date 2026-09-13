use super::*;

pub(crate) fn spawn_wire_network_worker(
    server_address: String,
    preferred_character_id: Option<u64>,
    command_rx: Receiver<ClientCommand>,
    event_tx: Sender<NetworkEvent>,
) {
    thread::spawn(move || {
        let Some(address) = server_address
            .to_socket_addrs()
            .ok()
            .and_then(|mut addresses| addresses.next())
        else {
            let _ = event_tx.send(NetworkEvent::Status(
                "wire address could not be resolved".to_owned(),
            ));
            return;
        };
        let mut deferred_commands = VecDeque::new();

        loop {
            if event_tx
                .send(NetworkEvent::Status(format!(
                    "connecting to typed wire {server_address}"
                )))
                .is_err()
            {
                return;
            }
            let mut stream = match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
                Ok(stream) => stream,
                Err(error) => {
                    if event_tx
                        .send(NetworkEvent::Status(format!(
                            "wire connection failed: {error}; retrying"
                        )))
                        .is_err()
                    {
                        return;
                    }
                    thread::sleep(RECONNECT_DELAY);
                    continue;
                }
            };
            if let Err(error) = stream.set_nonblocking(true) {
                if event_tx
                    .send(NetworkEvent::Status(format!(
                        "cannot configure wire socket: {error}; retrying"
                    )))
                    .is_err()
                {
                    return;
                }
                thread::sleep(RECONNECT_DELAY);
                continue;
            }
            if event_tx
                .send(NetworkEvent::Status(
                    "typed wire socket connected".to_owned(),
                ))
                .is_err()
            {
                return;
            }
            let mut outgoing = VecDeque::<Vec<u8>>::new();
            let mut session = TypedSession::with_content_digest(
                DEV_AUTH_TOKEN,
                starter_catalog().content_digest(),
            );
            queue_session_outputs(&mut outgoing, session.handle(SessionInput::Connect));
            let mut incoming = Vec::new();

            'connection: loop {
                match command_rx.try_recv() {
                    Err(TryRecvError::Disconnected) => return,
                    Ok(ClientCommand::SelectCharacter { character_id }) => queue_session_outputs(
                        &mut outgoing,
                        session.handle(SessionInput::SelectCharacter(character_id)),
                    ),
                    Ok(command) if session.state() == SessionState::Ready => {
                        queue_session_outputs(
                            &mut outgoing,
                            session.handle(SessionInput::Intent(client_command_to_wire(command))),
                        );
                    }
                    Ok(command) if deferred_commands.len() < MAX_DEFERRED_COMMANDS => {
                        deferred_commands.push_back(command)
                    }
                    Ok(_) | Err(TryRecvError::Empty) => {}
                }

                if !flush_outgoing(&mut stream, &mut outgoing) {
                    let _ = event_tx.send(NetworkEvent::Status(
                        "typed wire write failed; reconnecting".to_owned(),
                    ));
                    break 'connection;
                }

                let mut buffer = [0_u8; 4096];
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => {
                            let _ = event_tx.send(NetworkEvent::Status(
                                "typed wire server closed the connection; reconnecting".to_owned(),
                            ));
                            break 'connection;
                        }
                        Ok(bytes_read) => {
                            incoming.extend_from_slice(&buffer[..bytes_read]);
                            if incoming.len() > MAX_WIRE_INPUT_BYTES {
                                let _ = event_tx.send(NetworkEvent::Status(
                                    "typed wire input buffer exceeded its limit; reconnecting"
                                        .to_owned(),
                                ));
                                break 'connection;
                            }
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                        Err(error) => {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire read failed: {error}; reconnecting"
                            )));
                            break 'connection;
                        }
                    }
                }

                loop {
                    let decoded = match decode_one(&incoming) {
                        Ok(decoded) => decoded,
                        Err(WireDecodeError::Truncated { .. }) => break,
                        Err(error) => {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire frame rejected: {error}; reconnecting"
                            )));
                            break 'connection;
                        }
                    };
                    let consumed = decoded.consumed;
                    let kind = decoded.envelope.kind;
                    let payload = decoded.envelope.payload;
                    incoming.drain(..consumed);
                    if kind != MessageKind::Event {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "typed wire server sent a non-event message; reconnecting".to_owned(),
                        ));
                        break 'connection;
                    }
                    let (sequence, message) = match SequencedServerMessage::decode_payload(&payload)
                    {
                        Ok(message) => (Some(message.sequence), message.message),
                        Err(_) => match ServerMessage::decode_payload(&payload) {
                            Ok(message) => (None, message),
                            Err(error) => {
                                let _ = event_tx.send(NetworkEvent::Status(format!(
                                    "typed wire server message rejected: {error}; reconnecting"
                                )));
                                break 'connection;
                            }
                        },
                    };
                    let session_input = match sequence {
                        Some(sequence) => SessionInput::Sequenced {
                            sequence,
                            message: message.clone(),
                        },
                        None => SessionInput::Server(message.clone()),
                    };
                    let session_outputs = session.handle(session_input);
                    if session_outputs
                        .iter()
                        .any(|output| matches!(output, SessionOutput::Rejected { .. }))
                    {
                        if let ServerMessage::Error { message } = &message {
                            let _ = event_tx.send(NetworkEvent::Status(format!(
                                "typed wire session rejected message: {message}"
                            )));
                        }
                    }
                    queue_session_outputs(&mut outgoing, session_outputs);
                    if let ServerMessage::CharacterSelected {
                        character_id,
                        name,
                        role,
                    } = &message
                    {
                        println!("server selected character {character_id} {name} role={role:?}");
                    }
                    if let ServerMessage::Connected { player_id, role } = &message {
                        println!("server connected player {player_id} role={role:?}");
                    }
                    if matches!(message, ServerMessage::CharacterList { ref characters, .. } if characters.is_empty())
                    {
                        let _ = event_tx.send(NetworkEvent::Status(
                            "authenticated account has no characters".to_owned(),
                        ));
                        continue;
                    }
                    if let ServerMessage::CharacterList { characters, .. } = &message
                        && let Some(character_id) = preferred_character_id
                        && characters
                            .iter()
                            .any(|character| character.character_id == character_id)
                    {
                        queue_session_outputs(
                            &mut outgoing,
                            session.handle(SessionInput::SelectCharacter(character_id)),
                        );
                        println!("selecting startup character {character_id}");
                        let _ = event_tx.send(NetworkEvent::Status(format!(
                            "selecting startup character {character_id}"
                        )));
                    }
                    if matches!(message, ServerMessage::Snapshot(_)) {
                        while let Some(command) = deferred_commands.pop_front() {
                            queue_session_outputs(
                                &mut outgoing,
                                session
                                    .handle(SessionInput::Intent(client_command_to_wire(command))),
                            );
                        }
                    }
                    if event_tx.send(NetworkEvent::ServerMessage(message)).is_err() {
                        return;
                    }
                }

                thread::sleep(Duration::from_millis(5));
            }
            let _ = session.handle(SessionInput::Disconnected);
            deferred_commands.clear();
            thread::sleep(RECONNECT_DELAY);
        }
    });
}

pub(crate) fn client_command_to_wire(command: ClientCommand) -> WireCommand {
    match command {
        ClientCommand::SelectCharacter { character_id } => {
            WireCommand::SelectCharacter { character_id }
        }
        ClientCommand::Move { dx, dy } => WireCommand::Move { dx, dy },
        ClientCommand::Target(EntityId(target_id)) => WireCommand::SelectTarget { target_id },
        ClientCommand::Attack => WireCommand::BasicAttack,
        ClientCommand::Taunt => WireCommand::Taunt,
        ClientCommand::Heal(EntityId(target_id)) => WireCommand::Heal { target_id },
        ClientCommand::ListVendor(EntityId(vendor_id)) => WireCommand::ListVendor { vendor_id },
        ClientCommand::ListQuestOffers(EntityId(npc_id)) => WireCommand::ListQuestOffers { npc_id },
        ClientCommand::BuyItem {
            vendor_id: EntityId(vendor_id),
            item_id,
            quantity,
        } => WireCommand::BuyItem {
            vendor_id,
            item_id: item_id.0,
            quantity,
        },
        ClientCommand::AcceptQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => WireCommand::AcceptQuest {
            npc_id,
            quest_id: quest_id.0,
        },
        ClientCommand::TurnInQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => WireCommand::TurnInQuest {
            npc_id,
            quest_id: quest_id.0,
        },
        ClientCommand::Loot(EntityId(enemy_id)) => WireCommand::LootEnemy { enemy_id },
        ClientCommand::InvitePartyMember(EntityId(target_id)) => {
            WireCommand::InvitePartyMember { target_id }
        }
        ClientCommand::AcceptPartyInvite(party_id) => WireCommand::AcceptPartyInvite { party_id },
    }
}

pub(crate) fn queue_wire_command(outgoing: &mut VecDeque<Vec<u8>>, command: WireCommand) {
    if outgoing.len() >= MAX_OUTGOING_LINES {
        return;
    }
    let Ok(payload) = command.encode_payload() else {
        return;
    };
    let Ok(frame) =
        Envelope::new(MessageKind::Command, payload).and_then(|envelope| envelope.encode())
    else {
        return;
    };
    outgoing.push_back(frame);
}

pub(crate) fn queue_session_outputs(outgoing: &mut VecDeque<Vec<u8>>, outputs: Vec<SessionOutput>) {
    for output in outputs {
        if let SessionOutput::Send(command) = output {
            queue_wire_command(outgoing, command);
        }
    }
}

#[cfg(test)]
pub(crate) fn queue_command_line(outgoing: &mut VecDeque<Vec<u8>>, command: &str) {
    let mut line = command.as_bytes().to_vec();
    line.push(b'\n');
    outgoing.push_back(line);
}

#[cfg(test)]
pub(crate) fn queue_command_bounded(outgoing: &mut VecDeque<Vec<u8>>, command: ClientCommand) {
    if outgoing.len() >= MAX_OUTGOING_LINES {
        return;
    }
    match command {
        // Character selection is a typed-wire session transition. The legacy
        // development line protocol has no account or character catalog.
        ClientCommand::SelectCharacter { .. } => {}
        ClientCommand::Move { dx, dy } => queue_command_line(outgoing, &format!("move {dx} {dy}")),
        ClientCommand::Target(EntityId(id)) => {
            queue_command_line(outgoing, &format!("target {id}"));
        }
        ClientCommand::Attack => queue_command_line(outgoing, "attack"),
        ClientCommand::Taunt => queue_command_line(outgoing, "taunt"),
        ClientCommand::Heal(EntityId(id)) => {
            queue_command_line(outgoing, &format!("heal {id}"));
        }
        ClientCommand::ListVendor(EntityId(id)) => {
            queue_command_line(outgoing, &format!("vendor {id}"));
        }
        ClientCommand::ListQuestOffers(EntityId(id)) => {
            queue_command_line(outgoing, &format!("quest-offers {id}"));
        }
        ClientCommand::BuyItem {
            vendor_id: EntityId(vendor_id),
            item_id,
            quantity,
        } => queue_command_line(outgoing, &format!("buy {vendor_id} {item_id} {quantity}")),
        ClientCommand::AcceptQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => queue_command_line(outgoing, &format!("accept-quest {npc_id} {quest_id}")),
        ClientCommand::TurnInQuest {
            npc_id: EntityId(npc_id),
            quest_id,
        } => queue_command_line(outgoing, &format!("turn-in-quest {npc_id} {quest_id}")),
        ClientCommand::Loot(EntityId(enemy_id)) => {
            queue_command_line(outgoing, &format!("loot {enemy_id}"));
        }
        ClientCommand::InvitePartyMember(_) | ClientCommand::AcceptPartyInvite(_) => {}
    }
}

pub(crate) fn flush_outgoing(stream: &mut TcpStream, outgoing: &mut VecDeque<Vec<u8>>) -> bool {
    while let Some(line) = outgoing.front_mut() {
        match stream.write(line) {
            Ok(0) => return false,
            Ok(bytes_written) => {
                line.drain(..bytes_written);
                if line.is_empty() {
                    outgoing.pop_front();
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(_) => return false,
        }
    }
    true
}
