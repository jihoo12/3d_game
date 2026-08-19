use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::mesh::{Indices, PrimitiveTopology},
};
use bevy_rapier3d::math::IVect;
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;

use super::block::Block;
use super::grid::VoxelGrid;
use super::terrain::build_voxel_grid;
use super::VoxelWorld;

#[derive(Default)]
struct MeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshData {
    fn push_quad(&mut self, bottom_left: Vec3, right: Vec3, up: Vec3, normal: Vec3) {
        let base = self.positions.len() as u32;
        let p0 = bottom_left;
        let p1 = bottom_left + right;
        let p2 = bottom_left + right + up;
        let p3 = bottom_left + up;

        self.positions
            .extend([p0.to_array(), p1.to_array(), p2.to_array(), p3.to_array()]);
        self.normals.extend([normal.to_array(); 4]);
        let w = right.length();
        let h = up.length();
        self.uvs.extend([[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]);
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn axis_vec(axis: usize, len: f32) -> Vec3 {
    match axis {
        0 => Vec3::new(len, 0.0, 0.0),
        1 => Vec3::new(0.0, len, 0.0),
        2 => Vec3::new(0.0, 0.0, len),
        _ => unreachable!(),
    }
}

/// Classic greedy-meshing sweep (per mikolalysenko's algorithm): for each
/// axis, slide a plane through the grid, mark visible faces in a 2D mask,
/// then merge adjacent same-material mask cells into the largest rectangles
/// possible.
fn greedy_mesh(grid: &VoxelGrid) -> HashMap<Block, MeshData> {
    let mut out: HashMap<Block, MeshData> = HashMap::new();
    let dims = grid.dims;

    for axis in 0..3 {
        let u_axis = (axis + 1) % 3;
        let v_axis = (axis + 2) % 3;

        let mut x = [0i32; 3];
        let mut q = [0i32; 3];
        q[axis] = 1;

        let mut mask: Vec<Option<(Block, bool)>> =
            vec![None; (dims[u_axis] * dims[v_axis]) as usize];

        x[axis] = -1;
        while x[axis] < dims[axis] {
            // Build the mask for the plane between layer x[axis] and x[axis]+1
            let mut n = 0usize;
            for j in 0..dims[v_axis] {
                x[v_axis] = j;
                for i in 0..dims[u_axis] {
                    x[u_axis] = i;

                    let a = grid.get(x[0], x[1], x[2]);
                    let b = grid.get(x[0] + q[0], x[1] + q[1], x[2] + q[2]);

                    mask[n] = if a.is_solid() != b.is_solid() {
                        if a.is_solid() {
                            Some((a, true)) // face visible, points toward +axis
                        } else {
                            Some((b, false)) // face visible, points toward -axis
                        }
                    } else {
                        None
                    };
                    n += 1;
                }
            }

            x[axis] += 1;

            // Merge the mask into rectangles
            let mut n = 0usize;
            for j in 0..dims[v_axis] {
                let mut i = 0;
                while i < dims[u_axis] {
                    if let Some(entry) = mask[n] {
                        // Grow width
                        let mut w = 1;
                        while i + w < dims[u_axis] && mask[n + w as usize] == Some(entry) {
                            w += 1;
                        }

                        // Grow height while the whole row matches
                        let mut h = 1;
                        'grow: while j + h < dims[v_axis] {
                            for k in 0..w {
                                if mask[(n + k as usize) + (h * dims[u_axis]) as usize] != Some(entry) {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }

                        let (block, forward) = entry;
                        let mut base_pos = [0.0f32; 3];
                        base_pos[axis] = x[axis] as f32;
                        base_pos[u_axis] = i as f32;
                        base_pos[v_axis] = j as f32;

                        let right = axis_vec(u_axis, w as f32);
                        let up = axis_vec(v_axis, h as f32);
                        let normal = axis_vec(axis, if forward { 1.0 } else { -1.0 });

                        let bottom_left = Vec3::from_array(base_pos);
                        let mesh_data = out.entry(block).or_default();

                        if forward {
                            mesh_data.push_quad(bottom_left, right, up, normal);
                        } else {
                            // Swap edges to flip winding so the normal stays correct
                            mesh_data.push_quad(bottom_left, up, right, normal);
                        }

                        // Clear the merged region so it isn't processed again
                        for l in 0..h {
                            for k in 0..w {
                                mask[(n + k as usize) + (l * dims[u_axis]) as usize] = None;
                            }
                        }

                        i += w;
                        n += w as usize;
                    } else {
                        i += 1;
                        n += 1;
                    }
                }
            }
        }
    }

    out
}

pub fn spawn_voxel_terrain(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let mut voxel_world = VoxelWorld {
        grid: build_voxel_grid(),
        mesh_entities: Vec::new(),
        collider_entity: None,
    };

    rebuild_voxel_meshes(commands, meshes, materials, &mut voxel_world);

    commands.insert_resource(voxel_world);
}

/// Rebuilds EVERY mesh entity and the ENTIRE physics collider from the full
/// voxel grid, from scratch, every time it's called. There's no dirty-chunk
/// tracking and no partial update path on purpose: this is called once at
/// startup, and then again on every single block mined or placed, so it
/// re-runs greedy meshing over the whole world and re-uploads brand new GPU
/// mesh buffers each time. That's what makes mining/placing feel laggy as
/// the world gets bigger — a real implementation would only touch the
/// affected chunk.
pub fn rebuild_voxel_meshes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    voxel_world: &mut VoxelWorld,
) {
    // Despawn everything from the previous rebuild first.
    for entity in voxel_world.mesh_entities.drain(..) {
        commands.entity(entity).despawn();
    }
    if let Some(collider_entity) = voxel_world.collider_entity.take() {
        commands.entity(collider_entity).despawn();
    }

    let origin_offset = voxel_world.grid.origin_offset();
    let chunk_meshes = greedy_mesh(&voxel_world.grid);

    // One draw call per material instead of one per block.
    for (block, mesh_data) in chunk_meshes {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, mesh_data.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, mesh_data.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, mesh_data.uvs);
        mesh.insert_indices(Indices::U32(mesh_data.indices));

        let entity = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: block.color(),
                    ..default()
                })),
                Transform::from_translation(origin_offset),
            ))
            .id();
        voxel_world.mesh_entities.push(entity);
    }

    // Physics: ONE voxels collider for the whole terrain instead of one box
    // per column. This is Rapier's purpose-built shape for cube-grid terrain
    // — it treats neighboring cells as a single continuous surface, so the
    // player doesn't snag on the seams between adjacent boxes the way it
    // would with separate Collider::cuboid entities (the "internal edges"
    // problem). It also uses the SAME integer grid coordinates as the mesh,
    // so with the matching origin_offset transform it lines up automatically.
    let mut solid_coords: Vec<IVect> = Vec::new();
    for x in 0..voxel_world.grid.dims[0] {
        for y in 0..voxel_world.grid.dims[1] {
            for z in 0..voxel_world.grid.dims[2] {
                if voxel_world.grid.get(x, y, z).is_solid() {
                    solid_coords.push(IVect::new(x, y, z));
                }
            }
        }
    }

    let collider_entity = commands
        .spawn((
            RigidBody::Fixed,
            Collider::voxels(Vec3::splat(1.0), &solid_coords),
            Transform::from_translation(origin_offset),
        ))
        .id();
    voxel_world.collider_entity = Some(collider_entity);
}
