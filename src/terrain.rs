use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, Mesh},
    prelude::*,
    render::gpu_readback::{Readback, ReadbackComplete},
    render::render_resource::*,
    render::storage::ShaderStorageBuffer,
};

use crate::shaders;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            init_terrain.run_if(in_state(crate::AppState::GeneratingTerrain)),
        );
    }
}

fn init_terrain(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<shaders::ImageHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<shaders::TerrainMaterial>>,
    mut r_buffer_handles: ResMut<shaders::ComputeSSBOHandles>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
) {
    let Some(image) = images.get_mut(&image_handle.0) else {
        error!("image not yet loaded...");
        return;
    };

    assert_eq!(image.width(), image.height());

    // TODO: Parameterize this somehow
    let height_scale = 3000.0;

    // Collect the values of the texture into a vector.
    // Range of heights is [0..1]
    let heights: Vec<f32> = match &image.data {
        Some(data) => data
            .iter()
            .step_by(4)
            .map(|texture_value| *texture_value as f32 / 256.0 * height_scale)
            .collect(),
        None => panic!("whoops, no data"),
    };

    let width_scale = 78.0;
    let image_size = image.width() as usize;
    let (positions, indices) = generate_terrain(heights.clone(), image_size, width_scale);

    // NOTE: Overwrite the existing dummy handles with ones pointing to the actual data
    r_buffer_handles.height_a = shaders::prepare_ssbo(&mut buffers, heights.clone());

    // Print data from the CPU for debugging
    commands
        .spawn(Readback::buffer(r_buffer_handles.height_a.clone()))
        .observe(|event: On<ReadbackComplete>| {
            let heights_a: Vec<f32> = event.to_shader_type();
            info!("A: Terrain [0..10] {:?}", &heights_a[0..10]);
        });

    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.height_b.clone()))
    //     .observe(|event: On<ReadbackComplete>| {
    //         let data: Vec<f32> = event.to_shader_type();
    //         info!("B: heights[0..10] {:?}", &data[0..10]);
    //     });

    // TODO: stream_a is also the output buffer half the time
    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.stream_a.clone()))
    //     .observe(print_stream_buffer);

    commands
        .spawn(Readback::buffer(r_buffer_handles.stream_b.clone()))
        .observe(print_output_stream_buffer);

    // Create the custom material and add it to the materials assets
    let terrain_material_handle = materials.add(shaders::TerrainMaterial {
        height_buffer_handle: r_buffer_handles.height_a.clone(),
    });

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    terrain_mesh.insert_indices(Indices::U32(indices));
    terrain_mesh.compute_normals();

    // Spawn the plane
    commands.spawn((
        Name::new("plane"),
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(terrain_material_handle.clone()),
        Transform::from_translation(Vec3::new(-4.0, -1.0, 0.0)),
    ));

    // Overwrite the dummy default values for the uniforms
    // TODO: Make this nicer...
    let a = &positions[0];
    let b = &positions[image_size * image_size - 1];
    // NOTE: We take the `x` and `z` coordinate here, since that is what forms the surface of the grid, `y` is up
    let a = Vec2::new(a[0], a[2]);
    let b = Vec2::new(b[0], b[2]);
    let cell_size = compute_cell_size(&positions, image_size);

    // NOTE: Overwrite the dummy parameters
    commands.insert_resource(shaders::ErosionUniforms {
        cell_size,
        a,
        b,
        ..default()
    });

    info!(?cell_size, ?a, ?b, "updated params");

    s_next_app_state.set(crate::AppState::Running);
}

fn generate_terrain(
    height_data: Vec<f32>,
    size: usize,
    width_scale: f32,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut positions = Vec::with_capacity(size * size);
    let mut indices = Vec::new();

    // Generate vertices
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let height = height_data[i];

            positions.push([x as f32 * width_scale, height, z as f32 * width_scale]);
        }
    }

    // Generate indices (two triangles per quad)
    for z in 0..(size - 1) {
        for x in 0..(size - 1) {
            let i = z * size + x;

            let i0 = i as u32;
            let i1 = (i + 1) as u32;
            let i2 = (i + size) as u32;
            let i3 = (i + size + 1) as u32;

            // Triangle 1
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);

            // Triangle 2
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    (positions, indices)
}

fn compute_cell_size(terrain: &Vec<[f32; 3]>, grid_length: usize) -> Vec2 {
    let a = terrain[0];
    let b = terrain[grid_length * grid_length - 1];
    let cell_size_x = (b[0] - a[0]) / (grid_length - 1) as f32;
    let cell_size_y = (b[2] - a[2]) / (grid_length - 1) as f32;

    Vec2::new(cell_size_x, cell_size_y)
}

/// Prints the stream buffer read from the GPU
/// TODO: The output (e.g. out_stream) buffer changes each frame, so we should make some logic to only print the actual output buffer
fn print_output_stream_buffer(event: On<ReadbackComplete>) {
    let stream_data: Vec<f32> = event.to_shader_type();
    let min = stream_data
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&-1.0);
    let max = stream_data
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&-1.0);

    info!("Stream {:?}, min {}, max {}", &stream_data[0..10], min, max);
}
