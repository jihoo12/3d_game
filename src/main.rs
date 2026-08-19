use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::Skybox,
    input::mouse::MouseMotion,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_resource::{TextureViewDescriptor, TextureViewDimension},
    },
    window::{CursorGrabMode, CursorOptions},
};
use bevy_rapier3d::prelude::*;
use std::collections::HashMap;
use bevy_rapier3d::math::IVect;

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
            RapierDebugRenderPlugin::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                move_player,
                camera_orbit,
                check_skybox_loaded,
                mine_block,
                highlight_targeted_block,
                grab_mouse_cursor,
            ),
        )
        .run();
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct CameraRig {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

#[derive(Resource)]
struct SceneSkybox {
    handle: Handle<Image>,
    is_loaded: bool,
}

// ---------- Voxel terrain ----------

const CHUNK_RADIUS: i32 = 20; // world spans -CHUNK_RADIUS..CHUNK_RADIUS on x/z
const MAX_HEIGHT: i32 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Block {
    Air,
    Grass,
    Dirt,
    Stone,
}

impl Block {
    fn is_solid(self) -> bool {
        self != Block::Air
    }

    fn color(self) -> Color {
        match self {
            Block::Grass => Color::srgb(0.35, 0.7, 0.3),
            Block::Dirt => Color::srgb(0.5, 0.35, 0.2),
            Block::Stone => Color::srgb(0.5, 0.5, 0.5),
            Block::Air => Color::WHITE,
        }
    }
}

struct VoxelGrid {
    dims: [i32; 3], // [width(x), height(y), depth(z)]
    blocks: Vec<Block>,
}

impl VoxelGrid {
    fn new(width: i32, height: i32, depth: i32) -> Self {
        Self {
            dims: [width, height, depth],
            blocks: vec![Block::Air; (width * height * depth) as usize],
        }
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        (x + y * self.dims[0] + z * self.dims[0] * self.dims[1]) as usize
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Block {
        if x < 0 || y < 0 || z < 0 || x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return Block::Air;
        }
        self.blocks[self.index(x, y, z)]
    }

    fn set(&mut self, x: i32, y: i32, z: i32, block: Block) {
        let idx = self.index(x, y, z);
        self.blocks[idx] = block;
    }
}

// Holds the CPU-side voxel grid plus every entity currently representing it
// on screen (one mesh entity per block type, plus the physics collider), so
// a mining action can despawn all of them and rebuild from scratch.
#[derive(Resource)]
struct VoxelWorld {
    grid: VoxelGrid,
    mesh_entities: Vec<Entity>,
    collider_entity: Option<Entity>,
}

// Cheap "blocky noise" heightmap — swap for the `noise` crate's Perlin/Simplex
// later for less repetitive terrain.
fn voxel_height(x: i32, z: i32) -> i32 {
    let fx = x as f32 * 0.15;
    let fz = z as f32 * 0.15;
    let n = (fx.sin() + fz.cos() + (fx * 0.5 + fz * 0.5).sin()) / 3.0;
    (((n + 1.0) * 2.0) as i32).clamp(0, 4) + 1 // 1..=5 blocks tall
}

fn build_voxel_grid() -> VoxelGrid {
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

// Classic greedy-meshing sweep (per mikolalysenko's algorithm): for each axis,
// slide a plane through the grid, mark visible faces in a 2D mask, then merge
// adjacent same-material mask cells into the largest rectangles possible.
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

fn spawn_voxel_terrain(
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

// Rebuilds EVERY mesh entity and the ENTIRE physics collider from the full
// voxel grid, from scratch, every time it's called. There's no dirty-chunk
// tracking and no partial update path on purpose: this is called once at
// startup, and then again on every single block mined, so it re-runs greedy
// meshing over the whole world and re-uploads brand new GPU mesh buffers
// each time. That's what makes mining feel laggy as the world gets bigger —
// a real implementation would only touch the affected chunk.
fn rebuild_voxel_meshes(
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

    let origin_offset = Vec3::new(-CHUNK_RADIUS as f32, 0.0, -CHUNK_RADIUS as f32);
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

// Simple fixed-step ray march through the voxel grid (no DDA/amanatides-woo
// acceleration) to find the first solid block along a ray. `origin` must
// already be in grid-local space (i.e. with origin_offset subtracted out).
fn raycast_voxel(grid: &VoxelGrid, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(i32, i32, i32)> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    let step = 0.05;
    let steps = (max_dist / step) as i32;
    let mut last_cell: Option<(i32, i32, i32)> = None;

    for i in 0..steps {
        let p = origin + dir * (i as f32 * step);
        let cell = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if Some(cell) == last_cell {
            continue;
        }
        last_cell = Some(cell);

        if grid.get(cell.0, cell.1, cell.2).is_solid() {
            return Some(cell);
        }
    }

    None
}

// Mining: right-click removes the first solid block the camera is looking
// at, then rebuilds the ENTIRE world mesh + collider from scratch (see
// rebuild_voxel_meshes). Every mine is therefore an O(whole grid) operation
// instead of an O(one chunk) one, as requested.
// Shared aim logic: the camera always looks_at the player (see
// camera_orbit), so camera_transform.forward() points AT the player — the
// ray needs to start at the player and continue outward from there (not
// start at the camera) or it burns its whole range just reaching them.
fn find_targeted_block(
    camera_transform: &Transform,
    player_transform: &Transform,
    grid: &VoxelGrid,
) -> Option<(i32, i32, i32)> {
    let origin_offset = Vec3::new(-CHUNK_RADIUS as f32, 0.0, -CHUNK_RADIUS as f32);
    let ray_origin = (player_transform.translation + Vec3::Y * 0.5) - origin_offset;
    let ray_dir = camera_transform.forward().as_vec3();
    let mine_range = 6.0;

    raycast_voxel(grid, ray_origin, ray_dir, mine_range)
}

// Draws a thin wireframe cube around whatever block the player is currently
// aiming at, Minecraft-style. Runs every frame via Gizmos (immediate-mode
// lines, nothing spawned/despawned), so it's cheap and independent of the
// mining rebuild cost.
fn highlight_targeted_block(
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

    let Some((gx, gy, gz)) =
        find_targeted_block(camera_transform, player_transform, &voxel_world.grid)
    else {
        return;
    };

    let origin_offset = Vec3::new(-CHUNK_RADIUS as f32, 0.0, -CHUNK_RADIUS as f32);
    let block_center = origin_offset + Vec3::new(gx as f32 + 0.5, gy as f32 + 0.5, gz as f32 + 0.5);

    // Slightly larger than the 1x1x1 block so the outline doesn't z-fight
    // with the block's own faces — same trick Minecraft's overlay uses.
    gizmos.cube(
        Transform::from_translation(block_center).with_scale(Vec3::splat(1.02)),
        Color::BLACK,
    );
}

fn mine_block(
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

    if let Some((gx, gy, gz)) =
        find_targeted_block(camera_transform, player_transform, &voxel_world.grid)
    {
        voxel_world.grid.set(gx, gy, gz, Block::Air);
        rebuild_voxel_meshes(&mut commands, &mut meshes, &mut materials, &mut voxel_world);
    }
}

// ---------- Setup ----------

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
    spawn_voxel_terrain(&mut commands, &mut meshes, &mut materials);

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

fn check_skybox_loaded(
    mut skybox_res: ResMut<SceneSkybox>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    if !skybox_res.is_loaded && asset_server.load_state(&skybox_res.handle).is_loaded() {
        skybox_res.is_loaded = true;
        if let Some(mut image) = images.get_mut(&skybox_res.handle) {
            if image.texture_descriptor.array_layer_count() == 1 {
                let layers = image.height() / image.width();
                image
                    .reinterpret_stacked_2d_as_array(layers)
                    .expect("Failed to reinterpret skybox image as an array texture");

                image.texture_view_descriptor = Some(TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::Cube),
                    ..default()
                });
            }
        }
    }
}

// Esc releases the cursor (shows it, stops locking it to the window) so you
// can get to a menu, alt-tab, etc. Clicking back in the window re-grabs and
// re-hides it. The cursor starts grabbed already via primary_cursor_options
// in main(), so this only needs to handle the release/re-grab toggle.
fn grab_mouse_cursor(
    mut cursor_options: Single<&mut CursorOptions>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
    }

    if mouse_button.just_pressed(MouseButton::Left)
        && cursor_options.grab_mode == CursorGrabMode::None
    {
        cursor_options.visible = false;
        cursor_options.grab_mode = CursorGrabMode::Locked;
    }
}

fn move_player(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player_query: Query<(&mut Transform, &mut Velocity), With<Player>>,
    camera_query: Query<&Transform, (With<Camera3d>, Without<Player>)>,
) {
    let Ok(camera_transform) = camera_query.single() else { return; };
    let Ok((mut transform, mut velocity)) = player_query.single_mut() else { return; };

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

fn camera_orbit(
    mut mouse_motion_events: MessageReader<MouseMotion>,
    player_query: Query<&Transform, (With<Player>, Without<CameraRig>)>,
    mut camera_query: Query<(&mut Transform, &mut CameraRig), Without<Player>>,
) {
    let Ok(player_transform) = player_query.single() else { return; };
    let player_pos = player_transform.translation;
    let target_focus = player_pos + Vec3::Y * 0.5;

    let mut delta = Vec2::ZERO;
    for event in mouse_motion_events.read() {
        delta += event.delta;
    }

    for (mut cam_transform, mut rig) in &mut camera_query {
        // Cursor is locked to the window (see grab_mouse_cursor), so just
        // follow raw mouse movement every frame — no need to hold a button
        // down first, same as most 3rd-person open-world cameras.
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