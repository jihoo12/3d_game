use bevy::prelude::Color;

/// A single voxel type. `Air` is empty space; everything else is solid and
/// gets its own material/draw call in `mesh::rebuild_voxel_meshes`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
}

impl Block {
    pub fn is_solid(self) -> bool {
        self != Block::Air
    }

    pub fn color(self) -> Color {
        match self {
            Block::Grass => Color::srgb(0.35, 0.7, 0.3),
            Block::Dirt => Color::srgb(0.5, 0.35, 0.2),
            Block::Stone => Color::srgb(0.5, 0.5, 0.5),
            Block::Air => Color::WHITE,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Block::Grass => "Grass",
            Block::Dirt => "Dirt",
            Block::Stone => "Stone",
            Block::Air => "None",
        }
    }
}
