//! Typed helpers for the temporary line-oriented development protocol.
//!
//! This crate only constructs command lines. It deliberately does not open
//! sockets, read from a server, or parse authoritative results. In
//! particular, callers must not treat the server's human-readable `EVENT`,
//! `WORLD`, or `PLAYER` output as a stable protocol. A future production
//! client protocol must have its own versioned, machine-readable result
//! messages.

use std::fmt;

pub use mmorpg_core::{EntityId, ItemId, QuestId, Role};

const MAX_NAME_BYTES: usize = 24;
const MAX_MOVE_PER_COMMAND: f32 = 10.0;

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
}
