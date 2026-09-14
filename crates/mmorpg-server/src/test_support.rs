//! Inert legacy line protocol parser and snapshot formatter fixtures for equivalence tests.

use mmorpg_core::{Command, EntityId, Event, ItemId, QuestId, Role, World};

pub fn format_event(event: &Event) -> String {
    match event {
        Event::PlayerJoined { player } => format!(
            "EVENT player_joined id={} name={} role={} pos={:.2},{:.2}",
            player.id,
            player.name,
            player.role.as_str(),
            player.position.x,
            player.position.y
        ),
        Event::PlayerLeft { player_id } => format!("EVENT player_left id={player_id}"),
        Event::PlayerMoved {
            player_id,
            position,
            area,
        } => format!(
            "EVENT player_moved id={} pos={:.2},{:.2} area={area:?}",
            player_id, position.x, position.y
        ),
        Event::TargetSelected {
            player_id,
            target_id,
        } => {
            format!("EVENT target_selected player={player_id} target={target_id}")
        }
        Event::AttackResolved {
            player_id,
            target_id,
            damage,
            target_health,
        } => format!(
            "EVENT attack player={} target={} damage={} target_hp={}",
            player_id, target_id, damage, target_health
        ),
        Event::CombatCooldownStarted {
            player_id,
            ready_tick,
        } => format!(
            "EVENT cooldown player={} ready_tick={ready_tick}",
            player_id
        ),
        Event::HealResolved {
            player_id,
            target_id,
            amount,
            target_health,
        } => format!(
            "EVENT heal player={} target={} amount={} target_hp={}",
            player_id, target_id, amount, target_health
        ),
        Event::TauntResolved {
            player_id,
            target_id,
        } => format!("EVENT taunt player={} target={}", player_id, target_id),
        Event::PlayerReleasedToTown {
            player_id,
            position,
            health,
        } => format!(
            "EVENT released_to_town player={} pos={:.2},{:.2} health={}",
            player_id, position.x, position.y, health
        ),
        Event::EnemyCorpseExpired {
            enemy_id,
            spawn_generation,
        } => format!(
            "EVENT enemy_corpse_expired id={} generation={}",
            enemy_id, spawn_generation
        ),
        Event::EnemyDefeated { enemy_id } => format!("EVENT enemy_defeated id={enemy_id}"),
        Event::EnemyAttackResolved {
            enemy_id,
            target_id,
            damage,
            target_health,
        } => format!(
            "EVENT enemy_attack enemy={} target={} damage={} target_hp={}",
            enemy_id, target_id, damage, target_health
        ),
        Event::PlayerDefeated { player_id } => {
            format!("EVENT player_defeated id={player_id}")
        }
        Event::EnemyRespawned {
            enemy_id,
            spawn_generation,
        } => format!(
            "EVENT enemy_respawned id={} generation={}",
            enemy_id, spawn_generation
        ),
        Event::VendorListed {
            player_id,
            vendor_id,
            listings,
        } => {
            let listing_text = listings
                .iter()
                .map(|listing| {
                    format!(
                        "item={} name={} price={} stock={} max_stack={}",
                        listing.item_id,
                        listing.name.replace(' ', "_"),
                        listing.unit_price,
                        listing.remaining_quantity,
                        listing.max_stack
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT vendor_listed player={} vendor={} listings={listing_text}",
                player_id, vendor_id
            )
        }
        Event::ItemPurchased {
            player_id,
            vendor_id,
            item_id,
            quantity,
            total_price,
            gold_remaining,
        } => format!(
            "EVENT item_purchased player={} vendor={} item={} quantity={} total_price={} gold={}",
            player_id, vendor_id, item_id, quantity, total_price, gold_remaining
        ),
        Event::LootRewarded {
            player_id,
            enemy_id,
            item_id,
            quantity,
        } => format!(
            "EVENT loot_rewarded player={} enemy={} item={} quantity={}",
            player_id, enemy_id, item_id, quantity
        ),
        Event::PartyInviteCreated {
            party_id,
            inviter_id,
            invitee_id,
            expires_at_tick,
        } => format!(
            "EVENT party_invite party={} inviter={} invitee={} expires={}",
            party_id, inviter_id, invitee_id, expires_at_tick
        ),
        Event::PartyInviteAccepted { party, player_id } => format!(
            "EVENT party_joined party={} player={} leader={} members={:?}",
            party.id, player_id, party.leader_id, party.member_ids
        ),
        Event::PartyInviteDeclined {
            party_id,
            player_id,
        } => format!(
            "EVENT party_invite_declined party={} player={}",
            party_id, player_id
        ),
        Event::PartyInviteExpired {
            party_id,
            player_id,
        } => format!(
            "EVENT party_invite_expired party={} player={}",
            party_id, player_id
        ),
        Event::PartyMemberLeft {
            party_id,
            player_id,
        } => format!("EVENT party_left party={} player={}", party_id, player_id),
        Event::PartyMemberRemoved {
            party_id,
            player_id,
            removed_by,
        } => format!(
            "EVENT party_removed party={} player={} removed_by={}",
            party_id, player_id, removed_by
        ),
        Event::PartyLeaderTransferred {
            party_id,
            previous_leader_id,
            leader_id,
        } => format!(
            "EVENT party_leader party={} previous={} leader={}",
            party_id, previous_leader_id, leader_id
        ),
        Event::PartyDisbanded {
            party_id,
            member_ids,
        } => format!(
            "EVENT party_disbanded party={} members={:?}",
            party_id, member_ids
        ),
        Event::TransactionRejected { player_id, reason } => format!(
            "EVENT transaction_rejected player={} reason={reason}",
            player_id
        ),
        Event::QuestOffersListed {
            player_id,
            npc_id,
            quests,
        } => {
            let quest_text = quests
                .iter()
                .map(|quest| {
                    format!(
                        "id={} name={}",
                        quest.quest_id,
                        quest.name.replace(' ', "_")
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!(
                "EVENT quest_offers player={} npc={} quests={quest_text}",
                player_id, npc_id
            )
        }
        Event::QuestAccepted {
            player_id,
            npc_id,
            quest_id,
        } => format!(
            "EVENT quest_accepted player={} npc={} quest={}",
            player_id, npc_id, quest_id
        ),
        Event::QuestProgressed {
            player_id,
            quest_id,
            progress,
            required_count,
        } => format!(
            "EVENT quest_progressed player={} quest={} progress={}/{}",
            player_id, quest_id, progress, required_count
        ),
        Event::QuestCompleted {
            player_id,
            quest_id,
        } => format!(
            "EVENT quest_completed player={} quest={}",
            player_id, quest_id
        ),
        Event::QuestRewarded {
            player_id,
            quest_id,
            gold,
            item_id,
            item_quantity,
            gold_remaining,
        } => format!(
            "EVENT quest_rewarded player={} quest={} gold={} item={} quantity={} gold_remaining={}",
            player_id,
            quest_id,
            gold,
            item_id.map_or_else(|| "none".to_owned(), |item| item.to_string()),
            item_quantity,
            gold_remaining
        ),
        Event::QuestRejected { player_id, reason } => {
            format!("EVENT quest_rejected player={} reason={reason}", player_id)
        }
        Event::CommandRejected { reason } => format!("EVENT rejected reason={reason}"),
    }
}

/// Formats the temporary graphical-client bootstrap response.
///
/// The `TEMP_SNAPSHOT` prefix is deliberately not part of the future wire
/// protocol. Records are whitespace-delimited key/value pairs, and text
/// values use percent encoding so names containing spaces cannot change the
/// record shape. Positions use Rust's shortest round-trippable float format.
pub fn format_machine_snapshot(world: &World) -> Vec<String> {
    let summary = world.summary();
    let mut lines = vec!["TEMP_SNAPSHOT_BEGIN version=2".to_owned()];
    lines.push(format!(
        "TEMP_SNAPSHOT WORLD tick={} players={} npcs={} enemies={} vendors={}",
        summary.tick,
        summary.player_count,
        summary.npc_count,
        summary.enemy_count,
        summary.vendor_count
    ));
    for player in world.players() {
        lines.push(format!(
            "TEMP_SNAPSHOT PLAYER id={} name={} role={} position={:?},{:?} health={} max_health={} gold={} capacity={} target={}",
            player.id,
            encode_snapshot_text(&player.name),
            player.role.as_str(),
            player.position.x,
            player.position.y,
            player.health,
            player.max_health,
            player.gold,
            player.inventory.capacity(),
            player
                .target
                .map_or_else(|| "none".to_owned(), |target| target.to_string())
        ));
        for stack in player.inventory.stacks() {
            lines.push(format!(
                "TEMP_SNAPSHOT ITEM player={} item={} quantity={}",
                player.id, stack.item_id, stack.quantity
            ));
        }
        for quest in &player.quests {
            lines.push(format!(
                "TEMP_SNAPSHOT QUEST player={} quest={} progress={}/{} status={:?}",
                player.id, quest.quest_id, quest.progress, quest.required_count, quest.status
            ));
        }
    }
    for npc in world.npcs() {
        lines.push(format!(
            "TEMP_SNAPSHOT NPC id={} template_id={} name={} kind={} position={:?},{:?} health={} max_health={}",
            npc.id,
            npc.template_id,
            encode_snapshot_text(&npc.name),
            machine_npc_kind(npc.kind),
            npc.position.x,
            npc.position.y,
            npc.health,
            npc.max_health,
        ));
    }
    lines.push("TEMP_SNAPSHOT_END".to_owned());
    lines
}

pub fn machine_npc_kind(kind: mmorpg_core::NpcKind) -> &'static str {
    match kind {
        mmorpg_core::NpcKind::Vendor => "vendor",
        mmorpg_core::NpcKind::Enemy => "enemy",
    }
}

pub fn encode_snapshot_text(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

pub enum ParsedLine {
    Command(Command),
    State,
    Snapshot,
    Inventory,
    Help,
    Quit,
}

pub fn parse_line(line: &str, bound_player: Option<EntityId>) -> Result<ParsedLine, String> {
    let tokens: Vec<_> = line.split_whitespace().collect();
    let Some(command) = tokens.first().copied() else {
        return Err("empty command".to_owned());
    };
    match command.to_ascii_lowercase().as_str() {
        "connect" => {
            if tokens.len() != 3 {
                return Err("usage: connect <name> <tank|healer|damage>".to_owned());
            }
            let role = tokens[2]
                .parse::<Role>()
                .map_err(|error| error.to_string())?;
            Ok(ParsedLine::Command(Command::JoinPlayer {
                name: tokens[1].to_owned(),
                role,
            }))
        }
        "move" => {
            let (dx, dy) = match tokens.len() {
                3 => (tokens[1], tokens[2]),
                4 => {
                    ensure_bound_id(tokens[1], bound_player)?;
                    (tokens[2], tokens[3])
                }
                _ => return Err("usage: move <dx> <dy>".to_owned()),
            };
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::Move {
                player_id,
                dx: dx.parse().map_err(|_| "dx must be a number".to_owned())?,
                dy: dy.parse().map_err(|_| "dy must be a number".to_owned())?,
            }))
        }
        "target" => {
            if tokens.len() != 2 {
                return Err("usage: target <entity-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::SelectTarget {
                player_id,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "attack" => {
            if tokens.len() != 1 {
                return Err("usage: attack".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BasicAttack { player_id }))
        }
        "vendor" | "list-vendor" => {
            if tokens.len() != 2 {
                return Err("usage: vendor <vendor-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListVendor {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
            }))
        }
        "buy" => {
            if tokens.len() != 4 {
                return Err("usage: buy <vendor-id> <item-id> <quantity>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::BuyItem {
                player_id,
                vendor_id: parse_entity_id(tokens[1])?,
                item_id: parse_item_id(tokens[2])?,
                quantity: parse_positive_quantity(tokens[3])?,
            }))
        }
        "loot" => {
            if tokens.len() != 2 {
                return Err("usage: loot <enemy-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::LootEnemy {
                player_id,
                enemy_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-invite" => {
            if tokens.len() != 2 {
                return Err("usage: party-invite <player-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::InvitePartyMember {
                player_id,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-accept" => {
            if tokens.len() != 2 {
                return Err("usage: party-accept <party-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::AcceptPartyInvite {
                player_id,
                party_id: tokens[1]
                    .parse()
                    .map(mmorpg_core::PartyId)
                    .map_err(|_| "party-id must be an integer".to_owned())?,
            }))
        }
        "party-decline" => {
            if tokens.len() != 2 {
                return Err("usage: party-decline <party-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::DeclinePartyInvite {
                player_id,
                party_id: tokens[1]
                    .parse()
                    .map(mmorpg_core::PartyId)
                    .map_err(|_| "party-id must be an integer".to_owned())?,
            }))
        }
        "party-leave" => {
            if tokens.len() != 1 {
                return Err("usage: party-leave".to_owned());
            }
            Ok(ParsedLine::Command(Command::LeaveParty {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
            }))
        }
        "party-remove" => {
            if tokens.len() != 2 {
                return Err("usage: party-remove <player-id>".to_owned());
            }
            Ok(ParsedLine::Command(Command::RemovePartyMember {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-leader" => {
            if tokens.len() != 2 {
                return Err("usage: party-leader <player-id>".to_owned());
            }
            Ok(ParsedLine::Command(Command::TransferPartyLeader {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
                target_id: parse_entity_id(tokens[1])?,
            }))
        }
        "party-disband" => {
            if tokens.len() != 1 {
                return Err("usage: party-disband".to_owned());
            }
            Ok(ParsedLine::Command(Command::DisbandParty {
                player_id: bound_player.ok_or_else(|| "connect first".to_owned())?,
            }))
        }
        "inventory" => {
            if tokens.len() != 1 {
                return Err("usage: inventory".to_owned());
            }
            Ok(ParsedLine::Inventory)
        }
        "quest-offers" | "quests" => {
            if tokens.len() != 2 {
                return Err("usage: quest-offers <npc-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::ListQuestOffers {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
            }))
        }
        "accept-quest" => {
            if tokens.len() != 3 {
                return Err("usage: accept-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::AcceptQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "turn-in-quest" => {
            if tokens.len() != 3 {
                return Err("usage: turn-in-quest <npc-id> <quest-id>".to_owned());
            }
            let player_id = bound_player.ok_or_else(|| "connect first".to_owned())?;
            Ok(ParsedLine::Command(Command::TurnInQuest {
                player_id,
                npc_id: parse_entity_id(tokens[1])?,
                quest_id: parse_quest_id(tokens[2])?,
            }))
        }
        "state" => Ok(ParsedLine::State),
        "snapshot" => Ok(ParsedLine::Snapshot),
        "help" => Ok(ParsedLine::Help),
        "quit" | "exit" => Ok(ParsedLine::Quit),
        _ => Err(format!("unknown command '{command}'; try help")),
    }
}

pub fn ensure_bound_id(value: &str, bound_player: Option<EntityId>) -> Result<(), String> {
    let requested = parse_entity_id(value)?;
    if Some(requested) != bound_player {
        return Err("a client may only control its own player".to_owned());
    }
    Ok(())
}

pub fn parse_entity_id(value: &str) -> Result<EntityId, String> {
    value
        .parse::<u64>()
        .map(EntityId)
        .map_err(|_| "entity-id must be an integer".to_owned())
}

pub fn parse_item_id(value: &str) -> Result<ItemId, String> {
    value
        .parse::<u32>()
        .map(ItemId)
        .map_err(|_| "item-id must be an integer".to_owned())
}

pub fn parse_quest_id(value: &str) -> Result<QuestId, String> {
    value
        .parse::<u32>()
        .map(QuestId)
        .map_err(|_| "quest-id must be an integer".to_owned())
}

pub fn parse_positive_quantity(value: &str) -> Result<u32, String> {
    let quantity = value
        .parse::<u32>()
        .map_err(|_| "quantity must be a positive integer".to_owned())?;
    if quantity == 0 {
        return Err("quantity must be a positive integer".to_owned());
    }
    Ok(quantity)
}

pub trait ParsedLineExt {
    fn unwrap_command(self) -> Command;
}

impl ParsedLineExt for Result<ParsedLine, String> {
    fn unwrap_command(self) -> Command {
        match self.expect("expected a parsed command") {
            ParsedLine::Command(command) => command,
            ParsedLine::State | ParsedLine::Inventory | ParsedLine::Help | ParsedLine::Quit => {
                panic!("expected a command")
            }
            ParsedLine::Snapshot => panic!("expected a command"),
        }
    }
}
