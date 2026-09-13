use super::*;

#[cfg(test)]
pub(crate) fn apply_server_line(state: &mut ClientState, line: &str) {
    // TEMPORARY ADAPTER: the development protocol is not production-ready,
    // but all gameplay state changes still pass through its strict decoder.
    if line.starts_with("TEMP_SNAPSHOT") {
        match state.snapshot.push_line(line) {
            Ok(Some(snapshot)) => apply_snapshot(state, snapshot),
            Ok(None) => {}
            Err(error) => state.log(format!("snapshot rejected: {error}")),
        }
        return;
    }
    match decode_server_line(line) {
        Ok(ServerLine::SnapshotBegin { .. } | ServerLine::SnapshotEnd) => {}
        Ok(ServerLine::World(_)) => {}
        Ok(ServerLine::Connected(connected)) => {
            state.player_id = Some(connected.player_id);
            state.connection_status = "authenticated development session".to_owned();
            state.log(format!("connected as {}", connected.player_id.0));
        }
        // Legacy diagnostic records are intentionally not projected. The
        // graphical client consumes complete TEMP_SNAPSHOT frames and typed
        // EVENT records so its renderer has one authoritative state source.
        Ok(
            ServerLine::Player(_) | ServerLine::Npc(_) | ServerLine::Item(_) | ServerLine::Quest(_),
        ) => {}
        Ok(ServerLine::Event(event)) => apply_event(state, event),
        Err(_) => match line.split_whitespace().next() {
            Some("ERR") => state.log(line.to_owned()),
            Some("WELCOME") => state.log("server greeted client"),
            Some("TYPE") => {}
            Some(_) | None => state.log(line.to_owned()),
        },
    }
}

pub(crate) fn apply_server_message(state: &mut ClientState, message: &ServerMessage) {
    let is_snapshot = matches!(message, ServerMessage::Snapshot(_));
    match message {
        ServerMessage::Response { message, .. } => return apply_server_message(state, message),
        ServerMessage::Welcome { server } => state.log(format!("server greeted {server}")),
        ServerMessage::Authenticated { account_id, .. } => {
            state.log(format!("authenticated account {account_id}"));
        }
        ServerMessage::CharacterList { characters, .. } => {
            state.available_characters = characters.clone();
            state.selected_character_id = None;
            state.connection_status = "select a character with Enter".to_owned();
            state.log(format!(
                "received {} available character(s)",
                characters.len()
            ));
        }
        ServerMessage::CharacterSelected { name, role, .. } => {
            state.connection_status = "character selected; entering world".to_owned();
            state.log(format!("selected character {name} ({role:?})"));
        }
        ServerMessage::Connected {
            player_id, role, ..
        } => {
            state.player_id = Some(EntityId(*player_id));
            state.connected_role = Some(*role);
            state.connection_status = "authenticated typed development session".to_owned();
            state.log(format!("connected as {player_id}"));
        }
        ServerMessage::ContentAccepted { .. } => {
            state.connection_status = "content compatible; entering world".to_owned();
            state.log("content compatibility accepted".to_owned());
        }
        ServerMessage::ContentMismatch { .. } => {
            state.connection_status = "content mismatch".to_owned();
            state.log("content compatibility rejected".to_owned());
        }
        ServerMessage::Error { message } => state.log(format!("rejected: {message}")),
        ServerMessage::Event(event) => {
            println!("GRAPHICAL_EVENT {event:?}");
        }
        ServerMessage::SkippedEvent { .. } | ServerMessage::Snapshot(_) => {}
    }
    if let Err(error) = apply_wire_message(&mut state.presentation, message) {
        state.log(format!("authoritative presentation rejected: {error}"));
    } else if is_snapshot {
        log_graphical_bootstrap(state);
    }
}

pub(crate) fn log_graphical_bootstrap(state: &ClientState) {
    let Some(player_id) = state.player_id else {
        return;
    };
    let Some(player) = state.presentation.player(player_id) else {
        return;
    };
    let quest = player
        .quests
        .iter()
        .find(|quest| quest.quest_id == QuestId::CLEAR_THE_FIELD);
    let (progress, status) = quest.map_or((0, "None".to_owned()), |quest| {
        (quest.progress, format!("{:?}", quest.status))
    });
    println!(
        "GRAPHICAL_BOOTSTRAP player={} gold={} clear_field_progress={} status={}",
        player_id.0, player.gold, progress, status
    );
}

#[cfg(test)]
pub(crate) fn apply_snapshot(state: &mut ClientState, snapshot: Snapshot) {
    if let Err(error) = apply_presentation_snapshot(&mut state.presentation, &snapshot) {
        state.log(format!("authoritative presentation rejected: {error}"));
        return;
    }
}

#[cfg(test)]
pub(crate) fn apply_event(state: &mut ClientState, event: ServerEvent) {
    match apply_presentation_event(&mut state.presentation, &event) {
        Ok(_) => {}
        Err(error) => state.log(format!("authoritative presentation rejected: {error}")),
    }
    match event {
        ServerEvent::TargetSelected {
            player_id,
            target_id,
        } if Some(player_id) == state.player_id => {
            state.log(format!("target {}", target_id.0));
        }
        ServerEvent::AttackResolved { player_id, .. } if Some(player_id) == state.player_id => {
            state.log("server resolved attack".to_owned());
        }
        ServerEvent::EnemyDefeated { .. } => {
            state.log("enemy defeated; server decides rewards".to_owned());
        }
        ServerEvent::CommandRejected { reason } => state.log(format!("rejected: {reason}")),
        ServerEvent::PlayerJoined { .. } => {}
        _ => {}
    }
}

pub(crate) fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(material),
        Transform::from_translation(position),
    ));
}
pub(crate) fn consume_network_events(bridge: Res<NetworkBridge>, mut state: ResMut<ClientState>) {
    let Ok(receiver) = bridge.event_rx.lock() else {
        state.connection_status = "network bridge unavailable".to_owned();
        return;
    };
    while let Ok(event) = receiver.try_recv() {
        match event {
            NetworkEvent::Status(status) => {
                if let Err(error) = state.snapshot.finish() {
                    state.log(format!("discarded incomplete snapshot: {error}"));
                }
                if status.starts_with("connecting to typed wire") {
                    state.player_id = None;
                    state.connected_role = None;
                    state.available_characters.clear();
                    state.selected_character_id = None;
                    state.presentation = ClientWorld::default();
                }
                state.connection_status = status.clone();
                state.log(status);
            }
            NetworkEvent::ServerMessage(message) => apply_server_message(&mut state, &message),
        }
    }
}
pub(crate) fn sync_authoritative_presentation(
    mut commands: Commands,
    state: Res<ClientState>,
    assets: Res<NpcPresentationAssets>,
    mut player_query: Query<&mut Transform, (With<PlayerMarker>, Without<StarterNpc>)>,
    mut npc_query: Query<(Entity, &StarterNpc, &mut Transform), Without<PlayerMarker>>,
) {
    if let Some(player_id) = state.player_id
        && let Some(player) = state.presentation.player(player_id)
        && let Ok(mut transform) = player_query.single_mut()
    {
        transform.translation.x = player.position.x;
        transform.translation.z = player.position.y;
    }
    let mut rendered_ids = BTreeMap::new();
    for (entity, marker, mut transform) in &mut npc_query {
        if let Some(ClientEntity::Npc(npc)) = state.presentation.entity(marker.id) {
            transform.translation.x = npc.position.x;
            transform.translation.z = npc.position.y;
            transform.translation.y = if npc.defeated { 0.2 } else { 0.75 };
            rendered_ids.insert(marker.id, entity);
        } else {
            commands.entity(entity).despawn();
        }
    }

    for entity in state.presentation.entities() {
        let ClientEntity::Npc(npc) = entity else {
            continue;
        };
        if rendered_ids.contains_key(&npc.id) {
            continue;
        }
        let material = match npc.kind {
            NpcKind::Vendor => assets.vendor_material.clone(),
            NpcKind::Enemy => assets.enemy_material.clone(),
        };
        commands.spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(
                npc.position.x,
                if npc.defeated { 0.2 } else { 0.75 },
                npc.position.y,
            ),
            StarterNpc { id: npc.id },
        ));
    }
}
