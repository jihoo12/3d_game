mod block;
mod grid;
mod interaction;
pub mod mesh;
mod terrain;

use bevy::prelude::*;

// Re-exported for API completeness (e.g. hud.rs matches on `Block::name()`
// results); not referenced by name anywhere else in this crate today.
#[allow(unused_imports)]
pub use block::Block;
pub use grid::VoxelGrid;
pub use interaction::SelectedBlock;

use crate::cursor::cursor_captured;

/// Holds the CPU-side voxel grid plus every entity currently representing it
/// on screen (one mesh entity per block type, plus the physics collider), so
/// a mine or place action can despawn all of them and rebuild from scratch.
#[derive(Resource)]
pub struct VoxelWorld {
    pub grid: VoxelGrid,
    pub mesh_entities: Vec<Entity>,
    pub collider_entity: Option<Entity>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SelectedBlock>().add_systems(
        Update,
        (
            // Mining, placing, and the hotbar only react to input while the
            // cursor is actually captured — see `cursor::cursor_captured`.
            interaction::mine_block.run_if(cursor_captured),
            interaction::place_block.run_if(cursor_captured),
            interaction::cycle_selected_block.run_if(cursor_captured),
            interaction::highlight_targeted_block,
        ),
    );
}
