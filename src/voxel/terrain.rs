use super::block::Block;
use super::grid::{VoxelGrid, CHUNK_RADIUS, MAX_HEIGHT};

/// Cheap "blocky noise" heightmap — swap for the `noise` crate's
/// Perlin/Simplex later for less repetitive terrain.
pub fn voxel_height(x: i32, z: i32) -> i32 {
    let fx = x as f32 * 0.15;
    let fz = z as f32 * 0.15;
    let n = (fx.sin() + fz.cos() + (fx * 0.5 + fz * 0.5).sin()) / 3.0;
    (((n + 1.0) * 2.0) as i32).clamp(0, 4) + 1 // 1..=5 blocks tall
}

pub fn build_voxel_grid() -> VoxelGrid {
    let size = CHUNK_RADIUS * 2;
    let mut grid = VoxelGrid::new(size, MAX_HEIGHT, size);

    for x in 0..size {
        for z in 0..size {
            let world_x = x - CHUNK_RADIUS;
            let world_z = z - CHUNK_RADIUS;
            let h = voxel_height(world_x, world_z);

            for y in 0..h {
                let block = if y == h - 1 {
                    Block::Grass
                } else if y >= h - 3 {
                    Block::Dirt
                } else {
                    Block::Stone
                };
                grid.set(x, y, z, block);
            }
        }
    }

    grid
}
