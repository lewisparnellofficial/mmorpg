//! Bounded transports for the temporary development protocol and its
//! versioned-wire bridge.
//!
//! This crate owns only blocking socket I/O. It sends typed command lines and
//! returns bounded diagnostic lines; it does not parse the server's
//! human-readable output into authoritative client state. Production client
//! networking belongs behind the versioned machine-readable protocol. The
//! [`WireConnection`] bridge below provides bounded framing while its payload
//! still carries an existing validated command/event line.

use std::fmt;
use std::io::{self, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use mmorpg_client_protocol::ProtocolLine;
use mmorpg_wire::{ClientCommand, Envelope, MAX_FRAME_SIZE, MessageKind, decode_one};

pub const DEFAULT_MAX_LINE_BYTES: usize = 8 * 1024;
pub const DEFAULT_MAX_WIRE_FRAME_SIZE: usize = MAX_FRAME_SIZE;

/// Configuration for a temporary development-protocol connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionConfig {
    pub address: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub max_line_bytes: usize,
}

impl ConnectionConfig {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            max_line_bytes: DEFAULT_MAX_LINE_BYTES,
        }
    }

    fn validate(&self) -> Result<(), TransportError> {
        if self.address.trim().is_empty() {
            return Err(TransportError::InvalidConfig("address must not be empty"));
        }
        if self.connect_timeout.is_zero()
            || self.read_timeout.is_zero()
            || self.write_timeout.is_zero()
        {
            return Err(TransportError::InvalidConfig(
                "socket timeouts must be greater than zero",
            ));
        }
        if self.max_line_bytes == 0 {
            return Err(TransportError::InvalidConfig(
                "maximum line size must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Errors returned by the bounded development transport.
#[derive(Debug)]
pub enum TransportError {
    InvalidConfig(&'static str),
    InvalidCommandLine,
    LineTooLong { maximum: usize },
    InvalidUtf8,
    ConnectionClosed,
    Io(io::Error),
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid connection config: {message}")
            }
            Self::InvalidCommandLine => {
                write!(formatter, "command line contains a line terminator")
            }
            Self::LineTooLong { maximum } => {
                write!(formatter, "server line exceeds {maximum} bytes")
            }
            Self::InvalidUtf8 => write!(formatter, "server line is not valid UTF-8"),
            Self::ConnectionClosed => write!(formatter, "server closed the connection mid-line"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A blocking connection to the temporary development server.
pub struct DevelopmentConnection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    max_line_bytes: usize,
}

impl DevelopmentConnection {
    pub fn connect(config: &ConnectionConfig) -> Result<Self, TransportError> {
        config.validate()?;
        let mut addresses = config.address.to_socket_addrs()?;
        let address = addresses
            .next()
            .ok_or(TransportError::InvalidConfig("address did not resolve"))?;
        let writer = TcpStream::connect_timeout(&address, config.connect_timeout)?;
        writer.set_read_timeout(Some(config.read_timeout))?;
        writer.set_write_timeout(Some(config.write_timeout))?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Self {
            writer,
            reader,
            max_line_bytes: config.max_line_bytes,
        })
    }

    /// Sends one typed command and appends exactly one line terminator.
    ///
    /// The configured maximum applies to the command payload, excluding the
    /// terminator. Rejecting before writing keeps an oversized command from
    /// being partially emitted to the server.
    pub fn send(&mut self, command: &ProtocolLine) -> Result<(), TransportError> {
        if command.as_str().contains(['\r', '\n']) {
            return Err(TransportError::InvalidCommandLine);
        }
        if command.as_str().len() > self.max_line_bytes {
            return Err(TransportError::LineTooLong {
                maximum: self.max_line_bytes,
            });
        }
        self.writer.write_all(command.as_str().as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }

    /// Reads one bounded diagnostic line, without its line terminator.
    ///
    /// The returned text is intentionally unparsed. Callers must not treat
    /// the temporary server's human-readable responses as authoritative
    /// events or snapshots.
    pub fn read_line(&mut self) -> Result<Option<String>, TransportError> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            match self.reader.read(&mut byte)? {
                0 if bytes.is_empty() => return Ok(None),
                0 => return Err(TransportError::ConnectionClosed),
                1 if byte[0] == b'\n' => {
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    return String::from_utf8(bytes)
                        .map(Some)
                        .map_err(|_| TransportError::InvalidUtf8);
                }
                1 => {
                    if bytes.len() >= self.max_line_bytes {
                        return Err(TransportError::LineTooLong {
                            maximum: self.max_line_bytes,
                        });
                    }
                    bytes.push(byte[0]);
                }
                _ => unreachable!("a one-byte read cannot return more than one byte"),
            }
        }
    }
}

/// Configuration for a bounded versioned-wire connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireConnectionConfig {
    pub address: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub max_frame_size: usize,
}

impl WireConnectionConfig {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(5),
            max_frame_size: DEFAULT_MAX_WIRE_FRAME_SIZE,
        }
    }

    fn validate(&self) -> Result<(), WireTransportError> {
        if self.address.trim().is_empty() {
            return Err(WireTransportError::InvalidConfig(
                "address must not be empty",
            ));
        }
        if self.connect_timeout.is_zero()
            || self.read_timeout.is_zero()
            || self.write_timeout.is_zero()
        {
            return Err(WireTransportError::InvalidConfig(
                "socket timeouts must be greater than zero",
            ));
        }
        if self.max_frame_size < mmorpg_wire::HEADER_LEN + 1 || self.max_frame_size > MAX_FRAME_SIZE
        {
            return Err(WireTransportError::InvalidConfig(
                "maximum frame size must fit the wire bounds and carry a payload",
            ));
        }
        Ok(())
    }
}

/// Errors returned by the versioned-wire transport bridge.
#[derive(Debug)]
pub enum WireTransportError {
    InvalidConfig(&'static str),
    FrameTooLarge { length: usize, maximum: usize },
    UnexpectedMessageKind(MessageKind),
    InvalidUtf8,
    ConnectionClosed,
    Io(io::Error),
    Command(mmorpg_wire::CommandCodecError),
    Encode(mmorpg_wire::EncodeError),
    Decode(mmorpg_wire::DecodeError),
}

impl fmt::Display for WireTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(formatter, "invalid wire config: {message}"),
            Self::FrameTooLarge { length, maximum } => {
                write!(formatter, "wire frame length {length} exceeds {maximum}")
            }
            Self::UnexpectedMessageKind(kind) => {
                write!(formatter, "expected an event envelope, received {kind:?}")
            }
            Self::InvalidUtf8 => write!(formatter, "wire event payload is not valid UTF-8"),
            Self::ConnectionClosed => write!(formatter, "server closed the wire connection"),
            Self::Io(error) => error.fmt(formatter),
            Self::Command(error) => write!(formatter, "invalid typed command: {error}"),
            Self::Encode(error) => write!(formatter, "cannot encode wire envelope: {error}"),
            Self::Decode(error) => write!(formatter, "cannot decode wire envelope: {error}"),
        }
    }
}

impl std::error::Error for WireTransportError {}

impl From<io::Error> for WireTransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<mmorpg_wire::EncodeError> for WireTransportError {
    fn from(error: mmorpg_wire::EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<mmorpg_wire::CommandCodecError> for WireTransportError {
    fn from(error: mmorpg_wire::CommandCodecError) -> Self {
        Self::Command(error)
    }
}

impl From<mmorpg_wire::DecodeError> for WireTransportError {
    fn from(error: mmorpg_wire::DecodeError) -> Self {
        Self::Decode(error)
    }
}

/// A blocking transport bridge for versioned command/event envelopes.
///
/// The bridge supports both validated temporary line payloads and the first
/// typed client-command payload schema. This lets callers prove framing,
/// bounded I/O, and message-kind separation while the server session adapter
/// is still being migrated.
pub struct WireConnection {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    max_frame_size: usize,
}

impl WireConnection {
    pub fn connect(config: &WireConnectionConfig) -> Result<Self, WireTransportError> {
        config.validate()?;
        let mut addresses = config.address.to_socket_addrs()?;
        let address = addresses
            .next()
            .ok_or(WireTransportError::InvalidConfig("address did not resolve"))?;
        let writer = TcpStream::connect_timeout(&address, config.connect_timeout)?;
        writer.set_read_timeout(Some(config.read_timeout))?;
        writer.set_write_timeout(Some(config.write_timeout))?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Self {
            writer,
            reader,
            max_frame_size: config.max_frame_size,
        })
    }

    /// Sends one validated temporary command inside a versioned envelope.
    pub fn send_command(&mut self, command: &ProtocolLine) -> Result<(), WireTransportError> {
        let envelope = Envelope::new(MessageKind::Command, command.as_str().as_bytes().to_vec())?;
        self.write_envelope(&envelope)
    }

    /// Sends one typed application command inside a versioned envelope.
    pub fn send_typed_command(
        &mut self,
        command: &ClientCommand,
    ) -> Result<(), WireTransportError> {
        let envelope = Envelope::new(MessageKind::Command, command.encode_payload()?)?;
        self.write_envelope(&envelope)
    }

    /// Reads one event envelope and returns its UTF-8 temporary-protocol
    /// payload without interpreting its wording as authoritative state.
    pub fn read_event_payload(&mut self) -> Result<String, WireTransportError> {
        let frame = self.read_frame()?;
        let decoded = decode_one(&frame)?;
        if decoded.envelope.kind != MessageKind::Event {
            return Err(WireTransportError::UnexpectedMessageKind(
                decoded.envelope.kind,
            ));
        }
        String::from_utf8(decoded.envelope.payload).map_err(|_| WireTransportError::InvalidUtf8)
    }

    fn write_envelope(&mut self, envelope: &Envelope) -> Result<(), WireTransportError> {
        let encoded = envelope.encode()?;
        let body_length = encoded
            .len()
            .checked_sub(mmorpg_wire::LENGTH_PREFIX_LEN)
            .expect("wire envelope includes a length prefix");
        if body_length > self.max_frame_size {
            return Err(WireTransportError::FrameTooLarge {
                length: body_length,
                maximum: self.max_frame_size,
            });
        }
        self.writer.write_all(&encoded)?;
        self.writer.flush()?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Vec<u8>, WireTransportError> {
        let mut prefix = [0_u8; mmorpg_wire::LENGTH_PREFIX_LEN];
        match self.reader.read_exact(&mut prefix) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(WireTransportError::ConnectionClosed);
            }
            Err(error) => return Err(error.into()),
        }
        let body_length = u32::from_be_bytes(prefix) as usize;
        if body_length > self.max_frame_size {
            return Err(WireTransportError::FrameTooLarge {
                length: body_length,
                maximum: self.max_frame_size,
            });
        }
        let mut frame = prefix.to_vec();
        frame.resize(prefix.len() + body_length, 0);
        self.reader.read_exact(&mut frame[prefix.len()..])?;
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use mmorpg_client_protocol::{CommandLine, Role};

    use super::*;

    #[test]
    fn loopback_connection_sends_typed_line_and_reads_bounded_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            stream
                .write_all(b"WELCOME mmorpg-server\n")
                .expect("write greeting");
            let mut command = String::new();
            BufReader::new(stream)
                .read_line(&mut command)
                .expect("read command");
            assert_eq!(command, "state\n");
        });

        let mut connection =
            DevelopmentConnection::connect(&ConnectionConfig::new(address.to_string()))
                .expect("connect to loopback");
        assert_eq!(
            connection.read_line().expect("read greeting"),
            Some("WELCOME mmorpg-server".to_owned())
        );
        connection
            .send(&CommandLine::state())
            .expect("send state command");
        server.join().expect("server thread");
    }

    #[test]
    fn rejects_invalid_configuration_and_oversized_lines() {
        let mut empty = ConnectionConfig::new("");
        assert!(matches!(
            DevelopmentConnection::connect(&empty),
            Err(TransportError::InvalidConfig(_))
        ));

        empty.address = "127.0.0.1:1".to_owned();
        empty.max_line_bytes = 0;
        assert!(matches!(
            DevelopmentConnection::connect(&empty),
            Err(TransportError::InvalidConfig(_))
        ));
    }

    #[test]
    fn rejects_a_server_line_that_exceeds_the_configured_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            stream.write_all(b"12345\n").expect("write oversized line");
        });

        let mut config = ConnectionConfig::new(address.to_string());
        config.max_line_bytes = 4;
        let mut connection = DevelopmentConnection::connect(&config).expect("connect to loopback");
        assert!(matches!(
            connection.read_line(),
            Err(TransportError::LineTooLong { maximum: 4 })
        ));
        server.join().expect("server thread");
    }

    #[test]
    fn rejects_an_oversized_command_before_writing() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .expect("set server read timeout");
            let mut bytes = [0_u8; 1];
            assert_eq!(stream.read(&mut bytes).expect("read from client"), 0);
        });

        let mut config = ConnectionConfig::new(address.to_string());
        config.max_line_bytes = 4;
        let mut connection = DevelopmentConnection::connect(&config).expect("connect to loopback");
        let command = CommandLine::connect("A", Role::Tank).unwrap();
        assert!(matches!(
            connection.send(&command),
            Err(TransportError::LineTooLong { maximum: 4 })
        ));
        drop(connection);
        server.join().expect("server thread");
    }

    #[test]
    fn wire_connection_round_trips_a_framed_command_and_event_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            let mut prefix = [0_u8; mmorpg_wire::LENGTH_PREFIX_LEN];
            stream.read_exact(&mut prefix).expect("read frame prefix");
            let body_length = u32::from_be_bytes(prefix) as usize;
            let mut frame = prefix.to_vec();
            frame.resize(prefix.len() + body_length, 0);
            stream
                .read_exact(&mut frame[prefix.len()..])
                .expect("read frame body");
            let decoded = mmorpg_wire::decode_one(&frame).expect("decode command envelope");
            assert_eq!(decoded.envelope.kind, MessageKind::Command);
            assert_eq!(
                ClientCommand::decode_payload(&decoded.envelope.payload),
                Ok(ClientCommand::Snapshot)
            );

            let response = Envelope::new(MessageKind::Event, b"EVENT ready".to_vec())
                .expect("construct event envelope")
                .encode()
                .expect("encode event envelope");
            stream.write_all(&response).expect("write event envelope");
        });

        let mut connection =
            WireConnection::connect(&WireConnectionConfig::new(address.to_string()))
                .expect("connect to loopback");
        connection
            .send_typed_command(&ClientCommand::Snapshot)
            .expect("send framed command");
        assert_eq!(
            connection.read_event_payload().expect("read framed event"),
            "EVENT ready"
        );
        server.join().expect("server thread");
    }

    #[test]
    fn wire_connection_rejects_invalid_limits_and_wrong_message_kind() {
        let mut invalid = WireConnectionConfig::new("127.0.0.1:1");
        invalid.max_frame_size = mmorpg_wire::HEADER_LEN;
        assert!(matches!(
            WireConnection::connect(&invalid),
            Err(WireTransportError::InvalidConfig(_))
        ));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            let response = Envelope::new(MessageKind::Command, b"not-an-event".to_vec())
                .expect("construct command envelope")
                .encode()
                .expect("encode command envelope");
            stream.write_all(&response).expect("write command envelope");
        });

        let mut connection =
            WireConnection::connect(&WireConnectionConfig::new(address.to_string()))
                .expect("connect to loopback");
        assert!(matches!(
            connection.read_event_payload(),
            Err(WireTransportError::UnexpectedMessageKind(
                MessageKind::Command
            ))
        ));
        server.join().expect("server thread");
    }
}
