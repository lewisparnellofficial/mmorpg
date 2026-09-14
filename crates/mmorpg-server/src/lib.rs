//! Authoritative Linux headless development server library.

pub mod account_repository;
pub mod client;
pub mod interest;
pub mod journal_batch;
pub mod server;
pub mod session;
pub mod test_support;
pub mod wire_adapter;

pub use account_repository::{
    AccountCharacterRepository, CharacterFence, CharacterFenceStore, CheckpointJob,
    CheckpointWorker, DevelopmentAccountRepository, OperationJournalJob, OperationJournalWorker,
    OperationKey, PersistedOperation,
};
pub use client::{
    ClientOrigin, MAX_REPLACEABLE_EVENTS, MAX_WIRE_CLIENTS, MAX_WIRE_COMMANDS_PER_CLIENT_POLL,
    MAX_WIRE_COMMANDS_PER_POLL, MAX_WIRE_INPUT_BYTES, MAX_WIRE_OUTPUT_BYTES, WireClient,
    origin_client_id,
};
pub use interest::{INTEREST_RANGE, event_recipient, event_visible_to_player};
pub use journal_batch::{
    LoadedOperations, MAX_COMPLETED_OPERATIONS, StagedOperationBatch, load_persisted_operations,
    trim_operation_maps,
};
pub use server::{
    CHECKPOINT_INTERVAL_TICKS, DEFAULT_ADDRESS, DEFAULT_CAST_TIME_TICKS,
    DEFAULT_COMBAT_COOLDOWN_TICKS, DEFAULT_TICK_HZ, MAX_COMMANDS_PER_TICK, MAX_PENDING_COMMANDS,
    SHUTDOWN_DRAIN_TIMEOUT, Server, SimulationTimingStats, interleave_wire_commands,
};
pub use session::{
    AuthenticatedSession, DISCONNECT_GRACE_TICKS, DetachedCharacter, PendingCommand,
    character_reserved_by_other,
};
pub use wire_adapter::{
    command_player_id, command_to_wire_payload, is_retryable_core_command, wire_area,
    wire_command_to_core, wire_event, wire_live_player, wire_npc, wire_player, wire_role,
    wire_snapshot, wire_snapshot_for_player, wire_status,
};
