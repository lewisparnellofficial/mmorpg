#![forbid(unsafe_code)]

//! The renderer- and language-neutral `ui.v1` addon boundary.
//!
//! This crate deliberately has no knowledge of Luau, Bevy, Qt, sockets, the
//! filesystem, or the authoritative simulation.  Runtime adapters consume
//! these values and must treat a validated operation batch as one atomic
//! presentation transaction.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

pub const API_VERSION: &str = "ui.v1";
pub const DEFAULT_EVENT_COUNT: usize = 128;
pub const DEFAULT_EVENT_BYTES: usize = 256 * 1024;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_STORAGE_BYTES: usize = 64 * 1024;
pub const MAX_STORAGE_KEYS: usize = 128;
pub const MAX_STORAGE_KEY_BYTES: usize = 128;
pub const MAX_STORAGE_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_STORAGE_DEPTH: usize = 16;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
        pub struct $name(u64);
        impl $name {
            pub const fn new(value: u64) -> Option<Self> {
                if value == 0 { None } else { Some(Self(value)) }
            }
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(PackageId);
id_type!(AccountId);
id_type!(InstanceId);
id_type!(NodeId);
id_type!(Generation);
id_type!(TimerId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handle {
    pub node: NodeId,
    pub owner: PackageId,
    pub generation: Generation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewRecord {
    pub player_name: String,
    pub target_name: Option<String>,
    pub inventory_slots_used: u32,
    pub quest_progress: u32,
}

impl ViewRecord {
    pub fn new(
        player_name: impl Into<String>,
        target_name: Option<String>,
        inventory_slots_used: u32,
        quest_progress: u32,
    ) -> Self {
        Self {
            player_name: player_name.into(),
            target_name,
            inventory_slots_used,
            quest_progress,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventClass {
    Replaceable,
    Ordered,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiEvent {
    pub name: String,
    pub class: EventClass,
    pub coalesce_key: Option<String>,
    pub view: Option<ViewRecord>,
    pub bytes: usize,
}

impl UiEvent {
    pub fn replaceable(name: impl Into<String>, key: impl Into<String>, view: ViewRecord) -> Self {
        let name = name.into();
        let key = key.into();
        let bytes = name.len().saturating_add(key.len()).saturating_add(64);
        Self {
            name,
            class: EventClass::Replaceable,
            coalesce_key: Some(key),
            view: Some(view),
            bytes,
        }
    }
    pub fn ordered(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            bytes: name.len().saturating_add(32),
            name,
            class: EventClass::Ordered,
            coalesce_key: None,
            view: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueueOutcome {
    Enqueued,
    Coalesced,
    DroppedReplaceable,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueStats {
    pub coalesced: u64,
    pub dropped: u64,
    pub disabled: bool,
}

pub struct EventQueue {
    events: VecDeque<UiEvent>,
    max_events: usize,
    max_bytes: usize,
    bytes: usize,
    stats: QueueStats,
}

impl EventQueue {
    pub fn new(max_events: usize, max_bytes: usize) -> Result<Self, ContractError> {
        if max_events == 0 || max_bytes == 0 {
            return Err(ContractError::InvalidQuota);
        }
        Ok(Self {
            events: VecDeque::new(),
            max_events,
            max_bytes,
            bytes: 0,
            stats: QueueStats {
                coalesced: 0,
                dropped: 0,
                disabled: false,
            },
        })
    }
    pub fn default_for_addon() -> Self {
        Self::new(DEFAULT_EVENT_COUNT, DEFAULT_EVENT_BYTES).expect("contract defaults are valid")
    }
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    pub fn pop_front(&mut self) -> Option<UiEvent> {
        let event = self.events.pop_front()?;
        self.bytes = self.bytes.saturating_sub(event.bytes);
        Some(event)
    }
    pub fn push(&mut self, event: UiEvent) -> QueueOutcome {
        if self.stats.disabled {
            return QueueOutcome::Disabled;
        }
        if event.class == EventClass::Replaceable {
            if let Some(key) = event.coalesce_key.as_deref() {
                if let Some(existing) = self.events.iter_mut().find(|old| {
                    old.class == EventClass::Replaceable && old.coalesce_key.as_deref() == Some(key)
                }) {
                    self.bytes = self
                        .bytes
                        .saturating_sub(existing.bytes)
                        .saturating_add(event.bytes);
                    *existing = event;
                    self.stats.coalesced += 1;
                    return QueueOutcome::Coalesced;
                }
            }
        }
        while self.events.len() >= self.max_events
            || self.bytes.saturating_add(event.bytes) > self.max_bytes
        {
            let Some(index) = self
                .events
                .iter()
                .position(|old| old.class == EventClass::Replaceable)
            else {
                if event.class == EventClass::Replaceable {
                    self.stats.dropped += 1;
                    return QueueOutcome::DroppedReplaceable;
                }
                self.stats.disabled = true;
                return QueueOutcome::Disabled;
            };
            if let Some(old) = self.events.remove(index) {
                self.bytes = self.bytes.saturating_sub(old.bytes);
                self.stats.dropped += 1;
            }
        }
        self.bytes = self.bytes.saturating_add(event.bytes);
        self.events.push_back(event);
        QueueOutcome::Enqueued
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Property {
    Text(String),
    Position { x: i32, y: i32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiOperation {
    CreatePanel { handle: Handle, text: String },
    SetProperty { handle: Handle, property: Property },
    Destroy { handle: Handle },
    Subscribe { event: String },
    Unsubscribe { event: String },
    CreateTimer { timer: TimerId, interval_ms: u32 },
    PresentSecureAction { handle: Handle, action: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    InvalidQuota,
    InvalidText,
    TextTooLarge,
    InvalidHandle,
    ForeignHandle,
    StaleHandle,
    DuplicateNode,
    UnknownNode,
    InvalidEvent,
    TooManyOperations,
    TooManyNodes,
    UnsupportedCapability(String),
    InvalidManifest(String),
    InvalidStorage(String),
}
impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ContractError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLimits {
    pub max_nodes: usize,
    pub max_operations: usize,
    pub max_subscriptions: usize,
}
impl Default for UiLimits {
    fn default() -> Self {
        Self {
            max_nodes: 256,
            max_operations: 512,
            max_subscriptions: 64,
        }
    }
}

pub fn validate_operations(
    ops: &[UiOperation],
    package: PackageId,
    generation: Generation,
    limits: &UiLimits,
) -> Result<(), ContractError> {
    if ops.len() > limits.max_operations {
        return Err(ContractError::TooManyOperations);
    }
    let mut nodes = BTreeSet::new();
    let mut subscriptions = 0usize;
    for op in ops {
        let handle = match op {
            UiOperation::CreatePanel { handle, .. }
            | UiOperation::SetProperty { handle, .. }
            | UiOperation::Destroy { handle }
            | UiOperation::PresentSecureAction { handle, .. } => Some(handle),
            _ => None,
        };
        if let Some(handle) = handle {
            if handle.owner != package {
                return Err(ContractError::ForeignHandle);
            }
            if handle.generation != generation {
                return Err(ContractError::StaleHandle);
            }
            if handle.node.get() == 0 {
                return Err(ContractError::InvalidHandle);
            }
        }
        match op {
            UiOperation::CreatePanel { handle, text } => {
                if !nodes.insert(handle.node) {
                    return Err(ContractError::DuplicateNode);
                }
                validate_text(text)?;
            }
            UiOperation::SetProperty {
                property: Property::Text(text),
                ..
            } => validate_text(text)?,
            UiOperation::Subscribe { event } | UiOperation::Unsubscribe { event } => {
                if event.is_empty() || event.len() > 128 {
                    return Err(ContractError::InvalidEvent);
                }
                subscriptions += 1;
            }
            UiOperation::CreateTimer { interval_ms, .. } if *interval_ms == 0 => {
                return Err(ContractError::InvalidQuota);
            }
            UiOperation::PresentSecureAction { action, .. }
                if action.is_empty() || action.len() > 128 =>
            {
                return Err(ContractError::InvalidEvent);
            }
            _ => {}
        }
    }
    if nodes.len() > limits.max_nodes {
        return Err(ContractError::TooManyNodes);
    }
    if subscriptions > limits.max_subscriptions {
        return Err(ContractError::InvalidQuota);
    }
    Ok(())
}
fn validate_text(text: &str) -> Result<(), ContractError> {
    if text.len() > MAX_TEXT_BYTES {
        Err(ContractError::TextTooLarge)
    } else if text.contains('\0') {
        Err(ContractError::InvalidText)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub package_id: PackageId,
    pub name: String,
    pub version: String,
    pub manifest_schema: u32,
    pub api_range: String,
    pub runtime_range: String,
    pub entry: String,
    pub load_order: i32,
    pub dependencies: Vec<PackageId>,
    pub capabilities: BTreeSet<String>,
    pub saved_data: bool,
    pub asset_ids: Vec<String>,
    pub integrity_sha256: String,
}
pub fn validate_manifest(
    manifest: &Manifest,
    known_capabilities: &BTreeSet<String>,
    package_ids: &BTreeSet<PackageId>,
) -> Result<(), ContractError> {
    if manifest.package_id.get() == 0
        || manifest.name.is_empty()
        || manifest.name.len() > 128
        || manifest.version.is_empty()
    {
        return Err(ContractError::InvalidManifest("invalid identity".into()));
    }
    if manifest.manifest_schema != 1
        || manifest.entry.is_empty()
        || manifest.entry.len() > 256
        || manifest.entry.starts_with('/')
        || manifest.entry.contains("..")
        || manifest.entry.contains('\\')
    {
        return Err(ContractError::InvalidManifest(
            "invalid source entry".into(),
        ));
    }
    if manifest.integrity_sha256.len() != 64
        || !manifest
            .integrity_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(ContractError::InvalidManifest(
            "integrity hash must be hexadecimal sha256".into(),
        ));
    }
    for capability in &manifest.capabilities {
        if !known_capabilities.contains(capability) {
            return Err(ContractError::UnsupportedCapability(capability.clone()));
        }
    }
    for dependency in &manifest.dependencies {
        if *dependency == manifest.package_id || !package_ids.contains(dependency) {
            return Err(ContractError::InvalidManifest(
                "unknown or self dependency".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StoredValue {
    Null,
    Bool(bool),
    Integer(i64),
    Number(f64),
    String(String),
    List(Vec<StoredValue>),
    Record(BTreeMap<String, StoredValue>),
}
impl StoredValue {
    pub fn validate(&self, depth: usize) -> Result<usize, ContractError> {
        if depth > MAX_STORAGE_DEPTH {
            return Err(ContractError::InvalidStorage(
                "maximum depth exceeded".into(),
            ));
        }
        let size = match self {
            Self::Null => 1,
            Self::Bool(_) => 2,
            Self::Integer(_) => 9,
            Self::Number(value) if value.is_finite() => 9,
            Self::Number(_) => {
                return Err(ContractError::InvalidStorage(
                    "number must be finite".into(),
                ));
            }
            Self::String(value) => {
                if value.len() > MAX_STORAGE_VALUE_BYTES {
                    return Err(ContractError::InvalidStorage("string too large".into()));
                }
                value.len() + 1
            }
            Self::List(values) => {
                1 + values.iter().try_fold(0usize, |sum, value| {
                    Ok::<_, ContractError>(sum.saturating_add(value.validate(depth + 1)?))
                })?
            }
            Self::Record(values) => {
                if values.len() > MAX_STORAGE_KEYS {
                    return Err(ContractError::InvalidStorage("too many keys".into()));
                }
                values.iter().try_fold(1usize, |sum, (key, value)| {
                    if key.is_empty() || key.len() > MAX_STORAGE_KEY_BYTES {
                        return Err(ContractError::InvalidStorage("invalid key".into()));
                    }
                    Ok(sum
                        .saturating_add(key.len())
                        .saturating_add(value.validate(depth + 1)?))
                })?
            }
        };
        if size > MAX_STORAGE_VALUE_BYTES {
            return Err(ContractError::InvalidStorage("value too large".into()));
        }
        Ok(size)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StorageNamespace {
    pub account_id: AccountId,
    pub package_id: PackageId,
    pub schema_version: u32,
}
#[derive(Clone, Debug, Default)]
pub struct Storage {
    values: BTreeMap<StorageNamespace, BTreeMap<String, StoredValue>>,
}
impl Storage {
    pub fn get(&self, namespace: &StorageNamespace, key: &str) -> Option<&StoredValue> {
        self.values.get(namespace)?.get(key)
    }
    pub fn set(
        &mut self,
        namespace: StorageNamespace,
        key: String,
        value: StoredValue,
    ) -> Result<(), ContractError> {
        if key.is_empty() || key.len() > MAX_STORAGE_KEY_BYTES {
            return Err(ContractError::InvalidStorage("invalid key".into()));
        }
        value.validate(0)?;
        let map = self.values.entry(namespace).or_default();
        if !map.contains_key(&key) && map.len() >= MAX_STORAGE_KEYS {
            return Err(ContractError::InvalidStorage("too many keys".into()));
        }
        let mut candidate = map.clone();
        candidate.insert(key, value);
        let total = candidate.iter().try_fold(0usize, |sum, (key, value)| {
            Ok::<_, ContractError>(
                sum.saturating_add(key.len())
                    .saturating_add(value.validate(0)?),
            )
        })?;
        if total > MAX_STORAGE_BYTES {
            return Err(ContractError::InvalidStorage("namespace too large".into()));
        }
        *map = candidate;
        Ok(())
    }
    pub fn delete(&mut self, namespace: &StorageNamespace, key: &str) {
        if let Some(map) = self.values.get_mut(namespace) {
            map.remove(key);
        }
    }

    pub fn snapshot_namespace(
        &self,
        namespace: &StorageNamespace,
    ) -> BTreeMap<String, StoredValue> {
        self.values.get(namespace).cloned().unwrap_or_default()
    }

    pub fn replace_namespace(
        &mut self,
        namespace: StorageNamespace,
        values: BTreeMap<String, StoredValue>,
    ) -> Result<(), ContractError> {
        let mut candidate = Storage::default();
        for (key, value) in values {
            candidate.set(namespace.clone(), key, value)?;
        }
        let snapshot = candidate.snapshot_namespace(&namespace);
        self.values.insert(namespace, snapshot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids() -> (PackageId, Generation, NodeId) {
        (
            PackageId::new(1).unwrap(),
            Generation::new(1).unwrap(),
            NodeId::new(1).unwrap(),
        )
    }
    #[test]
    fn queue_coalesces_state_but_preserves_ordered_events() {
        let mut q = EventQueue::new(3, 10_000).unwrap();
        q.push(UiEvent::ordered("combat.received"));
        q.push(UiEvent::replaceable(
            "player.updated",
            "player",
            ViewRecord::new("A", None, 0, 0),
        ));
        assert_eq!(
            q.push(UiEvent::replaceable(
                "player.updated",
                "player",
                ViewRecord::new("B", None, 0, 0)
            )),
            QueueOutcome::Coalesced
        );
        assert_eq!(q.pop_front().unwrap().name, "combat.received");
        assert_eq!(q.pop_front().unwrap().view.unwrap().player_name, "B");
    }
    #[test]
    fn ordered_event_disables_only_full_queue() {
        let mut q = EventQueue::new(1, 100).unwrap();
        q.push(UiEvent::ordered("one"));
        assert_eq!(q.push(UiEvent::ordered("two")), QueueOutcome::Disabled);
        assert!(q.stats().disabled);
    }
    #[test]
    fn queue_pressure_drops_replaceable_state_once_and_then_disables_ordered_work() {
        let mut q = EventQueue::new(2, 10_000).unwrap();
        assert_eq!(
            q.push(UiEvent::replaceable(
                "player.updated",
                "player",
                ViewRecord::new("A", None, 0, 0),
            )),
            QueueOutcome::Enqueued
        );
        q.push(UiEvent::ordered("combat.received"));
        assert_eq!(
            q.push(UiEvent::replaceable(
                "target.updated",
                "target",
                ViewRecord::new("A", Some("Wolf".into()), 0, 0),
            )),
            QueueOutcome::Enqueued
        );
        assert_eq!(q.stats().dropped, 1);
        assert_eq!(
            q.push(UiEvent::ordered("notification.received")),
            QueueOutcome::Enqueued
        );
        assert_eq!(
            q.push(UiEvent::ordered("host-error")),
            QueueOutcome::Disabled
        );
        assert_eq!(q.stats().dropped, 2);
        assert!(q.stats().disabled);
    }
    #[test]
    fn operations_are_atomic_on_foreign_or_stale_handles() {
        let (package, generation, node) = ids();
        let good = Handle {
            node,
            owner: package,
            generation,
        };
        let bad = Handle {
            node,
            owner: PackageId::new(2).unwrap(),
            generation,
        };
        let ops = [
            UiOperation::CreatePanel {
                handle: good,
                text: "ok".into(),
            },
            UiOperation::SetProperty {
                handle: bad,
                property: Property::Text("must not commit".into()),
            },
        ];
        assert_eq!(
            validate_operations(&ops, package, generation, &UiLimits::default()),
            Err(ContractError::ForeignHandle)
        );
    }
    #[test]
    fn manifest_rejects_traversal_and_unknown_capability() {
        let (package, _, _) = ids();
        let mut capabilities = BTreeSet::new();
        capabilities.insert("ui.panel".into());
        let manifest = Manifest {
            package_id: package,
            name: "demo".into(),
            version: "1".into(),
            manifest_schema: 1,
            api_range: API_VERSION.into(),
            runtime_range: "1".into(),
            entry: "../main.lua".into(),
            load_order: 0,
            dependencies: vec![],
            capabilities: ["network".into()].into_iter().collect(),
            saved_data: false,
            asset_ids: vec![],
            integrity_sha256: "0".repeat(64),
        };
        let packages = [package].into_iter().collect();
        assert!(matches!(
            validate_manifest(&manifest, &capabilities, &packages),
            Err(ContractError::InvalidManifest(_))
        ));
    }
    #[test]
    fn storage_is_account_and_package_scoped_and_preserves_failed_write() {
        let account = AccountId::new(1).unwrap();
        let package = PackageId::new(1).unwrap();
        let namespace = StorageNamespace {
            account_id: account,
            package_id: package,
            schema_version: 1,
        };
        let mut storage = Storage::default();
        storage
            .set(
                namespace.clone(),
                "name".into(),
                StoredValue::String("Aria".into()),
            )
            .unwrap();
        assert_eq!(
            storage.set(
                namespace.clone(),
                "name".into(),
                StoredValue::String("x".repeat(MAX_STORAGE_VALUE_BYTES + 1))
            ),
            Err(ContractError::InvalidStorage("string too large".into()))
        );
        assert_eq!(
            storage.get(&namespace, "name"),
            Some(&StoredValue::String("Aria".into()))
        );
    }
}
