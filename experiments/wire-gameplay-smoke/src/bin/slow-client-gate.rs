//! Real-TCP gate for bounded typed output under a non-reading client.

use mmorpg_content::starter_catalog;
use mmorpg_wire::{
    ClientCommand, Envelope, MessageKind, SequencedServerMessage, ServerMessage, decode_one,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

const DEFAULT_ADDRESS: &str = "127.0.0.1:4501";
const FLOOD_BATCHES: usize = 400;
const COMMANDS_PER_BATCH: usize = 32;

struct RawClient {
    stream: TcpStream,
}

impl RawClient {
    fn connect(address: &str) -> Self {
        let socket_address = address
            .to_socket_addrs()
            .expect("socket address should resolve")
            .next()
            .expect("socket address should contain one address");
        let socket = Socket::new(
            Domain::for_address(socket_address),
            Type::STREAM,
            Some(Protocol::TCP),
        )
        .expect("slow-client socket should be created");
        socket
            .set_recv_buffer_size(1024)
            .expect("slow-client receive buffer should be bounded");
        socket
            .connect(&socket_address.into())
            .unwrap_or_else(|error| panic!("cannot connect to {address}: {error}"));
        let stream: TcpStream = socket.into();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("set write timeout");
        Self { stream }
    }

    fn send(&mut self, command: &ClientCommand) {
        let payload = command.encode_payload().expect("command should encode");
        let frame = Envelope::new(MessageKind::Command, payload)
            .expect("command envelope should encode")
            .encode()
            .expect("command frame should encode");
        self.stream
            .write_all(&frame)
            .expect("command frame should send");
    }

    fn read_message(&mut self) -> ServerMessage {
        let mut prefix = [0_u8; 4];
        self.stream
            .read_exact(&mut prefix)
            .expect("server frame prefix should arrive");
        let body_length = u32::from_be_bytes(prefix) as usize;
        let mut frame = Vec::with_capacity(4 + body_length);
        frame.extend_from_slice(&prefix);
        frame.resize(4 + body_length, 0);
        self.stream
            .read_exact(&mut frame[4..])
            .expect("server frame body should arrive");
        let decoded = decode_one(&frame).expect("server frame should decode");
        assert_eq!(decoded.envelope.kind, MessageKind::Event);
        match SequencedServerMessage::decode_payload(&decoded.envelope.payload) {
            Ok(message) => message.message,
            Err(_) => ServerMessage::decode_payload(&decoded.envelope.payload)
                .expect("server message should decode"),
        }
    }

    fn expect(&mut self, predicate: impl Fn(&ServerMessage) -> bool) {
        loop {
            let message = self.read_message();
            if predicate(&message) {
                return;
            }
        }
    }
}

fn enter_world(address: &str, character_id: u64) -> RawClient {
    let mut client = RawClient::connect(address);
    client.expect(|message| matches!(message, ServerMessage::Welcome { .. }));
    client.send(&ClientCommand::Authenticate {
        token: "dev-local".to_owned(),
    });
    client.expect(|message| matches!(message, ServerMessage::Authenticated { .. }));
    client.send(&ClientCommand::ListCharacters);
    client.expect(|message| matches!(message, ServerMessage::CharacterList { .. }));
    client.send(&ClientCommand::SelectCharacter { character_id });
    client.expect(|message| {
        matches!(message, ServerMessage::CharacterSelected { character_id: id, .. } if *id == character_id)
    });
    client.send(&ClientCommand::ContentDigest {
        digest: starter_catalog().content_digest(),
    });
    client.expect(|message| matches!(message, ServerMessage::ContentAccepted { .. }));
    client.send(&ClientCommand::EnterWorld);
    client.expect(|message| matches!(message, ServerMessage::Connected { .. }));
    client
}

fn main() {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_owned());
    let mut slow = enter_world(&address, 1);
    let mut healthy = enter_world(&address, 2);
    let snapshot_frame = {
        let payload = ClientCommand::Snapshot
            .encode_payload()
            .expect("snapshot should encode");
        Envelope::new(MessageKind::Command, payload)
            .expect("snapshot envelope should encode")
            .encode()
            .expect("snapshot frame should encode")
    };

    let mut sent = 0_usize;
    'flood: for _ in 0..FLOOD_BATCHES {
        for _ in 0..COMMANDS_PER_BATCH {
            if slow.stream.write_all(&snapshot_frame).is_err() {
                break 'flood;
            }
            sent += 1;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(sent >= 128, "slow-client flood sent only {sent} commands");

    healthy.send(&ClientCommand::Snapshot);
    healthy.expect(|message| matches!(message, ServerMessage::Snapshot(_)));
    println!("slow-client gate: healthy peer remained responsive (flood_commands={sent})");
}
