#![forbid(unsafe_code)]

//! Minimal Luau UI-addon boundary for the client technology spike.
//!
//! The VM is intentionally useful only for presentation. It receives a
//! sanitized visible view model, can create and update owned UI nodes, and
//! can register callbacks for permitted UI events. Secure gameplay input is a
//! native host operation and is never exposed as a script function.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, Result as LuaResult, VmState};
pub use mmorpg_ui_contract::ViewRecord;
use mmorpg_ui_contract::{
    AccountId, EventQueue, Generation, Handle, Manifest, NodeId, PackageId, Property, QueueOutcome,
    Storage, StorageNamespace, StoredValue, UiEvent, UiLimits, UiOperation, validate_manifest,
    validate_operations,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_MAX_NODES: usize = 64;
pub const DEFAULT_MAX_EVENTS: usize = 16;
pub const DEFAULT_MAX_SCRIPT_BYTES: usize = 64 * 1024;
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 100_000;
pub const DEFAULT_MAX_OPERATIONS: usize = 512;
const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_SECURE_INTENTS: usize = 64;
static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PACKAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonPolicy {
    pub max_nodes: usize,
    pub max_events: usize,
    pub max_script_bytes: usize,
    pub max_memory_bytes: usize,
    pub max_instructions: u64,
    pub max_operations: usize,
}

impl Default for AddonPolicy {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES,
            max_events: DEFAULT_MAX_EVENTS,
            max_script_bytes: DEFAULT_MAX_SCRIPT_BYTES,
            max_memory_bytes: DEFAULT_MAX_MEMORY_BYTES,
            max_instructions: DEFAULT_MAX_INSTRUCTIONS,
            max_operations: DEFAULT_MAX_OPERATIONS,
        }
    }
}

/// Compatibility name for the Luau adapter. The value itself is owned by the
/// language-neutral `ui.v1` contract, so the VM cannot define a parallel view
/// schema.
pub type VisibleState = ViewRecord;

const ALLOWLISTED_SECURE_ACTIONS: &[&str] = &["basic_attack"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiNode {
    pub id: u64,
    pub owner: String,
    pub text: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecureIntent {
    pub action: String,
    pub node_id: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostSnapshot {
    pub nodes: Vec<UiNode>,
    pub registered_events: usize,
    pub secure_intents: Vec<SecureIntent>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddonError {
    ScriptTooLarge { bytes: usize, maximum: usize },
    InvalidPolicy,
    InvalidManifest(String),
    IntegrityMismatch { expected: String, actual: String },
    Runtime(String),
    Disabled(String),
    PackageIo(String),
    PackageParse(String),
}

impl std::fmt::Display for AddonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptTooLarge { bytes, maximum } => {
                write!(formatter, "script has {bytes} bytes; maximum is {maximum}")
            }
            Self::InvalidPolicy => formatter.write_str("addon policy limits must be positive"),
            Self::InvalidManifest(message) => {
                write!(formatter, "invalid addon manifest: {message}")
            }
            Self::IntegrityMismatch { expected, actual } => write!(
                formatter,
                "addon source integrity mismatch: expected {expected}, got {actual}"
            ),
            Self::Runtime(message) => write!(formatter, "addon runtime error: {message}"),
            Self::Disabled(message) => write!(formatter, "addon is disabled: {message}"),
            Self::PackageIo(message) => write!(formatter, "addon package I/O error: {message}"),
            Self::PackageParse(message) => {
                write!(formatter, "addon manifest parse error: {message}")
            }
        }
    }
}

impl std::error::Error for AddonError {}

const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_DISCOVERED_PACKAGES: usize = 64;

#[derive(Debug, Deserialize)]
struct ManifestFile {
    package_id: u64,
    name: String,
    version: String,
    manifest_schema: u32,
    api_range: String,
    runtime_range: String,
    entry: String,
    load_order: i32,
    #[serde(default)]
    dependencies: Vec<u64>,
    #[serde(default)]
    capabilities: BTreeSet<String>,
    #[serde(default)]
    saved_data: bool,
    #[serde(default)]
    asset_ids: Vec<String>,
    integrity_sha256: String,
}

impl TryFrom<ManifestFile> for Manifest {
    type Error = AddonError;

    fn try_from(file: ManifestFile) -> Result<Self, Self::Error> {
        let package_id = PackageId::new(file.package_id)
            .ok_or_else(|| AddonError::PackageParse("package_id must be non-zero".into()))?;
        let dependencies = file
            .dependencies
            .into_iter()
            .map(|value| {
                PackageId::new(value).ok_or_else(|| {
                    AddonError::PackageParse("dependency IDs must be non-zero".into())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Manifest {
            package_id,
            name: file.name,
            version: file.version,
            manifest_schema: file.manifest_schema,
            api_range: file.api_range,
            runtime_range: file.runtime_range,
            entry: file.entry,
            load_order: file.load_order,
            dependencies,
            capabilities: file.capabilities,
            saved_data: file.saved_data,
            asset_ids: file.asset_ids,
            integrity_sha256: file.integrity_sha256,
        })
    }
}

/// Bounded source-package loader. The repository layout is one directory per
/// numeric package ID containing `manifest.toml` and the manifest's source
/// entry. It performs all filesystem access before VM creation.
#[derive(Clone, Debug)]
pub struct PackageRepository {
    root: PathBuf,
}

impl PackageRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Discovers the bounded numeric package directories beneath the
    /// repository root. Non-directory and non-numeric entries are ignored so
    /// repositories can contain documentation or other local metadata.
    pub fn discover_package_ids(&self) -> Result<BTreeSet<PackageId>, AddonError> {
        let entries = fs::read_dir(&self.root)
            .map_err(|error| AddonError::PackageIo(format!("cannot read package root: {error}")))?;
        let mut package_ids = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                AddonError::PackageIo(format!("cannot read package entry: {error}"))
            })?;
            if !entry
                .file_type()
                .map_err(|error| {
                    AddonError::PackageIo(format!("cannot inspect package entry: {error}"))
                })?
                .is_dir()
            {
                continue;
            }
            let Some(value) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u64>().ok())
            else {
                continue;
            };
            let package_id = PackageId::new(value)
                .ok_or_else(|| AddonError::PackageParse("package IDs must be non-zero".into()))?;
            if package_ids.len() == MAX_DISCOVERED_PACKAGES {
                return Err(AddonError::PackageParse(
                    "package repository contains too many packages".into(),
                ));
            }
            package_ids.insert(package_id);
        }
        Ok(package_ids)
    }

    fn read_manifest(&self, package_id: PackageId) -> Result<Manifest, AddonError> {
        let package_dir = self.root.join(package_id.get().to_string());
        let manifest_path = package_dir.join("manifest.toml");
        let manifest_source = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let file: ManifestFile = toml::from_str(&manifest_source)
            .map_err(|error| AddonError::PackageParse(error.to_string()))?;
        let manifest = Manifest::try_from(file)?;
        if manifest.package_id != package_id {
            return Err(AddonError::PackageParse(
                "directory and manifest package IDs differ".into(),
            ));
        }
        Ok(manifest)
    }

    /// Resolves a complete package set before any VM is created. Dependencies
    /// always precede dependents; unrelated packages use manifest load order
    /// and then stable package ID as deterministic tie-breakers.
    pub fn resolve_order(
        &self,
        package_ids: &BTreeSet<PackageId>,
        known_capabilities: &BTreeSet<String>,
    ) -> Result<Vec<PackageId>, AddonError> {
        let mut manifests = BTreeMap::new();
        for package_id in package_ids {
            let manifest = self.read_manifest(*package_id)?;
            validate_manifest(&manifest, known_capabilities, package_ids)
                .map_err(|error| AddonError::InvalidManifest(error.to_string()))?;
            manifests.insert(*package_id, manifest);
        }

        let mut indegree = manifests
            .keys()
            .copied()
            .map(|package_id| (package_id, 0usize))
            .collect::<BTreeMap<_, _>>();
        let mut dependents = BTreeMap::<PackageId, Vec<PackageId>>::new();
        for (package_id, manifest) in &manifests {
            for dependency in &manifest.dependencies {
                *indegree
                    .get_mut(package_id)
                    .expect("manifest IDs were inserted above") += 1;
                dependents.entry(*dependency).or_default().push(*package_id);
            }
        }

        let mut ready = manifests
            .iter()
            .filter_map(|(package_id, manifest)| {
                (indegree[package_id] == 0).then_some((manifest.load_order, *package_id))
            })
            .collect::<Vec<_>>();
        let mut order = Vec::with_capacity(manifests.len());
        while !ready.is_empty() {
            ready.sort_unstable();
            let (_, package_id) = ready.remove(0);
            order.push(package_id);
            if let Some(children) = dependents.get(&package_id) {
                for child in children {
                    let count = indegree
                        .get_mut(child)
                        .expect("dependent IDs were validated above");
                    *count -= 1;
                    if *count == 0 {
                        ready.push((manifests[child].load_order, *child));
                    }
                }
            }
        }
        if order.len() != manifests.len() {
            return Err(AddonError::InvalidManifest(
                "addon dependency graph contains a cycle".into(),
            ));
        }
        Ok(order)
    }

    pub fn load(
        &self,
        package_id: PackageId,
        policy: AddonPolicy,
        known_capabilities: &BTreeSet<String>,
        package_ids: &BTreeSet<PackageId>,
    ) -> Result<AddonRunner, AddonError> {
        let package_dir = self.root.join(package_id.get().to_string());
        let manifest = self.read_manifest(package_id)?;
        let entry = safe_entry_path(&package_dir, &manifest.entry)?;
        let source = read_bounded(&entry, policy.max_script_bytes)?;
        AddonRunner::load_from_manifest(&manifest, &source, policy, known_capabilities, package_ids)
    }

    /// Discovers and loads the complete repository in deterministic dependency
    /// order. All manifests and source files are validated before the first
    /// VM is constructed.
    pub fn load_all(
        &self,
        policy: AddonPolicy,
        known_capabilities: &BTreeSet<String>,
    ) -> Result<Vec<AddonRunner>, AddonError> {
        let package_ids = self.discover_package_ids()?;
        let order = self.resolve_order(&package_ids, known_capabilities)?;
        order
            .into_iter()
            .map(|package_id| {
                self.load(package_id, policy.clone(), known_capabilities, &package_ids)
            })
            .collect()
    }
}

const STORAGE_QUEUE_CAPACITY: usize = 32;
const MAX_STORAGE_FILE_BYTES: usize = mmorpg_ui_contract::MAX_STORAGE_BYTES + 16 * 1024;
pub const MAX_STORAGE_COMMITS_PER_MINUTE: usize = 10;
const STORAGE_RATE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StorageFile {
    schema_version: u32,
    values: BTreeMap<String, StoredValue>,
}

enum StorageJob {
    Set {
        request_id: u64,
        key: String,
        value: StoredValue,
    },
    Delete {
        request_id: u64,
        key: String,
    },
    Stop,
}

pub struct StorageResult {
    pub request_id: u64,
    pub result: Result<(), String>,
}

/// Bounded off-thread persistence for one account/package/schema namespace.
/// The caller only queues validated values and polls ordered results; the
/// worker owns all file writes and atomically replaces the last good file.
pub struct StorageWorker {
    sender: Option<SyncSender<StorageJob>>,
    results: Receiver<StorageResult>,
    thread: Option<JoinHandle<()>>,
}

impl StorageWorker {
    pub fn open(path: PathBuf, namespace: StorageNamespace) -> Result<Self, AddonError> {
        let mut initial = Storage::default();
        if path.exists() {
            let source = read_bounded(&path, MAX_STORAGE_FILE_BYTES)?;
            let file: StorageFile = toml::from_str(&source)
                .map_err(|error| AddonError::PackageParse(error.to_string()))?;
            if file.schema_version != namespace.schema_version {
                return Err(AddonError::PackageParse(
                    "storage schema version mismatch".into(),
                ));
            }
            initial
                .replace_namespace(namespace.clone(), file.values)
                .map_err(|error| AddonError::PackageParse(error.to_string()))?;
        }
        let (sender, receiver) = mpsc::sync_channel(STORAGE_QUEUE_CAPACITY);
        let (result_sender, results) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("mmorpg-addon-storage".to_owned())
            .spawn(move || {
                let mut storage = initial;
                let mut committed_at = VecDeque::new();
                while let Ok(job) = receiver.recv() {
                    match job {
                        StorageJob::Set {
                            request_id,
                            key,
                            value,
                        } => {
                            let now = Instant::now();
                            committed_at.retain(|committed| {
                                now.duration_since(*committed) < STORAGE_RATE_WINDOW
                            });
                            let result = if committed_at.len() >= MAX_STORAGE_COMMITS_PER_MINUTE {
                                Err("storage commit rate limit exceeded".to_owned())
                            } else {
                                let mut candidate = storage.clone();
                                let result = candidate
                                    .set(namespace.clone(), key, value)
                                    .map_err(|error| error.to_string())
                                    .and_then(|()| {
                                        persist_namespace(&path, &candidate, &namespace)
                                    });
                                if result.is_ok() {
                                    storage = candidate;
                                    committed_at.push_back(now);
                                }
                                result
                            };
                            let _ = result_sender.send(StorageResult { request_id, result });
                        }
                        StorageJob::Delete { request_id, key } => {
                            let now = Instant::now();
                            committed_at.retain(|committed| {
                                now.duration_since(*committed) < STORAGE_RATE_WINDOW
                            });
                            let result = if committed_at.len() >= MAX_STORAGE_COMMITS_PER_MINUTE {
                                Err("storage commit rate limit exceeded".to_owned())
                            } else {
                                let mut candidate = storage.clone();
                                candidate.delete(&namespace, &key);
                                let result = persist_namespace(&path, &candidate, &namespace);
                                if result.is_ok() {
                                    storage = candidate;
                                    committed_at.push_back(now);
                                }
                                result
                            };
                            let _ = result_sender.send(StorageResult { request_id, result });
                        }
                        StorageJob::Stop => break,
                    }
                }
            })
            .map_err(|error| {
                AddonError::PackageIo(format!("cannot start storage worker: {error}"))
            })?;
        Ok(Self {
            sender: Some(sender),
            results,
            thread: Some(thread),
        })
    }

    pub fn set(&self, request_id: u64, key: String, value: StoredValue) -> Result<(), String> {
        if key.is_empty() || key.len() > mmorpg_ui_contract::MAX_STORAGE_KEY_BYTES {
            return Err("invalid storage key".to_owned());
        }
        value.validate(0).map_err(|error| error.to_string())?;
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| "worker stopped".to_owned())?;
        sender
            .try_send(StorageJob::Set {
                request_id,
                key,
                value,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => "storage queue is full".to_owned(),
                TrySendError::Disconnected(_) => "storage worker disconnected".to_owned(),
            })
    }

    pub fn delete(&self, request_id: u64, key: String) -> Result<(), String> {
        if key.is_empty() || key.len() > mmorpg_ui_contract::MAX_STORAGE_KEY_BYTES {
            return Err("invalid storage key".to_owned());
        }
        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| "worker stopped".to_owned())?;
        sender
            .try_send(StorageJob::Delete { request_id, key })
            .map_err(|error| match error {
                TrySendError::Full(_) => "storage queue is full".to_owned(),
                TrySendError::Disconnected(_) => "storage worker disconnected".to_owned(),
            })
    }

    pub fn try_result(&self) -> Option<StorageResult> {
        self.results.try_recv().ok()
    }
}

impl Drop for StorageWorker {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(StorageJob::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn persist_namespace(
    path: &Path,
    storage: &Storage,
    namespace: &StorageNamespace,
) -> Result<(), String> {
    let source = toml::to_string(&StorageFile {
        schema_version: namespace.schema_version,
        values: storage.snapshot_namespace(namespace),
    })
    .map_err(|error| format!("storage serialization failed: {error}"))?;
    if source.len() > MAX_STORAGE_FILE_BYTES {
        return Err("storage file exceeds the bounded file limit".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("storage directory failed: {error}"))?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = match File::create(&temporary) {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(format!("storage temporary write failed: {error}"));
        }
    };
    if let Err(error) = file
        .write_all(source.as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = fs::remove_file(&temporary);
        return Err(format!("storage temporary sync failed: {error}"));
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("storage atomic replace failed: {error}"));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
        return Err(format!("storage directory sync failed: {error}"));
    }
    Ok(())
}

fn read_bounded(path: &Path, maximum: usize) -> Result<String, AddonError> {
    let metadata = fs::metadata(path)
        .map_err(|error| AddonError::PackageIo(format!("{}: {error}", path.display())))?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err(AddonError::PackageIo(format!(
            "{} exceeds the bounded package file limit",
            path.display()
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| AddonError::PackageIo(format!("{}: {error}", path.display())))?;
    String::from_utf8(bytes)
        .map_err(|_| AddonError::PackageParse(format!("{} is not UTF-8", path.display())))
}

fn safe_entry_path(package_dir: &Path, entry: &str) -> Result<PathBuf, AddonError> {
    let entry_path = Path::new(entry);
    if entry_path.is_absolute()
        || entry_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(AddonError::PackageParse(
            "manifest entry escapes its package directory".into(),
        ));
    }
    Ok(package_dir.join(entry_path))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchResult {
    Applied,
    Disabled(String),
}

#[derive(Clone, Debug)]
struct HostState {
    addon_id: String,
    account_id: AccountId,
    package_id: PackageId,
    generation: Generation,
    policy: AddonPolicy,
    nodes: BTreeMap<u64, UiNode>,
    registered_events: Vec<String>,
    secure_intents: Vec<SecureIntent>,
    errors: Vec<String>,
    storage: Rc<RefCell<Storage>>,
}

impl HostState {
    fn snapshot(&self) -> HostSnapshot {
        HostSnapshot {
            nodes: self.nodes.values().cloned().collect(),
            registered_events: self.registered_events.len(),
            secure_intents: self.secure_intents.clone(),
            errors: self.errors.clone(),
        }
    }

    fn check_owner(&self, node_id: u64) -> LuaResult<()> {
        match self.nodes.get(&node_id) {
            Some(node) if node.owner == self.addon_id => Ok(()),
            _ => Err(mlua::Error::RuntimeError(
                "UI node is not owned by this addon".to_owned(),
            )),
        }
    }

    fn handle(&self, node_id: u64) -> LuaResult<Handle> {
        let node = NodeId::new(node_id)
            .ok_or_else(|| mlua::Error::RuntimeError("UI node handle is invalid".to_owned()))?;
        Ok(Handle {
            node,
            owner: self.package_id,
            generation: self.generation,
        })
    }

    fn apply_operations(&mut self, operations: &[UiOperation]) -> LuaResult<()> {
        validate_operations(
            operations,
            self.package_id,
            self.generation,
            &UiLimits {
                max_nodes: self.policy.max_nodes,
                max_operations: self.policy.max_operations,
                max_subscriptions: self.policy.max_events,
            },
        )
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;

        let mut nodes = self.nodes.clone();
        for operation in operations {
            match operation {
                UiOperation::CreatePanel { handle, text } => {
                    if nodes.contains_key(&handle.node.get()) {
                        return Err(mlua::Error::RuntimeError(
                            "UI node already exists".to_owned(),
                        ));
                    }
                    if nodes.len() >= self.policy.max_nodes {
                        return Err(mlua::Error::RuntimeError(
                            "UI node budget exceeded".to_owned(),
                        ));
                    }
                    nodes.insert(
                        handle.node.get(),
                        UiNode {
                            id: handle.node.get(),
                            owner: self.addon_id.clone(),
                            text: text.clone(),
                            x: 0,
                            y: 0,
                        },
                    );
                }
                UiOperation::SetProperty { handle, property } => {
                    let node = nodes.get_mut(&handle.node.get()).ok_or_else(|| {
                        mlua::Error::RuntimeError("UI node is not owned by this addon".to_owned())
                    })?;
                    match property {
                        Property::Text(text) => node.text = text.clone(),
                        Property::Position { x, y } => {
                            node.x = *x;
                            node.y = *y;
                        }
                    }
                }
                UiOperation::Destroy { handle } => {
                    if nodes.remove(&handle.node.get()).is_none() {
                        return Err(mlua::Error::RuntimeError(
                            "UI node is not owned by this addon".to_owned(),
                        ));
                    }
                }
                UiOperation::PresentSecureAction { .. }
                | UiOperation::Subscribe { .. }
                | UiOperation::Unsubscribe { .. }
                | UiOperation::CreateTimer { .. } => {}
            }
        }
        self.nodes = nodes;
        Ok(())
    }

    fn storage_namespace(&self) -> StorageNamespace {
        StorageNamespace {
            account_id: self.account_id,
            package_id: self.package_id,
            schema_version: 1,
        }
    }
}

/// One isolated Luau addon instance.
pub struct AddonRunner {
    lua: Lua,
    host: Rc<RefCell<HostState>>,
    callbacks: Rc<RefCell<Vec<(String, mlua::RegistryKey)>>>,
    operation_buffer: Rc<RefCell<Option<Vec<UiOperation>>>>,
    registration_buffer: Rc<RefCell<Option<Vec<(String, mlua::RegistryKey)>>>>,
    event_queue: EventQueue,
    instruction_count: Arc<AtomicU64>,
    disabled: Option<String>,
}

impl AddonRunner {
    pub fn load(
        addon_id: impl Into<String>,
        source: &str,
        policy: AddonPolicy,
    ) -> Result<Self, AddonError> {
        let package_id = PackageId::new(NEXT_PACKAGE_ID.fetch_add(1, Ordering::Relaxed))
            .expect("package IDs cannot be zero");
        Self::load_internal(
            addon_id.into(),
            AccountId::new(1).expect("default account cannot be zero"),
            package_id,
            Rc::new(RefCell::new(Storage::default())),
            source,
            policy,
        )
    }

    pub fn load_for_account(
        addon_id: impl Into<String>,
        account_id: AccountId,
        storage: Rc<RefCell<Storage>>,
        source: &str,
        policy: AddonPolicy,
    ) -> Result<Self, AddonError> {
        let package_id = PackageId::new(NEXT_PACKAGE_ID.fetch_add(1, Ordering::Relaxed))
            .expect("package IDs cannot be zero");
        Self::load_for_account_with_package(
            addon_id, account_id, package_id, storage, source, policy,
        )
    }

    pub fn load_for_account_with_package(
        addon_id: impl Into<String>,
        account_id: AccountId,
        package_id: PackageId,
        storage: Rc<RefCell<Storage>>,
        source: &str,
        policy: AddonPolicy,
    ) -> Result<Self, AddonError> {
        Self::load_internal(
            addon_id.into(),
            account_id,
            package_id,
            storage,
            source,
            policy,
        )
    }

    /// Validates a source-only package completely before constructing its VM.
    /// The manifest remains an adapter entry point until package loading gains
    /// a filesystem/package repository; callers provide the already-read
    /// source and the known package/capability catalog explicitly.
    pub fn load_from_manifest(
        manifest: &Manifest,
        source: &str,
        policy: AddonPolicy,
        known_capabilities: &std::collections::BTreeSet<String>,
        package_ids: &std::collections::BTreeSet<PackageId>,
    ) -> Result<Self, AddonError> {
        validate_manifest(manifest, known_capabilities, package_ids)
            .map_err(|error| AddonError::InvalidManifest(error.to_string()))?;
        let actual = source_sha256(source);
        if !manifest.integrity_sha256.eq_ignore_ascii_case(&actual) {
            return Err(AddonError::IntegrityMismatch {
                expected: manifest.integrity_sha256.clone(),
                actual,
            });
        }
        Self::load_internal(
            manifest.name.clone(),
            AccountId::new(1).expect("default account cannot be zero"),
            manifest.package_id,
            Rc::new(RefCell::new(Storage::default())),
            source,
            policy,
        )
    }

    fn load_internal(
        addon_id: String,
        account_id: AccountId,
        package_id: PackageId,
        storage: Rc<RefCell<Storage>>,
        source: &str,
        policy: AddonPolicy,
    ) -> Result<Self, AddonError> {
        validate_policy(&policy)?;
        if source.len() > policy.max_script_bytes {
            return Err(AddonError::ScriptTooLarge {
                bytes: source.len(),
                maximum: policy.max_script_bytes,
            });
        }

        let lua = Lua::new();
        // Remove dangerous libraries before Luau makes the global table
        // read-only.  The sandbox then protects the reduced library set.
        let globals = lua.globals();
        for name in [
            "io", "os", "debug", "package", "require", "loadfile", "dofile",
        ] {
            globals
                .set(name, mlua::Value::Nil)
                .map_err(|error| AddonError::Runtime(error.to_string()))?;
        }
        lua.sandbox(true)
            .map_err(|error| AddonError::Runtime(error.to_string()))?;
        lua.set_memory_limit(policy.max_memory_bytes)
            .map_err(|error| AddonError::Runtime(error.to_string()))?;
        let instruction_count = Arc::new(AtomicU64::new(0));
        let interrupt_count = Arc::clone(&instruction_count);
        let instruction_limit = policy.max_instructions;
        lua.set_interrupt(move |_| {
            let count = interrupt_count.fetch_add(1, Ordering::Relaxed) + 1;
            if count > instruction_limit {
                Err(mlua::Error::RuntimeError(
                    "instruction budget exceeded".to_owned(),
                ))
            } else {
                Ok(VmState::Continue)
            }
        });

        let host = Rc::new(RefCell::new(HostState {
            addon_id,
            account_id,
            package_id,
            generation: Generation::new(1).expect("initial generation cannot be zero"),
            policy,
            nodes: BTreeMap::new(),
            registered_events: Vec::new(),
            secure_intents: Vec::new(),
            errors: Vec::new(),
            storage,
        }));
        let callbacks = Rc::new(RefCell::new(Vec::new()));
        let operation_buffer = Rc::new(RefCell::new(None));
        let registration_buffer = Rc::new(RefCell::new(None));
        install_api(
            &lua,
            Rc::clone(&host),
            Rc::clone(&callbacks),
            Rc::clone(&operation_buffer),
            Rc::clone(&registration_buffer),
        )
        .map_err(|error| AddonError::Runtime(error.to_string()))?;

        let mut runner = Self {
            lua,
            host,
            callbacks,
            operation_buffer,
            registration_buffer,
            event_queue: EventQueue::default_for_addon(),
            instruction_count,
            disabled: None,
        };
        if let Err(error) = runner.lua.load(source).exec() {
            let message = error.to_string();
            runner.disable(message.clone());
            return Err(AddonError::Runtime(message));
        }
        Ok(runner)
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled.is_some()
    }

    pub fn instruction_interrupts(&self) -> u64 {
        self.instruction_count.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> HostSnapshot {
        self.host.borrow().snapshot()
    }

    /// Enqueues a contract event without running addon code on the producer.
    /// Replaceable state is coalesced by the language-neutral contract; an
    /// ordered event that cannot fit disables only this addon.
    pub fn enqueue_event(&mut self, event: UiEvent) -> QueueOutcome {
        let outcome = self.event_queue.push(event);
        if outcome == QueueOutcome::Disabled {
            self.disable("addon event queue overflow".to_owned());
        }
        outcome
    }

    pub fn queued_event_count(&self) -> usize {
        self.event_queue.len()
    }

    /// Delivers all currently queued events in contract order. The queue is
    /// drained before each callback, so the producer remains independent from
    /// VM execution and a callback failure stops only this addon.
    pub fn dispatch_queued(&mut self) -> DispatchResult {
        let mut result = DispatchResult::Applied;
        while let Some(event) = self.event_queue.pop_front() {
            let view = event.view.as_ref();
            result = self.deliver_event_with_view(&event.name, view);
            if matches!(result, DispatchResult::Disabled(_)) {
                break;
            }
        }
        result
    }

    /// Delivers only the sanitized fields in the visible client view model.
    pub fn deliver_event(&mut self, event_name: &str, state: &VisibleState) -> DispatchResult {
        self.deliver_event_with_view(event_name, Some(state))
    }

    fn deliver_event_with_view(
        &mut self,
        event_name: &str,
        state: Option<&VisibleState>,
    ) -> DispatchResult {
        if let Some(reason) = self.disabled.clone() {
            return DispatchResult::Disabled(reason);
        }
        let result = (|| -> LuaResult<()> {
            let event = self.lua.create_table()?;
            event.set("name", event_name)?;
            if let Some(state) = state {
                let visible = self.lua.create_table()?;
                visible.set("player_name", state.player_name.as_str())?;
                visible.set("target_name", state.target_name.as_deref())?;
                visible.set("inventory_slots_used", state.inventory_slots_used)?;
                visible.set("quest_progress", state.quest_progress)?;
                event.set("visible", visible)?;
            }

            let callback_indices = self
                .callbacks
                .borrow()
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| name == event_name)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            for index in callback_indices {
                let checkpoint = self.host.borrow().clone();
                let storage_checkpoint = self.host.borrow().storage.borrow().clone();
                *self.operation_buffer.borrow_mut() = Some(Vec::new());
                *self.registration_buffer.borrow_mut() = Some(Vec::new());
                let callbacks = self.callbacks.borrow();
                let callback: Function = self.lua.registry_value(&callbacks[index].1)?;
                drop(callbacks);
                let callback_result = callback.call::<()>(event.clone());
                let operations = self
                    .operation_buffer
                    .borrow_mut()
                    .take()
                    .unwrap_or_default();
                let registrations = self
                    .registration_buffer
                    .borrow_mut()
                    .take()
                    .unwrap_or_default();
                match callback_result {
                    Ok(()) => {
                        let apply_result = self.host.borrow_mut().apply_operations(&operations);
                        if let Err(error) = apply_result {
                            *self.host.borrow_mut() = checkpoint;
                            *self.host.borrow().storage.borrow_mut() = storage_checkpoint;
                            return Err(error);
                        }
                        let mut host = self.host.borrow_mut();
                        host.registered_events
                            .extend(registrations.iter().map(|(name, _)| name.clone()));
                        drop(host);
                        self.callbacks.borrow_mut().extend(registrations);
                    }
                    Err(error) => {
                        *self.host.borrow_mut() = checkpoint;
                        *self.host.borrow().storage.borrow_mut() = storage_checkpoint;
                        return Err(error);
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => DispatchResult::Applied,
            Err(error) => {
                let message = error.to_string();
                // A callback is one presentation transaction. Restore every
                // host-owned mutation made during the dispatch before
                // recording the sanitized failure and disabling this addon.
                self.disable(message.clone());
                DispatchResult::Disabled(message)
            }
        }
    }

    /// A native secure-input path. Scripts cannot call this method because it
    /// is not registered in the `ui`, `game`, or `storage` tables.
    pub fn secure_input(&mut self, node_id: u64, action: &str) -> Result<(), AddonError> {
        if let Some(reason) = &self.disabled {
            return Err(AddonError::Disabled(reason.clone()));
        }
        if !ALLOWLISTED_SECURE_ACTIONS.contains(&action) {
            return Err(AddonError::Runtime(
                "secure action is not allowlisted".to_owned(),
            ));
        }
        let mut host = self.host.borrow_mut();
        host.check_owner(node_id)
            .map_err(|error| AddonError::Runtime(error.to_string()))?;
        if host.secure_intents.len() >= MAX_SECURE_INTENTS {
            return Err(AddonError::Runtime(
                "secure intent budget exceeded".to_owned(),
            ));
        }
        host.secure_intents.push(SecureIntent {
            action: action.to_owned(),
            node_id,
        });
        Ok(())
    }

    fn disable(&mut self, reason: String) {
        if self.disabled.is_none() {
            self.host.borrow_mut().errors.push(reason.clone());
            self.disabled = Some(reason);
        }
    }
}

fn submit_operation(
    host: &Rc<RefCell<HostState>>,
    operation_buffer: &Rc<RefCell<Option<Vec<UiOperation>>>>,
    operation: UiOperation,
) -> LuaResult<()> {
    if let Some(operations) = operation_buffer.borrow_mut().as_mut() {
        if operations.len() >= host.borrow().policy.max_operations {
            return Err(mlua::Error::RuntimeError(
                "UI operation budget exceeded".to_owned(),
            ));
        }
        operations.push(operation);
        Ok(())
    } else {
        host.borrow_mut().apply_operations(&[operation])
    }
}

fn source_sha256(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn lua_to_stored(value: mlua::Value, depth: usize) -> LuaResult<StoredValue> {
    if depth > mmorpg_ui_contract::MAX_STORAGE_DEPTH {
        return Err(mlua::Error::RuntimeError(
            "storage value is too deeply nested".to_owned(),
        ));
    }
    let stored = match value {
        mlua::Value::Nil => StoredValue::Null,
        mlua::Value::Boolean(value) => StoredValue::Bool(value),
        mlua::Value::Integer(value) => StoredValue::Integer(value),
        mlua::Value::Number(value) => StoredValue::Number(value),
        mlua::Value::String(value) => StoredValue::String(
            value
                .to_str()
                .map_err(|_| mlua::Error::RuntimeError("storage string is not UTF-8".to_owned()))?
                .to_owned(),
        ),
        mlua::Value::Table(table) => {
            let mut entries = Vec::new();
            for entry in table.pairs::<mlua::Value, mlua::Value>() {
                if entries.len() >= mmorpg_ui_contract::MAX_STORAGE_KEYS {
                    return Err(mlua::Error::RuntimeError(
                        "storage value has too many entries".to_owned(),
                    ));
                }
                let (key, value) = entry?;
                entries.push((key, lua_to_stored(value, depth + 1)?));
            }
            let is_list = entries.iter().enumerate().all(|(index, (key, _))| {
                matches!(key, mlua::Value::Integer(value) if *value == index as i64 + 1)
            });
            if is_list {
                StoredValue::List(entries.into_iter().map(|(_, value)| value).collect())
            } else {
                let mut record = std::collections::BTreeMap::new();
                for (key, value) in entries {
                    let mlua::Value::String(key) = key else {
                        return Err(mlua::Error::RuntimeError(
                            "storage records require string keys".to_owned(),
                        ));
                    };
                    record.insert(
                        key.to_str()
                            .map_err(|_| {
                                mlua::Error::RuntimeError("storage key is not UTF-8".to_owned())
                            })?
                            .to_owned(),
                        value,
                    );
                }
                StoredValue::Record(record)
            }
        }
        _ => {
            return Err(mlua::Error::RuntimeError(
                "storage value type is unsupported".to_owned(),
            ));
        }
    };
    stored
        .validate(0)
        .map_err(|error| mlua::Error::RuntimeError(error.to_string()))?;
    Ok(stored)
}

fn stored_to_lua(lua: &Lua, value: &StoredValue) -> LuaResult<mlua::Value> {
    Ok(match value {
        StoredValue::Null => mlua::Value::Nil,
        StoredValue::Bool(value) => mlua::Value::Boolean(*value),
        StoredValue::Integer(value) => mlua::Value::Integer(*value),
        StoredValue::Number(value) => mlua::Value::Number(*value),
        StoredValue::String(value) => mlua::Value::String(lua.create_string(value)?),
        StoredValue::List(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.iter().enumerate() {
                table.set(index + 1, stored_to_lua(lua, value)?)?;
            }
            mlua::Value::Table(table)
        }
        StoredValue::Record(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key.as_str(), stored_to_lua(lua, value)?)?;
            }
            mlua::Value::Table(table)
        }
    })
}

fn validate_policy(policy: &AddonPolicy) -> Result<(), AddonError> {
    if policy.max_nodes == 0
        || policy.max_events == 0
        || policy.max_script_bytes == 0
        || policy.max_memory_bytes == 0
        || policy.max_instructions == 0
        || policy.max_operations == 0
    {
        return Err(AddonError::InvalidPolicy);
    }
    Ok(())
}

fn install_api(
    lua: &Lua,
    host: Rc<RefCell<HostState>>,
    callbacks: Rc<RefCell<Vec<(String, mlua::RegistryKey)>>>,
    operation_buffer: Rc<RefCell<Option<Vec<UiOperation>>>>,
    registration_buffer: Rc<RefCell<Option<Vec<(String, mlua::RegistryKey)>>>>,
) -> LuaResult<()> {
    let ui = lua.create_table()?;

    let create_host = Rc::clone(&host);
    let create_buffer = Rc::clone(&operation_buffer);
    ui.set(
        "create_panel",
        lua.create_function(move |_, text: String| {
            if text.len() > MAX_TEXT_BYTES {
                return Err(mlua::Error::RuntimeError(
                    "panel text is too large".to_owned(),
                ));
            }
            let id = NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed);
            let handle = create_host.borrow().handle(id)?;
            submit_operation(
                &create_host,
                &create_buffer,
                UiOperation::CreatePanel { handle, text },
            )?;
            Ok(id)
        })?,
    )?;

    let set_text_host = Rc::clone(&host);
    let set_text_buffer = Rc::clone(&operation_buffer);
    ui.set(
        "set_text",
        lua.create_function(move |_, (node_id, text): (u64, String)| {
            if text.len() > MAX_TEXT_BYTES {
                return Err(mlua::Error::RuntimeError("text is too large".to_owned()));
            }
            let handle = set_text_host.borrow().handle(node_id)?;
            submit_operation(
                &set_text_host,
                &set_text_buffer,
                UiOperation::SetProperty {
                    handle,
                    property: Property::Text(text),
                },
            )
        })?,
    )?;

    let set_position_host = Rc::clone(&host);
    let set_position_buffer = Rc::clone(&operation_buffer);
    ui.set(
        "set_position",
        lua.create_function(move |_, (node_id, x, y): (u64, i32, i32)| {
            let handle = set_position_host.borrow().handle(node_id)?;
            submit_operation(
                &set_position_host,
                &set_position_buffer,
                UiOperation::SetProperty {
                    handle,
                    property: Property::Position { x, y },
                },
            )
        })?,
    )?;

    let register_host = Rc::clone(&host);
    let register_callbacks = Rc::clone(&callbacks);
    let register_operations = Rc::clone(&operation_buffer);
    let register_buffer = Rc::clone(&registration_buffer);
    ui.set(
        "on",
        lua.create_function(move |lua, (event_name, callback): (String, Function)| {
            if event_name.is_empty() || event_name.len() > 64 {
                return Err(mlua::Error::RuntimeError(
                    "event name is invalid".to_owned(),
                ));
            }
            let key = lua.create_registry_value(callback)?;
            if register_operations.borrow().is_some() {
                submit_operation(
                    &register_host,
                    &register_operations,
                    UiOperation::Subscribe {
                        event: event_name.clone(),
                    },
                )?;
                register_buffer
                    .borrow_mut()
                    .as_mut()
                    .expect("registration buffer is active with operation buffer")
                    .push((event_name, key));
                Ok(())
            } else {
                let mut host = register_host.borrow_mut();
                if host.registered_events.len() >= host.policy.max_events {
                    return Err(mlua::Error::RuntimeError(
                        "event budget exceeded".to_owned(),
                    ));
                }
                host.registered_events.push(event_name.clone());
                drop(host);
                register_callbacks.borrow_mut().push((event_name, key));
                Ok(())
            }
        })?,
    )?;

    let globals = lua.globals();
    globals.set("ui", ui)?;
    globals.set("game", lua.create_table()?)?;
    let storage = lua.create_table()?;
    let storage_host = Rc::clone(&host);
    storage.set(
        "get",
        lua.create_function(move |lua, key: String| {
            let host = storage_host.borrow();
            let namespace = host.storage_namespace();
            let value = host.storage.borrow().get(&namespace, &key).cloned();
            value.map(|value| stored_to_lua(lua, &value)).transpose()
        })?,
    )?;

    let storage_host = Rc::clone(&host);
    storage.set(
        "set",
        lua.create_function(move |_, (key, value): (String, mlua::Value)| {
            let value = lua_to_stored(value, 0)?;
            let host = storage_host.borrow();
            let namespace = host.storage_namespace();
            host.storage
                .borrow_mut()
                .set(namespace, key, value)
                .map_err(|error| mlua::Error::RuntimeError(error.to_string()))
        })?,
    )?;

    let storage_host = Rc::clone(&host);
    storage.set(
        "delete",
        lua.create_function(move |_, key: String| {
            let host = storage_host.borrow();
            let namespace = host.storage_namespace();
            host.storage.borrow_mut().delete(&namespace, &key);
            Ok(())
        })?,
    )?;
    globals.set("storage", storage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> VisibleState {
        VisibleState {
            player_name: "Aria".to_owned(),
            target_name: Some("Field Wolf".to_owned()),
            inventory_slots_used: 2,
            quest_progress: 1,
        }
    }

    #[test]
    fn default_ui_and_addon_use_the_same_ui_api() {
        let source = r#"
            local panel = ui.create_panel("health")
            ui.set_position(panel, 12, 20)
            ui.on("frame", function(event)
                ui.set_text(panel, event.visible.player_name)
            end)
        "#;
        let mut default_ui =
            AddonRunner::load("default-ui", source, AddonPolicy::default()).unwrap();
        let mut addon = AddonRunner::load("player-addon", source, AddonPolicy::default()).unwrap();
        assert_eq!(
            default_ui.deliver_event("frame", &state()),
            DispatchResult::Applied
        );
        assert_eq!(
            addon.deliver_event("frame", &state()),
            DispatchResult::Applied
        );
        assert_eq!(default_ui.snapshot().nodes[0].text, "Aria");
        assert_eq!(addon.snapshot().nodes[0].text, "Aria");
        assert_eq!(default_ui.snapshot().nodes[0].x, 12);
    }

    #[test]
    fn visible_view_model_is_read_only_and_protected_actions_are_unavailable() {
        let source = r#"
            ui.on("frame", function(event)
                event.visible.player_name = "spoofed"
                ui.activate = function() error("should not be callable") end
            end)
        "#;
        let mut runner = AddonRunner::load("addon", source, AddonPolicy::default()).unwrap();
        assert_eq!(
            runner.deliver_event("frame", &state()),
            DispatchResult::Applied
        );
        assert!(runner.snapshot().secure_intents.is_empty());
        assert_eq!(
            runner.secure_input(999, "basic_attack").unwrap_err(),
            AddonError::Runtime("runtime error: UI node is not owned by this addon".to_owned())
        );
    }

    #[test]
    fn forged_node_handles_cannot_cross_addon_ownership() {
        let source = r#"panel = ui.create_panel("owned")"#;
        let mut first = AddonRunner::load("first", source, AddonPolicy::default()).unwrap();
        let mut second = AddonRunner::load("second", source, AddonPolicy::default()).unwrap();
        let node_id = first.snapshot().nodes[0].id;
        assert_ne!(node_id, second.snapshot().nodes[0].id);
        assert_eq!(
            first.secure_input(node_id, "open").unwrap_err(),
            AddonError::Runtime("secure action is not allowlisted".to_owned())
        );
        assert!(first.secure_input(node_id, "basic_attack").is_ok());
        assert!(second.secure_input(node_id, "open").is_err());
    }

    #[test]
    fn secure_intent_retention_is_bounded() {
        let source = r#"ui.create_panel("secure")"#;
        let mut runner = AddonRunner::load("bounded", source, AddonPolicy::default()).unwrap();
        let node_id = runner.snapshot().nodes[0].id;
        for _ in 0..MAX_SECURE_INTENTS {
            runner
                .secure_input(node_id, "basic_attack")
                .expect("secure intent should fit its host budget");
        }
        assert_eq!(
            runner.secure_input(node_id, "basic_attack").unwrap_err(),
            AddonError::Runtime("secure intent budget exceeded".to_owned())
        );
        assert_eq!(runner.snapshot().secure_intents.len(), MAX_SECURE_INTENTS);
    }

    #[test]
    fn node_and_event_budgets_disable_only_the_failing_addon() {
        let policy = AddonPolicy {
            max_nodes: 1,
            max_events: 1,
            ..AddonPolicy::default()
        };
        let too_many_nodes = r#"ui.create_panel("one"); ui.create_panel("two")"#;
        assert!(matches!(
            AddonRunner::load("bad-nodes", too_many_nodes, policy.clone()),
            Err(AddonError::Runtime(_))
        ));
        let too_many_events = r#"
            ui.on("a", function() end)
            ui.on("b", function() end)
        "#;
        assert!(matches!(
            AddonRunner::load("bad-events", too_many_events, policy),
            Err(AddonError::Runtime(_))
        ));
        let good =
            AddonRunner::load("good", "ui.create_panel(\"ok\")", AddonPolicy::default()).unwrap();
        assert_eq!(good.snapshot().nodes[0].text, "ok");
    }

    #[test]
    fn forbidden_os_apis_are_absent_in_the_sandbox() {
        let source = r#"
            assert(io == nil, "io must be absent")
            assert(os == nil, "os must be absent")
            assert(debug == nil, "debug must be absent")
            assert(package == nil, "package must be absent")
            assert(game.send == nil, "game.send must be absent")
            assert(storage.read_file == nil, "storage.read_file must be absent")
        "#;
        let runner = AddonRunner::load("safe", source, AddonPolicy::default());
        assert!(
            runner.is_ok(),
            "sandbox must remove OS and gameplay APIs: {:?}",
            runner.err()
        );
    }

    #[test]
    fn infinite_loop_is_interrupted_and_default_ui_can_continue() {
        let policy = AddonPolicy {
            max_instructions: 2,
            ..AddonPolicy::default()
        };
        let loop_result = AddonRunner::load("loop", "while true do end", policy);
        assert!(
            matches!(loop_result, Err(AddonError::Runtime(message)) if message.contains("instruction budget"))
        );

        let mut default_ui = AddonRunner::load(
            "default-ui",
            "ui.create_panel(\"still alive\")",
            AddonPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            default_ui.deliver_event("frame", &state()),
            DispatchResult::Applied
        );
    }

    #[test]
    fn memory_and_source_limits_are_enforced_before_unbounded_growth() {
        let source = "x".repeat(DEFAULT_MAX_SCRIPT_BYTES + 1);
        match AddonRunner::load("large-source", &source, AddonPolicy::default()) {
            Err(AddonError::ScriptTooLarge { bytes, maximum }) => {
                assert_eq!(bytes, DEFAULT_MAX_SCRIPT_BYTES + 1);
                assert_eq!(maximum, DEFAULT_MAX_SCRIPT_BYTES);
            }
            Ok(_) => panic!("source-size limit unexpectedly accepted the script"),
            Err(other) => panic!("unexpected source-limit error: {other}"),
        }

        let policy = AddonPolicy {
            max_memory_bytes: 256 * 1024,
            ..AddonPolicy::default()
        };
        let result = AddonRunner::load(
            "large-table",
            "local values = {}; for i = 1, 100000 do values[i] = string.rep('x', 64) end",
            policy,
        );
        assert!(matches!(result, Err(AddonError::Runtime(_))));
    }

    #[test]
    fn callback_error_disables_one_addon_and_records_a_diagnostic() {
        let mut failing = AddonRunner::load(
            "failing",
            "ui.on('frame', function() error('addon failure') end)",
            AddonPolicy::default(),
        )
        .unwrap();
        assert!(matches!(
            failing.deliver_event("frame", &state()),
            DispatchResult::Disabled(message) if message.contains("addon failure")
        ));
        assert!(failing.is_disabled());
        assert_eq!(failing.snapshot().errors.len(), 1);
    }

    #[test]
    fn callback_failure_rolls_back_all_presentation_mutations() {
        let mut failing = AddonRunner::load(
            "transactional",
            r#"
                local panel = ui.create_panel("before failure")
                ui.on("frame", function()
                    ui.set_text(panel, "must not commit")
                    ui.create_panel("also must not commit")
                    error("transaction aborted")
                end)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
        let before = failing.snapshot();
        assert_eq!(before.nodes.len(), 1);
        assert!(matches!(
            failing.deliver_event("frame", &state()),
            DispatchResult::Disabled(message) if message.contains("transaction aborted")
        ));
        let after = failing.snapshot();
        assert_eq!(after.nodes.len(), before.nodes.len());
        assert_eq!(after.nodes[0].text, "before failure");
        assert_eq!(after.secure_intents, before.secure_intents);
        assert_eq!(after.errors.len(), 1);
    }

    #[test]
    fn queued_replaceable_events_coalesce_before_luau_dispatch() {
        let mut runner = AddonRunner::load(
            "queued",
            r#"
                local panel = ui.create_panel("initial")
                ui.on("player.updated", function(event)
                    ui.set_text(panel, event.visible.player_name)
                end)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            runner.enqueue_event(UiEvent::replaceable(
                "player.updated",
                "player",
                VisibleState::new("old", None, 0, 0),
            )),
            QueueOutcome::Enqueued
        );
        assert_eq!(
            runner.enqueue_event(UiEvent::replaceable(
                "player.updated",
                "player",
                VisibleState::new("new", None, 0, 0),
            )),
            QueueOutcome::Coalesced
        );
        assert_eq!(runner.queued_event_count(), 1);
        assert_eq!(runner.dispatch_queued(), DispatchResult::Applied);
        assert_eq!(runner.snapshot().nodes[0].text, "new");
    }

    #[test]
    fn contract_validation_discards_an_invalid_operation_batch() {
        let mut runner = AddonRunner::load(
            "invalid-batch",
            r#"
                local panel = ui.create_panel("stable")
                ui.on("frame", function()
                    ui.set_text(panel, string.char(0))
                end)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
        assert!(matches!(
            runner.deliver_event("frame", &state()),
            DispatchResult::Disabled(message) if message.contains("InvalidText")
        ));
        assert_eq!(runner.snapshot().nodes[0].text, "stable");
        assert_eq!(runner.snapshot().errors.len(), 1);
    }

    #[test]
    fn failed_callback_does_not_commit_new_event_registrations() {
        let mut runner = AddonRunner::load(
            "registration-transaction",
            r#"
                local panel = ui.create_panel("stable")
                ui.on("frame", function()
                    ui.on("later", function() end)
                    ui.set_text(panel, string.char(0))
                end)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
        assert_eq!(runner.snapshot().registered_events, 1);
        assert!(matches!(
            runner.deliver_event("frame", &state()),
            DispatchResult::Disabled(_)
        ));
        assert_eq!(runner.snapshot().registered_events, 1);
    }

    #[test]
    fn manifest_and_source_integrity_are_checked_before_vm_load() {
        let source = "ui.create_panel(\"manifested\")";
        let package = PackageId::new(9_000).unwrap();
        let manifest = Manifest {
            package_id: package,
            name: "manifested-addon".into(),
            version: "1.0.0".into(),
            manifest_schema: 1,
            api_range: "ui.v1".into(),
            runtime_range: "luau-0.12".into(),
            entry: "main.lua".into(),
            load_order: 0,
            dependencies: vec![],
            capabilities: ["ui.panel".to_owned()].into_iter().collect(),
            saved_data: false,
            asset_ids: vec![],
            integrity_sha256: source_sha256(source),
        };
        let known = ["ui.panel".to_owned()].into_iter().collect();
        let packages = [package].into_iter().collect();
        let runner = AddonRunner::load_from_manifest(
            &manifest,
            source,
            AddonPolicy::default(),
            &known,
            &packages,
        )
        .unwrap();
        assert_eq!(runner.snapshot().nodes[0].text, "manifested");

        let mut tampered = manifest;
        tampered.integrity_sha256 = "0".repeat(64);
        assert!(matches!(
            AddonRunner::load_from_manifest(
                &tampered,
                source,
                AddonPolicy::default(),
                &known,
                &packages,
            ),
            Err(AddonError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn package_repository_loads_bounded_manifest_and_source_before_vm_creation() {
        let root = std::env::temp_dir().join(format!(
            "mmorpg-ui-package-repository-{}",
            std::process::id()
        ));
        let package_dir = root.join("9003");
        std::fs::create_dir_all(&package_dir).unwrap();
        let source = "ui.create_panel(\"from repository\")";
        let manifest = format!(
            "package_id = 9003\nname = \"repository-addon\"\nversion = \"1.0.0\"\nmanifest_schema = 1\napi_range = \"ui.v1\"\nruntime_range = \"luau-0.12\"\nentry = \"main.lua\"\nload_order = 0\ncapabilities = [\"ui.panel\"]\nintegrity_sha256 = \"{}\"\n",
            source_sha256(source)
        );
        std::fs::write(package_dir.join("manifest.toml"), &manifest).unwrap();
        std::fs::write(package_dir.join("main.lua"), source).unwrap();

        let known = ["ui.panel".to_owned()].into_iter().collect();
        let packages = [PackageId::new(9003).unwrap()].into_iter().collect();
        assert_eq!(
            PackageRepository::new(&root)
                .discover_package_ids()
                .unwrap(),
            packages
        );
        let loaded = PackageRepository::new(&root)
            .load_all(AddonPolicy::default(), &known)
            .unwrap();
        assert_eq!(loaded[0].snapshot().nodes[0].text, "from repository");
        let runner = PackageRepository::new(&root)
            .load(
                PackageId::new(9003).unwrap(),
                AddonPolicy::default(),
                &known,
                &packages,
            )
            .unwrap();
        assert_eq!(runner.snapshot().nodes[0].text, "from repository");

        std::fs::write(
            package_dir.join("manifest.toml"),
            manifest.replace("entry = \"main.lua\"", "entry = \"../escape.lua\""),
        )
        .unwrap();
        assert!(matches!(
            PackageRepository::new(&root).load(
                PackageId::new(9003).unwrap(),
                AddonPolicy::default(),
                &known,
                &packages,
            ),
            Err(AddonError::InvalidManifest(_)) | Err(AddonError::PackageParse(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_repository_resolves_stable_dependency_order_and_rejects_cycles() {
        let root =
            std::env::temp_dir().join(format!("mmorpg-ui-package-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for package_id in [9005_u64, 9006, 9007] {
            std::fs::create_dir_all(root.join(package_id.to_string())).unwrap();
            std::fs::write(
                root.join(package_id.to_string()).join("main.lua"),
                "ui.create_panel(\"ordered\")",
            )
            .unwrap();
        }
        let manifest = |package_id: u64, load_order: i32, dependencies: &str| {
            format!(
                "package_id = {package_id}\nname = \"package-{package_id}\"\nversion = \"1.0.0\"\nmanifest_schema = 1\napi_range = \"ui.v1\"\nruntime_range = \"luau-0.12\"\nentry = \"main.lua\"\nload_order = {load_order}\ndependencies = {dependencies}\ncapabilities = [\"ui.panel\"]\nintegrity_sha256 = \"{}\"\n",
                source_sha256("ui.create_panel(\"ordered\")")
            )
        };
        std::fs::write(
            root.join("9005").join("manifest.toml"),
            manifest(9005, 20, "[9006]"),
        )
        .unwrap();
        std::fs::write(
            root.join("9006").join("manifest.toml"),
            manifest(9006, 30, "[9007]"),
        )
        .unwrap();
        std::fs::write(
            root.join("9007").join("manifest.toml"),
            manifest(9007, 10, "[]"),
        )
        .unwrap();
        let packages = [
            PackageId::new(9005).unwrap(),
            PackageId::new(9006).unwrap(),
            PackageId::new(9007).unwrap(),
        ]
        .into_iter()
        .collect();
        let known = ["ui.panel".to_owned()].into_iter().collect();
        let repository = PackageRepository::new(&root);
        assert_eq!(
            repository.resolve_order(&packages, &known).unwrap(),
            [
                PackageId::new(9007).unwrap(),
                PackageId::new(9006).unwrap(),
                PackageId::new(9005).unwrap(),
            ]
        );

        std::fs::write(
            root.join("9007").join("manifest.toml"),
            manifest(9007, 10, "[9005]"),
        )
        .unwrap();
        assert!(matches!(
            repository.resolve_order(&packages, &known),
            Err(AddonError::InvalidManifest(message)) if message.contains("cycle")
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn storage_worker_commits_atomically_and_reloads_namespace() {
        let path = std::env::temp_dir().join(format!(
            "mmorpg-ui-storage-worker-{}.state",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let namespace = StorageNamespace {
            account_id: AccountId::new(41).unwrap(),
            package_id: PackageId::new(9004).unwrap(),
            schema_version: 1,
        };
        let worker = StorageWorker::open(path.clone(), namespace.clone()).unwrap();
        worker
            .set(1, "name".into(), StoredValue::String("Aria".into()))
            .unwrap();
        let result = wait_for_storage_result(&worker);
        assert_eq!(result.request_id, 1);
        assert!(result.result.is_ok());
        let committed = std::fs::read_to_string(&path).unwrap();
        assert!(committed.contains("Aria"));

        let error = worker
            .set(
                2,
                "too-large".into(),
                StoredValue::String("x".repeat(mmorpg_ui_contract::MAX_STORAGE_VALUE_BYTES + 1)),
            )
            .expect_err("oversized value should fail before queue admission");
        assert!(error.contains("string too large"));
        assert!(worker.try_result().is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), committed);
        drop(worker);

        let worker = StorageWorker::open(path.clone(), namespace).unwrap();
        worker.delete(3, "name".into()).unwrap();
        let deleted = wait_for_storage_result(&worker);
        assert_eq!(deleted.request_id, 3);
        assert!(deleted.result.is_ok());
        drop(worker);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("Aria"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn storage_worker_rejects_the_eleventh_commit_in_one_minute() {
        let path = std::env::temp_dir().join(format!(
            "mmorpg-ui-storage-rate-{}.state",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let namespace = StorageNamespace {
            account_id: AccountId::new(43).unwrap(),
            package_id: PackageId::new(9008).unwrap(),
            schema_version: 1,
        };
        let worker = StorageWorker::open(path.clone(), namespace).unwrap();
        for request_id in 1..=MAX_STORAGE_COMMITS_PER_MINUTE as u64 + 1 {
            worker
                .set(
                    request_id,
                    format!("key-{request_id}"),
                    StoredValue::Bool(true),
                )
                .unwrap();
        }

        let mut results = Vec::new();
        for _ in 0..=MAX_STORAGE_COMMITS_PER_MINUTE {
            results.push(wait_for_storage_result(&worker));
        }
        results.sort_by_key(|result| result.request_id);
        assert_eq!(results.len(), MAX_STORAGE_COMMITS_PER_MINUTE + 1);
        assert!(
            results[..MAX_STORAGE_COMMITS_PER_MINUTE]
                .iter()
                .all(|result| result.result.is_ok())
        );
        assert_eq!(
            results[MAX_STORAGE_COMMITS_PER_MINUTE].result,
            Err("storage commit rate limit exceeded".to_owned())
        );
        drop(worker);
        std::fs::remove_file(path).unwrap();
    }

    fn wait_for_storage_result(worker: &StorageWorker) -> StorageResult {
        for _ in 0..100 {
            if let Some(result) = worker.try_result() {
                return result;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("storage worker did not return a result");
    }

    #[test]
    fn storage_is_shared_by_account_and_package_but_isolated_from_other_accounts() {
        let shared = Rc::new(RefCell::new(Storage::default()));
        let account = AccountId::new(41).unwrap();
        let other_account = AccountId::new(42).unwrap();
        let package = PackageId::new(9_001).unwrap();
        AddonRunner::load_for_account_with_package(
            "preferences",
            account,
            package,
            Rc::clone(&shared),
            r#"storage.set("settings", {name = "Aria", enabled = true, slots = {1, 2}})"#,
            AddonPolicy::default(),
        )
        .unwrap();

        AddonRunner::load_for_account_with_package(
            "preferences",
            account,
            package,
            Rc::clone(&shared),
            r#"
                local settings = storage.get("settings")
                assert(settings.name == "Aria")
                assert(settings.enabled == true)
                assert(settings.slots[2] == 2)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();

        AddonRunner::load_for_account_with_package(
            "preferences",
            other_account,
            package,
            shared,
            r#"assert(storage.get("settings") == nil)"#,
            AddonPolicy::default(),
        )
        .unwrap();
    }

    #[test]
    fn failed_callback_rolls_back_storage_writes() {
        let shared = Rc::new(RefCell::new(Storage::default()));
        let account = AccountId::new(51).unwrap();
        let package = PackageId::new(9_002).unwrap();
        let mut runner = AddonRunner::load_for_account_with_package(
            "transactional-storage",
            account,
            package,
            Rc::clone(&shared),
            r#"
                storage.set("stable", "yes")
                ui.on("frame", function()
                    storage.set("temporary", "must roll back")
                    error("storage transaction aborted")
                end)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
        assert!(matches!(
            runner.deliver_event("frame", &state()),
            DispatchResult::Disabled(message) if message.contains("storage transaction aborted")
        ));
        AddonRunner::load_for_account_with_package(
            "transactional-storage",
            account,
            package,
            shared,
            r#"
                assert(storage.get("stable") == "yes")
                assert(storage.get("temporary") == nil)
            "#,
            AddonPolicy::default(),
        )
        .unwrap();
    }

    #[test]
    fn ordered_event_storm_disables_only_the_overloaded_addon() {
        let mut overloaded = AddonRunner::load(
            "overloaded",
            r#"ui.on("combat.received", function() end)"#,
            AddonPolicy::default(),
        )
        .unwrap();
        for _ in 0..mmorpg_ui_contract::DEFAULT_EVENT_COUNT {
            assert_eq!(
                overloaded.enqueue_event(UiEvent::ordered("combat.received")),
                QueueOutcome::Enqueued
            );
        }
        assert_eq!(
            overloaded.enqueue_event(UiEvent::ordered("combat.received")),
            QueueOutcome::Disabled
        );
        assert!(overloaded.is_disabled());

        let mut healthy = AddonRunner::load(
            "healthy",
            r#"ui.create_panel("default UI remains available")"#,
            AddonPolicy::default(),
        )
        .unwrap();
        assert!(!healthy.is_disabled());
        assert_eq!(
            healthy.deliver_event("frame", &state()),
            DispatchResult::Applied
        );
        assert_eq!(healthy.snapshot().nodes.len(), 1);
    }

    #[test]
    fn hostile_source_corpus_fails_closed_without_affecting_a_new_runner() {
        let corpus = [
            "while true do end",
            "local function recurse() return recurse() end; recurse()",
            "local x = {}; for i = 1, 100000 do x[i] = string.rep('x', 128) end",
            "assert(io == nil); assert(os == nil); assert(require == nil)",
            "error(string.rep('diagnostic', 4096))",
        ];
        for (index, source) in corpus.into_iter().enumerate() {
            let policy = AddonPolicy {
                max_instructions: 2_000,
                max_memory_bytes: 256 * 1024,
                ..AddonPolicy::default()
            };
            let result = AddonRunner::load(format!("hostile-{index}"), source, policy);
            assert!(
                result.is_err(),
                "hostile corpus entry {index} unexpectedly loaded"
            );
        }
        let healthy = AddonRunner::load(
            "after-hostile-corpus",
            "ui.create_panel('still isolated')",
            AddonPolicy::default(),
        )
        .unwrap();
        assert_eq!(healthy.snapshot().nodes[0].text, "still isolated");
    }
}
