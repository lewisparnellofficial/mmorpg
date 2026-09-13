use super::*;

pub(crate) fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let catalog = starter_catalog();
    catalog
        .validate()
        .expect("the compiled starter catalog must validate before client startup");

    println!(
        "loaded zone '{}' with {} NPC definitions and {} spawn placements",
        catalog.zones[0].name,
        catalog.npcs.len(),
        catalog.zones[0].spawns.len()
    );

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 24.0, 25.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 18_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-8.0, 18.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let field_material = materials.add(Color::srgb(0.22, 0.45, 0.19));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(FIELD_SIZE.x, FIELD_SIZE.y))),
        MeshMaterial3d(field_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    let town_material = materials.add(Color::srgb(0.58, 0.42, 0.24));
    let roof_material = materials.add(Color::srgb(0.36, 0.12, 0.08));

    // The town is intentionally made from primitive meshes: this spike tests
    // the runtime and content boundary, not the art pipeline.
    spawn_block(
        &mut commands,
        &mut meshes,
        town_material.clone(),
        Vec3::new(-9.0, 0.3, -2.0),
        Vec3::new(5.0, 0.6, 4.0),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        town_material.clone(),
        Vec3::new(-3.0, 0.3, -2.0),
        Vec3::new(4.0, 0.6, 4.0),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        roof_material.clone(),
        Vec3::new(-9.0, 1.8, -2.0),
        Vec3::new(5.4, 2.4, 4.4),
    );
    spawn_block(
        &mut commands,
        &mut meshes,
        roof_material,
        Vec3::new(-3.0, 1.8, -2.0),
        Vec3::new(4.4, 2.4, 4.4),
    );

    // NPC presentation entities are created only after the server publishes
    // a completed snapshot. Catalog spawn order is not an entity identity.
    commands.insert_resource(NpcPresentationAssets {
        mesh: meshes.add(Capsule3d::new(0.45, 1.0)),
        vendor_material: materials.add(Color::srgb(0.95, 0.78, 0.16)),
        enemy_material: materials.add(Color::srgb(0.72, 0.72, 0.76)),
    });

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.5, 1.1))),
        MeshMaterial3d(materials.add(Color::srgb(0.18, 0.46, 0.95))),
        Transform::from_xyz(0.0, 0.85, 0.0),
        PlayerMarker,
    ));
}

pub(crate) fn setup_ui(mut commands: Commands, scripted_ui: Res<ScriptedUiPresentation>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(12.0),
                left: px(12.0),
                width: px(560.0),
                padding: UiRect::all(px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.88)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("connecting..."),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                StatusText,
            ));
            parent
                .spawn((
                    Node {
                        width: px(260.0),
                        height: px(32.0),
                        margin: UiRect::top(px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.12, 0.32, 0.58, 0.95)),
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new(format!(
                            "{} / {} — click or Space",
                            scripted_ui.default_label, scripted_ui.addon_label
                        )),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}
