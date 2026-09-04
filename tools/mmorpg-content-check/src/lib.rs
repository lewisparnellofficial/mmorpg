use mmorpg_content::{ContentCatalog, starter_catalog};

/// The compact, human-readable report emitted by the smoke-test CLI.
pub fn catalog_summary(catalog: ContentCatalog) -> String {
    let spawn_count = catalog
        .zones
        .iter()
        .map(|zone| zone.spawns.len())
        .sum::<usize>();

    format!(
        "catalog: items={} npcs={} vendor_listings={} quests={} zones={} spawns={}",
        catalog.items.len(),
        catalog.npcs.len(),
        catalog.vendor_listings.len(),
        catalog.quests.len(),
        catalog.zones.len(),
        spawn_count,
    )
}

/// Validates a catalog and returns the summary only when it is safe to load.
pub fn validate_and_summarize(catalog: ContentCatalog) -> Result<String, String> {
    catalog.validate().map_err(|errors| {
        let details = errors
            .iter()
            .map(|error| format!("{error:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "content validation failed ({} error(s)): {details}",
            errors.len()
        )
    })?;

    Ok(catalog_summary(catalog))
}

/// Runs the starter-catalog smoke test used by the command-line entry point.
pub fn run() -> Result<String, String> {
    validate_and_summarize(starter_catalog())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmorpg_content::{
        ContentError, ItemDefinition, ItemId, NpcArchetype, NpcDefinition, NpcTemplateId,
        QuestDefinition, VendorListingDefinition, ZoneDefinition,
    };

    #[test]
    fn summary_reports_all_starter_catalog_counts() {
        assert_eq!(
            catalog_summary(starter_catalog()),
            "catalog: items=3 npcs=2 vendor_listings=2 quests=1 zones=1 spawns=3"
        );
    }

    #[test]
    fn valid_catalog_returns_summary() {
        let result = validate_and_summarize(starter_catalog());

        assert_eq!(
            result.expect("starter catalog should validate"),
            "catalog: items=3 npcs=2 vendor_listings=2 quests=1 zones=1 spawns=3"
        );
    }

    #[test]
    fn invalid_catalog_returns_validation_error_instead_of_summary() {
        const ITEMS: &[ItemDefinition] = &[ItemDefinition {
            id: ItemId(1),
            name: "",
            max_stack: 0,
        }];
        const NPCS: &[NpcDefinition] = &[NpcDefinition {
            id: NpcTemplateId(1),
            name: "Broken NPC",
            archetype: NpcArchetype::Enemy,
            max_health: 1,
        }];
        const LISTINGS: &[VendorListingDefinition] = &[];
        const QUESTS: &[QuestDefinition] = &[];
        const ZONES: &[ZoneDefinition] = &[];

        let catalog = ContentCatalog {
            items: ITEMS,
            npcs: NPCS,
            vendor_listings: LISTINGS,
            quests: QUESTS,
            zones: ZONES,
        };
        let error = validate_and_summarize(catalog).expect_err("invalid content must fail");

        assert!(error.starts_with("content validation failed (2 error(s))"));
        assert!(error.contains(&format!("{:?}", ContentError::EmptyName("1".to_owned()))));
        assert!(error.contains(&format!("{:?}", ContentError::ZeroStackSize(ItemId(1)))));
    }
}
