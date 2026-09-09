use mmorpg_content::starter_catalog;
use mmorpg_wire::{
    CharacterSummary, ClientCommand, Envelope, MessageKind, SequencedServerMessage, ServerMessage,
    WorldSnapshot, decode_one,
};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_ADDRESS: &str = "127.0.0.1:4000";
const DEV_AUTH_TOKEN: &str = "dev-local";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (address, character_id, token) = parse_args()?;
    let mut stream = TcpStream::connect_timeout(
        &address
            .to_socket_addrs()?
            .next()
            .ok_or("diagnostic address did not resolve")?,
        Duration::from_secs(5),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    expect_welcome(&mut stream)?;
    send(&mut stream, ClientCommand::Authenticate { token })?;
    let ServerMessage::Authenticated { account_id, .. } = read(&mut stream)? else {
        return Err("server did not authenticate the diagnostic session".into());
    };
    println!("authenticated account={account_id}");

    send(&mut stream, ClientCommand::ListCharacters)?;
    let ServerMessage::CharacterList { characters, .. } = read(&mut stream)? else {
        return Err("server did not return a character list".into());
    };
    let character = choose_character(&characters, character_id)?;
    println!(
        "character id={} name={} role={:?}",
        character.character_id, character.name, character.role
    );

    send(
        &mut stream,
        ClientCommand::SelectCharacter {
            character_id: character.character_id,
        },
    )?;
    expect_character_selected(&mut stream, character.character_id)?;

    let digest = starter_catalog().content_digest();
    send(&mut stream, ClientCommand::ContentDigest { digest })?;
    match read(&mut stream)? {
        ServerMessage::ContentAccepted { digest: accepted } if accepted == digest => {}
        ServerMessage::ContentMismatch { .. } => {
            return Err("server rejected the diagnostic content digest".into());
        }
        message => return Err(format!("unexpected content response: {message:?}").into()),
    }

    send(&mut stream, ClientCommand::EnterWorld)?;
    let ServerMessage::Connected { player_id, .. } = read(&mut stream)? else {
        return Err("server did not bind the selected character".into());
    };
    send(&mut stream, ClientCommand::Snapshot)?;
    let snapshot = read_until_snapshot(&mut stream)?;
    print_snapshot(player_id, &snapshot);
    Ok(())
}

fn parse_args() -> Result<(String, u64, String), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let address = args.next().unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let mut character_id = 1;
    let mut token = DEV_AUTH_TOKEN.to_owned();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--character-id" => {
                character_id = args
                    .next()
                    .ok_or("--character-id requires an integer")?
                    .parse()?;
                if character_id == 0 {
                    return Err("--character-id must be nonzero".into());
                }
            }
            "--token" => token = args.next().ok_or("--token requires a value")?,
            _ => return Err(format!("unknown argument '{argument}'").into()),
        }
    }
    Ok((address, character_id, token))
}

fn choose_character(
    characters: &[CharacterSummary],
    requested_id: u64,
) -> Result<&CharacterSummary, Box<dyn std::error::Error>> {
    characters
        .iter()
        .find(|character| character.character_id == requested_id)
        .ok_or_else(|| format!("character {requested_id} is not available").into())
}

fn expect_welcome(stream: &mut TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    if !matches!(read(stream)?, ServerMessage::Welcome { .. }) {
        return Err("server did not send a welcome".into());
    }
    Ok(())
}

fn expect_character_selected(
    stream: &mut TcpStream,
    character_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    match read(stream)? {
        ServerMessage::CharacterSelected {
            character_id: selected,
            ..
        } if selected == character_id => Ok(()),
        message => Err(format!("unexpected character response: {message:?}").into()),
    }
}

fn send(stream: &mut TcpStream, command: ClientCommand) -> Result<(), Box<dyn std::error::Error>> {
    let payload = command.encode_payload()?;
    let frame = Envelope::new(MessageKind::Command, payload)?.encode()?;
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

fn read(stream: &mut TcpStream) -> Result<ServerMessage, Box<dyn std::error::Error>> {
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix)?;
    let body_length = u32::from_be_bytes(prefix) as usize;
    if body_length > mmorpg_wire::MAX_FRAME_SIZE {
        return Err("server frame exceeds the diagnostic bound".into());
    }
    let mut frame = prefix.to_vec();
    frame.resize(prefix.len() + body_length, 0);
    stream.read_exact(&mut frame[prefix.len()..])?;
    let decoded = decode_one(&frame)?;
    if decoded.envelope.kind != MessageKind::Event {
        return Err("server sent a non-event envelope".into());
    }
    Ok(
        SequencedServerMessage::decode_payload(&decoded.envelope.payload)
            .map(|message| message.message)
            .or_else(|_| ServerMessage::decode_payload(&decoded.envelope.payload))?,
    )
}

fn read_until_snapshot(
    stream: &mut TcpStream,
) -> Result<WorldSnapshot, Box<dyn std::error::Error>> {
    loop {
        match read(stream)? {
            ServerMessage::Snapshot(snapshot) => return Ok(snapshot),
            ServerMessage::Event(_) | ServerMessage::SkippedEvent { .. } => {}
            ServerMessage::Error { message } => {
                return Err(format!("server rejected bootstrap: {message}").into());
            }
            message => return Err(format!("unexpected bootstrap response: {message:?}").into()),
        }
    }
}

fn print_snapshot(player_id: u64, snapshot: &WorldSnapshot) {
    println!(
        "ready player={} tick={} players={} npcs={} enemies={} vendors={}",
        player_id,
        snapshot.tick,
        snapshot.player_count,
        snapshot.npc_count,
        snapshot.enemy_count,
        snapshot.vendor_count
    );
}
