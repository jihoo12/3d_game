use bevy::prelude::Vec3;

use super::block::Block;

/// World spans -CHUNK_RADIUS..CHUNK_RADIUS on x/z.
pub const CHUNK_RADIUS: i32 = 20;
pub const MAX_HEIGHT: i32 = 6;

/// A dense 3D array of blocks. Coordinates passed to `get`/`set` are
/// grid-local (0..dims), not world space — use `origin_offset()` to convert
/// to/from Bevy world positions.
pub struct VoxelGrid {
    pub dims: [i32; 3], // [width(x), height(y), depth(z)]
    blocks: Vec<Block>,
}

impl VoxelGrid {
    pub fn new(width: i32, height: i32, depth: i32) -> Self {
        Self {
            dims: [width, height, depth],
            blocks: vec![Block::Air; (width * height * depth) as usize],
        }
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        (x + y * self.dims[0] + z * self.dims[0] * self.dims[1]) as usize
    }

    pub fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.dims[0] && y < self.dims[1] && z < self.dims[2]
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> Block {
        if !self.in_bounds(x, y, z) {
            return Block::Air;
        }
        self.blocks[self.index(x, y, z)]
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if !self.in_bounds(x, y, z) {
            return;
        }
        let idx = self.index(x, y, z);
        self.blocks[idx] = block;
    }

    /// World-space translation of grid-local (0,0,0). Every mesh, collider,
    /// and raycast needs this to convert between grid and world coordinates,
    /// so it lives here once instead of being recomputed at each call site.
    pub fn origin_offset(&self) -> Vec3 {
        Vec3::new(-CHUNK_RADIUS as f32, 0.0, -CHUNK_RADIUS as f32)
    }
}
