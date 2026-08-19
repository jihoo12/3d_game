use super::block::Block;
use super::grid::{VoxelGrid, CHUNK_RADIUS, MAX_HEIGHT};
use super::noise::{fbm, scatter_check};

/// Ground at or below this height counts as underwater/shoreline. Also
/// where `mesh::spawn_water_plane` places its water plane.
pub const SEA_LEVEL: i32 = 4;
const SNOW_LEVEL: i32 = 11;
const DIRT_DEPTH: i32 = 3;
const TREE_DENSITY: f32 = 0.02;

/// Height of the terrain surface at a world column, in blocks. Two stacked
/// fBm octaves give broad rolling hills with smaller bumpy detail on top,
/// instead of the old raw sin/cos heightmap's obviously repeating ripples.
/// Remapped from `fbm`'s [0, 1) into roughly 2..=14, leaving headroom under
/// `MAX_HEIGHT` for trees.
fn surface_height(x: i32, z: i32) -> i32 {
    let n = fbm(x as f32 * 0.045, z as f32 * 0.045, 4, 2.0, 0.5);
    (2.0 + n * 12.0) as i32
}

/// A second, decorrelated noise channel (different frequency and offset, so
/// it doesn't just track terrain height) used only to vary surface material
/// at mid elevations — without it every column at the same height looks
/// identical, which reads as banded rather than natural.
fn moisture(x: i32, z: i32) -> f32 {
    fbm(x as f32 * 0.08 + 100.0, z as f32 * 0.08 - 100.0, 3, 2.0, 0.5)
}

fn surface_block(height: i32, moisture: f32) -> Block {
    if height <= SEA_LEVEL {
        Block::Sand // underwater / lakebed
    } else if height >= SNOW_LEVEL {
        Block::Snow // mountain caps
    } else if height <= SEA_LEVEL + 2 && moisture < 0.35 {
        Block::Sand // dry beach just above the waterline
    } else {
        Block::Grass
    }
}

pub fn build_voxel_grid() -> VoxelGrid {
    let size = CHUNK_RADIUS * 2;
    let mut grid = VoxelGrid::new(size, MAX_HEIGHT, size);
    let mut heights = vec![0i32; (size * size) as usize];
    let mut top_blocks = vec![Block::Air; (size * size) as usize];

    for x in 0..size {
        for z in 0..size {
            let world_x = x - CHUNK_RADIUS;
            let world_z = z - CHUNK_RADIUS;
            let h = surface_height(world_x, world_z);
            let top_block = surface_block(h, moisture(world_x, world_z));

            for y in 0..h {
                let block = if y == h - 1 {
                    top_block
                } else if y >= h - 1 - DIRT_DEPTH {
                    Block::Dirt
                } else {
                    Block::Stone
                };
                grid.set(x, y, z, block);
            }

            let idx = (x + z * size) as usize;
            heights[idx] = h;
            top_blocks[idx] = top_block;
        }
    }

    // Second pass: scatter trees only after every column's terrain is
    // final, so a tree's canopy (which reaches into neighboring columns)
    // never gets clipped by a neighbor that hadn't been generated yet.
    for x in 0..size {
        for z in 0..size {
            let idx = (x + z * size) as usize;
            let world_x = x - CHUNK_RADIUS;
            let world_z = z - CHUNK_RADIUS;
            maybe_plant_tree(&mut grid, x, z, world_x, world_z, heights[idx], top_blocks[idx]);
        }
    }

    grid
}

/// Scatters simple trees on grass columns: a straight trunk plus a small
/// blocky leaf canopy. Uses `scatter_check` (hash-based) rather than `rand`
/// so terrain generation stays fully deterministic and dependency-free.
fn maybe_plant_tree(
    grid: &mut VoxelGrid,
    local_x: i32,
    local_z: i32,
    world_x: i32,
    world_z: i32,
    ground_height: i32,
    top_block: Block,
) {
    if top_block != Block::Grass {
        return; // only plant on grass — not sand, snow, or underwater
    }
    if !scatter_check(world_x, world_z, 91_017, TREE_DENSITY) {
        return;
    }
    // Leave room at the grid edges for the canopy so it doesn't get clipped.
    if local_x < 2 || local_z < 2 || local_x >= grid.dims[0] - 2 || local_z >= grid.dims[2] - 2 {
        return;
    }

    let trunk_height = 3;
    let trunk_top = ground_height + trunk_height - 1;
    if trunk_top + 2 >= grid.dims[1] {
        return; // not enough headroom this column — skip rather than clip
    }

    for y in ground_height..ground_height + trunk_height {
        grid.set(local_x, y, local_z, Block::Wood);
    }

    // A 3x3 canopy layer plus a single cap block — enough to read as a tree
    // without a real foliage mesh.
    for dx in -1..=1 {
        for dz in -1..=1 {
            grid.set(local_x + dx, trunk_top, local_z + dz, Block::Leaves);
        }
    }
    grid.set(local_x, trunk_top + 1, local_z, Block::Leaves);
}
