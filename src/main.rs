//! Minecraft-style voxel sandbox.
//!
//! Controls:
//!   WASD          move
//!   Mouse         look around (orbits the 3rd-person camera)
//!   Space         jump
//!   Left click    place the selected block
//!   Right click   mine the targeted block
//!   1 / 2 / 3     select Grass / Dirt / Stone to place
//!   Esc           release the cursor; click the window to re-grab it
//!
//! Module layout:
//!   cursor  - pointer-lock capture/release, incl. the focus-regrab fix
//!   player  - player + camera-rig components and their movement systems
//!   skybox  - deferred skybox texture loading
//!   hud     - crosshair, selected-block label, paused indicator
//!   voxel   - grid storage, terrain gen, greedy meshing, mine/place/highlight

mod cursor;
mod hud;
mod player;
mod skybox;
mod voxel;

use bevy::{
    core_pipeline::Skybox,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_rapier3d::prelude::*;

use player::{CameraRig, Player};
use skybox::SceneSkybox;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "game".into(),
                resolution: (800, 600).into(),
                ..default()
            }),
            // Grab + hide the cursor from the start, like Minecraft. This is
            // set here (rather than mutated in a Startup system) because
            // toggling CursorGrabMode on frame 0 is unreliable on X11 —
            // setting it as part of window creation avoids that.
            primary_cursor_options: Some(CursorOptions {
                grab_mode: CursorGrabMode::Locked,
                visible: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            RapierPhysicsPlugin::<NoUserData>::default(),
            // Draws collider wireframes — handy for debugging the voxel
            // collider, but the first thing to cut for a release build.
            RapierDebugRenderPlugin::default(),
        ))
        .add_plugins((cursor::plugin, player::plugin, skybox::plugin, voxel::plugin, hud::plugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let skybox_handle = asset_server.load("textures/sky.png");

    commands.insert_resource(SceneSkybox {
        handle: skybox_handle.clone(),
        is_loaded: false,
    });

    commands.spawn((
        DirectionalLight {
            illuminance: 5000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Voxel terrain (Minecraft-style, greedy-meshed)
    voxel::mesh::spawn_voxel_terrain(&mut commands, &mut meshes, &mut materials);

    // Player capsule — spawn high since terrain height now varies (1-5 blocks)
    let player_pos = Vec3::new(0.0, 10.0, 0.0);
    let capsule_radius = 0.4;
    let capsule_length = 1.0;

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(capsule_radius, capsule_length))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        })),
        Transform::from_translation(player_pos),
        Player,
        RigidBody::Dynamic,
        LockedAxes::ROTATION_LOCKED,
        Collider::cylinder(0.9, 0.4),
        Velocity::default(),
    ));

    let initial_yaw = 0.0_f32;
    let initial_pitch = 0.3_f32;
    let initial_distance = 6.0_f32;

    let offset_x = initial_distance * initial_yaw.cos() * initial_pitch.cos();
    let offset_y = initial_distance * initial_pitch.sin();
    let offset_z = initial_distance * initial_yaw.sin() * initial_pitch.cos();

    let camera_pos = player_pos + Vec3::new(offset_x, offset_y + 1.0, offset_z);

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_pos).looking_at(player_pos + Vec3::Y * 0.5, Vec3::Y),
        CameraRig {
            yaw: initial_yaw,
            pitch: initial_pitch,
            distance: initial_distance,
        },
        Skybox {
            image: Some(skybox_handle),
            brightness: 1000.0,
            ..default()
        },
    ));
}
