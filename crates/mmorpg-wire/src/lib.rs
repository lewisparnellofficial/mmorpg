//! A small, socket-independent wire-envelope prototype.
//!
//! The envelope deliberately treats the payload as opaque bytes. Command and
//! event schemas can be added above this layer without making the framing
//! implementation depend on a transport, serializer, or simulation crate.

use std::fmt;

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
