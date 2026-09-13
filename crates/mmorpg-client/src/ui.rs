use super::*;

pub(crate) fn update_status_text(
    state: Res<ClientState>,
    mut query: Query<&mut Text, With<StatusText>>,
) {
    let Ok(mut text) = query.single_mut() else {
        return;
    };
    text.0 = format_hud_text(&state);
}

pub(crate) fn format_hud_text(state: &ClientState) -> String {
    let character_selection = if state.player_id.is_some() {
        String::new()
    } else if state.available_characters.is_empty() {
        "characters: waiting for account response".to_owned()
    } else if state.selected_character_id.is_some() {
        "characters: selection sent; entering world".to_owned()
    } else {
        format!(
            "characters: {} — press Enter to select",
            state
                .available_characters
                .iter()
                .map(|character| format!("{} ({:?})", character.name, character.role))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let player = state.player_id.and_then(|id| state.presentation.player(id));
    let target = player.and_then(|player| player.target).map_or_else(
        || "none".to_owned(),
        |target| {
            state.presentation.npc(target).map_or_else(
                || target.0.to_string(),
                |npc| {
                    let threat = state
                        .presentation
                        .enemy_threat_target(target)
                        .map_or_else(|| "none".to_owned(), |player_id| player_id.0.to_string());
                    format!(
                        "{} ({}/{}) threat={threat}",
                        target.0, npc.health, npc.max_health
                    )
                },
            )
        },
    );
    let player_summary = state.player_id.map_or_else(
        || "not connected".to_owned(),
        |id| match state.presentation.player(id) {
            Some(player) => format!(
                "player {} ({:?})  hp {}/{}  target {}",
                id.0, player.role, player.health, player.max_health, target
            ),
            None => format!("player {}  waiting for snapshot", id.0),
        },
    );
    let party = state.presentation.parties().next().map_or_else(
        || "party: none".to_owned(),
        |party| {
            format!(
                "party {}  leader {}  members {}",
                party.id.0,
                party.leader_id.0,
                party
                    .member_ids
                    .iter()
                    .map(|member_id| member_id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
    );
    let tick = state
        .presentation
        .world_tick()
        .map_or_else(|| "-".to_owned(), |tick| tick.to_string());
    let combat_status = state.player_id.map_or_else(
        || "combat: unavailable".to_owned(),
        |player_id| {
            state.presentation.combat_ready_tick(player_id).map_or_else(
                || "combat: ready".to_owned(),
                |ready_tick| format!("combat: cooldown through tick {ready_tick}"),
            )
        },
    );
    let logs = state.logs.iter().cloned().collect::<Vec<_>>().join("\n");
    let inventory = player.map_or_else(
        || "inventory: waiting for snapshot".to_owned(),
        |player| {
            let stacks = player
                .inventory
                .stacks()
                .map(|stack| {
                    let name = item_definition(stack.item_id)
                        .map(|definition| definition.name)
                        .unwrap_or("unknown item");
                    format!("{name} x{}", stack.quantity)
                })
                .collect::<Vec<_>>();
            if stacks.is_empty() {
                format!(
                    "inventory ({}/{}): empty",
                    player.inventory.used_slots(),
                    player.inventory.capacity()
                )
            } else {
                format!(
                    "inventory ({}/{}): {}",
                    player.inventory.used_slots(),
                    player.inventory.capacity(),
                    stacks.join(", ")
                )
            }
        },
    );
    let quests = player.map_or_else(
        || "quests: waiting for snapshot".to_owned(),
        |player| {
            if player.quests.is_empty() {
                return "quests: none".to_owned();
            }
            player
                .quests
                .iter()
                .map(|quest| {
                    let name = starter_catalog()
                        .quests
                        .iter()
                        .find(|definition| definition.id == quest.quest_id)
                        .map(|definition| definition.name)
                        .unwrap_or("unknown quest");
                    let required = quest
                        .required_count
                        .map_or_else(|| "?".to_owned(), |count| count.to_string());
                    format!(
                        "{name} {}/{} ({:?})",
                        quest.progress, required, quest.status
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        },
    );
    let vendor = first_vendor_id(state).map_or_else(
        || "vendor: not discovered".to_owned(),
        |vendor_id| match state.presentation.vendor_listings(vendor_id) {
            Some(listings) if !listings.is_empty() => format!(
                "vendor: {}",
                listings
                    .iter()
                    .map(|listing| {
                        format!(
                            "{} {}g ({} left)",
                            listing.name, listing.unit_price, listing.remaining_quantity
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "vendor: press V to list stock".to_owned(),
        },
    );
    let offers = first_vendor_id(state).map_or_else(
        || "offers: not discovered".to_owned(),
        |vendor_id| match state.presentation.quest_offers(vendor_id) {
            Some(offers) if !offers.is_empty() => format!(
                "offers: {}",
                offers
                    .iter()
                    .map(|offer| offer.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "offers: press O to ask the town NPC".to_owned(),
        },
    );
    let notification = state.presentation.last_notification().map_or_else(
        || "".to_owned(),
        |notification| format!("\nnotice: {notification:?}"),
    );
    format!(
        "server: {} ({})\n{}\n{}\n{}  tick {}\n{}\n{}\n{}\n{}\n{}\n\ncontrols: Enter select character | WASD move | Tab target | Space attack | L loot | V vendor | B buy | O offers | E accept | R turn in{}\n\n{}",
        state.connection_status,
        state.server_address,
        character_selection,
        party,
        player_summary,
        tick,
        combat_status,
        inventory,
        quests,
        vendor,
        offers,
        notification,
        logs,
    )
}
