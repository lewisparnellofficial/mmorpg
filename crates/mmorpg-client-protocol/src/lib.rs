//! Typed helpers for the temporary line-oriented development protocol.
//!
//! This crate constructs command lines and decodes the bounded, structured
//! result lines emitted by the temporary development server. It deliberately
//! does not open sockets or make authoritative decisions. The result decoder
//! accepts only the explicitly supported prefixes and field schemas; it is
//! not a general-purpose parser for server logs or human-readable prose.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub use mmorpg_core::{EntityId, ItemId, NpcKind, Position, QuestId, QuestStatus, Role, ZoneArea};

const MAX_NAME_BYTES: usize = 24;
const MAX_MOVE_PER_COMMAND: f32 = 10.0;

/// Maximum accepted size of one server result line, measured in UTF-8 bytes.
pub const MAX_SERVER_LINE_BYTES: usize = 4096;
/// Version of the temporary line snapshot contract. Version 2 carries each
/// player's inventory capacity explicitly instead of making the client assume
/// the starter value.
pub const TEMP_SNAPSHOT_VERSION: u32 = 2;
const MAX_SERVER_FIELD_BYTES: usize = 256;
const MAX_SERVER_NAME_BYTES: usize = 64;
const MAX_SERVER_REASON_BYTES: usize = 256;

/// Maximum number of records a [`SnapshotAssembler`] accepts in one frame.
///
/// The limit applies to the `WORLD`, `PLAYER`, and `NPC` records together.
/// It bounds the assembler's temporary maps and prevents an untrusted stream
/// from growing the in-progress snapshot without limit.
pub const MAX_SNAPSHOT_RECORDS: usize = 4096;

/// A validated command line without its trailing newline.
///
/// The line can be written to a line-oriented transport by appending exactly
/// one `\n`. Keeping this type opaque ensures that callers cannot accidentally
/// construct a line containing a command separator or control character.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolLine(String);

impl ProtocolLine {
    /// Returns the command line without a trailing newline.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the command line as owned text, without a trailing newline.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ProtocolLine {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ProtocolLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Reasons a typed command could not be safely encoded for the development
/// server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncodeError {
    EmptyName,
    NameTooLong { max_bytes: usize },
    NameContainsWhitespace,
    NameContainsControl,
    InvalidEntityId,
    InvalidItemId,
    InvalidQuestId,
    ZeroQuantity,
    NonFiniteMovement { axis: &'static str },
    MovementTooLarge { axis: &'static str },
    MovementMagnitudeTooLarge,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(formatter, "player name must not be empty"),
            Self::NameTooLong { max_bytes } => {
                write!(formatter, "player name must be at most {max_bytes} bytes")
            }
            Self::NameContainsWhitespace => {
                write!(formatter, "player name must not contain whitespace")
            }
            Self::NameContainsControl => {
                write!(formatter, "player name must not contain control characters")
            }
            Self::InvalidEntityId => write!(formatter, "entity ID must be greater than zero"),
            Self::InvalidItemId => write!(formatter, "item ID must be greater than zero"),
            Self::InvalidQuestId => write!(formatter, "quest ID must be greater than zero"),
            Self::ZeroQuantity => write!(formatter, "quantity must be greater than zero"),
            Self::NonFiniteMovement { axis } => {
                write!(formatter, "movement {axis} must be finite")
            }
            Self::MovementTooLarge { axis } => {
                write!(
                    formatter,
                    "movement {axis} must be between -{MAX_MOVE_PER_COMMAND} and {MAX_MOVE_PER_COMMAND}"
                )
            }
            Self::MovementMagnitudeTooLarge => {
                write!(
                    formatter,
                    "movement vector exceeds {MAX_MOVE_PER_COMMAND} units"
                )
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Constructors for commands accepted by the current development server.
///
/// These helpers model the connection-bound protocol. The server determines
/// the player ID from the connection after `connect`; no helper accepts a
/// caller-supplied player ID for gameplay commands.
pub struct CommandLine;

impl CommandLine {
    /// Encodes `connect <name> <role>`.
    pub fn connect(name: &str, role: Role) -> Result<ProtocolLine, EncodeError> {
        validate_name(name)?;
        Ok(line(format_args!("connect {name} {}", role.as_str())))
    }

    /// Encodes `move <dx> <dy>`.
    pub fn move_by(dx: f32, dy: f32) -> Result<ProtocolLine, EncodeError> {
        validate_movement(dx, "dx")?;
        validate_movement(dy, "dy")?;
        if dx.hypot(dy) > MAX_MOVE_PER_COMMAND {
            return Err(EncodeError::MovementMagnitudeTooLarge);
        }
        Ok(line(format_args!("move {dx} {dy}")))
    }

    /// Encodes `target <entity-id>`.
    pub fn target(entity_id: EntityId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(entity_id)?;
        Ok(line(format_args!("target {entity_id}")))
    }

    /// Encodes `attack`.
    pub fn attack() -> ProtocolLine {
        line(format_args!("attack"))
    }

    /// Encodes `vendor <vendor-id>`.
    pub fn vendor(vendor_id: EntityId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(vendor_id)?;
        Ok(line(format_args!("vendor {vendor_id}")))
    }

    /// Encodes `buy <vendor-id> <item-id> <quantity>`.
    pub fn buy(
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    ) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(vendor_id)?;
        validate_item_id(item_id)?;
        validate_quantity(quantity)?;
        Ok(line(format_args!("buy {vendor_id} {item_id} {quantity}")))
    }

    /// Encodes `loot <enemy-id>`.
    pub fn loot(enemy_id: EntityId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(enemy_id)?;
        Ok(line(format_args!("loot {enemy_id}")))
    }

    /// Encodes `quest-offers <npc-id>`.
    pub fn quest_offers(npc_id: EntityId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(npc_id)?;
        Ok(line(format_args!("quest-offers {npc_id}")))
    }

    /// Encodes `accept-quest <npc-id> <quest-id>`.
    pub fn accept_quest(npc_id: EntityId, quest_id: QuestId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(npc_id)?;
        validate_quest_id(quest_id)?;
        Ok(line(format_args!("accept-quest {npc_id} {quest_id}")))
    }

    /// Encodes `turn-in-quest <npc-id> <quest-id>`.
    pub fn turn_in_quest(npc_id: EntityId, quest_id: QuestId) -> Result<ProtocolLine, EncodeError> {
        validate_entity_id(npc_id)?;
        validate_quest_id(quest_id)?;
        Ok(line(format_args!("turn-in-quest {npc_id} {quest_id}")))
    }

    /// Encodes `state`.
    pub fn state() -> ProtocolLine {
        line(format_args!("state"))
    }

    /// Encodes `help`.
    pub fn help() -> ProtocolLine {
        line(format_args!("help"))
    }

    /// Encodes `inventory`.
    pub fn inventory() -> ProtocolLine {
        line(format_args!("inventory"))
    }

    /// Encodes `quit`.
    pub fn quit() -> ProtocolLine {
        line(format_args!("quit"))
    }
}

/// A decoded line from the temporary development server.
///
/// This is intentionally limited to the state and events needed by the
/// graphical client. Human-oriented lines such as `WELCOME` and `HELP` are
/// ignored; machine-readable snapshot and `EVENT` forms are decoded below.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerLine {
    SnapshotBegin { version: u32 },
    World(WorldState),
    Player(PlayerState),
    Npc(NpcState),
    Item(ItemState),
    Quest(QuestState),
    Connected(ConnectedState),
    Event(ServerEvent),
    SnapshotEnd,
}

/// A completed, validated temporary machine-readable snapshot.
///
/// The assembler publishes this value only after receiving a valid
/// `TEMP_SNAPSHOT_END`. Callers can replace their currently displayed state
/// with the returned value in one operation; a malformed or truncated frame
/// never produces a partial [`Snapshot`].
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub version: u32,
    pub world: WorldState,
    pub players: BTreeMap<EntityId, PlayerState>,
    pub npcs: BTreeMap<EntityId, NpcState>,
    pub items: BTreeMap<EntityId, Vec<ItemState>>,
    pub quests: BTreeMap<EntityId, Vec<QuestState>>,
}

/// The record kinds recognized by [`SnapshotAssembler`] errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotRecordKind {
    Begin,
    World,
    Player,
    Npc,
    Item,
    Quest,
    End,
}

impl fmt::Display for SnapshotRecordKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Begin => "begin",
            Self::World => "world",
            Self::Player => "player",
            Self::Npc => "npc",
            Self::Item => "item",
            Self::Quest => "quest",
            Self::End => "end",
        };
        formatter.write_str(name)
    }
}

/// Errors produced while assembling a framed temporary snapshot.
#[derive(Clone, Debug, PartialEq)]
pub enum SnapshotError {
    /// The individual line failed the existing bounded decoder.
    Decode(DecodeError),
    /// A snapshot marker or record was received without the required frame.
    SnapshotNotActive { record: SnapshotRecordKind },
    /// A second begin marker arrived before the active frame ended.
    DuplicateBegin,
    /// A world, player, NPC, item, or quest record was repeated. Entity IDs
    /// are unique across player and NPC records; item and quest keys are
    /// unique per player in a snapshot.
    DuplicateRecord {
        record: SnapshotRecordKind,
        id: Option<EntityId>,
    },
    /// An item or quest record referred to a player absent from the frame.
    UnknownPlayer {
        record: SnapshotRecordKind,
        player_id: EntityId,
    },
    /// An end marker arrived without an active frame.
    EndOutsideSnapshot,
    /// The frame ended without its required world record.
    MissingWorld,
    /// The stream ended while a frame was still in progress.
    IncompleteSnapshot,
    /// The frame exceeded the configured record bound.
    TooManyRecords { max_records: usize },
    /// A configured record bound of zero cannot accept a valid snapshot.
    InvalidRecordLimit,
    /// The world summary did not agree with the records carried by the frame.
    RecordCountMismatch {
        field: &'static str,
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "snapshot line rejected: {error}"),
            Self::SnapshotNotActive { record } => {
                write!(
                    formatter,
                    "snapshot {record} record received outside an active frame"
                )
            }
            Self::DuplicateBegin => formatter.write_str("duplicate snapshot begin marker"),
            Self::DuplicateRecord { record, id } => match id {
                Some(id) => write!(
                    formatter,
                    "duplicate snapshot {record} record for entity {id}"
                ),
                None => write!(formatter, "duplicate snapshot {record} record"),
            },
            Self::UnknownPlayer { record, player_id } => {
                write!(
                    formatter,
                    "snapshot {record} record refers to unknown player {player_id}"
                )
            }
            Self::EndOutsideSnapshot => {
                formatter.write_str("snapshot end marker received outside an active frame")
            }
            Self::MissingWorld => formatter.write_str("snapshot is missing its world record"),
            Self::IncompleteSnapshot => formatter.write_str("snapshot ended before its end marker"),
            Self::TooManyRecords { max_records } => {
                write!(formatter, "snapshot exceeds the {max_records}-record limit")
            }
            Self::InvalidRecordLimit => {
                formatter.write_str("snapshot record limit must be greater than zero")
            }
            Self::RecordCountMismatch {
                field,
                expected,
                actual,
            } => write!(
                formatter,
                "snapshot world field '{field}' expects {expected} records but received {actual}"
            ),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Incrementally validates and assembles the temporary snapshot framing.
///
/// Legacy `WORLD`, `PLAYER`, and `NPC` diagnostics, connection lines, and
/// events are ignored by this component because they are not part of a
/// `TEMP_SNAPSHOT` frame. Snapshot records are identified from their exact
/// `TEMP_SNAPSHOT` prefix before decoding, so a legacy record can never be
/// accidentally inserted into the pending snapshot.
#[derive(Debug)]
pub struct SnapshotAssembler {
    pending: Option<PendingSnapshot>,
    max_records: usize,
}

impl Default for SnapshotAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotAssembler {
    /// Creates an assembler with the repository-wide safe record bound.
    pub fn new() -> Self {
        Self {
            pending: None,
            max_records: MAX_SNAPSHOT_RECORDS,
        }
    }

    /// Creates an assembler with a smaller bound, useful for tests or a
    /// caller with a tighter content contract. The bound cannot exceed the
    /// fixed process-wide maximum.
    pub fn with_max_records(max_records: usize) -> Result<Self, SnapshotError> {
        if max_records == 0 || max_records > MAX_SNAPSHOT_RECORDS {
            return Err(SnapshotError::InvalidRecordLimit);
        }
        Ok(Self {
            pending: None,
            max_records,
        })
    }

    /// Returns whether a snapshot frame is currently being buffered.
    pub fn is_active(&self) -> bool {
        self.pending.is_some()
    }

    /// Feeds one complete server line into the assembler.
    ///
    /// `Ok(Some(snapshot))` is returned exactly once for a valid completed
    /// frame, at the `TEMP_SNAPSHOT_END` line. Other valid lines return
    /// `Ok(None)`. Any snapshot framing or validation error discards the
    /// in-progress frame, leaving it safe for the caller to retain its last
    /// completed snapshot and start again.
    pub fn push_line(&mut self, line: &str) -> Result<Option<Snapshot>, SnapshotError> {
        match snapshot_line_kind(line) {
            Some(SnapshotRecordKind::Begin) => self.push_begin(line),
            Some(SnapshotRecordKind::World)
            | Some(SnapshotRecordKind::Player)
            | Some(SnapshotRecordKind::Npc)
            | Some(SnapshotRecordKind::Item)
            | Some(SnapshotRecordKind::Quest) => self.push_record(line),
            Some(SnapshotRecordKind::End) => self.push_end(line),
            None if line.split_whitespace().next() == Some("TEMP_SNAPSHOT") => {
                self.push_record(line)
            }
            None => Ok(None),
        }
    }

    /// Reports and discards a truncated frame at end-of-stream.
    pub fn finish(&mut self) -> Result<(), SnapshotError> {
        if self.pending.take().is_some() {
            Err(SnapshotError::IncompleteSnapshot)
        } else {
            Ok(())
        }
    }

    fn push_begin(&mut self, line: &str) -> Result<Option<Snapshot>, SnapshotError> {
        if self.pending.is_some() {
            return self.abort(SnapshotError::DuplicateBegin);
        }
        let decoded = self.decode_snapshot_line(line)?;
        let ServerLine::SnapshotBegin { version } = decoded else {
            unreachable!("snapshot line classifier and decoder disagree");
        };
        self.pending = Some(PendingSnapshot::new(version));
        Ok(None)
    }

    fn push_record(&mut self, line: &str) -> Result<Option<Snapshot>, SnapshotError> {
        let decoded = self.decode_snapshot_line(line)?;
        let Some(pending) = self.pending.as_ref() else {
            let record = snapshot_record_kind(&decoded);
            return Err(SnapshotError::SnapshotNotActive { record });
        };

        let record = snapshot_record_kind(&decoded);
        let duplicate = match &decoded {
            ServerLine::World(_) => pending.world.is_some(),
            ServerLine::Player(player) => pending.ids.contains(&player.id),
            ServerLine::Npc(npc) => pending.ids.contains(&npc.id),
            ServerLine::Item(item) => pending.item_keys.contains(&(item.player_id, item.item_id)),
            ServerLine::Quest(quest) => pending
                .quest_keys
                .contains(&(quest.player_id, quest.quest_id)),
            _ => unreachable!("snapshot record classifier and decoder disagree"),
        };
        if duplicate {
            let id = match decoded {
                ServerLine::Player(player) => Some(player.id),
                ServerLine::Npc(npc) => Some(npc.id),
                ServerLine::Item(item) => Some(item.player_id),
                ServerLine::Quest(quest) => Some(quest.player_id),
                ServerLine::World(_) => None,
                _ => unreachable!("snapshot record classifier and decoder disagree"),
            };
            return self.abort(SnapshotError::DuplicateRecord { record, id });
        }
        if pending.record_count >= self.max_records {
            return self.abort(SnapshotError::TooManyRecords {
                max_records: self.max_records,
            });
        }

        let pending = self
            .pending
            .as_mut()
            .expect("pending snapshot was checked above");
        pending.record_count += 1;
        match decoded {
            ServerLine::World(world) => pending.world = Some(world),
            ServerLine::Player(player) => {
                pending.ids.insert(player.id);
                pending.players.insert(player.id, player);
            }
            ServerLine::Npc(npc) => {
                pending.ids.insert(npc.id);
                pending.npcs.insert(npc.id, npc);
            }
            ServerLine::Item(item) => {
                pending.item_keys.insert((item.player_id, item.item_id));
                pending.items.entry(item.player_id).or_default().push(item);
            }
            ServerLine::Quest(quest) => {
                pending.quest_keys.insert((quest.player_id, quest.quest_id));
                pending
                    .quests
                    .entry(quest.player_id)
                    .or_default()
                    .push(quest);
            }
            _ => unreachable!("snapshot record classifier and decoder disagree"),
        }
        Ok(None)
    }

    fn push_end(&mut self, line: &str) -> Result<Option<Snapshot>, SnapshotError> {
        if self.pending.is_none() {
            return Err(SnapshotError::EndOutsideSnapshot);
        }
        let decoded = self.decode_snapshot_line(line)?;
        if !matches!(decoded, ServerLine::SnapshotEnd) {
            unreachable!("snapshot line classifier and decoder disagree");
        }
        let mut pending = self
            .pending
            .take()
            .expect("pending snapshot was checked above");
        let Some(world) = pending.world.as_ref() else {
            return Err(SnapshotError::MissingWorld);
        };
        pending.validate_counts(world)?;
        let world = pending
            .world
            .take()
            .expect("world was checked immediately above");
        Ok(Some(Snapshot {
            version: pending.version,
            world,
            players: pending.players,
            npcs: pending.npcs,
            items: pending.items,
            quests: pending.quests,
        }))
    }

    fn decode_snapshot_line(&mut self, line: &str) -> Result<ServerLine, SnapshotError> {
        decode_server_line(line).map_err(|error| self.abort_error(SnapshotError::Decode(error)))
    }

    fn abort<T>(&mut self, error: SnapshotError) -> Result<T, SnapshotError> {
        self.pending = None;
        Err(error)
    }

    fn abort_error(&mut self, error: SnapshotError) -> SnapshotError {
        self.pending = None;
        error
    }
}

#[derive(Debug)]
struct PendingSnapshot {
    version: u32,
    world: Option<WorldState>,
    players: BTreeMap<EntityId, PlayerState>,
    npcs: BTreeMap<EntityId, NpcState>,
    items: BTreeMap<EntityId, Vec<ItemState>>,
    quests: BTreeMap<EntityId, Vec<QuestState>>,
    ids: BTreeSet<EntityId>,
    item_keys: BTreeSet<(EntityId, ItemId)>,
    quest_keys: BTreeSet<(EntityId, QuestId)>,
    record_count: usize,
}

impl PendingSnapshot {
    fn new(version: u32) -> Self {
        Self {
            version,
            world: None,
            players: BTreeMap::new(),
            npcs: BTreeMap::new(),
            items: BTreeMap::new(),
            quests: BTreeMap::new(),
            ids: BTreeSet::new(),
            item_keys: BTreeSet::new(),
            quest_keys: BTreeSet::new(),
            record_count: 0,
        }
    }

    fn validate_counts(&self, world: &WorldState) -> Result<(), SnapshotError> {
        let counts = [
            ("players", world.players, self.players.len() as u64),
            ("npcs", world.npcs, self.npcs.len() as u64),
            (
                "enemies",
                world.enemies,
                self.npcs
                    .values()
                    .filter(|npc| npc.kind == NpcKind::Enemy)
                    .count() as u64,
            ),
            (
                "vendors",
                world.vendors,
                self.npcs
                    .values()
                    .filter(|npc| npc.kind == NpcKind::Vendor)
                    .count() as u64,
            ),
        ];
        for (field, expected, actual) in counts {
            if expected != actual {
                return Err(SnapshotError::RecordCountMismatch {
                    field,
                    expected,
                    actual,
                });
            }
        }
        for item in self.items.values().flatten() {
            if !self.players.contains_key(&item.player_id) {
                return Err(SnapshotError::UnknownPlayer {
                    record: SnapshotRecordKind::Item,
                    player_id: item.player_id,
                });
            }
        }
        for quest in self.quests.values().flatten() {
            if !self.players.contains_key(&quest.player_id) {
                return Err(SnapshotError::UnknownPlayer {
                    record: SnapshotRecordKind::Quest,
                    player_id: quest.player_id,
                });
            }
        }
        Ok(())
    }
}

fn snapshot_line_kind(line: &str) -> Option<SnapshotRecordKind> {
    match line.split_whitespace().next()? {
        "TEMP_SNAPSHOT_BEGIN" => Some(SnapshotRecordKind::Begin),
        "TEMP_SNAPSHOT" => match line.split_whitespace().nth(1) {
            Some("WORLD") => Some(SnapshotRecordKind::World),
            Some("PLAYER") => Some(SnapshotRecordKind::Player),
            Some("NPC") => Some(SnapshotRecordKind::Npc),
            Some("ITEM") => Some(SnapshotRecordKind::Item),
            Some("QUEST") => Some(SnapshotRecordKind::Quest),
            _ => None,
        },
        "TEMP_SNAPSHOT_END" => Some(SnapshotRecordKind::End),
        _ => None,
    }
}

fn snapshot_record_kind(line: &ServerLine) -> SnapshotRecordKind {
    match line {
        ServerLine::World(_) => SnapshotRecordKind::World,
        ServerLine::Player(_) => SnapshotRecordKind::Player,
        ServerLine::Npc(_) => SnapshotRecordKind::Npc,
        ServerLine::Item(_) => SnapshotRecordKind::Item,
        ServerLine::Quest(_) => SnapshotRecordKind::Quest,
        _ => unreachable!("only snapshot records reach this helper"),
    }
}

/// Counts and simulation time from a `WORLD` line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldState {
    pub tick: u64,
    pub players: u64,
    pub npcs: u64,
    pub enemies: u64,
    pub vendors: u64,
}

/// The public player fields currently emitted by a `PLAYER` line.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub id: EntityId,
    pub name: String,
    pub role: Role,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
    pub gold: u32,
    /// Present for snapshot records. Legacy diagnostic `PLAYER` lines do not
    /// carry inventory state and therefore leave this unset.
    pub inventory_capacity: Option<usize>,
    pub target: Option<EntityId>,
}

/// The public NPC fields currently emitted by an `NPC` line.
#[derive(Clone, Debug, PartialEq)]
pub struct NpcState {
    pub id: EntityId,
    /// Present for `TEMP_SNAPSHOT NPC` records and absent from legacy `NPC`
    /// diagnostics, which do not include the content template ID.
    pub template_id: Option<u64>,
    pub name: String,
    pub kind: NpcKind,
    pub position: Position,
    pub health: u32,
    pub max_health: u32,
}

/// One inventory stack carried by a temporary authoritative snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemState {
    pub player_id: EntityId,
    pub item_id: ItemId,
    pub quantity: u32,
}

/// One quest state carried by a temporary authoritative snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestState {
    pub player_id: EntityId,
    pub quest_id: QuestId,
    pub progress: u32,
    pub required_count: u32,
    pub status: QuestStatus,
}

/// One vendor listing carried by a temporary authoritative event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VendorListingState {
    pub item_id: ItemId,
    pub name: String,
    pub unit_price: u32,
    pub remaining_quantity: u32,
    pub max_stack: u32,
}

/// A quest offer carried by a temporary authoritative event.
///
/// The event intentionally carries only the stable ID and display name. The
/// client can resolve the description from its matching validated content
/// package instead of treating server-provided prose as executable data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestOfferState {
    pub quest_id: QuestId,
    pub name: String,
}

/// The connection-to-player binding emitted after a successful join.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectedState {
    pub player_id: EntityId,
    pub role: Role,
}

/// The explicitly supported development-server events.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    PlayerJoined {
        id: EntityId,
        name: String,
        role: Role,
        position: Position,
    },
    PlayerLeft {
        player_id: EntityId,
    },
    PlayerMoved {
        id: EntityId,
        position: Position,
        area: ZoneArea,
    },
    TargetSelected {
        player_id: EntityId,
        target_id: EntityId,
    },
    AttackResolved {
        player_id: EntityId,
        target_id: EntityId,
        damage: u32,
        target_health: u32,
    },
    EnemyAttackResolved {
        enemy_id: EntityId,
        target_id: EntityId,
        damage: u32,
        target_health: u32,
    },
    PlayerDefeated {
        player_id: EntityId,
    },
    EnemyDefeated {
        enemy_id: EntityId,
    },
    VendorListed {
        player_id: EntityId,
        vendor_id: EntityId,
        listings: Vec<VendorListingState>,
    },
    ItemPurchased {
        player_id: EntityId,
        vendor_id: EntityId,
        item_id: ItemId,
        quantity: u32,
        total_price: u32,
        gold_remaining: u32,
    },
    LootRewarded {
        player_id: EntityId,
        enemy_id: EntityId,
        item_id: ItemId,
        quantity: u32,
    },
    TransactionRejected {
        player_id: EntityId,
        reason: String,
    },
    QuestOffersListed {
        player_id: EntityId,
        npc_id: EntityId,
        quests: Vec<QuestOfferState>,
    },
    QuestAccepted {
        player_id: EntityId,
        npc_id: EntityId,
        quest_id: QuestId,
    },
    QuestProgressed {
        player_id: EntityId,
        quest_id: QuestId,
        progress: u32,
        required_count: u32,
    },
    QuestCompleted {
        player_id: EntityId,
        quest_id: QuestId,
    },
    QuestRewarded {
        player_id: EntityId,
        quest_id: QuestId,
        gold: u32,
        item_id: Option<ItemId>,
        item_quantity: u32,
        gold_remaining: u32,
    },
    QuestRejected {
        player_id: EntityId,
        reason: String,
    },
    CommandRejected {
        reason: String,
    },
}

/// Errors returned when a server result line does not match the bounded
/// machine-readable development schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    EmptyLine,
    LineTooLong {
        max_bytes: usize,
    },
    LineContainsControl,
    LeadingOrTrailingWhitespace,
    UnknownPrefix {
        prefix: String,
    },
    UnsupportedEvent {
        name: String,
    },
    UnsupportedSnapshotRecord {
        name: String,
    },
    UnsupportedSnapshotVersion {
        version: u32,
    },
    MissingField {
        field: &'static str,
    },
    DuplicateField {
        field: String,
    },
    UnknownField {
        field: String,
    },
    MalformedField {
        token: String,
    },
    EmptyField {
        field: &'static str,
    },
    FieldTooLong {
        field: &'static str,
        max_bytes: usize,
    },
    InvalidValue {
        field: &'static str,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLine => formatter.write_str("server line must not be empty"),
            Self::LineTooLong { max_bytes } => {
                write!(formatter, "server line exceeds {max_bytes} bytes")
            }
            Self::LineContainsControl => {
                formatter.write_str("server line must not contain control characters")
            }
            Self::LeadingOrTrailingWhitespace => {
                formatter.write_str("server line must not have leading or trailing whitespace")
            }
            Self::UnknownPrefix { prefix } => {
                write!(formatter, "unknown server line prefix '{prefix}'")
            }
            Self::UnsupportedEvent { name } => {
                write!(formatter, "unsupported server event '{name}'")
            }
            Self::UnsupportedSnapshotRecord { name } => {
                write!(formatter, "unsupported snapshot record '{name}'")
            }
            Self::UnsupportedSnapshotVersion { version } => {
                write!(formatter, "unsupported snapshot version {version}")
            }
            Self::MissingField { field } => write!(formatter, "missing server field '{field}'"),
            Self::DuplicateField { field } => write!(formatter, "duplicate server field '{field}'"),
            Self::UnknownField { field } => write!(formatter, "unknown server field '{field}'"),
            Self::MalformedField { token } => write!(formatter, "malformed server field '{token}'"),
            Self::EmptyField { field } => {
                write!(formatter, "server field '{field}' must not be empty")
            }
            Self::FieldTooLong { field, max_bytes } => {
                write!(
                    formatter,
                    "server field '{field}' exceeds {max_bytes} bytes"
                )
            }
            Self::InvalidValue { field } => {
                write!(formatter, "invalid value for server field '{field}'")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

impl ServerLine {
    /// Decodes one complete server line without a trailing newline.
    pub fn decode(line: &str) -> Result<Self, DecodeError> {
        decode_server_line(line)
    }
}

/// Decodes one complete server line without a trailing newline.
///
/// The decoder is deliberately schema-first: it does not try to interpret
/// `WELCOME`, `HELP`, `ERR`, or any other diagnostic as state. Values are
/// separated by ASCII whitespace, except that the schema-defined `name` and
/// `reason` fields may contain spaces until the next `key=` field.
pub fn decode_server_line(line: &str) -> Result<ServerLine, DecodeError> {
    if line.is_empty() {
        return Err(DecodeError::EmptyLine);
    }
    if line.len() > MAX_SERVER_LINE_BYTES {
        return Err(DecodeError::LineTooLong {
            max_bytes: MAX_SERVER_LINE_BYTES,
        });
    }
    if line.chars().any(char::is_control) {
        return Err(DecodeError::LineContainsControl);
    }
    if line.trim() != line {
        return Err(DecodeError::LeadingOrTrailingWhitespace);
    }

    let mut tokens = line.split_whitespace();
    let prefix = tokens.next().ok_or(DecodeError::EmptyLine)?;
    match prefix {
        "TEMP_SNAPSHOT_BEGIN" => decode_snapshot_begin(tokens.collect()),
        "WORLD" => decode_world(parse_fields(
            tokens.collect(),
            &["tick", "players", "npcs", "enemies", "vendors"],
            None,
        )?),
        "PLAYER" => decode_player(parse_fields(
            tokens.collect(),
            &["id", "name", "role", "pos", "hp", "gold", "target"],
            None,
        )?),
        "NPC" => decode_npc(parse_fields(
            tokens.collect(),
            &["id", "name", "kind", "pos", "hp"],
            Some("name"),
        )?),
        "CONNECTED" => decode_connected(parse_fields(
            tokens.collect(),
            &["player_id", "role"],
            None,
        )?),
        "EVENT" => {
            let event_name = tokens
                .next()
                .ok_or(DecodeError::MissingField { field: "event" })?;
            decode_event(event_name, tokens.collect())
        }
        "TEMP_SNAPSHOT" => {
            let record_name = tokens
                .next()
                .ok_or(DecodeError::MissingField { field: "record" })?;
            decode_snapshot_record(record_name, tokens.collect())
        }
        "TEMP_SNAPSHOT_END" => {
            if let Some(token) = tokens.next() {
                return Err(DecodeError::MalformedField {
                    token: token.to_owned(),
                });
            }
            Ok(ServerLine::SnapshotEnd)
        }
        other => Err(DecodeError::UnknownPrefix {
            prefix: other.to_owned(),
        }),
    }
}

fn decode_world(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    Ok(ServerLine::World(WorldState {
        tick: parse_u64(take(&mut fields, "tick")?, "tick")?,
        players: parse_u64(take(&mut fields, "players")?, "players")?,
        npcs: parse_u64(take(&mut fields, "npcs")?, "npcs")?,
        enemies: parse_u64(take(&mut fields, "enemies")?, "enemies")?,
        vendors: parse_u64(take(&mut fields, "vendors")?, "vendors")?,
    }))
}

fn decode_player(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    let (health, max_health) = parse_health(take(&mut fields, "hp")?, "hp")?;
    Ok(ServerLine::Player(PlayerState {
        id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        name: parse_string(take(&mut fields, "name")?, "name", MAX_NAME_BYTES)?,
        role: parse_role(take(&mut fields, "role")?, "role")?,
        position: parse_position(take(&mut fields, "pos")?, "pos")?,
        health,
        max_health,
        gold: parse_u32(take(&mut fields, "gold")?, "gold")?,
        inventory_capacity: None,
        target: parse_target(take(&mut fields, "target")?, "target")?,
    }))
}

fn decode_npc(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    let (health, max_health) = parse_health(take(&mut fields, "hp")?, "hp")?;
    Ok(ServerLine::Npc(NpcState {
        id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        template_id: None,
        name: parse_string(take(&mut fields, "name")?, "name", MAX_SERVER_NAME_BYTES)?,
        kind: parse_npc_kind(take(&mut fields, "kind")?, "kind")?,
        position: parse_position(take(&mut fields, "pos")?, "pos")?,
        health,
        max_health,
    }))
}

fn decode_snapshot_begin(tokens: Vec<&str>) -> Result<ServerLine, DecodeError> {
    let mut fields = parse_fields(tokens, &["version"], None)?;
    let version = parse_u32(take(&mut fields, "version")?, "version")?;
    if version != TEMP_SNAPSHOT_VERSION {
        return Err(DecodeError::UnsupportedSnapshotVersion { version });
    }
    Ok(ServerLine::SnapshotBegin { version })
}

fn decode_snapshot_record(record_name: &str, tokens: Vec<&str>) -> Result<ServerLine, DecodeError> {
    match record_name {
        "WORLD" => decode_snapshot_world(parse_fields(
            tokens,
            &["tick", "players", "npcs", "enemies", "vendors"],
            None,
        )?),
        "PLAYER" => decode_snapshot_player(parse_fields(
            tokens,
            &[
                "id",
                "name",
                "role",
                "position",
                "health",
                "max_health",
                "gold",
                "target",
                "capacity",
            ],
            None,
        )?),
        "NPC" => decode_snapshot_npc(parse_fields(
            tokens,
            &[
                "id",
                "template_id",
                "name",
                "kind",
                "position",
                "health",
                "max_health",
            ],
            None,
        )?),
        "ITEM" => {
            decode_snapshot_item(parse_fields(tokens, &["player", "item", "quantity"], None)?)
        }
        "QUEST" => decode_snapshot_quest(parse_fields(
            tokens,
            &["player", "quest", "progress", "status"],
            None,
        )?),
        other => Err(DecodeError::UnsupportedSnapshotRecord {
            name: other.to_owned(),
        }),
    }
}

fn decode_snapshot_world(fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    decode_world(fields)
}

fn decode_snapshot_player(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    let health = parse_u32(take(&mut fields, "health")?, "health")?;
    let max_health = parse_u32(take(&mut fields, "max_health")?, "max_health")?;
    validate_health(health, max_health, "health")?;
    let encoded_name = take(&mut fields, "name")?;
    Ok(ServerLine::Player(PlayerState {
        id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        name: parse_snapshot_string(encoded_name, "name", MAX_SERVER_NAME_BYTES)?,
        role: parse_role(take(&mut fields, "role")?, "role")?,
        position: parse_position(take(&mut fields, "position")?, "position")?,
        health,
        max_health,
        gold: parse_u32(take(&mut fields, "gold")?, "gold")?,
        inventory_capacity: Some(parse_u32(take(&mut fields, "capacity")?, "capacity")? as usize),
        target: parse_target(take(&mut fields, "target")?, "target")?,
    }))
}

fn decode_snapshot_npc(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    let health = parse_u32(take(&mut fields, "health")?, "health")?;
    let max_health = parse_u32(take(&mut fields, "max_health")?, "max_health")?;
    validate_health(health, max_health, "health")?;
    let template_id = parse_u64(take(&mut fields, "template_id")?, "template_id")?;
    if template_id == 0 {
        return Err(DecodeError::InvalidValue {
            field: "template_id",
        });
    }
    let encoded_name = take(&mut fields, "name")?;
    Ok(ServerLine::Npc(NpcState {
        id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        template_id: Some(template_id),
        name: parse_snapshot_string(encoded_name, "name", MAX_SERVER_NAME_BYTES)?,
        kind: parse_snapshot_npc_kind(take(&mut fields, "kind")?, "kind")?,
        position: parse_position(take(&mut fields, "position")?, "position")?,
        health,
        max_health,
    }))
}

fn decode_snapshot_item(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    Ok(ServerLine::Item(ItemState {
        player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
        item_id: parse_item_id(take(&mut fields, "item")?, "item")?,
        quantity: parse_positive_u32(take(&mut fields, "quantity")?, "quantity")?,
    }))
}

fn decode_snapshot_quest(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    let (progress, required_count) = parse_progress(take(&mut fields, "progress")?, "progress")?;
    Ok(ServerLine::Quest(QuestState {
        player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
        quest_id: parse_quest_id(take(&mut fields, "quest")?, "quest")?,
        progress,
        required_count,
        status: parse_quest_status(take(&mut fields, "status")?, "status")?,
    }))
}

fn decode_connected(mut fields: BTreeMap<String, String>) -> Result<ServerLine, DecodeError> {
    Ok(ServerLine::Connected(ConnectedState {
        player_id: parse_entity_id(take(&mut fields, "player_id")?, "player_id")?,
        role: parse_role(take(&mut fields, "role")?, "role")?,
    }))
}

fn decode_event(event_name: &str, tokens: Vec<&str>) -> Result<ServerLine, DecodeError> {
    let (allowed, free_field): (&[&str], Option<&str>) = match event_name {
        "player_joined" => (&["id", "name", "role", "pos"], Some("name")),
        "player_left" => (&["id"], None),
        "player_moved" => (&["id", "pos", "area"], None),
        "target_selected" => (&["player", "target"], None),
        "attack" => (&["player", "target", "damage", "target_hp"], None),
        "enemy_attack" => (&["enemy", "target", "damage", "target_hp"], None),
        "player_defeated" => (&["id"], None),
        "enemy_defeated" => (&["id"], None),
        "vendor_listed" => (&["player", "vendor", "listings"], Some("listings")),
        "item_purchased" => (
            &[
                "player",
                "vendor",
                "item",
                "quantity",
                "total_price",
                "gold",
            ],
            None,
        ),
        "loot_rewarded" => (&["player", "enemy", "item", "quantity"], None),
        "transaction_rejected" => (&["player", "reason"], Some("reason")),
        "quest_offers" => (&["player", "npc", "quests"], Some("quests")),
        "quest_accepted" => (&["player", "npc", "quest"], None),
        "quest_progressed" => (&["player", "quest", "progress"], None),
        "quest_completed" => (&["player", "quest"], None),
        "quest_rewarded" => (
            &[
                "player",
                "quest",
                "gold",
                "item",
                "quantity",
                "gold_remaining",
            ],
            None,
        ),
        "quest_rejected" => (&["player", "reason"], Some("reason")),
        "rejected" => (&["reason"], Some("reason")),
        other => {
            return Err(DecodeError::UnsupportedEvent {
                name: other.to_owned(),
            });
        }
    };
    let mut fields = parse_fields(tokens, allowed, free_field)?;
    let event = match event_name {
        "player_joined" => ServerEvent::PlayerJoined {
            id: parse_entity_id(take(&mut fields, "id")?, "id")?,
            name: parse_string(take(&mut fields, "name")?, "name", MAX_SERVER_NAME_BYTES)?,
            role: parse_role(take(&mut fields, "role")?, "role")?,
            position: parse_position(take(&mut fields, "pos")?, "pos")?,
        },
        "player_left" => ServerEvent::PlayerLeft {
            player_id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        },
        "player_moved" => ServerEvent::PlayerMoved {
            id: parse_entity_id(take(&mut fields, "id")?, "id")?,
            position: parse_position(take(&mut fields, "pos")?, "pos")?,
            area: parse_zone_area(take(&mut fields, "area")?, "area")?,
        },
        "target_selected" => ServerEvent::TargetSelected {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            target_id: parse_entity_id(take(&mut fields, "target")?, "target")?,
        },
        "attack" => ServerEvent::AttackResolved {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            target_id: parse_entity_id(take(&mut fields, "target")?, "target")?,
            damage: parse_u32(take(&mut fields, "damage")?, "damage")?,
            target_health: parse_u32(take(&mut fields, "target_hp")?, "target_hp")?,
        },
        "enemy_attack" => ServerEvent::EnemyAttackResolved {
            enemy_id: parse_entity_id(take(&mut fields, "enemy")?, "enemy")?,
            target_id: parse_entity_id(take(&mut fields, "target")?, "target")?,
            damage: parse_u32(take(&mut fields, "damage")?, "damage")?,
            target_health: parse_u32(take(&mut fields, "target_hp")?, "target_hp")?,
        },
        "player_defeated" => ServerEvent::PlayerDefeated {
            player_id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        },
        "enemy_defeated" => ServerEvent::EnemyDefeated {
            enemy_id: parse_entity_id(take(&mut fields, "id")?, "id")?,
        },
        "vendor_listed" => ServerEvent::VendorListed {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            vendor_id: parse_entity_id(take(&mut fields, "vendor")?, "vendor")?,
            listings: parse_vendor_listings(take(&mut fields, "listings")?)?,
        },
        "item_purchased" => ServerEvent::ItemPurchased {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            vendor_id: parse_entity_id(take(&mut fields, "vendor")?, "vendor")?,
            item_id: parse_item_id(take(&mut fields, "item")?, "item")?,
            quantity: parse_positive_u32(take(&mut fields, "quantity")?, "quantity")?,
            total_price: parse_u32(take(&mut fields, "total_price")?, "total_price")?,
            gold_remaining: parse_u32(take(&mut fields, "gold")?, "gold")?,
        },
        "loot_rewarded" => ServerEvent::LootRewarded {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            enemy_id: parse_entity_id(take(&mut fields, "enemy")?, "enemy")?,
            item_id: parse_item_id(take(&mut fields, "item")?, "item")?,
            quantity: parse_positive_u32(take(&mut fields, "quantity")?, "quantity")?,
        },
        "transaction_rejected" => ServerEvent::TransactionRejected {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            reason: parse_string(
                take(&mut fields, "reason")?,
                "reason",
                MAX_SERVER_REASON_BYTES,
            )?,
        },
        "quest_offers" => ServerEvent::QuestOffersListed {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            npc_id: parse_entity_id(take(&mut fields, "npc")?, "npc")?,
            quests: parse_quest_offers(take(&mut fields, "quests")?)?,
        },
        "quest_accepted" => ServerEvent::QuestAccepted {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            npc_id: parse_entity_id(take(&mut fields, "npc")?, "npc")?,
            quest_id: parse_quest_id(take(&mut fields, "quest")?, "quest")?,
        },
        "quest_progressed" => {
            let (progress, required_count) =
                parse_progress(take(&mut fields, "progress")?, "progress")?;
            ServerEvent::QuestProgressed {
                player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
                quest_id: parse_quest_id(take(&mut fields, "quest")?, "quest")?,
                progress,
                required_count,
            }
        }
        "quest_completed" => ServerEvent::QuestCompleted {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            quest_id: parse_quest_id(take(&mut fields, "quest")?, "quest")?,
        },
        "quest_rewarded" => {
            let item = take(&mut fields, "item")?;
            let item_id = (item != "none")
                .then(|| parse_item_id(item, "item"))
                .transpose()?;
            let item_quantity = parse_u32(take(&mut fields, "quantity")?, "quantity")?;
            if item_id.is_none() && item_quantity != 0 {
                return Err(DecodeError::InvalidValue { field: "quantity" });
            }
            if item_id.is_some() && item_quantity == 0 {
                return Err(DecodeError::InvalidValue { field: "quantity" });
            }
            ServerEvent::QuestRewarded {
                player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
                quest_id: parse_quest_id(take(&mut fields, "quest")?, "quest")?,
                gold: parse_u32(take(&mut fields, "gold")?, "gold")?,
                item_id,
                item_quantity,
                gold_remaining: parse_u32(take(&mut fields, "gold_remaining")?, "gold_remaining")?,
            }
        }
        "quest_rejected" => ServerEvent::QuestRejected {
            player_id: parse_entity_id(take(&mut fields, "player")?, "player")?,
            reason: parse_string(
                take(&mut fields, "reason")?,
                "reason",
                MAX_SERVER_REASON_BYTES,
            )?,
        },
        "rejected" => ServerEvent::CommandRejected {
            reason: parse_string(
                take(&mut fields, "reason")?,
                "reason",
                MAX_SERVER_REASON_BYTES,
            )?,
        },
        _ => unreachable!("event name was validated above"),
    };
    Ok(ServerLine::Event(event))
}

fn parse_vendor_listings(value: String) -> Result<Vec<VendorListingState>, DecodeError> {
    value
        .split(';')
        .map(|entry| {
            let mut fields = parse_fields(
                entry.split_whitespace().collect(),
                &["item", "name", "price", "stock", "max_stack"],
                None,
            )?;
            let max_stack = parse_positive_u32(take(&mut fields, "max_stack")?, "max_stack")?;
            Ok(VendorListingState {
                item_id: parse_item_id(take(&mut fields, "item")?, "item")?,
                name: parse_display_name(take(&mut fields, "name")?, "name")?,
                unit_price: parse_u32(take(&mut fields, "price")?, "price")?,
                remaining_quantity: parse_u32(take(&mut fields, "stock")?, "stock")?,
                max_stack,
            })
        })
        .collect()
}

fn parse_quest_offers(value: String) -> Result<Vec<QuestOfferState>, DecodeError> {
    value
        .split(';')
        .map(|entry| {
            let mut fields =
                parse_fields(entry.split_whitespace().collect(), &["id", "name"], None)?;
            Ok(QuestOfferState {
                quest_id: parse_quest_id(take(&mut fields, "id")?, "id")?,
                name: parse_display_name(take(&mut fields, "name")?, "name")?,
            })
        })
        .collect()
}

fn parse_display_name(value: String, field: &'static str) -> Result<String, DecodeError> {
    let value = value.replace('_', " ");
    parse_string(value, field, MAX_SERVER_FIELD_BYTES)
}

fn parse_progress(value: String, field: &'static str) -> Result<(u32, u32), DecodeError> {
    let Some((progress, required_count)) = value.split_once('/') else {
        return Err(DecodeError::InvalidValue { field });
    };
    if required_count.contains('/') {
        return Err(DecodeError::InvalidValue { field });
    }
    let progress = parse_u32(progress.to_owned(), field)?;
    let required_count = parse_positive_u32(required_count.to_owned(), field)?;
    if progress > required_count {
        return Err(DecodeError::InvalidValue { field });
    }
    Ok((progress, required_count))
}

fn parse_fields(
    tokens: Vec<&str>,
    allowed: &[&'static str],
    free_field: Option<&str>,
) -> Result<BTreeMap<String, String>, DecodeError> {
    let mut fields = BTreeMap::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let Some((key, initial_value)) = token.split_once('=') else {
            return Err(DecodeError::MalformedField {
                token: token.to_owned(),
            });
        };
        if !allowed.contains(&key) {
            return Err(DecodeError::UnknownField {
                field: key.to_owned(),
            });
        }
        let field_name = allowed
            .iter()
            .copied()
            .find(|allowed_key| *allowed_key == key)
            .expect("the field was found in the allowed schema");
        if fields.contains_key(key) {
            return Err(DecodeError::DuplicateField {
                field: key.to_owned(),
            });
        }

        let mut value = initial_value.to_owned();
        index += 1;
        if free_field == Some(key) {
            // Compound fields contain their own key/value pairs, so their
            // inner `name=` tokens must not terminate the outer field.
            let consumes_remaining_tokens = matches!(key, "listings" | "quests");
            while index < tokens.len()
                && (consumes_remaining_tokens || !tokens[index].contains('='))
            {
                value.push(' ');
                value.push_str(tokens[index]);
                index += 1;
            }
        }
        if value.is_empty() {
            return Err(DecodeError::EmptyField { field: field_name });
        }
        if value.len() > MAX_SERVER_FIELD_BYTES {
            return Err(DecodeError::FieldTooLong {
                field: field_name,
                max_bytes: MAX_SERVER_FIELD_BYTES,
            });
        }
        fields.insert(key.to_owned(), value);
    }

    for key in allowed {
        if !fields.contains_key(*key) {
            return Err(DecodeError::MissingField { field: key });
        }
    }
    Ok(fields)
}

fn take(fields: &mut BTreeMap<String, String>, field: &'static str) -> Result<String, DecodeError> {
    fields
        .remove(field)
        .ok_or(DecodeError::MissingField { field })
}

fn parse_string(
    value: String,
    field: &'static str,
    max_bytes: usize,
) -> Result<String, DecodeError> {
    if value.is_empty() {
        return Err(DecodeError::EmptyField { field });
    }
    if value.len() > max_bytes {
        return Err(DecodeError::FieldTooLong { field, max_bytes });
    }
    Ok(value)
}

fn parse_entity_id(value: String, field: &'static str) -> Result<EntityId, DecodeError> {
    let id = parse_u64(value, field)?;
    (id != 0)
        .then_some(EntityId(id))
        .ok_or(DecodeError::InvalidValue { field })
}

fn parse_u64(value: String, field: &'static str) -> Result<u64, DecodeError> {
    value
        .parse()
        .map_err(|_| DecodeError::InvalidValue { field })
}

fn parse_u32(value: String, field: &'static str) -> Result<u32, DecodeError> {
    value
        .parse()
        .map_err(|_| DecodeError::InvalidValue { field })
}

fn parse_positive_u32(value: String, field: &'static str) -> Result<u32, DecodeError> {
    let value = parse_u32(value, field)?;
    (value > 0)
        .then_some(value)
        .ok_or(DecodeError::InvalidValue { field })
}

fn parse_item_id(value: String, field: &'static str) -> Result<ItemId, DecodeError> {
    Ok(ItemId(parse_positive_u32(value, field)?))
}

fn parse_quest_id(value: String, field: &'static str) -> Result<QuestId, DecodeError> {
    Ok(QuestId(parse_positive_u32(value, field)?))
}

fn parse_snapshot_string(
    encoded: String,
    field: &'static str,
    max_bytes: usize,
) -> Result<String, DecodeError> {
    let decoded = percent_decode(&encoded, field)?;
    parse_string(decoded, field, max_bytes)
}

fn percent_decode(value: &str, field: &'static str) -> Result<String, DecodeError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') => {
                decoded.push(byte);
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let Some(high) = hex_value(bytes[index + 1]) else {
                    return Err(DecodeError::InvalidValue { field });
                };
                let Some(low) = hex_value(bytes[index + 2]) else {
                    return Err(DecodeError::InvalidValue { field });
                };
                decoded.push((high << 4) | low);
                index += 3;
            }
            _ => return Err(DecodeError::InvalidValue { field }),
        }
    }
    String::from_utf8(decoded).map_err(|_| DecodeError::InvalidValue { field })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_f32(value: &str, field: &'static str) -> Result<f32, DecodeError> {
    let value = value
        .parse::<f32>()
        .map_err(|_| DecodeError::InvalidValue { field })?;
    value
        .is_finite()
        .then_some(value)
        .ok_or(DecodeError::InvalidValue { field })
}

fn parse_position(value: String, field: &'static str) -> Result<Position, DecodeError> {
    let Some((x, y)) = value.split_once(',') else {
        return Err(DecodeError::InvalidValue { field });
    };
    if y.contains(',') {
        return Err(DecodeError::InvalidValue { field });
    }
    Ok(Position::new(parse_f32(x, field)?, parse_f32(y, field)?))
}

fn parse_health(value: String, field: &'static str) -> Result<(u32, u32), DecodeError> {
    let Some((health, max_health)) = value.split_once('/') else {
        return Err(DecodeError::InvalidValue { field });
    };
    if max_health.contains('/') {
        return Err(DecodeError::InvalidValue { field });
    }
    let health = health
        .parse::<u32>()
        .map_err(|_| DecodeError::InvalidValue { field })?;
    let max_health = max_health
        .parse::<u32>()
        .map_err(|_| DecodeError::InvalidValue { field })?;
    validate_health(health, max_health, field)?;
    Ok((health, max_health))
}

fn validate_health(health: u32, max_health: u32, field: &'static str) -> Result<(), DecodeError> {
    (health <= max_health)
        .then_some(())
        .ok_or(DecodeError::InvalidValue { field })
}

fn parse_target(value: String, field: &'static str) -> Result<Option<EntityId>, DecodeError> {
    if value == "none" {
        return Ok(None);
    }
    Ok(Some(parse_entity_id(value, field)?))
}

fn parse_role(value: String, field: &'static str) -> Result<Role, DecodeError> {
    match value.as_str() {
        "tank" => Ok(Role::Tank),
        "healer" => Ok(Role::Healer),
        "damage" => Ok(Role::DamageDealer),
        _ => Err(DecodeError::InvalidValue { field }),
    }
}

fn parse_npc_kind(value: String, field: &'static str) -> Result<NpcKind, DecodeError> {
    match value.as_str() {
        "Vendor" => Ok(NpcKind::Vendor),
        "Enemy" => Ok(NpcKind::Enemy),
        _ => Err(DecodeError::InvalidValue { field }),
    }
}

fn parse_snapshot_npc_kind(value: String, field: &'static str) -> Result<NpcKind, DecodeError> {
    match value.as_str() {
        "vendor" => Ok(NpcKind::Vendor),
        "enemy" => Ok(NpcKind::Enemy),
        _ => Err(DecodeError::InvalidValue { field }),
    }
}

fn parse_quest_status(value: String, field: &'static str) -> Result<QuestStatus, DecodeError> {
    match value.as_str() {
        "Accepted" => Ok(QuestStatus::Accepted),
        "Completed" => Ok(QuestStatus::Completed),
        "Rewarded" => Ok(QuestStatus::Rewarded),
        _ => Err(DecodeError::InvalidValue { field }),
    }
}

fn parse_zone_area(value: String, field: &'static str) -> Result<ZoneArea, DecodeError> {
    match value.as_str() {
        "Town" => Ok(ZoneArea::Town),
        "Field" => Ok(ZoneArea::Field),
        _ => Err(DecodeError::InvalidValue { field }),
    }
}

fn line(arguments: fmt::Arguments<'_>) -> ProtocolLine {
    ProtocolLine(arguments.to_string())
}

fn validate_name(name: &str) -> Result<(), EncodeError> {
    if name.is_empty() {
        return Err(EncodeError::EmptyName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(EncodeError::NameTooLong {
            max_bytes: MAX_NAME_BYTES,
        });
    }
    if name.chars().any(char::is_control) {
        return Err(EncodeError::NameContainsControl);
    }
    if name.chars().any(char::is_whitespace) {
        return Err(EncodeError::NameContainsWhitespace);
    }
    Ok(())
}

fn validate_movement(value: f32, axis: &'static str) -> Result<(), EncodeError> {
    if !value.is_finite() {
        return Err(EncodeError::NonFiniteMovement { axis });
    }
    if value.abs() > MAX_MOVE_PER_COMMAND {
        return Err(EncodeError::MovementTooLarge { axis });
    }
    Ok(())
}

fn validate_entity_id(id: EntityId) -> Result<(), EncodeError> {
    (id.0 != 0)
        .then_some(())
        .ok_or(EncodeError::InvalidEntityId)
}

fn validate_item_id(id: ItemId) -> Result<(), EncodeError> {
    (id.0 != 0).then_some(()).ok_or(EncodeError::InvalidItemId)
}

fn validate_quest_id(id: QuestId) -> Result<(), EncodeError> {
    (id.0 != 0).then_some(()).ok_or(EncodeError::InvalidQuestId)
}

fn validate_quantity(quantity: u32) -> Result<(), EncodeError> {
    (quantity != 0)
        .then_some(())
        .ok_or(EncodeError::ZeroQuantity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(result: Result<ProtocolLine, EncodeError>) -> String {
        result.expect("command should encode").into_string()
    }

    #[test]
    fn encodes_all_supported_command_families() {
        assert_eq!(
            text(CommandLine::connect("Aria", Role::Healer)),
            "connect Aria healer"
        );
        assert_eq!(text(CommandLine::move_by(-2.5, 9.5)), "move -2.5 9.5");
        assert_eq!(text(CommandLine::target(EntityId(7))), "target 7");
        assert_eq!(CommandLine::attack().as_str(), "attack");
        assert_eq!(text(CommandLine::vendor(EntityId(1))), "vendor 1");
        assert_eq!(
            text(CommandLine::buy(EntityId(1), ItemId(2), 3)),
            "buy 1 2 3"
        );
        assert_eq!(text(CommandLine::loot(EntityId(9))), "loot 9");
        assert_eq!(
            text(CommandLine::quest_offers(EntityId(1))),
            "quest-offers 1"
        );
        assert_eq!(
            text(CommandLine::accept_quest(EntityId(1), QuestId(1))),
            "accept-quest 1 1"
        );
        assert_eq!(
            text(CommandLine::turn_in_quest(EntityId(1), QuestId(1))),
            "turn-in-quest 1 1"
        );
        assert_eq!(CommandLine::state().as_str(), "state");
        assert_eq!(CommandLine::help().as_str(), "help");
        assert_eq!(CommandLine::inventory().as_str(), "inventory");
        assert_eq!(CommandLine::quit().as_str(), "quit");
    }

    #[test]
    fn rejects_names_that_can_break_the_line_protocol() {
        assert_eq!(
            CommandLine::connect("", Role::Tank),
            Err(EncodeError::EmptyName)
        );
        assert_eq!(
            CommandLine::connect("two words", Role::Tank),
            Err(EncodeError::NameContainsWhitespace)
        );
        assert_eq!(
            CommandLine::connect("name\nquit", Role::Tank),
            Err(EncodeError::NameContainsControl)
        );
        assert_eq!(
            CommandLine::connect(&"x".repeat(MAX_NAME_BYTES + 1), Role::Tank),
            Err(EncodeError::NameTooLong {
                max_bytes: MAX_NAME_BYTES
            })
        );
    }

    #[test]
    fn rejects_invalid_ids_quantities_and_movement() {
        assert_eq!(
            CommandLine::target(EntityId(0)),
            Err(EncodeError::InvalidEntityId)
        );
        assert_eq!(
            CommandLine::buy(EntityId(1), ItemId(0), 1),
            Err(EncodeError::InvalidItemId)
        );
        assert_eq!(
            CommandLine::buy(EntityId(1), ItemId(2), 0),
            Err(EncodeError::ZeroQuantity)
        );
        assert_eq!(
            CommandLine::accept_quest(EntityId(1), QuestId(0)),
            Err(EncodeError::InvalidQuestId)
        );
        assert_eq!(
            CommandLine::move_by(f32::NAN, 0.0),
            Err(EncodeError::NonFiniteMovement { axis: "dx" })
        );
        assert_eq!(
            CommandLine::move_by(10.1, 0.0),
            Err(EncodeError::MovementTooLarge { axis: "dx" })
        );
        assert_eq!(
            CommandLine::move_by(8.0, 8.0),
            Err(EncodeError::MovementMagnitudeTooLarge)
        );
    }

    #[test]
    fn protocol_lines_do_not_include_a_newline() {
        let line = CommandLine::connect("A", Role::DamageDealer).unwrap();
        assert!(!line.as_str().contains(['\r', '\n']));
        assert_eq!(line.to_string(), "connect A damage");
    }

    #[test]
    fn decodes_bootstrap_state_lines() {
        assert_eq!(
            decode_server_line("WORLD tick=42 players=2 npcs=4 enemies=3 vendors=1"),
            Ok(ServerLine::World(WorldState {
                tick: 42,
                players: 2,
                npcs: 4,
                enemies: 3,
                vendors: 1,
            }))
        );
        assert_eq!(
            decode_server_line(
                "PLAYER id=5 name=Aria role=healer pos=1.25,-2.5 hp=87/100 gold=20 target=9"
            ),
            Ok(ServerLine::Player(PlayerState {
                id: EntityId(5),
                name: "Aria".to_owned(),
                role: Role::Healer,
                position: Position::new(1.25, -2.5),
                health: 87,
                max_health: 100,
                gold: 20,
                inventory_capacity: None,
                target: Some(EntityId(9)),
            }))
        );
        assert_eq!(
            decode_server_line(
                "NPC id=1 name=Mira the Merchant kind=Vendor pos=0.00,0.00 hp=100/100"
            ),
            Ok(ServerLine::Npc(NpcState {
                id: EntityId(1),
                template_id: None,
                name: "Mira the Merchant".to_owned(),
                kind: NpcKind::Vendor,
                position: Position::new(0.0, 0.0),
                health: 100,
                max_health: 100,
            }))
        );
        assert_eq!(
            ServerLine::decode("CONNECTED player_id=5 role=healer"),
            Ok(ServerLine::Connected(ConnectedState {
                player_id: EntityId(5),
                role: Role::Healer,
            }))
        );
        assert_eq!(
            decode_server_line(
                "PLAYER id=5 name=Aria role=damage pos=0,0 hp=100/100 gold=20 target=none"
            ),
            Ok(ServerLine::Player(PlayerState {
                id: EntityId(5),
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
                position: Position::new(0.0, 0.0),
                health: 100,
                max_health: 100,
                gold: 20,
                inventory_capacity: None,
                target: None,
            }))
        );
    }

    #[test]
    fn decodes_temporary_machine_snapshot_records() {
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT_BEGIN version=2"),
            Ok(ServerLine::SnapshotBegin { version: 2 })
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT WORLD tick=1 players=1 npcs=4 enemies=3 vendors=1"),
            Ok(ServerLine::World(WorldState {
                tick: 1,
                players: 1,
                npcs: 4,
                enemies: 3,
                vendors: 1,
            }))
        );
        assert_eq!(
            decode_server_line(
                "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0.0,0.0 health=100 max_health=100 gold=20 capacity=16 target=none"
            ),
            Ok(ServerLine::Player(PlayerState {
                id: EntityId(5),
                name: "Aria".to_owned(),
                role: Role::DamageDealer,
                position: Position::new(0.0, 0.0),
                health: 100,
                max_health: 100,
                gold: 20,
                inventory_capacity: Some(16),
                target: None,
            }))
        );
        assert_eq!(
            decode_server_line(
                "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0.0,0.0 health=100 max_health=100 gold=20 target=none"
            ),
            Err(DecodeError::MissingField { field: "capacity" })
        );
        assert_eq!(
            decode_server_line(
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0.0,0.0 health=1 max_health=1"
            ),
            Ok(ServerLine::Npc(NpcState {
                id: EntityId(1),
                template_id: Some(1),
                name: "Mira the Merchant".to_owned(),
                kind: NpcKind::Vendor,
                position: Position::new(0.0, 0.0),
                health: 1,
                max_health: 1,
            }))
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT ITEM player=5 item=2 quantity=4"),
            Ok(ServerLine::Item(ItemState {
                player_id: EntityId(5),
                item_id: ItemId(2),
                quantity: 4,
            }))
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT QUEST player=5 quest=1 progress=2/3 status=Accepted"),
            Ok(ServerLine::Quest(QuestState {
                player_id: EntityId(5),
                quest_id: QuestId(1),
                progress: 2,
                required_count: 3,
                status: QuestStatus::Accepted,
            }))
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT_END"),
            Ok(ServerLine::SnapshotEnd)
        );
    }

    #[test]
    fn decodes_supported_event_lines() {
        assert_eq!(
            decode_server_line("EVENT player_joined id=5 name=Aria role=tank pos=0.00,0.00"),
            Ok(ServerLine::Event(ServerEvent::PlayerJoined {
                id: EntityId(5),
                name: "Aria".to_owned(),
                role: Role::Tank,
                position: Position::new(0.0, 0.0),
            }))
        );
        assert_eq!(
            decode_server_line("EVENT player_moved id=5 pos=12.50,-3.00 area=Field"),
            Ok(ServerLine::Event(ServerEvent::PlayerMoved {
                id: EntityId(5),
                position: Position::new(12.5, -3.0),
                area: ZoneArea::Field,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT player_left id=5"),
            Ok(ServerLine::Event(ServerEvent::PlayerLeft {
                player_id: EntityId(5),
            }))
        );
        assert_eq!(
            decode_server_line("EVENT target_selected player=5 target=9"),
            Ok(ServerLine::Event(ServerEvent::TargetSelected {
                player_id: EntityId(5),
                target_id: EntityId(9),
            }))
        );
        assert_eq!(
            decode_server_line("EVENT attack player=5 target=9 damage=12 target_hp=88"),
            Ok(ServerLine::Event(ServerEvent::AttackResolved {
                player_id: EntityId(5),
                target_id: EntityId(9),
                damage: 12,
                target_health: 88,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT enemy_defeated id=9"),
            Ok(ServerLine::Event(ServerEvent::EnemyDefeated {
                enemy_id: EntityId(9),
            }))
        );
        assert_eq!(
            decode_server_line(
                "EVENT vendor_listed player=5 vendor=1 listings=item=2 name=Town_Ration price=2 stock=98 max_stack=20;item=3 name=Minor_Healing_Potion price=8 stock=10 max_stack=5",
            ),
            Ok(ServerLine::Event(ServerEvent::VendorListed {
                player_id: EntityId(5),
                vendor_id: EntityId(1),
                listings: vec![
                    VendorListingState {
                        item_id: ItemId(2),
                        name: "Town Ration".to_owned(),
                        unit_price: 2,
                        remaining_quantity: 98,
                        max_stack: 20,
                    },
                    VendorListingState {
                        item_id: ItemId(3),
                        name: "Minor Healing Potion".to_owned(),
                        unit_price: 8,
                        remaining_quantity: 10,
                        max_stack: 5,
                    },
                ],
            }))
        );
        assert_eq!(
            decode_server_line(
                "EVENT item_purchased player=5 vendor=1 item=2 quantity=2 total_price=4 gold=16",
            ),
            Ok(ServerLine::Event(ServerEvent::ItemPurchased {
                player_id: EntityId(5),
                vendor_id: EntityId(1),
                item_id: ItemId(2),
                quantity: 2,
                total_price: 4,
                gold_remaining: 16,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT loot_rewarded player=5 enemy=9 item=1 quantity=1"),
            Ok(ServerLine::Event(ServerEvent::LootRewarded {
                player_id: EntityId(5),
                enemy_id: EntityId(9),
                item_id: ItemId(1),
                quantity: 1,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT transaction_rejected player=5 reason=inventory is full"),
            Ok(ServerLine::Event(ServerEvent::TransactionRejected {
                player_id: EntityId(5),
                reason: "inventory is full".to_owned(),
            }))
        );
        assert_eq!(
            decode_server_line(
                "EVENT quest_offers player=5 npc=1 quests=id=1 name=Clear_the_Field",
            ),
            Ok(ServerLine::Event(ServerEvent::QuestOffersListed {
                player_id: EntityId(5),
                npc_id: EntityId(1),
                quests: vec![QuestOfferState {
                    quest_id: QuestId(1),
                    name: "Clear the Field".to_owned(),
                }],
            }))
        );
        assert_eq!(
            decode_server_line("EVENT quest_accepted player=5 npc=1 quest=1"),
            Ok(ServerLine::Event(ServerEvent::QuestAccepted {
                player_id: EntityId(5),
                npc_id: EntityId(1),
                quest_id: QuestId(1),
            }))
        );
        assert_eq!(
            decode_server_line("EVENT quest_progressed player=5 quest=1 progress=2/3"),
            Ok(ServerLine::Event(ServerEvent::QuestProgressed {
                player_id: EntityId(5),
                quest_id: QuestId(1),
                progress: 2,
                required_count: 3,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT quest_completed player=5 quest=1"),
            Ok(ServerLine::Event(ServerEvent::QuestCompleted {
                player_id: EntityId(5),
                quest_id: QuestId(1),
            }))
        );
        assert_eq!(
            decode_server_line(
                "EVENT quest_rewarded player=5 quest=1 gold=10 item=2 quantity=5 gold_remaining=30",
            ),
            Ok(ServerLine::Event(ServerEvent::QuestRewarded {
                player_id: EntityId(5),
                quest_id: QuestId(1),
                gold: 10,
                item_id: Some(ItemId(2)),
                item_quantity: 5,
                gold_remaining: 30,
            }))
        );
        assert_eq!(
            decode_server_line("EVENT quest_rejected player=5 reason=quest is not complete"),
            Ok(ServerLine::Event(ServerEvent::QuestRejected {
                player_id: EntityId(5),
                reason: "quest is not complete".to_owned(),
            }))
        );
        assert_eq!(
            decode_server_line("EVENT rejected reason=target out of attack range"),
            Ok(ServerLine::Event(ServerEvent::CommandRejected {
                reason: "target out of attack range".to_owned(),
            }))
        );
    }

    #[test]
    fn rejects_unknown_duplicate_missing_and_malformed_fields() {
        assert_eq!(
            decode_server_line("WELCOME mmorpg-server"),
            Err(DecodeError::UnknownPrefix {
                prefix: "WELCOME".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line("WORLD tick=1 players=0 npcs=0 enemies=0 vendors=0 region=starter"),
            Err(DecodeError::UnknownField {
                field: "region".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line("WORLD tick=1 tick=2 players=0 npcs=0 enemies=0 vendors=0"),
            Err(DecodeError::DuplicateField {
                field: "tick".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line("WORLD tick=1 players=0 npcs=0 enemies=0"),
            Err(DecodeError::MissingField { field: "vendors" })
        );
        assert_eq!(
            decode_server_line("WORLD tick players=0 npcs=0 enemies=0 vendors=0"),
            Err(DecodeError::MalformedField {
                token: "tick".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line("EVENT unknown id=9"),
            Err(DecodeError::UnsupportedEvent {
                name: "unknown".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line(
                "EVENT item_purchased player=5 vendor=1 item=2 quantity=0 total_price=0 gold=20"
            ),
            Err(DecodeError::InvalidValue { field: "quantity" })
        );
        assert_eq!(
            decode_server_line("EVENT quest_progressed player=5 quest=1 progress=4/3"),
            Err(DecodeError::InvalidValue { field: "progress" })
        );
        assert_eq!(
            decode_server_line(
                "EVENT quest_rewarded player=5 quest=1 gold=10 item=none quantity=1 gold_remaining=30"
            ),
            Err(DecodeError::InvalidValue { field: "quantity" })
        );
        assert_eq!(
            decode_server_line(
                "EVENT vendor_listed player=5 vendor=1 listings=item=2 name=Ration price=1 stock=1 max_stack=0"
            ),
            Err(DecodeError::InvalidValue { field: "max_stack" })
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT WEATHER rain=true"),
            Err(DecodeError::UnsupportedSnapshotRecord {
                name: "WEATHER".to_owned(),
            })
        );
        assert_eq!(
            decode_server_line("TEMP_SNAPSHOT_BEGIN version=1"),
            Err(DecodeError::UnsupportedSnapshotVersion { version: 1 })
        );
        assert_eq!(
            decode_server_line("EVENT rejected reason=nope extra=x"),
            Err(DecodeError::UnknownField {
                field: "extra".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_invalid_values_and_unsafe_line_shapes() {
        assert_eq!(
            decode_server_line("CONNECTED player_id=0 role=damage"),
            Err(DecodeError::InvalidValue { field: "player_id" })
        );
        assert_eq!(
            decode_server_line("CONNECTED player_id=5 role=DPS"),
            Err(DecodeError::InvalidValue { field: "role" })
        );
        assert_eq!(
            decode_server_line("NPC id=1 name=Wolf kind=Creature pos=0,0 hp=10/10"),
            Err(DecodeError::InvalidValue { field: "kind" })
        );
        assert_eq!(
            decode_server_line("EVENT player_moved id=5 pos=NaN,0 area=Field"),
            Err(DecodeError::InvalidValue { field: "pos" })
        );
        assert_eq!(
            decode_server_line(
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%ZZ kind=vendor position=0,0 health=1 max_health=1"
            ),
            Err(DecodeError::InvalidValue { field: "name" })
        );
        assert_eq!(
            decode_server_line(
                "PLAYER id=5 name=Aria role=tank pos=0,0 hp=101/100 gold=20 target=none"
            ),
            Err(DecodeError::InvalidValue { field: "hp" })
        );
        assert_eq!(
            decode_server_line(" WORLD tick=1 players=0 npcs=0 enemies=0 vendors=0"),
            Err(DecodeError::LeadingOrTrailingWhitespace)
        );
        assert_eq!(
            decode_server_line("WORLD tick=1 players=0 npcs=0 enemies=0 vendors=0\n"),
            Err(DecodeError::LineContainsControl)
        );
    }

    #[test]
    fn enforces_line_and_string_bounds() {
        let long_line = "x".repeat(MAX_SERVER_LINE_BYTES + 1);
        assert_eq!(
            decode_server_line(&long_line),
            Err(DecodeError::LineTooLong {
                max_bytes: MAX_SERVER_LINE_BYTES,
            })
        );

        let long_name = "N".repeat(MAX_SERVER_NAME_BYTES + 1);
        assert_eq!(
            decode_server_line(&format!(
                "NPC id=1 name={long_name} kind=Enemy pos=0,0 hp=10/10"
            )),
            Err(DecodeError::FieldTooLong {
                field: "name",
                max_bytes: MAX_SERVER_NAME_BYTES,
            })
        );

        let long_reason = "r".repeat(MAX_SERVER_FIELD_BYTES + 1);
        assert_eq!(
            decode_server_line(&format!("EVENT rejected reason={long_reason}")),
            Err(DecodeError::FieldTooLong {
                field: "reason",
                max_bytes: MAX_SERVER_FIELD_BYTES,
            })
        );
    }

    fn snapshot_lines(include_enemy: bool) -> Vec<String> {
        let mut lines = vec![
            "TEMP_SNAPSHOT_BEGIN version=2".to_owned(),
            format!(
                "TEMP_SNAPSHOT WORLD tick=7 players=1 npcs={} enemies={} vendors=1",
                if include_enemy { 2 } else { 1 },
                if include_enemy { 1 } else { 0 }
            ),
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0,0 health=100 max_health=100 gold=20 capacity=16 target=none".to_owned(),
            "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira%20the%20Merchant kind=vendor position=0,0 health=1 max_health=1".to_owned(),
        ];
        if include_enemy {
            lines.push(
                "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Field%20Wolf kind=enemy position=24,0 health=100 max_health=100"
                    .to_owned(),
            );
        }
        lines.push("TEMP_SNAPSHOT_END".to_owned());
        lines
    }

    fn assemble_with(
        assembler: &mut SnapshotAssembler,
        lines: impl IntoIterator<Item = String>,
    ) -> Snapshot {
        let mut completed = None;
        for line in lines {
            if let Some(snapshot) = assembler
                .push_line(&line)
                .expect("snapshot lines should be valid")
            {
                completed = Some(snapshot);
            }
        }
        completed.expect("snapshot should publish at its end marker")
    }

    fn assemble(lines: impl IntoIterator<Item = String>) -> Snapshot {
        assemble_with(&mut SnapshotAssembler::new(), lines)
    }

    #[test]
    fn assembles_and_publishes_only_a_valid_completed_snapshot() {
        let snapshot = assemble(snapshot_lines(true));

        assert_eq!(snapshot.version, 2);
        assert_eq!(snapshot.world.tick, 7);
        assert_eq!(snapshot.players.len(), 1);
        assert_eq!(snapshot.npcs.len(), 2);
        assert_eq!(snapshot.players[&EntityId(5)].name, "Aria");
        assert_eq!(snapshot.npcs[&EntityId(1)].kind, NpcKind::Vendor);
        assert_eq!(snapshot.npcs[&EntityId(2)].kind, NpcKind::Enemy);
        assert!(snapshot.items.is_empty());
        assert!(snapshot.quests.is_empty());
    }

    #[test]
    fn assembles_player_inventory_and_quest_records() {
        let snapshot = assemble([
            "TEMP_SNAPSHOT_BEGIN version=2".to_owned(),
            "TEMP_SNAPSHOT WORLD tick=9 players=1 npcs=0 enemies=0 vendors=0".to_owned(),
            "TEMP_SNAPSHOT PLAYER id=5 name=Aria role=damage position=0,0 health=100 max_health=100 gold=20 capacity=16 target=none".to_owned(),
            "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=4".to_owned(),
            "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=2/3 status=Accepted".to_owned(),
            "TEMP_SNAPSHOT_END".to_owned(),
        ]);

        assert_eq!(snapshot.items[&EntityId(5)][0].quantity, 4);
        assert_eq!(snapshot.quests[&EntityId(5)][0].progress, 2);
        assert_eq!(
            snapshot.quests[&EntityId(5)][0].status,
            QuestStatus::Accepted
        );
    }

    #[test]
    fn rejects_item_and_quest_records_for_players_missing_from_the_frame() {
        let mut assembler = SnapshotAssembler::new();
        for line in [
            "TEMP_SNAPSHOT_BEGIN version=2",
            "TEMP_SNAPSHOT WORLD tick=9 players=0 npcs=0 enemies=0 vendors=0",
            "TEMP_SNAPSHOT ITEM player=5 item=2 quantity=4",
        ] {
            assembler.push_line(line).unwrap();
        }
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::UnknownPlayer {
                record: SnapshotRecordKind::Item,
                player_id: EntityId(5),
            })
        );

        for line in [
            "TEMP_SNAPSHOT_BEGIN version=2",
            "TEMP_SNAPSHOT WORLD tick=9 players=0 npcs=0 enemies=0 vendors=0",
            "TEMP_SNAPSHOT QUEST player=5 quest=1 progress=0/3 status=Accepted",
        ] {
            assembler.push_line(line).unwrap();
        }
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::UnknownPlayer {
                record: SnapshotRecordKind::Quest,
                player_id: EntityId(5),
            })
        );
    }

    #[test]
    fn rejects_records_outside_frames_and_duplicate_markers_or_records() {
        let mut assembler = SnapshotAssembler::new();

        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT WORLD tick=7 players=0 npcs=0 enemies=0 vendors=0"),
            Err(SnapshotError::SnapshotNotActive {
                record: SnapshotRecordKind::World,
            })
        );
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::EndOutsideSnapshot)
        );

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT WORLD tick=7 players=0 npcs=0 enemies=0 vendors=0")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT ITEM player=5 item=2 quantity=1")
            .unwrap();
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT ITEM player=5 item=2 quantity=1"),
            Err(SnapshotError::DuplicateRecord {
                record: SnapshotRecordKind::Item,
                id: Some(EntityId(5)),
            })
        );
        assert!(!assembler.is_active());

        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_BEGIN version=2"),
            Ok(None)
        );
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_BEGIN version=2"),
            Err(SnapshotError::DuplicateBegin)
        );
        assert!(!assembler.is_active());

        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_BEGIN version=2"),
            Ok(None)
        );
        let world = "TEMP_SNAPSHOT WORLD tick=7 players=0 npcs=0 enemies=0 vendors=0";
        assert_eq!(assembler.push_line(world), Ok(None));
        assert_eq!(
            assembler.push_line(world),
            Err(SnapshotError::DuplicateRecord {
                record: SnapshotRecordKind::World,
                id: None,
            })
        );
        assert!(!assembler.is_active());

        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::EndOutsideSnapshot)
        );
    }

    #[test]
    fn rejects_invalid_version_missing_world_and_truncated_frames() {
        let mut assembler = SnapshotAssembler::new();

        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_BEGIN version=1"),
            Err(SnapshotError::Decode(
                DecodeError::UnsupportedSnapshotVersion { version: 1 }
            ))
        );
        assert!(!assembler.is_active());

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::MissingWorld)
        );
        assert!(!assembler.is_active());

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT WORLD tick=7 players=1 npcs=0 enemies=0 vendors=0")
            .unwrap();
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT_END"),
            Err(SnapshotError::RecordCountMismatch {
                field: "players",
                expected: 1,
                actual: 0,
            })
        );
        assert!(!assembler.is_active());

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT WORLD tick=7 players=0 npcs=0 enemies=0 vendors=0")
            .unwrap();
        assert_eq!(assembler.finish(), Err(SnapshotError::IncompleteSnapshot));
        assert!(!assembler.is_active());

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assert_eq!(
            assembler.push_line("TEMP_SNAPSHOT WEATHER rain=true"),
            Err(SnapshotError::Decode(
                DecodeError::UnsupportedSnapshotRecord {
                    name: "WEATHER".to_owned()
                }
            ))
        );
        assert!(!assembler.is_active());
    }

    #[test]
    fn malformed_second_frame_does_not_publish_partial_state() {
        let mut assembler = SnapshotAssembler::new();
        let first = assemble_with(&mut assembler, snapshot_lines(true));

        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT WORLD tick=8 players=1 npcs=1 enemies=0 vendors=1")
            .unwrap();
        assembler
            .push_line(
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira kind=vendor position=0,0 health=1 max_health=1",
            )
            .unwrap();
        assert!(matches!(
            assembler.push_line(
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira kind=vendor position=0,0 health=1"
            ),
            Err(SnapshotError::Decode(DecodeError::MissingField {
                field: "max_health"
            }))
        ));
        assert!(!assembler.is_active());

        assert_eq!(first.world.tick, 7);
        assert_eq!(first.npcs.len(), 2);
        assert!(first.npcs.contains_key(&EntityId(2)));
    }

    #[test]
    fn a_completed_second_snapshot_replaces_the_first_atomically() {
        let mut assembler = SnapshotAssembler::new();
        let first = assemble_with(&mut assembler, snapshot_lines(true));
        let second = assemble_with(&mut assembler, snapshot_lines(false));

        assert_eq!(first.npcs.len(), 2);
        assert_eq!(second.npcs.len(), 1);
        assert!(first.npcs.contains_key(&EntityId(2)));
        assert!(!second.npcs.contains_key(&EntityId(2)));
        assert_eq!(second.world.npcs, 1);
    }

    #[test]
    fn bounds_the_number_of_buffered_records() {
        let mut assembler = SnapshotAssembler::with_max_records(3).unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT_BEGIN version=2")
            .unwrap();
        assembler
            .push_line("TEMP_SNAPSHOT WORLD tick=7 players=0 npcs=2 enemies=1 vendors=1")
            .unwrap();
        assembler
            .push_line(
                "TEMP_SNAPSHOT NPC id=1 template_id=1 name=Mira kind=vendor position=0,0 health=1 max_health=1",
            )
            .unwrap();
        assembler
            .push_line(
                "TEMP_SNAPSHOT NPC id=2 template_id=2 name=Wolf kind=enemy position=1,0 health=100 max_health=100",
            )
            .unwrap();
        assert_eq!(
            assembler.push_line(
                "TEMP_SNAPSHOT NPC id=3 template_id=2 name=Wolf kind=enemy position=2,0 health=100 max_health=100",
            ),
            Err(SnapshotError::TooManyRecords { max_records: 3 })
        );
        assert!(!assembler.is_active());
        assert!(matches!(
            SnapshotAssembler::with_max_records(0),
            Err(SnapshotError::InvalidRecordLimit)
        ));
        assert!(matches!(
            SnapshotAssembler::with_max_records(MAX_SNAPSHOT_RECORDS + 1),
            Err(SnapshotError::InvalidRecordLimit)
        ));
    }
}
