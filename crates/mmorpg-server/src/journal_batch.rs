//! 2PC staged operation batches, persisted operation recovery, and operation outcome caching.

use crate::account_repository::{OperationKey, PersistedOperation};
use crate::client::ClientOrigin;
use mmorpg_core::{Event, World};
use mmorpg_wire::{ClientCommand as WireCommand, ServerMessage};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_COMPLETED_OPERATIONS: usize = 256;

pub struct StagedOperationBatch {
    pub world: World,
    pub events: Vec<Event>,
    pub join_origins: VecDeque<ClientOrigin>,
    pub operations: BTreeMap<OperationKey, u64>,
    pub operation_events: BTreeMap<OperationKey, Vec<Event>>,
    pub acknowledged: BTreeSet<OperationKey>,
}

pub fn trim_operation_maps(
    completed: &mut BTreeMap<OperationKey, Vec<ServerMessage>>,
    failed: &mut BTreeMap<OperationKey, String>,
) {
    while completed.len() + failed.len() > MAX_COMPLETED_OPERATIONS {
        let completed_key = completed.keys().next().copied();
        let failed_key = failed.keys().next().copied();
        match (completed_key, failed_key) {
            (Some(completed_key), Some(failed_key)) if failed_key < completed_key => {
                failed.remove(&failed_key);
            }
            (Some(completed_key), _) => {
                completed.remove(&completed_key);
            }
            (None, Some(failed_key)) => {
                failed.remove(&failed_key);
            }
            (None, None) => break,
        }
    }
}

pub struct LoadedOperations {
    pub completed: BTreeMap<OperationKey, Vec<ServerMessage>>,
    pub recovery: BTreeMap<OperationKey, (u64, WireCommand)>,
    pub failed: BTreeMap<OperationKey, String>,
    pub interrupted: Vec<OperationKey>,
}

pub fn load_persisted_operations(
    persisted_operations: BTreeMap<OperationKey, PersistedOperation>,
) -> LoadedOperations {
    let mut completed = BTreeMap::new();
    let mut recovery = BTreeMap::new();
    let mut failed = BTreeMap::new();
    let mut interrupted = Vec::new();
    for (key, operation) in persisted_operations {
        match operation {
            PersistedOperation::Prepared(_) => {
                failed.insert(
                    key,
                    "operation was interrupted before durable completion".to_owned(),
                );
                interrupted.push(key);
            }
            PersistedOperation::Completed {
                revision,
                payloads,
                command_payload,
            } => {
                let messages = payloads
                    .into_iter()
                    .filter_map(|payload| ServerMessage::decode_payload(&payload).ok())
                    .collect();
                completed.insert(key, messages);
                if let Some(command_payload) = command_payload
                    && let Ok(command) = WireCommand::decode_payload(&command_payload)
                {
                    recovery.insert(key, (revision, command));
                }
            }
            PersistedOperation::Failed(reason) => {
                failed.insert(key, reason);
            }
        }
    }
    trim_operation_maps(&mut completed, &mut failed);
    LoadedOperations {
        completed,
        recovery,
        failed,
        interrupted,
    }
}
