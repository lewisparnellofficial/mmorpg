//! Minimal Linux client technology spike.
//!
//! This binary deliberately demonstrates only the presentation boundary:
//! Bevy owns a window and draws a small scene, while the shared content crate
//! supplies the starter-zone definitions. There is no networking, input
//! command path, persistence, prediction, or addon runtime yet.

use bevy::prelude::*;
use mmorpg_content::{NpcArchetype, starter_catalog};

const FIELD_SIZE: Vec2 = Vec2::new(40.0, 30.0);

#[derive(Component)]
struct StarterNpc;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MMORPG Client — technology spike".to_owned(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.12, 0.18)))
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(
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

    let npc_materials = [
        materials.add(Color::srgb(0.95, 0.78, 0.16)),
        materials.add(Color::srgb(0.72, 0.72, 0.76)),
    ];

    for spawn in catalog.zones[0].spawns {
        let definition = catalog
            .npcs
            .iter()
            .find(|npc| npc.id == spawn.npc_template_id)
            .expect("every spawn must reference an NPC definition");
        let material = match definition.archetype {
            NpcArchetype::Vendor | NpcArchetype::QuestGiver => npc_materials[0].clone(),
            NpcArchetype::Enemy => npc_materials[1].clone(),
        };

        // Content coordinates use x/y; the 3D presentation maps content y to
        // world z. A primitive marker keeps every starter NPC visible.
        commands.spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.45, 1.0))),
            MeshMaterial3d(material),
            Transform::from_xyz(spawn.x, 0.75, spawn.y),
            StarterNpc,
        ));

        println!(
            "  instantiated {} '{}' at ({:.1}, {:.1})",
            match definition.archetype {
                NpcArchetype::Vendor => "vendor",
                NpcArchetype::Enemy => "enemy",
                NpcArchetype::QuestGiver => "quest giver",
            },
            definition.name,
            spawn.x,
            spawn.y
        );
    }
}

fn spawn_block(
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
