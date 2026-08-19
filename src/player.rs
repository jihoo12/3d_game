use bevy::{input::mouse::MouseMotion, prelude::*};
use bevy_rapier3d::prelude::*;

use crate::cursor::cursor_captured;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

pub fn plugin(app: &mut App) {
    // Movement and the camera both read raw mouse/keyboard state, so they're
    // gated behind `cursor_captured` the same as mining/placing — no reason
    // for WASD or mouse-look to keep working while the cursor is released
    // for a menu.
    app.add_systems(Update, (move_player, camera_orbit).run_if(cursor_captured));
}

pub fn move_player(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_query: Query<(&mut Transform, &mut Velocity), With<Player>>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<Player>)>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok((mut transform, mut velocity)) = player_query.single_mut() else {
        return;
    };

    if transform.translation.y < -15.0 {
        transform.translation = Vec3::new(0.0, 10.0, 0.0);
        velocity.linear = Vec3::ZERO;
        return;
    }

    let mut camera_forward = *camera_transform.forward();
    camera_forward.y = 0.0;
    let camera_forward = camera_forward.normalize_or_zero();

    let mut camera_right = *camera_transform.right();
    camera_right.y = 0.0;
    let camera_right = camera_right.normalize_or_zero();

    let mut direction = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        direction += camera_forward;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        direction -= camera_forward;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        direction -= camera_right;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        direction += camera_right;
    }

    let speed = 5.0;
    let move_dir = direction.normalize_or_zero();

    velocity.linear.x = move_dir.x * speed;
    velocity.linear.z = move_dir.z * speed;

    if keyboard.just_pressed(KeyCode::Space) && velocity.linear.y.abs() < 0.1 {
        velocity.linear.y = 7.0;
    }
}

pub fn camera_orbit(
    mut mouse_motion_events: MessageReader<MouseMotion>,
    player_query: Query<&Transform, (With<Player>, Without<CameraRig>)>,
    mut camera_query: Query<(&mut Transform, &mut CameraRig), Without<Player>>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let target_focus = player_pos + Vec3::Y * 0.5;

    let mut delta = Vec2::ZERO;
    for event in mouse_motion_events.read() {
        delta += event.delta;
    }

    for (mut cam_transform, mut rig) in &mut camera_query {
        // Cursor is locked to the window (see cursor.rs), so just follow raw
        // mouse movement every frame — no need to hold a button down first,
        // same as most 3rd-person open-world cameras.
        if delta != Vec2::ZERO {
            let sensitivity = 0.005;
            rig.yaw += delta.x * sensitivity;
            rig.pitch = (rig.pitch + delta.y * sensitivity).clamp(-1.5, 1.5);
        }

        let x = rig.distance * rig.yaw.cos() * rig.pitch.cos();
        let y = rig.distance * rig.pitch.sin();
        let z = rig.distance * rig.yaw.sin() * rig.pitch.cos();

        cam_transform.translation = player_pos + Vec3::new(x, y + 1.0, z);
        cam_transform.look_at(target_focus, Vec3::Y);
    }
}
