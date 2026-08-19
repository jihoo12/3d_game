use bevy::prelude::*;

use super::block::Block;
use super::grid::VoxelGrid;
use super::mesh::rebuild_voxel_meshes;
use super::VoxelWorld;
use crate::player::Player;

/// Which block type left-click will place. Cycled with the 1/2/3 hotbar
/// keys; shown in the HUD label (see `hud.rs`).
#[derive(Resource)]
pub struct SelectedBlock(pub Block);

impl Default for SelectedBlock {
    fn default() -> Self {
        Self(Block::Grass)
    }
}

pub fn cycle_selected_block(keyboard: Res<ButtonInput<KeyCode>>, mut selected: ResMut<SelectedBlock>) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        selected.0 = Block::Grass;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        selected.0 = Block::Dirt;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        selected.0 = Block::Stone;
    }
}

/// Result of a voxel raycast: the solid block that was hit, and the empty
/// cell immediately before it along the ray — i.e. where a placed block
/// would land, Minecraft-style.
struct BlockHit {
    block: (i32, i32, i32),
    place_at: (i32, i32, i32),
}

/// Simple fixed-step ray march through the voxel grid (no DDA/Amanatides-Woo
/// acceleration) to find the first solid block along a ray. `origin` must
/// already be in grid-local space (i.e. with origin_offset subtracted out).
fn raycast_voxel(grid: &VoxelGrid, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<BlockHit> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    let step = 0.05;
    let steps = (max_dist / step) as i32;
    let mut last_cell: Option<(i32, i32, i32)> = None;
    let mut previous_empty: Option<(i32, i32, i32)> = None;

    for i in 0..steps {
        let p = origin + dir * (i as f32 * step);
        let cell = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if Some(cell) == last_cell {
            continue;
        }

        if grid.get(cell.0, cell.1, cell.2).is_solid() {
            return Some(BlockHit {
                block: cell,
                place_at: previous_empty.unwrap_or(cell),
            });
        }

        previous_empty = Some(cell);
        last_cell = Some(cell);
    }

    None
}

/// Shared aim logic: the camera always looks_at the player (see
/// `player::camera_orbit`), so `camera_transform.forward()` points AT the
/// player — the ray needs to start at the player and continue outward from
/// there (not start at the camera) or it burns its whole range just
/// reaching them.
fn find_targeted_block(
    camera_transform: &Transform,
    player_transform: &Transform,
    grid: &VoxelGrid,
) -> Option<BlockHit> {
    let ray_origin = (player_transform.translation + Vec3::Y * 0.5) - grid.origin_offset();
    let ray_dir = camera_transform.forward().as_vec3();
    let mine_range = 6.0;

    raycast_voxel(grid, ray_origin, ray_dir, mine_range)
}

/// Draws a thin wireframe cube around whatever block the player is currently
/// aiming at, Minecraft-style. Runs every frame via Gizmos (immediate-mode
/// lines, nothing spawned/despawned), so it's cheap and independent of the
/// mining/placing rebuild cost.
pub fn highlight_targeted_block(
    camera_query: Query<&Transform, With<Camera3d>>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    voxel_world: Res<VoxelWorld>,
    mut gizmos: Gizmos,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let Some(hit) = find_targeted_block(camera_transform, player_transform, &voxel_world.grid)
    else {
        return;
    };

    let (gx, gy, gz) = hit.block;
    let block_center =
        voxel_world.grid.origin_offset() + Vec3::new(gx as f32 + 0.5, gy as f32 + 0.5, gz as f32 + 0.5);

    // Slightly larger than the 1x1x1 block so the outline doesn't z-fight
    // with the block's own faces — same trick Minecraft's overlay uses.
    gizmos.cube(
        Transform::from_translation(block_center).with_scale(Vec3::splat(1.02)),
        Color::BLACK,
    );
}

/// Mining: right-click removes the first solid block the camera is looking
/// at, then rebuilds the ENTIRE world mesh + collider from scratch (see
/// `mesh::rebuild_voxel_meshes`). Every mine is therefore an O(whole grid)
/// operation instead of an O(one chunk) one.
pub fn mine_block(
    mouse_button: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&Transform, With<Camera3d>>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    mut voxel_world: ResMut<VoxelWorld>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    if let Some(hit) = find_targeted_block(camera_transform, player_transform, &voxel_world.grid) {
        let (gx, gy, gz) = hit.block;
        voxel_world.grid.set(gx, gy, gz, Block::Air);
        rebuild_voxel_meshes(&mut commands, &mut meshes, &mut materials, &mut voxel_world);
    }
}

/// Placing: left-click adds a block of the currently `SelectedBlock` type
/// into the empty cell just in front of whatever face the player is aiming
/// at — shares the raycast with `mine_block`, just uses `place_at` instead
/// of `block`. Also rebuilds the whole world every call, same cost
/// trade-off as mining.
///
/// Left-click is also what re-grabs the cursor after Esc (see `cursor.rs`),
/// but that never collides with this: `cursor::toggle_cursor_capture` only
/// re-grabs while the cursor is *released*, and this system only runs while
/// the cursor is *captured* (gated in `voxel::plugin`), so exactly one of
/// the two ever reacts to a given click.
pub fn place_block(
    mouse_button: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&Transform, With<Camera3d>>,
    player_query: Query<&Transform, (With<Player>, Without<Camera3d>)>,
    selected: Res<SelectedBlock>,
    mut voxel_world: ResMut<VoxelWorld>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let Some(hit) = find_targeted_block(camera_transform, player_transform, &voxel_world.grid)
    else {
        return;
    };

    let (px, py, pz) = hit.place_at;
    if !voxel_world.grid.in_bounds(px, py, pz) {
        return;
    }
    if voxel_world.grid.get(px, py, pz).is_solid() {
        return;
    }
    if overlaps_player(&voxel_world.grid, (px, py, pz), player_transform.translation) {
        return;
    }

    voxel_world.grid.set(px, py, pz, selected.0);
    rebuild_voxel_meshes(&mut commands, &mut meshes, &mut materials, &mut voxel_world);
}

/// Refuses a placement that would land the new block inside the player's own
/// body — without this, aiming down at your feet and placing would wall you
/// into the ground.
fn overlaps_player(grid: &VoxelGrid, place_at: (i32, i32, i32), player_pos: Vec3) -> bool {
    let block_min =
        grid.origin_offset() + Vec3::new(place_at.0 as f32, place_at.1 as f32, place_at.2 as f32);
    let block_max = block_min + Vec3::ONE;

    let radius = 0.4; // matches the player's cylinder collider
    let half_height = 0.9;
    let player_min = player_pos - Vec3::new(radius, half_height, radius);
    let player_max = player_pos + Vec3::new(radius, half_height, radius);

    block_min.x < player_max.x
        && block_max.x > player_min.x
        && block_min.y < player_max.y
        && block_max.y > player_min.y
        && block_min.z < player_max.z
        && block_max.z > player_min.z
}
