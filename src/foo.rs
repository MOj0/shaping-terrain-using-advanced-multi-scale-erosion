use bevy::{
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
    },
};

const GRID_WIDTH: u32 = 128;
const GRID_HEIGHT: u32 = 128;

#[derive(Resource)]
struct TerrainBuffer(Buffer);

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // ------------------------------------------------------------
    // Create storage buffer for terrain vertices
    // ------------------------------------------------------------
    let vertex_count = (GRID_WIDTH * GRID_HEIGHT) as usize;
    let buffer_size = (vertex_count * std::mem::size_of::<[f32; 3]>()) as u64;

    let terrain_buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("terrain_buffer"),
        size: buffer_size,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    commands.insert_resource(TerrainBuffer(terrain_buffer.clone()));

    // ------------------------------------------------------------
    // For now: fake compute output with CPU-generated data
    // Replace this section with actual compute dispatch later
    // ------------------------------------------------------------
    let mut positions = Vec::new();

    for z in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let xf = x as f32;
            let zf = z as f32;

            let y = (xf * 0.1).sin() * (zf * 0.1).cos() * 4.0;

            positions.push([xf, y, zf]);
        }
    }

    // Upload to GPU buffer (simulating compute output)
    // TODO: Do this
    // render_queue.write_buffer(&terrain_buffer, 0, bytemuck::cast_slice(&positions));

    // ------------------------------------------------------------
    // Create mesh from same data
    // ------------------------------------------------------------
    let indices = generate_grid_indices(GRID_WIDTH, GRID_HEIGHT);

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );

    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; vertex_count]);

    terrain_mesh.insert_indices(Indices::U32(indices));

    // ------------------------------------------------------------
    // Spawn terrain
    // ------------------------------------------------------------
    commands.spawn((
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial::from_color(Color::srgb(0.3, 0.7, 0.3)))),
    ));

    // // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(64.0, 50.0, 140.0).looking_at(Vec3::new(64.0, 0.0, 64.0), Vec3::Y),
    ));

    // // Light
    // commands.spawn((
    //     DirectionalLight {
    //         color: Color::srgb(0.1, 0.1, 0.9),
    //         ..default()
    //     },
    //     Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, -0.5, 0.0)),
    // ));
}

fn generate_grid_indices(width: u32, height: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    for z in 0..height - 1 {
        for x in 0..width - 1 {
            let i = z * width + x;

            indices.extend_from_slice(&[i, i + width, i + 1, i + 1, i + width, i + width + 1]);
        }
    }

    indices
}
