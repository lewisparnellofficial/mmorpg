use super::*;

pub(crate) fn keyboard_input(
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut repeat: ResMut<MovementRepeat>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut secure_input: ResMut<SecureInputState>,
) {
    if input.just_pressed(KeyCode::Enter)
        && state.player_id.is_none()
        && state.selected_character_id.is_none()
        && let Some(character_id) = state
            .available_characters
            .first()
            .map(|character| character.character_id)
    {
        state.selected_character_id = Some(character_id);
        state.connection_status = "selecting character".to_owned();
        send_command(
            &bridge,
            &mut state,
            ClientCommand::SelectCharacter { character_id },
        );
    }

    let mut dx = 0.0;
    let mut dy = 0.0;
    if input.pressed(KeyCode::KeyA) {
        dx -= MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyD) {
        dx += MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyS) {
        dy -= MOVEMENT_STEP;
    }
    if input.pressed(KeyCode::KeyW) {
        dy += MOVEMENT_STEP;
    }
    let movement_length = dx.hypot(dy);
    if movement_length > MOVEMENT_STEP {
        let scale = MOVEMENT_STEP / movement_length;
        dx *= scale;
        dy *= scale;
    }

    let repeating = repeat.0.tick(time.delta()).just_finished();
    if dx != 0.0 || dy != 0.0 {
        if (input.just_pressed(KeyCode::KeyA)
            || input.just_pressed(KeyCode::KeyD)
            || input.just_pressed(KeyCode::KeyS)
            || input.just_pressed(KeyCode::KeyW))
            || repeating
        {
            send_command(&bridge, &mut state, ClientCommand::Move { dx, dy });
        }
    } else {
        repeat.0.reset();
    }

    if input.just_pressed(KeyCode::Tab)
        && let Some(target_id) = next_target(&mut state)
    {
        send_command(&bridge, &mut state, ClientCommand::Target(target_id));
    }
    if input.just_pressed(KeyCode::Space) {
        let physical_event_id = secure_input.next_physical_event_id();
        let default_addon = secure_input.default_addon;
        let default_node = secure_input.default_node;
        let node_generation = secure_input.node_generation;
        let attack_action = secure_input.attack_action;
        match secure_input.registry.dispatch(
            default_addon,
            default_node,
            node_generation,
            NativePress::Key {
                physical_event_id,
                repeat: false,
            },
        ) {
            Ok(trusted) => {
                let (action, _, _, _) = trusted.consume();
                if action == attack_action {
                    send_command(&bridge, &mut state, ClientCommand::Attack);
                }
            }
            Err(error) => state.log(error.to_string()),
        }
    }
    if input.just_pressed(KeyCode::KeyL)
        && let Some(target_id) = state
            .player_id
            .and_then(|id| state.presentation.player(id))
            .and_then(|player| player.target)
    {
        send_command(&bridge, &mut state, ClientCommand::Loot(target_id));
    }
    if input.just_pressed(KeyCode::KeyV)
        && let Some(vendor_id) = first_vendor_id(&state)
    {
        send_command(&bridge, &mut state, ClientCommand::ListVendor(vendor_id));
    }
    if input.just_pressed(KeyCode::KeyO)
        && let Some(vendor_id) = first_vendor_id(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::ListQuestOffers(vendor_id),
        );
    }
    if input.just_pressed(KeyCode::KeyB)
        && let Some((vendor_id, item_id)) = first_vendor_listing(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::BuyItem {
                vendor_id,
                item_id,
                quantity: 1,
            },
        );
    }
    if input.just_pressed(KeyCode::KeyE)
        && let Some((npc_id, quest_id)) = first_quest_offer(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::AcceptQuest { npc_id, quest_id },
        );
    }
    if input.just_pressed(KeyCode::KeyR)
        && let Some((npc_id, quest_id)) = first_player_quest(&state)
    {
        send_command(
            &bridge,
            &mut state,
            ClientCommand::TurnInQuest { npc_id, quest_id },
        );
    }
}

pub(crate) fn send_command(
    bridge: &NetworkBridge,
    state: &mut ClientState,
    command: ClientCommand,
) {
    match bridge.command_tx.try_send(command) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => {
            state.log("input queue full; command dropped".to_owned());
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            state.connection_status = "network worker stopped".to_owned();
            state.log("network worker stopped".to_owned());
        }
    }
}

pub(crate) fn next_target(state: &mut ClientState) -> Option<EntityId> {
    let ids: Vec<_> = state
        .presentation
        .entities()
        .filter_map(|entity| match entity {
            ClientEntity::Npc(npc) => Some(npc.id),
            ClientEntity::Player(_) => None,
        })
        .collect();
    let target = ids.get(state.target_cursor % ids.len().max(1)).copied()?;
    state.target_cursor = (state.target_cursor + 1) % ids.len();
    Some(target)
}

pub(crate) fn first_vendor_id(state: &ClientState) -> Option<EntityId> {
    state
        .presentation
        .entities()
        .find_map(|entity| match entity {
            ClientEntity::Npc(npc) if npc.kind == NpcKind::Vendor => Some(npc.id),
            _ => None,
        })
}

pub(crate) fn first_quest_offer(state: &ClientState) -> Option<(EntityId, QuestId)> {
    let npc_id = first_vendor_id(state)?;
    let quest_id = state
        .presentation
        .quest_offers(npc_id)?
        .first()
        .map(|offer| offer.quest_id)?;
    Some((npc_id, quest_id))
}

pub(crate) fn first_vendor_listing(state: &ClientState) -> Option<(EntityId, ItemId)> {
    let vendor_id = first_vendor_id(state)?;
    let item_id = state
        .presentation
        .vendor_listings(vendor_id)?
        .first()
        .map(|listing| listing.item_id)?;
    Some((vendor_id, item_id))
}

pub(crate) fn first_player_quest(state: &ClientState) -> Option<(EntityId, QuestId)> {
    let player_id = state.player_id?;
    let quest_id = state
        .presentation
        .player(player_id)?
        .quests
        .first()
        .map(|quest| quest.quest_id)?;
    Some((first_vendor_id(state)?, quest_id))
}
pub(crate) fn sample_frame_time(time: Res<Time>, mut stats: ResMut<FrameTimeStats>) {
    if !stats.enabled {
        return;
    }
    if !stats.collecting {
        if stats.warmup_timer.tick(time.delta()).just_finished() {
            stats.collecting = true;
            stats.report_timer.reset();
        }
        return;
    }
    if stats.samples_ms.len() < FRAME_TIME_SAMPLE_CAPACITY {
        stats.samples_ms.push(time.delta_secs_f64() * 1000.0);
    }
    if stats.report_timer.tick(time.delta()).just_finished() {
        stats.report();
    }
}

pub(crate) fn secure_window_focus(
    mut focus_events: MessageReader<WindowFocused>,
    mut secure_input: ResMut<SecureInputState>,
) {
    for event in focus_events.read() {
        // Any native focus transition invalidates bindings minted against the
        // previous focused surface. The registry deliberately does not
        // distinguish focus gain from loss; the next presentation commit must
        // re-register a binding before it can be activated.
        secure_input.registry.focus_changed();
        if event.focused {
            secure_input.refresh_default_binding();
        }
    }
}

pub(crate) fn acceptance_smoke_input(
    time: Res<Time>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut smoke: ResMut<AcceptanceSmoke>,
) {
    if !smoke.enabled
        || state.player_id.is_none()
        || !smoke.timer.tick(time.delta()).just_finished()
    {
        return;
    }
    let role = state.connected_role;
    let command = match role {
        Some(RoleCode::Tank) => match smoke.step {
            0 => ClientCommand::Target(EntityId(4)),
            8..=9 => ClientCommand::InvitePartyMember(EntityId(7)),
            10 => ClientCommand::Target(EntityId(4)),
            11..=15 => ClientCommand::Taunt,
            _ => {
                smoke.step = smoke.step.saturating_add(1);
                return;
            }
        },
        Some(RoleCode::Healer) => match smoke.step {
            8 => ClientCommand::AcceptPartyInvite(1),
            15..=25 => ClientCommand::Heal(EntityId(6)),
            _ => {
                smoke.step = smoke.step.saturating_add(1);
                return;
            }
        },
        Some(RoleCode::DamageDealer) => match smoke.step {
            0 => ClientCommand::ListVendor(EntityId(1)),
            1 => ClientCommand::ListQuestOffers(EntityId(1)),
            2 => ClientCommand::BuyItem {
                vendor_id: EntityId(1),
                item_id: ItemId::TOWN_RATION,
                quantity: 1,
            },
            3 => ClientCommand::AcceptQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
            4..=32 => ClientCommand::Move { dx: 0.35, dy: 0.0 },
            33 => ClientCommand::Target(EntityId(2)),
            34..=42 => ClientCommand::Attack,
            43 => ClientCommand::Loot(EntityId(2)),
            44 => ClientCommand::Target(EntityId(3)),
            45..=53 => ClientCommand::Attack,
            54 => ClientCommand::Loot(EntityId(3)),
            55 => ClientCommand::Target(EntityId(4)),
            56..=64 => ClientCommand::Attack,
            65 => ClientCommand::Loot(EntityId(4)),
            66..=94 => ClientCommand::Move { dx: -0.35, dy: 0.0 },
            95 => ClientCommand::TurnInQuest {
                npc_id: EntityId(1),
                quest_id: QuestId::CLEAR_THE_FIELD,
            },
            _ => return,
        },
        None => return,
    };
    send_command(&bridge, &mut state, command);
    smoke.step = smoke.step.saturating_add(1);
}

pub(crate) fn native_secure_pointer_input(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    bridge: Res<NetworkBridge>,
    mut state: ResMut<ClientState>,
    mut secure_input: ResMut<SecureInputState>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < secure_input.secure_attack_min.x
        || cursor.x > secure_input.secure_attack_max.x
        || cursor.y < secure_input.secure_attack_min.y
        || cursor.y > secure_input.secure_attack_max.y
    {
        return;
    }

    let physical_event_id = secure_input.next_physical_event_id();
    let default_addon = secure_input.default_addon;
    let default_node = secure_input.default_node;
    let node_generation = secure_input.node_generation;
    let attack_action = secure_input.attack_action;
    match secure_input.registry.dispatch(
        default_addon,
        default_node,
        node_generation,
        NativePress::PrimaryPointer { physical_event_id },
    ) {
        Ok(trusted) => {
            let (action, _, _, _) = trusted.consume();
            if action == attack_action {
                send_command(&bridge, &mut state, ClientCommand::Attack);
            }
        }
        Err(error) => state.log(error.to_string()),
    }
}
