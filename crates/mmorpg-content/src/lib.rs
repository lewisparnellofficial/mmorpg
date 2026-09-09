//! Shared, immutable content definitions for the client, server, and tools.
//!
//! This crate deliberately contains no file format, renderer, database, or
//! simulation dependency. The initial catalog is compiled into the binary so
//! that the content boundary can be tested before an external authoring and
//! packaging pipeline exists.

use std::collections::BTreeSet;
use std::fmt;
use std::fmt::Write as _;

/// Stable identifier for an item definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemId(pub u32);

impl ItemId {
    pub const FIELD_WOLF_PELT: Self = Self(1);
    pub const TOWN_RATION: Self = Self(2);
    pub const MINOR_HEALING_POTION: Self = Self(3);
}

impl fmt::Display for ItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Immutable item data consumed by inventory, vendors, and UI presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    pub id: ItemId,
    pub name: &'static str,
    pub max_stack: u32,
}

/// Stable identifier for an NPC template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NpcTemplateId(pub u32);

impl NpcTemplateId {
    pub const MIRA_THE_MERCHANT: Self = Self(1);
    pub const FIELD_WOLF: Self = Self(2);
}

impl fmt::Display for NpcTemplateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Content-level NPC category. The simulation maps this to its runtime kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NpcArchetype {
    Vendor,
    Enemy,
    QuestGiver,
}

/// Immutable NPC template data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NpcDefinition {
    pub id: NpcTemplateId,
    pub name: &'static str,
    pub archetype: NpcArchetype,
    pub max_health: u32,
}

/// Stable identifier for a quest definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct QuestId(pub u32);

impl QuestId {
    pub const CLEAR_THE_FIELD: Self = Self(1);
}

impl fmt::Display for QuestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Stable identifier for a zone definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ZoneId(pub u32);

impl ZoneId {
    pub const STARTER_ZONE: Self = Self(1);
}

impl fmt::Display for ZoneId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Objective types supported by the first content schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectiveDefinition {
    KillNpc {
        npc_template_id: NpcTemplateId,
        required_count: u32,
    },
}

/// A static item and currency reward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RewardDefinition {
    pub gold: u32,
    pub item_id: Option<ItemId>,
    pub item_quantity: u32,
}

/// Immutable quest data. Runtime progress belongs to a player in the core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestDefinition {
    pub id: QuestId,
    pub name: &'static str,
    pub description: &'static str,
    pub giver: NpcTemplateId,
    pub objectives: &'static [ObjectiveDefinition],
    pub reward: RewardDefinition,
}

/// A vendor's static listing. Remaining stock is runtime state and belongs in
/// the authoritative simulation rather than in this definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VendorListingDefinition {
    pub item_id: ItemId,
    pub unit_price: u32,
    pub initial_quantity: u32,
}

/// A spawn location for an NPC template in a zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnDefinition {
    pub npc_template_id: NpcTemplateId,
    pub x: f32,
    pub y: f32,
}

/// Static content for one playable zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneDefinition {
    pub id: ZoneId,
    pub name: &'static str,
    pub spawns: &'static [SpawnDefinition],
}

/// A read-only view over all definitions used by the initial slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentCatalog {
    pub items: &'static [ItemDefinition],
    pub npcs: &'static [NpcDefinition],
    pub vendor_listings: &'static [VendorListingDefinition],
    pub quests: &'static [QuestDefinition],
    pub zones: &'static [ZoneDefinition],
}

impl ContentCatalog {
    /// Checks references and invariants before content is loaded by a runtime.
    pub fn validate(self) -> Result<(), Vec<ContentError>> {
        let mut errors = Vec::new();
        validate_unique_ids(self.items.iter().map(|item| item.id), "item", &mut errors);
        validate_unique_ids(self.npcs.iter().map(|npc| npc.id), "npc", &mut errors);
        validate_unique_ids(
            self.quests.iter().map(|quest| quest.id),
            "quest",
            &mut errors,
        );

        for item in self.items {
            if item.name.trim().is_empty() {
                errors.push(ContentError::EmptyName(format!("{}", item.id)));
            }
            if item.max_stack == 0 {
                errors.push(ContentError::ZeroStackSize(item.id));
            }
        }

        for npc in self.npcs {
            if npc.name.trim().is_empty() {
                errors.push(ContentError::EmptyName(format!("{}", npc.id)));
            }
            if npc.max_health == 0 {
                errors.push(ContentError::ZeroNpcHealth(npc.id));
            }
        }

        let item_ids: BTreeSet<_> = self.items.iter().map(|item| item.id).collect();
        let npc_ids: BTreeSet<_> = self.npcs.iter().map(|npc| npc.id).collect();
        for listing in self.vendor_listings {
            if !item_ids.contains(&listing.item_id) {
                errors.push(ContentError::UnknownItem(listing.item_id));
            }
            if listing.initial_quantity == 0 {
                errors.push(ContentError::ZeroVendorStock(listing.item_id));
            }
        }
        for quest in self.quests {
            if !npc_ids.contains(&quest.giver) {
                errors.push(ContentError::UnknownNpc(quest.giver));
            }
            if quest.objectives.is_empty() {
                errors.push(ContentError::QuestHasNoObjectives(quest.id));
            }
            for objective in quest.objectives {
                match objective {
                    ObjectiveDefinition::KillNpc {
                        npc_template_id,
                        required_count,
                    } => {
                        if !npc_ids.contains(npc_template_id) {
                            errors.push(ContentError::UnknownNpc(*npc_template_id));
                        }
                        if *required_count == 0 {
                            errors.push(ContentError::ZeroObjectiveCount(quest.id));
                        }
                    }
                }
            }
            if let Some(item_id) = quest.reward.item_id
                && !item_ids.contains(&item_id)
            {
                errors.push(ContentError::UnknownItem(item_id));
            }
            if quest.reward.item_id.is_none() && quest.reward.item_quantity != 0 {
                errors.push(ContentError::RewardQuantityWithoutItem(quest.id));
            }
            if quest.reward.item_id.is_some() && quest.reward.item_quantity == 0 {
                errors.push(ContentError::ZeroRewardQuantity(quest.id));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Returns the canonical semantic representation used for compatibility
    /// checks. Records are sorted by stable IDs rather than source order, and
    /// floating-point spawn coordinates are represented by their exact IEEE
    /// bits. Runtime stock, player state, and other mutable values are not
    /// included.
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut output = String::new();
        let mut items: Vec<_> = self.items.iter().collect();
        items.sort_by_key(|item| item.id);
        for item in items {
            let _ = writeln!(
                output,
                "item|{}|{}|{}",
                item.id.0, item.name, item.max_stack
            );
        }
        let mut npcs: Vec<_> = self.npcs.iter().collect();
        npcs.sort_by_key(|npc| npc.id);
        for npc in npcs {
            let archetype = match npc.archetype {
                NpcArchetype::Vendor => "vendor",
                NpcArchetype::Enemy => "enemy",
                NpcArchetype::QuestGiver => "quest_giver",
            };
            let _ = writeln!(
                output,
                "npc|{}|{}|{}|{}",
                npc.id.0, npc.name, archetype, npc.max_health
            );
        }
        let mut listings: Vec<_> = self.vendor_listings.iter().collect();
        listings.sort_by_key(|listing| listing.item_id);
        for listing in listings {
            let _ = writeln!(
                output,
                "vendor_listing|{}|{}|{}",
                listing.item_id.0, listing.unit_price, listing.initial_quantity
            );
        }
        let mut quests: Vec<_> = self.quests.iter().collect();
        quests.sort_by_key(|quest| quest.id);
        for quest in quests {
            let _ = writeln!(
                output,
                "quest|{}|{}|{}|{}|{}|{}|{}|{}",
                quest.id.0,
                quest.name,
                quest.description,
                quest.giver.0,
                quest.reward.gold,
                quest.reward.item_id.map_or(0, |item| item.0),
                quest.reward.item_quantity,
                quest.objectives.len()
            );
            for objective in quest.objectives {
                match objective {
                    ObjectiveDefinition::KillNpc {
                        npc_template_id,
                        required_count,
                    } => {
                        let _ = writeln!(
                            output,
                            "objective|kill_npc|{}|{}",
                            npc_template_id.0, required_count
                        );
                    }
                }
            }
        }
        let mut zones: Vec<_> = self.zones.iter().collect();
        zones.sort_by_key(|zone| zone.id);
        for zone in zones {
            let _ = writeln!(
                output,
                "zone|{}|{}|{}",
                zone.id.0,
                zone.name,
                zone.spawns.len()
            );
            let mut spawns: Vec<_> = zone.spawns.iter().collect();
            spawns
                .sort_by_key(|spawn| (spawn.npc_template_id, spawn.x.to_bits(), spawn.y.to_bits()));
            for spawn in spawns {
                let _ = writeln!(
                    output,
                    "spawn|{}|{}|{}",
                    spawn.npc_template_id.0,
                    spawn.x.to_bits(),
                    spawn.y.to_bits()
                );
            }
        }
        output.into_bytes()
    }

    /// Computes a deterministic 256-bit compatibility digest without pulling
    /// a cryptographic dependency into this static content crate. This digest
    /// detects semantic catalog disagreement; it is not an authenticity or
    /// tamper-proof signature and must not replace package integrity checks.
    pub fn content_digest(self) -> [u8; 32] {
        let bytes = self.canonical_bytes();
        let seeds = [
            0xcbf29ce484222325_u64,
            0x84222325cbf29ce4_u64,
            0x9e3779b185ebca87_u64,
            0xd6e8feb86659fd93_u64,
        ];
        let mut lanes = seeds;
        for byte in bytes {
            for (index, lane) in lanes.iter_mut().enumerate() {
                *lane ^= u64::from(byte).wrapping_add((index as u64) << 8);
                *lane = lane.wrapping_mul(0x100000001b3);
                *lane ^= *lane >> 29;
            }
        }
        let mut digest = [0_u8; 32];
        for (index, lane) in lanes.into_iter().enumerate() {
            digest[index * 8..(index + 1) * 8].copy_from_slice(&lane.to_be_bytes());
        }
        digest
    }

    pub fn content_digest_hex(self) -> String {
        self.content_digest()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

fn validate_unique_ids<T>(ids: impl Iterator<Item = T>, kind: &str, errors: &mut Vec<ContentError>)
where
    T: Copy + Ord + fmt::Display,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            errors.push(ContentError::DuplicateId {
                kind: kind.to_owned(),
                id: id.to_string(),
            });
        }
    }
}

/// Validation failures for static content packages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentError {
    DuplicateId { kind: String, id: String },
    EmptyName(String),
    ZeroStackSize(ItemId),
    ZeroNpcHealth(NpcTemplateId),
    UnknownItem(ItemId),
    UnknownNpc(NpcTemplateId),
    ZeroVendorStock(ItemId),
    QuestHasNoObjectives(QuestId),
    ZeroObjectiveCount(QuestId),
    RewardQuantityWithoutItem(QuestId),
    ZeroRewardQuantity(QuestId),
}

const CLEAR_THE_FIELD_OBJECTIVES: &[ObjectiveDefinition] = &[ObjectiveDefinition::KillNpc {
    npc_template_id: NpcTemplateId::FIELD_WOLF,
    required_count: 3,
}];

pub static STARTER_ITEMS: &[ItemDefinition] = &[
    ItemDefinition {
        id: ItemId::FIELD_WOLF_PELT,
        name: "Field Wolf Pelt",
        max_stack: 20,
    },
    ItemDefinition {
        id: ItemId::TOWN_RATION,
        name: "Town Ration",
        max_stack: 20,
    },
    ItemDefinition {
        id: ItemId::MINOR_HEALING_POTION,
        name: "Minor Healing Potion",
        max_stack: 20,
    },
];

/// Looks up an item in the compiled starter catalog.
pub fn item_definition(item_id: ItemId) -> Option<ItemDefinition> {
    STARTER_ITEMS
        .iter()
        .find(|item| item.id == item_id)
        .copied()
}

pub static STARTER_NPCS: &[NpcDefinition] = &[
    NpcDefinition {
        id: NpcTemplateId::MIRA_THE_MERCHANT,
        name: "Mira the Merchant",
        archetype: NpcArchetype::Vendor,
        max_health: 100,
    },
    NpcDefinition {
        id: NpcTemplateId::FIELD_WOLF,
        name: "Field Wolf",
        archetype: NpcArchetype::Enemy,
        max_health: 100,
    },
];

pub static STARTER_VENDOR_LISTINGS: &[VendorListingDefinition] = &[
    VendorListingDefinition {
        item_id: ItemId::TOWN_RATION,
        unit_price: 2,
        initial_quantity: 100,
    },
    VendorListingDefinition {
        item_id: ItemId::MINOR_HEALING_POTION,
        unit_price: 5,
        initial_quantity: 50,
    },
];

pub static STARTER_QUESTS: &[QuestDefinition] = &[QuestDefinition {
    id: QuestId::CLEAR_THE_FIELD,
    name: "Clear the Field",
    description: "Thin the wolves threatening travelers outside town.",
    giver: NpcTemplateId::MIRA_THE_MERCHANT,
    objectives: CLEAR_THE_FIELD_OBJECTIVES,
    reward: RewardDefinition {
        gold: 10,
        item_id: Some(ItemId::TOWN_RATION),
        item_quantity: 5,
    },
}];

pub static STARTER_SPAWNS: &[SpawnDefinition] = &[
    SpawnDefinition {
        npc_template_id: NpcTemplateId::FIELD_WOLF,
        x: 24.0,
        y: 0.0,
    },
    SpawnDefinition {
        npc_template_id: NpcTemplateId::FIELD_WOLF,
        x: 30.0,
        y: 6.0,
    },
    SpawnDefinition {
        npc_template_id: NpcTemplateId::FIELD_WOLF,
        x: 30.0,
        y: -6.0,
    },
];

pub static STARTER_ZONES: &[ZoneDefinition] = &[ZoneDefinition {
    id: ZoneId::STARTER_ZONE,
    name: "Greenfield",
    spawns: STARTER_SPAWNS,
}];

/// Returns the immutable catalog for the first playable slice.
pub const fn starter_catalog() -> ContentCatalog {
    ContentCatalog {
        items: STARTER_ITEMS,
        npcs: STARTER_NPCS,
        vendor_listings: STARTER_VENDOR_LISTINGS,
        quests: STARTER_QUESTS,
        zones: STARTER_ZONES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_catalog_is_valid() {
        assert_eq!(starter_catalog().validate(), Ok(()));
    }

    #[test]
    fn starter_catalog_contains_the_shared_vertical_slice_definitions() {
        let catalog = starter_catalog();
        assert_eq!(catalog.items.len(), 3);
        assert_eq!(catalog.npcs.len(), 2);
        assert_eq!(catalog.vendor_listings.len(), 2);
        assert_eq!(catalog.quests.len(), 1);
        assert_eq!(catalog.zones[0].spawns.len(), 3);
        assert_eq!(catalog.quests[0].giver, NpcTemplateId::MIRA_THE_MERCHANT);
    }

    #[test]
    fn validation_rejects_broken_references_and_zero_values() {
        const OBJECTIVES: &[ObjectiveDefinition] = &[ObjectiveDefinition::KillNpc {
            npc_template_id: NpcTemplateId(998),
            required_count: 0,
        }];
        const ITEMS: &[ItemDefinition] = &[ItemDefinition {
            id: ItemId(1),
            name: "",
            max_stack: 0,
        }];
        const NPCS: &[NpcDefinition] = &[NpcDefinition {
            id: NpcTemplateId(999),
            name: "",
            archetype: NpcArchetype::Enemy,
            max_health: 0,
        }];
        const LISTINGS: &[VendorListingDefinition] = &[VendorListingDefinition {
            item_id: ItemId(999),
            unit_price: 1,
            initial_quantity: 0,
        }];
        const QUESTS: &[QuestDefinition] = &[QuestDefinition {
            id: QuestId(1),
            name: "Broken",
            description: "Broken",
            giver: NpcTemplateId(998),
            objectives: OBJECTIVES,
            reward: RewardDefinition {
                gold: 0,
                item_id: Some(ItemId(999)),
                item_quantity: 0,
            },
        }];
        const ZONES: &[ZoneDefinition] = &[];
        let result = (ContentCatalog {
            items: ITEMS,
            npcs: NPCS,
            vendor_listings: LISTINGS,
            quests: QUESTS,
            zones: ZONES,
        })
        .validate();
        let errors = result.expect_err("broken content should be rejected");
        assert!(errors.contains(&ContentError::EmptyName("1".to_owned())));
        assert!(errors.contains(&ContentError::ZeroStackSize(ItemId(1))));
        assert!(errors.contains(&ContentError::ZeroNpcHealth(NpcTemplateId(999))));
        assert!(errors.contains(&ContentError::UnknownItem(ItemId(999))));
        assert!(errors.contains(&ContentError::UnknownNpc(NpcTemplateId(998))));
        assert!(errors.contains(&ContentError::EmptyName("999".to_owned())));
        assert!(errors.contains(&ContentError::ZeroNpcHealth(NpcTemplateId(999))));
        assert!(errors.contains(&ContentError::ZeroObjectiveCount(QuestId(1))));
        assert!(errors.contains(&ContentError::ZeroRewardQuantity(QuestId(1))));
    }

    #[test]
    fn content_digest_is_stable_under_source_order_changes() {
        const REVERSED_ITEMS: &[ItemDefinition] =
            &[STARTER_ITEMS[2], STARTER_ITEMS[1], STARTER_ITEMS[0]];
        let mut reordered = starter_catalog();
        reordered.items = REVERSED_ITEMS;
        assert_eq!(
            starter_catalog().canonical_bytes(),
            reordered.canonical_bytes()
        );
        assert_eq!(
            starter_catalog().content_digest(),
            reordered.content_digest()
        );
        assert_eq!(starter_catalog().content_digest_hex().len(), 64);
    }

    #[test]
    fn content_digest_changes_for_semantic_content_changes() {
        const CHANGED_ITEMS: &[ItemDefinition] = &[
            ItemDefinition {
                id: ItemId::FIELD_WOLF_PELT,
                name: "Field Wolf Pelt",
                max_stack: 19,
            },
            STARTER_ITEMS[1],
            STARTER_ITEMS[2],
        ];
        let mut changed = starter_catalog();
        changed.items = CHANGED_ITEMS;
        assert_ne!(starter_catalog().content_digest(), changed.content_digest());
    }
}
