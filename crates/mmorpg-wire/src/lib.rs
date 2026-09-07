//! A small, socket-independent wire-envelope prototype.
//!
//! The envelope owns framing while typed command and server-message schemas
//! live above it. The schemas remain independent of sockets and the
//! authoritative simulation, so either side can be tested without a runtime
//! or serializer dependency.

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
    Authenticate {
        token: String,
    },
    Join {
        name: String,
        role: RoleCode,
    },
    EnterWorld,
    ListCharacters,
    SelectCharacter {
        character_id: u64,
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
            Self::Authenticate { token } => {
                let token = validate_command_name(token)?;
                payload.push(12);
                put_string(&mut payload, token);
            }
            Self::Join { name, role } => {
                let name = validate_command_name(name)?;
                payload.push(1);
                put_string(&mut payload, name);
                payload.push(*role as u8);
            }
            Self::EnterWorld => payload.push(13),
            Self::ListCharacters => payload.push(14),
            Self::SelectCharacter { character_id } => {
                put_nonzero_u64(&mut payload, 15, *character_id, "character_id")?;
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
            12 => Self::Authenticate {
                token: decoder.take_string("token")?,
            },
            1 => Self::Join {
                name: decoder.take_string("name")?,
                role: decoder.take_u8()?.try_into()?,
            },
            13 => Self::EnterWorld,
            14 => Self::ListCharacters,
            15 => Self::SelectCharacter {
                character_id: decoder.take_nonzero_u64("character_id")?,
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

/// Maximum number of repeated values accepted in one server payload.
pub const MAX_SERVER_COLLECTION_ENTRIES: usize = 4096;
const MAX_SERVER_STRING_BYTES: usize = 4096;

/// Version of the complete world-snapshot payload, independent of the
/// surrounding gameplay envelope version. Version 2 makes player inventory
/// capacity explicit instead of requiring clients to assume the starter
/// value.
pub const SNAPSHOT_SCHEMA_VERSION: u16 = 2;

/// Wire representation of an entity position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionState {
    pub x: f32,
    pub y: f32,
}

/// Wire representation of one inventory stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemStackState {
    pub item_id: u32,
    pub quantity: u32,
}

/// Wire representation of one quest in a player's log.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestState {
    pub quest_id: u32,
    pub progress: u32,
    pub required_count: u32,
    pub status: QuestStatusCode,
}

/// Quest status values used by the server snapshot and event codecs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum QuestStatusCode {
    Accepted = 1,
    Completed = 2,
    Rewarded = 3,
}

impl TryFrom<u8> for QuestStatusCode {
    type Error = ServerCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Accepted),
            2 => Ok(Self::Completed),
            3 => Ok(Self::Rewarded),
            other => Err(ServerCodecError::InvalidEnum {
                field: "quest_status",
                value: other,
            }),
        }
    }
}

/// Wire representation of a player included in an event or snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub player_id: u64,
    pub name: String,
    pub role: RoleCode,
    pub position: PositionState,
    pub health: u32,
    pub max_health: u32,
    pub target_id: Option<u64>,
    pub gold: u32,
    pub inventory_capacity: u32,
    pub inventory: Vec<ItemStackState>,
    pub quests: Vec<QuestState>,
}

/// NPC categories used by the server snapshot and movement events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NpcKindCode {
    Vendor = 1,
    Enemy = 2,
}

impl TryFrom<u8> for NpcKindCode {
    type Error = ServerCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Vendor),
            2 => Ok(Self::Enemy),
            other => Err(ServerCodecError::InvalidEnum {
                field: "npc_kind",
                value: other,
            }),
        }
    }
}

/// Wire representation of an NPC included in a snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcState {
    pub entity_id: u64,
    pub template_id: u32,
    pub name: String,
    pub kind: NpcKindCode,
    pub position: PositionState,
    pub health: u32,
    pub max_health: u32,
}

/// Wire representation of one vendor listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VendorListingState {
    pub item_id: u32,
    pub name: String,
    pub unit_price: u32,
    pub remaining_quantity: u32,
    pub max_stack: u32,
}

/// Wire representation of one quest offer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestOfferState {
    pub quest_id: u32,
    pub name: String,
    pub description: String,
}

/// Wire representation of a character available to an authenticated account.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterSummary {
    pub character_id: u64,
    pub name: String,
    pub role: RoleCode,
}

/// Complete authoritative bootstrap state for the current world.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSnapshot {
    pub version: u16,
    pub tick: u64,
    pub player_count: u32,
    pub npc_count: u32,
    pub enemy_count: u32,
    pub vendor_count: u32,
    pub players: Vec<PlayerState>,
    pub npcs: Vec<NpcState>,
}

/// Server-to-client messages carried in an event envelope.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    Welcome {
        server: String,
    },
    Connected {
        player_id: u64,
        role: RoleCode,
    },
    Error {
        message: String,
    },
    Event(ServerEvent),
    Snapshot(WorldSnapshot),
    Authenticated {
        account_id: u64,
        session_id: u64,
    },
    CharacterList {
        account_id: u64,
        characters: Vec<CharacterSummary>,
    },
    CharacterSelected {
        character_id: u64,
        name: String,
        role: RoleCode,
    },
}

/// Typed authoritative event payloads corresponding to the current core
/// event vocabulary. The server remains the source of all values.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    PlayerJoined {
        player: PlayerState,
    },
    PlayerLeft {
        player_id: u64,
    },
    PlayerMoved {
        player_id: u64,
        position: PositionState,
        area: ZoneAreaCode,
    },
    TargetSelected {
        player_id: u64,
        target_id: u64,
    },
    AttackResolved {
        player_id: u64,
        target_id: u64,
        damage: u32,
        target_health: u32,
    },
    EnemyDefeated {
        enemy_id: u64,
    },
    VendorListed {
        player_id: u64,
        vendor_id: u64,
        listings: Vec<VendorListingState>,
    },
    ItemPurchased {
        player_id: u64,
        vendor_id: u64,
        item_id: u32,
        quantity: u32,
        total_price: u32,
        gold_remaining: u32,
    },
    LootRewarded {
        player_id: u64,
        enemy_id: u64,
        item_id: u32,
        quantity: u32,
    },
    TransactionRejected {
        player_id: u64,
        reason: String,
    },
    QuestOffersListed {
        player_id: u64,
        npc_id: u64,
        quests: Vec<QuestOfferState>,
    },
    QuestAccepted {
        player_id: u64,
        npc_id: u64,
        quest_id: u32,
    },
    QuestProgressed {
        player_id: u64,
        quest_id: u32,
        progress: u32,
        required_count: u32,
    },
    QuestCompleted {
        player_id: u64,
        quest_id: u32,
    },
    QuestRewarded {
        player_id: u64,
        quest_id: u32,
        gold: u32,
        item_id: Option<u32>,
        item_quantity: u32,
        gold_remaining: u32,
    },
    QuestRejected {
        player_id: u64,
        reason: String,
    },
    CommandRejected {
        reason: String,
    },
}

/// Zone area values used by movement events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ZoneAreaCode {
    Town = 1,
    Field = 2,
}

impl TryFrom<u8> for ZoneAreaCode {
    type Error = ServerCodecError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Town),
            2 => Ok(Self::Field),
            other => Err(ServerCodecError::InvalidEnum {
                field: "zone_area",
                value: other,
            }),
        }
    }
}

impl ServerMessage {
    /// Encodes one typed server payload without its surrounding envelope.
    pub fn encode_payload(&self) -> Result<Vec<u8>, ServerCodecError> {
        let mut encoder = ServerEncoder::default();
        match self {
            Self::Welcome { server } => {
                encoder.put_u8(1);
                encoder.put_string(server, "server")?;
            }
            Self::Connected { player_id, role } => {
                encoder.put_u8(2);
                encoder.put_u64(*player_id, "player_id")?;
                encoder.put_u8(*role as u8);
            }
            Self::Error { message } => {
                encoder.put_u8(3);
                encoder.put_string(message, "message")?;
            }
            Self::Event(event) => {
                encoder.put_u8(4);
                encode_server_event(&mut encoder, event)?;
            }
            Self::Snapshot(snapshot) => {
                encoder.put_u8(5);
                encode_snapshot(&mut encoder, snapshot)?;
            }
            Self::Authenticated {
                account_id,
                session_id,
            } => {
                encoder.put_u8(6);
                encoder.put_u64(*account_id, "account_id")?;
                encoder.put_u64(*session_id, "session_id")?;
            }
            Self::CharacterList {
                account_id,
                characters,
            } => {
                encoder.put_u8(7);
                encoder.put_u64(*account_id, "account_id")?;
                encoder.put_count(characters.len(), "characters")?;
                for character in characters {
                    encode_character(&mut encoder, character)?;
                }
            }
            Self::CharacterSelected {
                character_id,
                name,
                role,
            } => {
                encoder.put_u8(8);
                encoder.put_u64(*character_id, "character_id")?;
                encoder.put_string(name, "character_name")?;
                encoder.put_u8(*role as u8);
            }
        }
        Ok(encoder.bytes)
    }

    /// Decodes exactly one typed server payload.
    pub fn decode_payload(payload: &[u8]) -> Result<Self, ServerCodecError> {
        if payload.is_empty() {
            return Err(ServerCodecError::Empty);
        }
        let mut decoder = ServerDecoder { payload, offset: 0 };
        let message = match decoder.take_u8("message")? {
            1 => Self::Welcome {
                server: decoder.take_string("server")?,
            },
            2 => Self::Connected {
                player_id: decoder.take_nonzero_u64("player_id")?,
                role: decode_role(&mut decoder)?,
            },
            3 => Self::Error {
                message: decoder.take_string("message")?,
            },
            4 => Self::Event(decode_server_event(&mut decoder)?),
            5 => Self::Snapshot(decode_snapshot(&mut decoder)?),
            6 => Self::Authenticated {
                account_id: decoder.take_nonzero_u64("account_id")?,
                session_id: decoder.take_nonzero_u64("session_id")?,
            },
            7 => {
                let account_id = decoder.take_nonzero_u64("account_id")?;
                let character_count = decoder.take_count("characters")?;
                let mut characters = Vec::with_capacity(character_count);
                for _ in 0..character_count {
                    characters.push(decode_character(&mut decoder)?);
                }
                Self::CharacterList {
                    account_id,
                    characters,
                }
            }
            8 => Self::CharacterSelected {
                character_id: decoder.take_nonzero_u64("character_id")?,
                name: decoder.take_string("character_name")?,
                role: decode_role(&mut decoder)?,
            },
            opcode => return Err(ServerCodecError::UnknownOpcode(opcode)),
        };
        if decoder.offset != payload.len() {
            return Err(ServerCodecError::TrailingBytes {
                count: payload.len() - decoder.offset,
            });
        }
        Ok(message)
    }
}

#[derive(Default)]
struct ServerEncoder {
    bytes: Vec<u8>,
}

impl ServerEncoder {
    fn put_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn put_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn put_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn put_u64(&mut self, value: u64, field: &'static str) -> Result<(), ServerCodecError> {
        if value == 0 {
            return Err(ServerCodecError::InvalidZero { field });
        }
        self.put_u64_unchecked(value);
        Ok(())
    }

    fn put_u64_unchecked(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn put_f32(&mut self, value: f32, field: &'static str) -> Result<(), ServerCodecError> {
        if !value.is_finite() {
            return Err(ServerCodecError::InvalidFloat { field });
        }
        self.put_u32(value.to_bits());
        Ok(())
    }

    fn put_string(&mut self, value: &str, field: &'static str) -> Result<(), ServerCodecError> {
        if value.is_empty() || value.len() > MAX_SERVER_STRING_BYTES {
            return Err(ServerCodecError::InvalidString { field });
        }
        let length =
            u16::try_from(value.len()).map_err(|_| ServerCodecError::InvalidString { field })?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn put_optional_u64(
        &mut self,
        value: Option<u64>,
        field: &'static str,
    ) -> Result<(), ServerCodecError> {
        match value {
            Some(value) => {
                self.put_u8(1);
                self.put_u64(value, field)?;
            }
            None => self.put_u8(0),
        }
        Ok(())
    }

    fn put_optional_u32(
        &mut self,
        value: Option<u32>,
        field: &'static str,
    ) -> Result<(), ServerCodecError> {
        match value {
            Some(value) => {
                self.put_u8(1);
                if value == 0 {
                    return Err(ServerCodecError::InvalidZero { field });
                }
                self.put_u32(value);
            }
            None => self.put_u8(0),
        }
        Ok(())
    }

    fn put_count(&mut self, count: usize, field: &'static str) -> Result<(), ServerCodecError> {
        if count > MAX_SERVER_COLLECTION_ENTRIES {
            return Err(ServerCodecError::CountTooLarge {
                field,
                count,
                maximum: MAX_SERVER_COLLECTION_ENTRIES,
            });
        }
        self.put_u32(count as u32);
        Ok(())
    }
}

struct ServerDecoder<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> ServerDecoder<'a> {
    fn take(&mut self, length: usize, field: &'static str) -> Result<&'a [u8], ServerCodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ServerCodecError::Truncated { field })?;
        if end > self.payload.len() {
            return Err(ServerCodecError::Truncated { field });
        }
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn take_u8(&mut self, field: &'static str) -> Result<u8, ServerCodecError> {
        Ok(self.take(1, field)?[0])
    }

    fn take_u16(&mut self, field: &'static str) -> Result<u16, ServerCodecError> {
        Ok(u16::from_be_bytes(
            self.take(2, field)?
                .try_into()
                .expect("two bytes requested"),
        ))
    }

    fn take_u32(&mut self, field: &'static str) -> Result<u32, ServerCodecError> {
        Ok(u32::from_be_bytes(
            self.take(4, field)?
                .try_into()
                .expect("four bytes requested"),
        ))
    }

    fn take_nonzero_u32(&mut self, field: &'static str) -> Result<u32, ServerCodecError> {
        let value = self.take_u32(field)?;
        if value == 0 {
            return Err(ServerCodecError::InvalidZero { field });
        }
        Ok(value)
    }

    fn take_u64(&mut self, field: &'static str) -> Result<u64, ServerCodecError> {
        Ok(u64::from_be_bytes(
            self.take(8, field)?
                .try_into()
                .expect("eight bytes requested"),
        ))
    }

    fn take_nonzero_u64(&mut self, field: &'static str) -> Result<u64, ServerCodecError> {
        let value = self.take_u64(field)?;
        if value == 0 {
            return Err(ServerCodecError::InvalidZero { field });
        }
        Ok(value)
    }

    fn take_f32(&mut self, field: &'static str) -> Result<f32, ServerCodecError> {
        let value = f32::from_bits(self.take_u32(field)?);
        if !value.is_finite() {
            return Err(ServerCodecError::InvalidFloat { field });
        }
        Ok(value)
    }

    fn take_string(&mut self, field: &'static str) -> Result<String, ServerCodecError> {
        let length = usize::from(u16::from_be_bytes(
            self.take(2, field)?
                .try_into()
                .expect("two bytes requested"),
        ));
        if length == 0 || length > MAX_SERVER_STRING_BYTES {
            return Err(ServerCodecError::InvalidString { field });
        }
        String::from_utf8(self.take(length, field)?.to_vec())
            .map_err(|_| ServerCodecError::InvalidUtf8 { field })
    }

    fn take_optional_u64(&mut self, field: &'static str) -> Result<Option<u64>, ServerCodecError> {
        match self.take_u8(field)? {
            0 => Ok(None),
            1 => Ok(Some(self.take_nonzero_u64(field)?)),
            value => Err(ServerCodecError::InvalidEnum { field, value }),
        }
    }

    fn take_optional_u32(&mut self, field: &'static str) -> Result<Option<u32>, ServerCodecError> {
        match self.take_u8(field)? {
            0 => Ok(None),
            1 => Ok(Some(self.take_nonzero_u32(field)?)),
            value => Err(ServerCodecError::InvalidEnum { field, value }),
        }
    }

    fn take_count(&mut self, field: &'static str) -> Result<usize, ServerCodecError> {
        let count = self.take_u32(field)? as usize;
        if count > MAX_SERVER_COLLECTION_ENTRIES {
            return Err(ServerCodecError::CountTooLarge {
                field,
                count,
                maximum: MAX_SERVER_COLLECTION_ENTRIES,
            });
        }
        Ok(count)
    }
}

fn decode_role(decoder: &mut ServerDecoder<'_>) -> Result<RoleCode, ServerCodecError> {
    match decoder.take_u8("role")? {
        1 => Ok(RoleCode::Tank),
        2 => Ok(RoleCode::Healer),
        3 => Ok(RoleCode::DamageDealer),
        value => Err(ServerCodecError::InvalidEnum {
            field: "role",
            value,
        }),
    }
}

fn encode_character(
    encoder: &mut ServerEncoder,
    character: &CharacterSummary,
) -> Result<(), ServerCodecError> {
    encoder.put_u64(character.character_id, "character_id")?;
    encoder.put_string(&character.name, "character_name")?;
    encoder.put_u8(character.role as u8);
    Ok(())
}

fn decode_character(decoder: &mut ServerDecoder<'_>) -> Result<CharacterSummary, ServerCodecError> {
    Ok(CharacterSummary {
        character_id: decoder.take_nonzero_u64("character_id")?,
        name: decoder.take_string("character_name")?,
        role: decode_role(decoder)?,
    })
}

fn encode_position(
    encoder: &mut ServerEncoder,
    position: PositionState,
) -> Result<(), ServerCodecError> {
    encoder.put_f32(position.x, "position.x")?;
    encoder.put_f32(position.y, "position.y")?;
    Ok(())
}

fn decode_position(decoder: &mut ServerDecoder<'_>) -> Result<PositionState, ServerCodecError> {
    Ok(PositionState {
        x: decoder.take_f32("position.x")?,
        y: decoder.take_f32("position.y")?,
    })
}

fn encode_player(
    encoder: &mut ServerEncoder,
    player: &PlayerState,
) -> Result<(), ServerCodecError> {
    encoder.put_u64(player.player_id, "player_id")?;
    encoder.put_string(&player.name, "name")?;
    encoder.put_u8(player.role as u8);
    encode_position(encoder, player.position)?;
    encoder.put_u32(player.health);
    encoder.put_u32(player.max_health);
    encoder.put_optional_u64(player.target_id, "target_id")?;
    encoder.put_u32(player.gold);
    encoder.put_u32(player.inventory_capacity);
    encoder.put_count(player.inventory.len(), "inventory")?;
    for stack in &player.inventory {
        if stack.item_id == 0 || stack.quantity == 0 {
            return Err(ServerCodecError::InvalidZero {
                field: "item_stack",
            });
        }
        encoder.put_u32(stack.item_id);
        encoder.put_u32(stack.quantity);
    }
    encoder.put_count(player.quests.len(), "quests")?;
    for quest in &player.quests {
        if quest.quest_id == 0 || quest.required_count == 0 {
            return Err(ServerCodecError::InvalidZero { field: "quest" });
        }
        encoder.put_u32(quest.quest_id);
        encoder.put_u32(quest.progress);
        encoder.put_u32(quest.required_count);
        encoder.put_u8(quest.status as u8);
    }
    Ok(())
}

fn decode_player(decoder: &mut ServerDecoder<'_>) -> Result<PlayerState, ServerCodecError> {
    let player_id = decoder.take_nonzero_u64("player_id")?;
    let name = decoder.take_string("name")?;
    let role = decode_role(decoder)?;
    let position = decode_position(decoder)?;
    let health = decoder.take_u32("health")?;
    let max_health = decoder.take_u32("max_health")?;
    let target_id = decoder.take_optional_u64("target_id")?;
    let gold = decoder.take_u32("gold")?;
    let inventory_capacity = decoder.take_u32("inventory_capacity")?;
    let inventory_count = decoder.take_count("inventory")?;
    let mut inventory = Vec::with_capacity(inventory_count);
    for _ in 0..inventory_count {
        inventory.push(ItemStackState {
            item_id: decoder.take_nonzero_u32("item_id")?,
            quantity: decoder.take_nonzero_u32("quantity")?,
        });
    }
    let quest_count = decoder.take_count("quests")?;
    let mut quests = Vec::with_capacity(quest_count);
    for _ in 0..quest_count {
        quests.push(QuestState {
            quest_id: decoder.take_nonzero_u32("quest_id")?,
            progress: decoder.take_u32("progress")?,
            required_count: decoder.take_nonzero_u32("required_count")?,
            status: QuestStatusCode::try_from(decoder.take_u8("quest_status")?)?,
        });
    }
    Ok(PlayerState {
        player_id,
        name,
        role,
        position,
        health,
        max_health,
        target_id,
        gold,
        inventory_capacity,
        inventory,
        quests,
    })
}

fn encode_npc(encoder: &mut ServerEncoder, npc: &NpcState) -> Result<(), ServerCodecError> {
    encoder.put_u64(npc.entity_id, "entity_id")?;
    if npc.template_id == 0 {
        return Err(ServerCodecError::InvalidZero {
            field: "template_id",
        });
    }
    encoder.put_u32(npc.template_id);
    encoder.put_string(&npc.name, "name")?;
    encoder.put_u8(npc.kind as u8);
    encode_position(encoder, npc.position)?;
    encoder.put_u32(npc.health);
    encoder.put_u32(npc.max_health);
    Ok(())
}

fn decode_npc(decoder: &mut ServerDecoder<'_>) -> Result<NpcState, ServerCodecError> {
    Ok(NpcState {
        entity_id: decoder.take_nonzero_u64("entity_id")?,
        template_id: decoder.take_nonzero_u32("template_id")?,
        name: decoder.take_string("name")?,
        kind: NpcKindCode::try_from(decoder.take_u8("npc_kind")?)?,
        position: decode_position(decoder)?,
        health: decoder.take_u32("health")?,
        max_health: decoder.take_u32("max_health")?,
    })
}

fn encode_snapshot(
    encoder: &mut ServerEncoder,
    snapshot: &WorldSnapshot,
) -> Result<(), ServerCodecError> {
    if snapshot.version != SNAPSHOT_SCHEMA_VERSION {
        return Err(ServerCodecError::UnsupportedSnapshotVersion {
            version: snapshot.version,
            supported: SNAPSHOT_SCHEMA_VERSION,
        });
    }
    encoder.put_u16(snapshot.version);
    encoder.put_u64_unchecked(snapshot.tick);
    encoder.put_u32(snapshot.player_count);
    encoder.put_u32(snapshot.npc_count);
    encoder.put_u32(snapshot.enemy_count);
    encoder.put_u32(snapshot.vendor_count);
    encoder.put_count(snapshot.players.len(), "players")?;
    for player in &snapshot.players {
        encode_player(encoder, player)?;
    }
    encoder.put_count(snapshot.npcs.len(), "npcs")?;
    for npc in &snapshot.npcs {
        encode_npc(encoder, npc)?;
    }
    Ok(())
}

fn decode_snapshot(decoder: &mut ServerDecoder<'_>) -> Result<WorldSnapshot, ServerCodecError> {
    let version = decoder.take_u16("snapshot_version")?;
    if version != SNAPSHOT_SCHEMA_VERSION {
        return Err(ServerCodecError::UnsupportedSnapshotVersion {
            version,
            supported: SNAPSHOT_SCHEMA_VERSION,
        });
    }
    let tick = decoder.take_u64("tick")?;
    let player_count = decoder.take_u32("player_count")?;
    let npc_count = decoder.take_u32("npc_count")?;
    let enemy_count = decoder.take_u32("enemy_count")?;
    let vendor_count = decoder.take_u32("vendor_count")?;
    let player_count_in_payload = decoder.take_count("players")?;
    let mut players = Vec::with_capacity(player_count_in_payload);
    for _ in 0..player_count_in_payload {
        players.push(decode_player(decoder)?);
    }
    let npc_count_in_payload = decoder.take_count("npcs")?;
    let mut npcs = Vec::with_capacity(npc_count_in_payload);
    for _ in 0..npc_count_in_payload {
        npcs.push(decode_npc(decoder)?);
    }
    Ok(WorldSnapshot {
        version,
        tick,
        player_count,
        npc_count,
        enemy_count,
        vendor_count,
        players,
        npcs,
    })
}

fn encode_listing(
    encoder: &mut ServerEncoder,
    listing: &VendorListingState,
) -> Result<(), ServerCodecError> {
    if listing.item_id == 0 || listing.max_stack == 0 {
        return Err(ServerCodecError::InvalidZero { field: "listing" });
    }
    encoder.put_u32(listing.item_id);
    encoder.put_string(&listing.name, "listing_name")?;
    encoder.put_u32(listing.unit_price);
    encoder.put_u32(listing.remaining_quantity);
    encoder.put_u32(listing.max_stack);
    Ok(())
}

fn decode_listing(decoder: &mut ServerDecoder<'_>) -> Result<VendorListingState, ServerCodecError> {
    Ok(VendorListingState {
        item_id: decoder.take_nonzero_u32("item_id")?,
        name: decoder.take_string("listing_name")?,
        unit_price: decoder.take_u32("unit_price")?,
        remaining_quantity: decoder.take_u32("remaining_quantity")?,
        max_stack: decoder.take_nonzero_u32("max_stack")?,
    })
}

fn encode_offer(
    encoder: &mut ServerEncoder,
    offer: &QuestOfferState,
) -> Result<(), ServerCodecError> {
    if offer.quest_id == 0 {
        return Err(ServerCodecError::InvalidZero { field: "quest_id" });
    }
    encoder.put_u32(offer.quest_id);
    encoder.put_string(&offer.name, "quest_name")?;
    encoder.put_string(&offer.description, "quest_description")?;
    Ok(())
}

fn decode_offer(decoder: &mut ServerDecoder<'_>) -> Result<QuestOfferState, ServerCodecError> {
    Ok(QuestOfferState {
        quest_id: decoder.take_nonzero_u32("quest_id")?,
        name: decoder.take_string("quest_name")?,
        description: decoder.take_string("quest_description")?,
    })
}

fn encode_server_event(
    encoder: &mut ServerEncoder,
    event: &ServerEvent,
) -> Result<(), ServerCodecError> {
    match event {
        ServerEvent::PlayerJoined { player } => {
            encoder.put_u8(1);
            encode_player(encoder, player)?;
        }
        ServerEvent::PlayerLeft { player_id } => {
            encoder.put_u8(2);
            encoder.put_u64(*player_id, "player_id")?;
        }
        ServerEvent::PlayerMoved {
            player_id,
            position,
            area,
        } => {
            encoder.put_u8(3);
            encoder.put_u64(*player_id, "player_id")?;
            encode_position(encoder, *position)?;
            encoder.put_u8(*area as u8);
        }
        ServerEvent::TargetSelected {
            player_id,
            target_id,
        } => {
            encoder.put_u8(4);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*target_id, "target_id")?;
        }
        ServerEvent::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => {
            encoder.put_u8(5);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*target_id, "target_id")?;
            encoder.put_u32(*damage);
            encoder.put_u32(*target_health);
        }
        ServerEvent::EnemyDefeated { enemy_id } => {
            encoder.put_u8(6);
            encoder.put_u64(*enemy_id, "enemy_id")?;
        }
        ServerEvent::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => {
            encoder.put_u8(7);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*vendor_id, "vendor_id")?;
            encoder.put_count(listings.len(), "listings")?;
            for listing in listings {
                encode_listing(encoder, listing)?;
            }
        }
        ServerEvent::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => {
            encoder.put_u8(8);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*vendor_id, "vendor_id")?;
            if *item_id == 0 || *quantity == 0 {
                return Err(ServerCodecError::InvalidZero { field: "purchase" });
            }
            encoder.put_u32(*item_id);
            encoder.put_u32(*quantity);
            encoder.put_u32(*total_price);
            encoder.put_u32(*gold_remaining);
        }
        ServerEvent::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => {
            encoder.put_u8(9);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*enemy_id, "enemy_id")?;
            if *item_id == 0 || *quantity == 0 {
                return Err(ServerCodecError::InvalidZero { field: "loot" });
            }
            encoder.put_u32(*item_id);
            encoder.put_u32(*quantity);
        }
        ServerEvent::TransactionRejected { player_id, reason } => {
            encoder.put_u8(10);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_string(reason, "reason")?;
        }
        ServerEvent::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => {
            encoder.put_u8(11);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*npc_id, "npc_id")?;
            encoder.put_count(quests.len(), "quests")?;
            for quest in quests {
                encode_offer(encoder, quest)?;
            }
        }
        ServerEvent::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => {
            encoder.put_u8(12);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_u64(*npc_id, "npc_id")?;
            if *quest_id == 0 {
                return Err(ServerCodecError::InvalidZero { field: "quest_id" });
            }
            encoder.put_u32(*quest_id);
        }
        ServerEvent::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => {
            encoder.put_u8(13);
            encoder.put_u64(*player_id, "player_id")?;
            if *quest_id == 0 || *required_count == 0 {
                return Err(ServerCodecError::InvalidZero { field: "quest" });
            }
            encoder.put_u32(*quest_id);
            encoder.put_u32(*progress);
            encoder.put_u32(*required_count);
        }
        ServerEvent::QuestCompleted {
            player_id,
            quest_id,
        } => {
            encoder.put_u8(14);
            encoder.put_u64(*player_id, "player_id")?;
            if *quest_id == 0 {
                return Err(ServerCodecError::InvalidZero { field: "quest_id" });
            }
            encoder.put_u32(*quest_id);
        }
        ServerEvent::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => {
            encoder.put_u8(15);
            encoder.put_u64(*player_id, "player_id")?;
            if *quest_id == 0 {
                return Err(ServerCodecError::InvalidZero { field: "quest_id" });
            }
            encoder.put_u32(*quest_id);
            encoder.put_u32(*gold);
            encoder.put_optional_u32(*item_id, "item_id")?;
            encoder.put_u32(*item_quantity);
            encoder.put_u32(*gold_remaining);
        }
        ServerEvent::QuestRejected { player_id, reason } => {
            encoder.put_u8(16);
            encoder.put_u64(*player_id, "player_id")?;
            encoder.put_string(reason, "reason")?;
        }
        ServerEvent::CommandRejected { reason } => {
            encoder.put_u8(17);
            encoder.put_string(reason, "reason")?;
        }
    }
    Ok(())
}

fn decode_server_event(decoder: &mut ServerDecoder<'_>) -> Result<ServerEvent, ServerCodecError> {
    match decoder.take_u8("event")? {
        1 => Ok(ServerEvent::PlayerJoined {
            player: decode_player(decoder)?,
        }),
        2 => Ok(ServerEvent::PlayerLeft {
            player_id: decoder.take_nonzero_u64("player_id")?,
        }),
        3 => Ok(ServerEvent::PlayerMoved {
            player_id: decoder.take_nonzero_u64("player_id")?,
            position: decode_position(decoder)?,
            area: ZoneAreaCode::try_from(decoder.take_u8("zone_area")?)?,
        }),
        4 => Ok(ServerEvent::TargetSelected {
            player_id: decoder.take_nonzero_u64("player_id")?,
            target_id: decoder.take_nonzero_u64("target_id")?,
        }),
        5 => Ok(ServerEvent::AttackResolved {
            player_id: decoder.take_nonzero_u64("player_id")?,
            target_id: decoder.take_nonzero_u64("target_id")?,
            damage: decoder.take_u32("damage")?,
            target_health: decoder.take_u32("target_health")?,
        }),
        6 => Ok(ServerEvent::EnemyDefeated {
            enemy_id: decoder.take_nonzero_u64("enemy_id")?,
        }),
        7 => {
            let player_id = decoder.take_nonzero_u64("player_id")?;
            let vendor_id = decoder.take_nonzero_u64("vendor_id")?;
            let count = decoder.take_count("listings")?;
            let mut listings = Vec::with_capacity(count);
            for _ in 0..count {
                listings.push(decode_listing(decoder)?);
            }
            Ok(ServerEvent::VendorListed {
                player_id,
                vendor_id,
                listings,
            })
        }
        8 => Ok(ServerEvent::ItemPurchased {
            player_id: decoder.take_nonzero_u64("player_id")?,
            vendor_id: decoder.take_nonzero_u64("vendor_id")?,
            item_id: decoder.take_nonzero_u32("item_id")?,
            quantity: decoder.take_nonzero_u32("quantity")?,
            total_price: decoder.take_u32("total_price")?,
            gold_remaining: decoder.take_u32("gold_remaining")?,
        }),
        9 => Ok(ServerEvent::LootRewarded {
            player_id: decoder.take_nonzero_u64("player_id")?,
            enemy_id: decoder.take_nonzero_u64("enemy_id")?,
            item_id: decoder.take_nonzero_u32("item_id")?,
            quantity: decoder.take_nonzero_u32("quantity")?,
        }),
        10 => Ok(ServerEvent::TransactionRejected {
            player_id: decoder.take_nonzero_u64("player_id")?,
            reason: decoder.take_string("reason")?,
        }),
        11 => {
            let player_id = decoder.take_nonzero_u64("player_id")?;
            let npc_id = decoder.take_nonzero_u64("npc_id")?;
            let count = decoder.take_count("quests")?;
            let mut quests = Vec::with_capacity(count);
            for _ in 0..count {
                quests.push(decode_offer(decoder)?);
            }
            Ok(ServerEvent::QuestOffersListed {
                player_id,
                npc_id,
                quests,
            })
        }
        12 => Ok(ServerEvent::QuestAccepted {
            player_id: decoder.take_nonzero_u64("player_id")?,
            npc_id: decoder.take_nonzero_u64("npc_id")?,
            quest_id: decoder.take_nonzero_u32("quest_id")?,
        }),
        13 => Ok(ServerEvent::QuestProgressed {
            player_id: decoder.take_nonzero_u64("player_id")?,
            quest_id: decoder.take_nonzero_u32("quest_id")?,
            progress: decoder.take_u32("progress")?,
            required_count: decoder.take_nonzero_u32("required_count")?,
        }),
        14 => Ok(ServerEvent::QuestCompleted {
            player_id: decoder.take_nonzero_u64("player_id")?,
            quest_id: decoder.take_nonzero_u32("quest_id")?,
        }),
        15 => Ok(ServerEvent::QuestRewarded {
            player_id: decoder.take_nonzero_u64("player_id")?,
            quest_id: decoder.take_nonzero_u32("quest_id")?,
            gold: decoder.take_u32("gold")?,
            item_id: decoder.take_optional_u32("item_id")?,
            item_quantity: decoder.take_u32("item_quantity")?,
            gold_remaining: decoder.take_u32("gold_remaining")?,
        }),
        16 => Ok(ServerEvent::QuestRejected {
            player_id: decoder.take_nonzero_u64("player_id")?,
            reason: decoder.take_string("reason")?,
        }),
        17 => Ok(ServerEvent::CommandRejected {
            reason: decoder.take_string("reason")?,
        }),
        opcode => Err(ServerCodecError::UnknownOpcode(opcode)),
    }
}

/// Errors found while encoding or decoding typed server payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerCodecError {
    Empty,
    Truncated {
        field: &'static str,
    },
    UnknownOpcode(u8),
    UnsupportedSnapshotVersion {
        version: u16,
        supported: u16,
    },
    InvalidEnum {
        field: &'static str,
        value: u8,
    },
    InvalidZero {
        field: &'static str,
    },
    InvalidFloat {
        field: &'static str,
    },
    InvalidString {
        field: &'static str,
    },
    InvalidUtf8 {
        field: &'static str,
    },
    CountTooLarge {
        field: &'static str,
        count: usize,
        maximum: usize,
    },
    TrailingBytes {
        count: usize,
    },
}

impl fmt::Display for ServerCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("server payload is empty"),
            Self::Truncated { field } => write!(formatter, "server field '{field}' is truncated"),
            Self::UnknownOpcode(opcode) => write!(formatter, "unknown server opcode {opcode}"),
            Self::UnsupportedSnapshotVersion { version, supported } => write!(
                formatter,
                "snapshot schema version {version} is unsupported; expected {supported}"
            ),
            Self::InvalidEnum { field, value } => {
                write!(
                    formatter,
                    "server field '{field}' has invalid value {value}"
                )
            }
            Self::InvalidZero { field } => {
                write!(formatter, "server field '{field}' must be nonzero")
            }
            Self::InvalidFloat { field } => {
                write!(formatter, "server field '{field}' is not finite")
            }
            Self::InvalidString { field } => {
                write!(formatter, "server field '{field}' is invalid")
            }
            Self::InvalidUtf8 { field } => {
                write!(formatter, "server field '{field}' is not UTF-8")
            }
            Self::CountTooLarge {
                field,
                count,
                maximum,
            } => write!(
                formatter,
                "server collection '{field}' has {count} entries; maximum is {maximum}"
            ),
            Self::TrailingBytes { count } => {
                write!(formatter, "server payload has {count} trailing bytes")
            }
        }
    }
}

impl std::error::Error for ServerCodecError {}

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
            ClientCommand::Authenticate {
                token: "dev-local".to_owned(),
            },
            ClientCommand::Join {
                name: "Aria".to_owned(),
                role: RoleCode::DamageDealer,
            },
            ClientCommand::EnterWorld,
            ClientCommand::ListCharacters,
            ClientCommand::SelectCharacter { character_id: 1 },
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
            ClientCommand::SelectCharacter { character_id: 0 }.encode_payload(),
            Err(CommandCodecError::InvalidZero {
                field: "character_id"
            })
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
    fn typed_server_snapshot_round_trips_nested_player_state() {
        let message = ServerMessage::Snapshot(WorldSnapshot {
            version: SNAPSHOT_SCHEMA_VERSION,
            tick: 42,
            player_count: 1,
            npc_count: 1,
            enemy_count: 1,
            vendor_count: 0,
            players: vec![PlayerState {
                player_id: 7,
                name: "Aria".to_owned(),
                role: RoleCode::Healer,
                position: PositionState { x: -2.5, y: 4.0 },
                health: 80,
                max_health: 100,
                target_id: Some(9),
                gold: 12,
                inventory_capacity: 16,
                inventory: vec![ItemStackState {
                    item_id: 2,
                    quantity: 4,
                }],
                quests: vec![QuestState {
                    quest_id: 1,
                    progress: 2,
                    required_count: 3,
                    status: QuestStatusCode::Accepted,
                }],
            }],
            npcs: vec![NpcState {
                entity_id: 9,
                template_id: 2,
                name: "Field Wolf".to_owned(),
                kind: NpcKindCode::Enemy,
                position: PositionState { x: 24.0, y: 0.0 },
                health: 25,
                max_health: 100,
            }],
        });

        let payload = message.encode_payload().expect("snapshot should encode");
        assert_eq!(ServerMessage::decode_payload(&payload), Ok(message));
        let envelope = Envelope::new(MessageKind::Event, payload)
            .expect("snapshot envelope should encode")
            .encode()
            .expect("snapshot frame should encode");
        assert_eq!(
            decode_one(&envelope).unwrap().envelope.kind,
            MessageKind::Event
        );
    }

    #[test]
    fn typed_snapshot_schema_rejects_an_unsupported_version() {
        let snapshot = WorldSnapshot {
            version: SNAPSHOT_SCHEMA_VERSION,
            tick: 1,
            player_count: 0,
            npc_count: 0,
            enemy_count: 0,
            vendor_count: 0,
            players: Vec::new(),
            npcs: Vec::new(),
        };
        let mut payload = ServerMessage::Snapshot(snapshot)
            .encode_payload()
            .expect("valid snapshot should encode");
        payload[1..3].copy_from_slice(&1_u16.to_be_bytes());
        assert_eq!(
            ServerMessage::decode_payload(&payload),
            Err(ServerCodecError::UnsupportedSnapshotVersion {
                version: 1,
                supported: SNAPSHOT_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn typed_server_events_round_trip_collection_and_optional_values() {
        let messages = [
            ServerMessage::Welcome {
                server: "mmorpg-server".to_owned(),
            },
            ServerMessage::Connected {
                player_id: 7,
                role: RoleCode::DamageDealer,
            },
            ServerMessage::Authenticated {
                account_id: 1,
                session_id: 11,
            },
            ServerMessage::CharacterList {
                account_id: 1,
                characters: vec![CharacterSummary {
                    character_id: 1,
                    name: "Aria".to_owned(),
                    role: RoleCode::DamageDealer,
                }],
            },
            ServerMessage::CharacterSelected {
                character_id: 1,
                name: "Aria".to_owned(),
                role: RoleCode::DamageDealer,
            },
            ServerMessage::Error {
                message: "connect first".to_owned(),
            },
            ServerMessage::Event(ServerEvent::VendorListed {
                player_id: 7,
                vendor_id: 1,
                listings: vec![VendorListingState {
                    item_id: 2,
                    name: "Town Ration".to_owned(),
                    unit_price: 2,
                    remaining_quantity: 98,
                    max_stack: 20,
                }],
            }),
            ServerMessage::Event(ServerEvent::QuestRewarded {
                player_id: 7,
                quest_id: 1,
                gold: 10,
                item_id: None,
                item_quantity: 0,
                gold_remaining: 22,
            }),
        ];

        for message in messages {
            let payload = message
                .encode_payload()
                .expect("server message should encode");
            assert_eq!(ServerMessage::decode_payload(&payload), Ok(message));
        }
    }

    #[test]
    fn typed_server_payload_rejects_trailing_bytes_and_oversized_collections() {
        let mut payload = ServerMessage::Error {
            message: "bad".to_owned(),
        }
        .encode_payload()
        .unwrap();
        payload.push(0);
        assert_eq!(
            ServerMessage::decode_payload(&payload),
            Err(ServerCodecError::TrailingBytes { count: 1 })
        );

        let snapshot = WorldSnapshot {
            version: SNAPSHOT_SCHEMA_VERSION,
            tick: 1,
            player_count: 0,
            npc_count: 0,
            enemy_count: 0,
            vendor_count: 0,
            players: Vec::new(),
            npcs: Vec::new(),
        };
        let mut payload = ServerMessage::Snapshot(snapshot).encode_payload().unwrap();
        let count_offset = 1 + 8 + 4 * 4;
        payload[count_offset..count_offset + 4]
            .copy_from_slice(&((MAX_SERVER_COLLECTION_ENTRIES as u32) + 1).to_be_bytes());
        assert!(matches!(
            ServerMessage::decode_payload(&payload),
            Err(ServerCodecError::CountTooLarge {
                field: "players",
                ..
            })
        ));
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
