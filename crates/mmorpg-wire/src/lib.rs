//! A small, socket-independent wire-envelope prototype.
//!
//! The envelope deliberately treats the payload as opaque bytes. Command and
//! event schemas can be added above this layer without making the framing
//! implementation depend on a transport, serializer, or simulation crate.

use std::fmt;

const MAX_COMMAND_NAME_BYTES: usize = 64;

/// The only protocol version understood by this prototype.
pub const PROTOCOL_VERSION: u16 = 1;

/// Four bytes at the beginning of every envelope body.
pub const MAGIC: [u8; 4] = *b"MMOW";

/// The length prefix is a big-endian `u32`.
pub const LENGTH_PREFIX_LEN: usize = 4;

/// The body header consists of magic, version, kind, flags, and payload length.
pub const HEADER_LEN: usize = 12;

/// Maximum number of bytes in an envelope body, excluding its length prefix.
pub const MAX_FRAME_SIZE: usize = 64 * 1024;

const MAX_PAYLOAD_SIZE: usize = MAX_FRAME_SIZE - HEADER_LEN;

/// The kind of application message carried by an envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageKind {
    Command = 1,
    Event = 2,
}

/// Role values used by the first typed command payload schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RoleCode {
    Tank = 1,
    Healer = 2,
    DamageDealer = 3,
}

impl TryFrom<u8> for RoleCode {
    type Error = CommandCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Tank),
            2 => Ok(Self::Healer),
            3 => Ok(Self::DamageDealer),
            other => Err(CommandCodecError::InvalidEnum {
                field: "role",
                value: other,
            }),
        }
    }
}

/// Typed client intent payload carried inside a command envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientCommand {
    Join {
        name: String,
        role: RoleCode,
    },
    Move {
        dx: f32,
        dy: f32,
    },
    SelectTarget {
        target_id: u64,
    },
    BasicAttack,
    ListVendor {
        vendor_id: u64,
    },
    BuyItem {
        vendor_id: u64,
        item_id: u32,
        quantity: u32,
    },
    LootEnemy {
        enemy_id: u64,
    },
    ListQuestOffers {
        npc_id: u64,
    },
    AcceptQuest {
        npc_id: u64,
        quest_id: u32,
    },
    TurnInQuest {
        npc_id: u64,
        quest_id: u32,
    },
    Snapshot,
}

impl ClientCommand {
    /// Encodes one command payload without the surrounding wire envelope.
    pub fn encode_payload(&self) -> Result<Vec<u8>, CommandCodecError> {
        let mut payload = Vec::new();
        match self {
            Self::Join { name, role } => {
                let name = validate_command_name(name)?;
                payload.push(1);
                put_string(&mut payload, name);
                payload.push(*role as u8);
            }
            Self::Move { dx, dy } => {
                validate_finite(*dx, "dx")?;
                validate_finite(*dy, "dy")?;
                payload.push(2);
                payload.extend_from_slice(&dx.to_bits().to_be_bytes());
                payload.extend_from_slice(&dy.to_bits().to_be_bytes());
            }
            Self::SelectTarget { target_id } => {
                put_nonzero_u64(&mut payload, 3, *target_id, "target_id")?;
            }
            Self::BasicAttack => payload.push(4),
            Self::ListVendor { vendor_id } => {
                put_nonzero_u64(&mut payload, 5, *vendor_id, "vendor_id")?;
            }
            Self::BuyItem {
                vendor_id,
                item_id,
                quantity,
            } => {
                put_nonzero_u64(&mut payload, 6, *vendor_id, "vendor_id")?;
                put_nonzero_u32(&mut payload, *item_id, "item_id")?;
                put_nonzero_u32(&mut payload, *quantity, "quantity")?;
            }
            Self::LootEnemy { enemy_id } => {
                put_nonzero_u64(&mut payload, 7, *enemy_id, "enemy_id")?;
            }
            Self::ListQuestOffers { npc_id } => {
                put_nonzero_u64(&mut payload, 8, *npc_id, "npc_id")?;
            }
            Self::AcceptQuest { npc_id, quest_id } => {
                put_nonzero_u64(&mut payload, 9, *npc_id, "npc_id")?;
                put_nonzero_u32(&mut payload, *quest_id, "quest_id")?;
            }
            Self::TurnInQuest { npc_id, quest_id } => {
                put_nonzero_u64(&mut payload, 10, *npc_id, "npc_id")?;
                put_nonzero_u32(&mut payload, *quest_id, "quest_id")?;
            }
            Self::Snapshot => payload.push(11),
        }
        Ok(payload)
    }

    /// Decodes exactly one typed command payload.
    pub fn decode_payload(payload: &[u8]) -> Result<Self, CommandCodecError> {
        if payload.is_empty() {
            return Err(CommandCodecError::Empty);
        }
        let mut decoder = CommandDecoder { payload, offset: 0 };
        let command = match decoder.take_u8()? {
            1 => Self::Join {
                name: decoder.take_string("name")?,
                role: decoder.take_u8()?.try_into()?,
            },
            2 => Self::Move {
                dx: f32::from_bits(decoder.take_u32("dx")?),
                dy: f32::from_bits(decoder.take_u32("dy")?),
            },
            3 => Self::SelectTarget {
                target_id: decoder.take_nonzero_u64("target_id")?,
            },
            4 => Self::BasicAttack,
            5 => Self::ListVendor {
                vendor_id: decoder.take_nonzero_u64("vendor_id")?,
            },
            6 => Self::BuyItem {
                vendor_id: decoder.take_nonzero_u64("vendor_id")?,
                item_id: decoder.take_nonzero_u32("item_id")?,
                quantity: decoder.take_nonzero_u32("quantity")?,
            },
            7 => Self::LootEnemy {
                enemy_id: decoder.take_nonzero_u64("enemy_id")?,
            },
            8 => Self::ListQuestOffers {
                npc_id: decoder.take_nonzero_u64("npc_id")?,
            },
            9 => Self::AcceptQuest {
                npc_id: decoder.take_nonzero_u64("npc_id")?,
                quest_id: decoder.take_nonzero_u32("quest_id")?,
            },
            10 => Self::TurnInQuest {
                npc_id: decoder.take_nonzero_u64("npc_id")?,
                quest_id: decoder.take_nonzero_u32("quest_id")?,
            },
            11 => Self::Snapshot,
            opcode => return Err(CommandCodecError::UnknownOpcode(opcode)),
        };
        if decoder.offset != payload.len() {
            return Err(CommandCodecError::TrailingBytes {
                count: payload.len() - decoder.offset,
            });
        }
        if let Self::Move { dx, dy } = command {
            validate_finite(dx, "dx")?;
            validate_finite(dy, "dy")?;
            return Ok(Self::Move { dx, dy });
        }
        Ok(command)
    }
}

fn validate_command_name(name: &str) -> Result<&str, CommandCodecError> {
    if name.is_empty() || name.len() > MAX_COMMAND_NAME_BYTES || name.contains(['\r', '\n']) {
        return Err(CommandCodecError::InvalidString { field: "name" });
    }
    Ok(name)
}

fn validate_finite(value: f32, field: &'static str) -> Result<(), CommandCodecError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CommandCodecError::InvalidFloat { field })
    }
}

fn put_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u16).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn put_nonzero_u64(
    payload: &mut Vec<u8>,
    opcode: u8,
    value: u64,
    field: &'static str,
) -> Result<(), CommandCodecError> {
    if value == 0 {
        return Err(CommandCodecError::InvalidZero { field });
    }
    payload.push(opcode);
    payload.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn put_nonzero_u32(
    payload: &mut Vec<u8>,
    value: u32,
    field: &'static str,
) -> Result<(), CommandCodecError> {
    if value == 0 {
        return Err(CommandCodecError::InvalidZero { field });
    }
    payload.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandCodecError {
    Empty,
    Truncated { field: &'static str },
    UnknownOpcode(u8),
    InvalidEnum { field: &'static str, value: u8 },
    InvalidZero { field: &'static str },
    InvalidFloat { field: &'static str },
    InvalidString { field: &'static str },
    InvalidUtf8 { field: &'static str },
    TrailingBytes { count: usize },
}

impl fmt::Display for CommandCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("command payload is empty"),
            Self::Truncated { field } => write!(formatter, "command field '{field}' is truncated"),
            Self::UnknownOpcode(opcode) => write!(formatter, "unknown command opcode {opcode}"),
            Self::InvalidEnum { field, value } => {
                write!(
                    formatter,
                    "command field '{field}' has invalid value {value}"
                )
            }
            Self::InvalidZero { field } => {
                write!(formatter, "command field '{field}' must be nonzero")
            }
            Self::InvalidFloat { field } => {
                write!(formatter, "command field '{field}' is not finite")
            }
            Self::InvalidString { field } => {
                write!(formatter, "command field '{field}' is invalid")
            }
            Self::InvalidUtf8 { field } => {
                write!(formatter, "command field '{field}' is not UTF-8")
            }
            Self::TrailingBytes { count } => {
                write!(formatter, "command payload has {count} trailing bytes")
            }
        }
    }
}

impl std::error::Error for CommandCodecError {}

struct CommandDecoder<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> CommandDecoder<'a> {
    fn take(&mut self, length: usize, field: &'static str) -> Result<&'a [u8], CommandCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CommandCodecError::Truncated { field })?;
        if end > self.payload.len() {
            return Err(CommandCodecError::Truncated { field });
        }
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn take_u8(&mut self) -> Result<u8, CommandCodecError> {
        Ok(*self
            .take(1, "opcode")?
            .first()
            .expect("one byte was requested"))
    }

    fn take_u32(&mut self, field: &'static str) -> Result<u32, CommandCodecError> {
        Ok(u32::from_be_bytes(
            self.take(4, field)?
                .try_into()
                .expect("four bytes were requested"),
        ))
    }

    fn take_nonzero_u32(&mut self, field: &'static str) -> Result<u32, CommandCodecError> {
        let value = self.take_u32(field)?;
        if value == 0 {
            return Err(CommandCodecError::InvalidZero { field });
        }
        Ok(value)
    }

    fn take_nonzero_u64(&mut self, field: &'static str) -> Result<u64, CommandCodecError> {
        let value = u64::from_be_bytes(
            self.take(8, field)?
                .try_into()
                .expect("eight bytes were requested"),
        );
        if value == 0 {
            return Err(CommandCodecError::InvalidZero { field });
        }
        Ok(value)
    }

    fn take_string(&mut self, field: &'static str) -> Result<String, CommandCodecError> {
        let length = usize::from(u16::from_be_bytes(
            self.take(2, field)?
                .try_into()
                .expect("two bytes were requested"),
        ));
        if length == 0 || length > MAX_COMMAND_NAME_BYTES {
            return Err(CommandCodecError::InvalidString { field });
        }
        String::from_utf8(self.take(length, field)?.to_vec())
            .map_err(|_| CommandCodecError::InvalidUtf8 { field })
    }
}

impl TryFrom<u8> for MessageKind {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Command),
            2 => Ok(Self::Event),
            other => Err(DecodeError::UnknownMessageKind(other)),
        }
    }
}

/// A complete versioned command or event envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Envelope {
    /// The protocol version for the envelope body.
    pub version: u16,
    /// Whether the payload is a command or an event.
    pub kind: MessageKind,
    /// Reserved for future envelope-level flags. This prototype requires zero.
    pub flags: u8,
    /// An opaque, schema-specific command or event payload.
    pub payload: Vec<u8>,
}

impl Envelope {
    /// Construct an envelope using the current protocol version and no flags.
    pub fn new(kind: MessageKind, payload: impl Into<Vec<u8>>) -> Result<Self, EncodeError> {
        Self::with_version(PROTOCOL_VERSION, kind, 0, payload)
    }

    /// Construct an envelope with explicit version and flags.
    pub fn with_version(
        version: u16,
        kind: MessageKind,
        flags: u8,
        payload: impl Into<Vec<u8>>,
    ) -> Result<Self, EncodeError> {
        let payload = payload.into();
        validate_payload_for_encoding(version, flags, payload.len())?;

        Ok(Self {
            version,
            kind,
            flags,
            payload,
        })
    }

    /// Encode one envelope, including its four-byte body-length prefix.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        validate_payload_for_encoding(self.version, self.flags, self.payload.len())?;

        let body_len = HEADER_LEN
            .checked_add(self.payload.len())
            .expect("payload length is bounded before adding the header");
        let body_len_u32 = u32::try_from(body_len).expect("bounded frame fits in u32");

        let mut encoded = Vec::with_capacity(LENGTH_PREFIX_LEN + body_len);
        encoded.extend_from_slice(&body_len_u32.to_be_bytes());
        encoded.extend_from_slice(&MAGIC);
        encoded.extend_from_slice(&self.version.to_be_bytes());
        encoded.push(self.kind as u8);
        encoded.push(self.flags);
        encoded.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        encoded.extend_from_slice(&self.payload);
        Ok(encoded)
    }
}

/// The result of decoding the first frame in a byte buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedFrame {
    pub envelope: Envelope,
    /// Number of bytes consumed, including the length prefix.
    pub consumed: usize,
}

/// Decode exactly one frame from the beginning of `input`.
///
/// Additional bytes after the first frame are left untouched and reported via
/// [`DecodedFrame::consumed`], which lets a future socket adapter retain a
/// partial receive buffer without putting I/O concerns in this crate.
pub fn decode_one(input: &[u8]) -> Result<DecodedFrame, DecodeError> {
    if input.len() < LENGTH_PREFIX_LEN {
        return Err(DecodeError::Truncated {
            needed: LENGTH_PREFIX_LEN,
            available: input.len(),
        });
    }

    let body_len = u32::from_be_bytes(input[..LENGTH_PREFIX_LEN].try_into().unwrap()) as usize;
    if body_len > MAX_FRAME_SIZE {
        return Err(DecodeError::FrameTooLarge {
            length: body_len,
            maximum: MAX_FRAME_SIZE,
        });
    }

    let total_len = LENGTH_PREFIX_LEN
        .checked_add(body_len)
        .expect("bounded body length cannot overflow total length");
    if input.len() < total_len {
        return Err(DecodeError::Truncated {
            needed: total_len,
            available: input.len(),
        });
    }
    if body_len < HEADER_LEN {
        return Err(DecodeError::HeaderTooShort {
            length: body_len,
            minimum: HEADER_LEN,
        });
    }

    let body = &input[LENGTH_PREFIX_LEN..total_len];
    if body[..MAGIC.len()] != MAGIC {
        return Err(DecodeError::InvalidMagic {
            actual: body[..MAGIC.len()].try_into().unwrap(),
        });
    }

    let version = u16::from_be_bytes(body[4..6].try_into().unwrap());
    if version != PROTOCOL_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            version,
            supported: PROTOCOL_VERSION,
        });
    }

    let kind = MessageKind::try_from(body[6])?;
    let flags = body[7];
    if flags != 0 {
        return Err(DecodeError::UnsupportedFlags(flags));
    }

    let payload_len = u32::from_be_bytes(body[8..12].try_into().unwrap()) as usize;
    let expected_body_len = HEADER_LEN
        .checked_add(payload_len)
        .expect("payload length is represented by u32");
    if expected_body_len != body_len {
        return Err(DecodeError::PayloadBoundaryMismatch {
            declared: payload_len,
            available: body_len - HEADER_LEN,
        });
    }

    Ok(DecodedFrame {
        envelope: Envelope {
            version,
            kind,
            flags,
            payload: body[HEADER_LEN..].to_vec(),
        },
        consumed: total_len,
    })
}

fn validate_payload_for_encoding(
    version: u16,
    flags: u8,
    payload_len: usize,
) -> Result<(), EncodeError> {
    if version != PROTOCOL_VERSION {
        return Err(EncodeError::UnsupportedVersion {
            version,
            supported: PROTOCOL_VERSION,
        });
    }
    if flags != 0 {
        return Err(EncodeError::UnsupportedFlags(flags));
    }
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(EncodeError::PayloadTooLarge {
            length: payload_len,
            maximum: MAX_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

/// Errors found while producing a frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncodeError {
    UnsupportedVersion { version: u16, supported: u16 },
    UnsupportedFlags(u8),
    PayloadTooLarge { length: usize, maximum: usize },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { version, supported } => write!(
                formatter,
                "protocol version {version} is unsupported; expected {supported}"
            ),
            Self::UnsupportedFlags(flags) => write!(
                formatter,
                "envelope flags 0x{flags:02x} are unsupported; expected zero"
            ),
            Self::PayloadTooLarge { length, maximum } => {
                write!(
                    formatter,
                    "payload length {length} exceeds maximum {maximum}"
                )
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Errors found while decoding a frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    Truncated { needed: usize, available: usize },
    FrameTooLarge { length: usize, maximum: usize },
    HeaderTooShort { length: usize, minimum: usize },
    InvalidMagic { actual: [u8; 4] },
    UnsupportedVersion { version: u16, supported: u16 },
    UnknownMessageKind(u8),
    UnsupportedFlags(u8),
    PayloadBoundaryMismatch { declared: usize, available: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { needed, available } => {
                write!(
                    formatter,
                    "truncated frame: need {needed} bytes, have {available}"
                )
            }
            Self::FrameTooLarge { length, maximum } => {
                write!(formatter, "frame length {length} exceeds maximum {maximum}")
            }
            Self::HeaderTooShort { length, minimum } => {
                write!(
                    formatter,
                    "frame body length {length} is below header minimum {minimum}"
                )
            }
            Self::InvalidMagic { actual } => write!(formatter, "invalid frame magic {actual:?}"),
            Self::UnsupportedVersion { version, supported } => write!(
                formatter,
                "protocol version {version} is unsupported; expected {supported}"
            ),
            Self::UnknownMessageKind(kind) => write!(formatter, "unknown message kind {kind}"),
            Self::UnsupportedFlags(flags) => write!(
                formatter,
                "envelope flags 0x{flags:02x} are unsupported; expected zero"
            ),
            Self::PayloadBoundaryMismatch {
                declared,
                available,
            } => write!(
                formatter,
                "payload length declares {declared} bytes but frame contains {available}"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_round_trips_and_reports_consumed_length() {
        let envelope = Envelope::new(MessageKind::Command, b"move 1 2".to_vec()).unwrap();
        let encoded = envelope.encode().unwrap();
        let decoded = decode_one(&encoded).unwrap();

        assert_eq!(decoded.envelope, envelope);
        assert_eq!(decoded.consumed, encoded.len());
    }

    #[test]
    fn typed_client_commands_round_trip_through_payload_codec() {
        let commands = [
            ClientCommand::Join {
                name: "Aria".to_owned(),
                role: RoleCode::DamageDealer,
            },
            ClientCommand::Move { dx: -2.5, dy: 4.0 },
            ClientCommand::SelectTarget { target_id: 9 },
            ClientCommand::BasicAttack,
            ClientCommand::ListVendor { vendor_id: 1 },
            ClientCommand::BuyItem {
                vendor_id: 1,
                item_id: 2,
                quantity: 3,
            },
            ClientCommand::LootEnemy { enemy_id: 2 },
            ClientCommand::ListQuestOffers { npc_id: 1 },
            ClientCommand::AcceptQuest {
                npc_id: 1,
                quest_id: 1,
            },
            ClientCommand::TurnInQuest {
                npc_id: 1,
                quest_id: 1,
            },
            ClientCommand::Snapshot,
        ];

        for command in commands {
            let payload = command.encode_payload().expect("command should encode");
            assert_eq!(ClientCommand::decode_payload(&payload), Ok(command));
            let envelope = Envelope::new(MessageKind::Command, payload)
                .expect("command envelope should encode");
            let decoded = decode_one(&envelope.encode().expect("envelope should encode")).unwrap();
            assert_eq!(decoded.envelope.kind, MessageKind::Command);
        }
    }

    #[test]
    fn typed_client_command_codec_rejects_invalid_values_and_trailing_bytes() {
        assert_eq!(
            ClientCommand::decode_payload(&[]),
            Err(CommandCodecError::Empty)
        );
        assert_eq!(
            ClientCommand::Join {
                name: "A\nB".to_owned(),
                role: RoleCode::Tank,
            }
            .encode_payload(),
            Err(CommandCodecError::InvalidString { field: "name" })
        );
        assert_eq!(
            ClientCommand::Move {
                dx: f32::NAN,
                dy: 0.0,
            }
            .encode_payload(),
            Err(CommandCodecError::InvalidFloat { field: "dx" })
        );
        assert_eq!(
            ClientCommand::decode_payload(&[5, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(CommandCodecError::InvalidZero { field: "vendor_id" })
        );
        let mut payload = ClientCommand::Snapshot.encode_payload().unwrap();
        payload.push(0);
        assert_eq!(
            ClientCommand::decode_payload(&payload),
            Err(CommandCodecError::TrailingBytes { count: 1 })
        );
    }

    #[test]
    fn event_round_trips_with_a_second_frame_after_it() {
        let first = Envelope::new(MessageKind::Event, [1, 2, 3])
            .unwrap()
            .encode()
            .unwrap();
        let second = Envelope::new(MessageKind::Command, [4, 5])
            .unwrap()
            .encode()
            .unwrap();
        let mut stream = first.clone();
        stream.extend_from_slice(&second);

        let decoded_first = decode_one(&stream).unwrap();
        let decoded_second = decode_one(&stream[decoded_first.consumed..]).unwrap();

        assert_eq!(decoded_first.envelope.payload, vec![1, 2, 3]);
        assert_eq!(decoded_second.envelope.payload, vec![4, 5]);
    }

    #[test]
    fn every_truncated_prefix_is_rejected() {
        let encoded = Envelope::new(MessageKind::Event, vec![9; 32])
            .unwrap()
            .encode()
            .unwrap();

        for end in 0..encoded.len() {
            let error = decode_one(&encoded[..end]).unwrap_err();
            assert!(
                matches!(error, DecodeError::Truncated { .. }),
                "end={end}: {error}"
            );
        }
    }

    #[test]
    fn maximum_payload_is_accepted_but_one_byte_more_is_rejected() {
        let accepted = Envelope::new(MessageKind::Command, vec![0; MAX_FRAME_SIZE - HEADER_LEN])
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(accepted.len(), LENGTH_PREFIX_LEN + MAX_FRAME_SIZE);
        assert!(decode_one(&accepted).is_ok());

        let error = Envelope::new(
            MessageKind::Command,
            vec![0; MAX_FRAME_SIZE - HEADER_LEN + 1],
        )
        .unwrap_err();
        assert_eq!(
            error,
            EncodeError::PayloadTooLarge {
                length: MAX_FRAME_SIZE - HEADER_LEN + 1,
                maximum: MAX_FRAME_SIZE - HEADER_LEN,
            }
        );
    }

    #[test]
    fn declared_frame_larger_than_limit_is_rejected_before_waiting_for_body() {
        let declared = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();
        let error = decode_one(&declared).unwrap_err();

        assert_eq!(
            error,
            DecodeError::FrameTooLarge {
                length: MAX_FRAME_SIZE + 1,
                maximum: MAX_FRAME_SIZE,
            }
        );
    }

    #[test]
    fn malformed_header_fields_are_rejected() {
        let mut encoded = Envelope::new(MessageKind::Command, [7])
            .unwrap()
            .encode()
            .unwrap();

        encoded[4..8].copy_from_slice(b"BAD\0");
        assert!(matches!(
            decode_one(&encoded),
            Err(DecodeError::InvalidMagic { .. })
        ));

        let mut encoded = Envelope::new(MessageKind::Command, [7])
            .unwrap()
            .encode()
            .unwrap();
        encoded[10] = 9;
        assert!(matches!(
            decode_one(&encoded),
            Err(DecodeError::UnknownMessageKind(9))
        ));

        let mut encoded = Envelope::new(MessageKind::Command, [7])
            .unwrap()
            .encode()
            .unwrap();
        encoded[11] = 1;
        assert!(matches!(
            decode_one(&encoded),
            Err(DecodeError::UnsupportedFlags(1))
        ));

        let mut encoded = Envelope::new(MessageKind::Command, [7])
            .unwrap()
            .encode()
            .unwrap();
        encoded[12..16].copy_from_slice(&2_u32.to_be_bytes());
        assert!(matches!(
            decode_one(&encoded),
            Err(DecodeError::PayloadBoundaryMismatch {
                declared: 2,
                available: 1
            })
        ));
    }
}
