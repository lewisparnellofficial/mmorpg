#![forbid(unsafe_code)]

//! Minimal Luau UI-addon boundary for the client technology spike.
//!
//! The VM is intentionally useful only for presentation. It receives a
//! sanitized visible view model, can create and update owned UI nodes, and
//! can register callbacks for permitted UI events. Secure gameplay input is a
//! native host operation and is never exposed as a script function.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use mlua::{Function, Lua, Result as LuaResult, VmState};
pub use mmorpg_ui_contract::ViewRecord;
use mmorpg_ui_contract::{
    EventQueue, Generation, Handle, NodeId, PackageId, Property, QueueOutcome, UiEvent, UiLimits,
    UiOperation, validate_operations,
};

pub const DEFAULT_MAX_NODES: usize = 64;
pub const DEFAULT_MAX_EVENTS: usize = 16;
pub const DEFAULT_MAX_SCRIPT_BYTES: usize = 64 * 1024;
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 100_000;
pub const DEFAULT_MAX_OPERATIONS: usize = 512;
const MAX_TEXT_BYTES: usize = 4 * 1024;
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
    Runtime(String),
    Disabled(String),
}

impl std::fmt::Display for AddonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptTooLarge { bytes, maximum } => {
                write!(formatter, "script has {bytes} bytes; maximum is {maximum}")
            }
            Self::InvalidPolicy => formatter.write_str("addon policy limits must be positive"),
            Self::Runtime(message) => write!(formatter, "addon runtime error: {message}"),
            Self::Disabled(message) => write!(formatter, "addon is disabled: {message}"),
        }
    }
}

impl std::error::Error for AddonError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchResult {
    Applied,
    Disabled(String),
}

#[derive(Clone, Debug)]
struct HostState {
    addon_id: String,
    package_id: PackageId,
    generation: Generation,
    policy: AddonPolicy,
    nodes: BTreeMap<u64, UiNode>,
    registered_events: Vec<String>,
    secure_intents: Vec<SecureIntent>,
    errors: Vec<String>,
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
            addon_id: addon_id.into(),
            package_id: PackageId::new(NEXT_PACKAGE_ID.fetch_add(1, Ordering::Relaxed))
                .expect("package IDs cannot be zero"),
            generation: Generation::new(1).expect("initial generation cannot be zero"),
            policy,
            nodes: BTreeMap::new(),
            registered_events: Vec::new(),
            secure_intents: Vec::new(),
            errors: Vec::new(),
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
        let mut host = self.host.borrow_mut();
        host.check_owner(node_id)
            .map_err(|error| AddonError::Runtime(error.to_string()))?;
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
    globals.set("storage", lua.create_table()?)?;
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
            runner.secure_input(999, "attack").unwrap_err(),
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
        assert!(first.secure_input(node_id, "open").is_ok());
        assert!(second.secure_input(node_id, "open").is_err());
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
}
