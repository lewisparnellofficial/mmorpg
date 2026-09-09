#![forbid(unsafe_code)]

//! Native provenance for allowlisted protected UI actions.
//!
//! This crate intentionally has no renderer, script, socket, or gameplay
//! dependency. A host registers presentation bindings, then asks the registry
//! to resolve one fresh native press. The returned intent is linear: consuming
//! it yields the ordinary action request, while scripts and replay data have no
//! API with which to construct one.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AddonId(u64);

impl AddonId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u64);

impl NodeId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Generation(u64);

impl Generation {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ActionId(u32);

impl ActionId {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BindingKey {
    addon: AddonId,
    node: NodeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Binding {
    action: ActionId,
    node_generation: Generation,
    focus_generation: Generation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePress {
    Key {
        physical_event_id: u64,
        repeat: bool,
    },
    PrimaryPointer {
        physical_event_id: u64,
    },
}

impl NativePress {
    fn id(self) -> u64 {
        match self {
            Self::Key {
                physical_event_id, ..
            }
            | Self::PrimaryPointer { physical_event_id } => physical_event_id,
        }
    }

    fn is_repeat(self) -> bool {
        matches!(self, Self::Key { repeat: true, .. })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TrustedIntent {
    action: ActionId,
    addon: AddonId,
    node: NodeId,
    physical_event_id: u64,
}

impl TrustedIntent {
    /// Consumes the native-only provenance token into an ordinary typed action.
    pub fn consume(self) -> (ActionId, AddonId, NodeId, u64) {
        (self.action, self.addon, self.node, self.physical_event_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Denial {
    InvalidBinding,
    UnknownBinding,
    StaleNode,
    StaleFocus,
    Repeat,
    ReplayedEvent,
    InvalidPhysicalEvent,
}

impl fmt::Display for Denial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "secure input denied: {self:?}")
    }
}

impl std::error::Error for Denial {}

#[derive(Default)]
pub struct SecureInputRegistry {
    bindings: BTreeMap<BindingKey, Binding>,
    focus_generation: u64,
    last_dispatched_event_id: u64,
}

impl SecureInputRegistry {
    pub fn new() -> Self {
        Self {
            focus_generation: 1,
            ..Self::default()
        }
    }

    pub fn register(
        &mut self,
        addon: AddonId,
        node: NodeId,
        node_generation: Generation,
        action: ActionId,
    ) -> Result<(), Denial> {
        let key = BindingKey { addon, node };
        if self.bindings.contains_key(&key) {
            return Err(Denial::InvalidBinding);
        }
        self.bindings.insert(
            key,
            Binding {
                action,
                node_generation,
                focus_generation: self.current_focus_generation()?,
            },
        );
        Ok(())
    }

    pub fn unload_addon(&mut self, addon: AddonId) {
        self.bindings.retain(|key, _| key.addon != addon);
    }

    pub fn focus_changed(&mut self) {
        self.focus_generation = self.focus_generation.saturating_add(1).max(1);
    }

    pub fn dispatch(
        &mut self,
        addon: AddonId,
        node: NodeId,
        node_generation: Generation,
        press: NativePress,
    ) -> Result<TrustedIntent, Denial> {
        if press.id() == 0 {
            return Err(Denial::InvalidPhysicalEvent);
        }
        if press.is_repeat() {
            return Err(Denial::Repeat);
        }
        if press.id() <= self.last_dispatched_event_id {
            return Err(Denial::ReplayedEvent);
        }
        let binding = *self
            .bindings
            .get(&BindingKey { addon, node })
            .ok_or(Denial::UnknownBinding)?;
        if binding.node_generation != node_generation {
            return Err(Denial::StaleNode);
        }
        if binding.focus_generation.get() != self.focus_generation {
            return Err(Denial::StaleFocus);
        }
        self.last_dispatched_event_id = press.id();
        Ok(TrustedIntent {
            action: binding.action,
            addon,
            node,
            physical_event_id: press.id(),
        })
    }

    fn current_focus_generation(&self) -> Result<Generation, Denial> {
        Generation::new(self.focus_generation).ok_or(Denial::InvalidBinding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (AddonId, NodeId, Generation, ActionId) {
        (
            AddonId::new(1).unwrap(),
            NodeId::new(2).unwrap(),
            Generation::new(3).unwrap(),
            ActionId::new(4).unwrap(),
        )
    }

    #[test]
    fn accepts_one_fresh_native_press_and_consumes_provenance() {
        let (addon, node, generation, action) = ids();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        let trusted = registry
            .dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 9,
                },
            )
            .unwrap();
        assert_eq!(trusted.consume(), (action, addon, node, 9));
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 9,
                },
            ),
            Err(Denial::ReplayedEvent)
        );
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 8,
                },
            ),
            Err(Denial::ReplayedEvent)
        );
    }

    #[test]
    fn rejects_repeat_stale_focus_stale_node_and_unknown_addon() {
        let (addon, node, generation, action) = ids();
        let other_addon = AddonId::new(8).unwrap();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::Key {
                    physical_event_id: 1,
                    repeat: true,
                },
            ),
            Err(Denial::Repeat)
        );
        assert_eq!(
            registry.dispatch(
                other_addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 2
                }
            ),
            Err(Denial::UnknownBinding)
        );
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                Generation::new(5).unwrap(),
                NativePress::PrimaryPointer {
                    physical_event_id: 3
                }
            ),
            Err(Denial::StaleNode)
        );
        registry.focus_changed();
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 4
                }
            ),
            Err(Denial::StaleFocus)
        );
    }

    #[test]
    fn unloading_addon_invalidates_its_bindings() {
        let (addon, node, generation, action) = ids();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        registry.unload_addon(addon);
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 1
                }
            ),
            Err(Denial::UnknownBinding)
        );
    }

    #[test]
    fn rejects_invalid_physical_ids_and_duplicate_bindings() {
        let (addon, node, generation, action) = ids();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        assert_eq!(
            registry.register(addon, node, generation, action),
            Err(Denial::InvalidBinding)
        );
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 0
                }
            ),
            Err(Denial::InvalidPhysicalEvent)
        );
    }

    #[test]
    fn reloading_a_node_requires_a_new_generation_and_drops_old_input() {
        let (addon, node, generation, action) = ids();
        let replacement_generation = Generation::new(7).unwrap();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        registry.unload_addon(addon);
        registry
            .register(addon, node, replacement_generation, action)
            .unwrap();
        assert_eq!(
            registry.dispatch(
                addon,
                node,
                generation,
                NativePress::Key {
                    physical_event_id: 10,
                    repeat: false,
                }
            ),
            Err(Denial::StaleNode)
        );
        let intent = registry
            .dispatch(
                addon,
                node,
                replacement_generation,
                NativePress::Key {
                    physical_event_id: 11,
                    repeat: false,
                },
            )
            .unwrap();
        assert_eq!(intent.consume(), (action, addon, node, 11));
    }

    #[test]
    fn a_consumed_intent_is_linear_at_the_rust_type_boundary() {
        let (addon, node, generation, action) = ids();
        let mut registry = SecureInputRegistry::new();
        registry.register(addon, node, generation, action).unwrap();
        let intent = registry
            .dispatch(
                addon,
                node,
                generation,
                NativePress::PrimaryPointer {
                    physical_event_id: 12,
                },
            )
            .unwrap();
        assert_eq!(intent.consume(), (action, addon, node, 12));
    }
}
