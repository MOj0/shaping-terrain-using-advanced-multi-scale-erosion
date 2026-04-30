use bevy::{
    asset::RenderAssetUsages,
    input::common_conditions,
    mesh::VertexAttributeValues,
    mesh::{Indices, Mesh},
    prelude::*,
    render::gpu_readback::{Readback, ReadbackComplete},
    render::render_resource::*,
    render::storage::ShaderStorageBuffer,
};
use std::fs::File;
use std::io::{BufWriter, Write};

use crate::shaders;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GPUData>()
            .add_systems(
                Update,
                init_terrain.run_if(in_state(crate::AppState::GeneratingTerrain)),
            )
            .add_systems(
                Update,
                write_res_data_to_file.run_if(common_conditions::input_just_pressed(KeyCode::KeyS)),
            )
            .add_systems(
                Update,
                synchronize_gpu_positions.run_if(
                    bevy::time::common_conditions::on_timer(std::time::Duration::from_secs_f32(
                        0.35,
                    ))
                    .and(resource_exists_and_equals(crate::DebugConfig {
                        is_wireframe_on: true,
                        ..default()
                    })),
                ),
            );
    }
}

/// Helper resource used for deubgging purposes
#[derive(Resource, Default, Reflect, Debug)]
#[reflect(Resource)]
struct GPUData {
    written_to_file: bool,
    data: Vec<f32>,
    vertex_positions: Vec<Vec3>,
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
    let height_scale = 3000.0; // Source: from the original size in C++ implementation
    let width_scale = 20000.0 / 255.0; // Source: from the original size in C++ implementation: celldiagonal = Vector2((b[0] - a[0]) / (nx - 1), (b[1] - a[1]) / (ny - 1));

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

    let image_size = image.width() as usize;
    let (positions, indices) = generate_terrain(heights.clone(), image_size, width_scale);

    // NOTE: Overwrite the existing dummy handles with ones pointing to the actual data
    r_buffer_handles.height_a = shaders::prepare_ssbo(&mut buffers, heights.clone());

    // // Print data from the CPU for debugging
    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.height_a.clone()))
    //     .observe(|event: On<ReadbackComplete>| {
    //         let heights_a: Vec<f32> = event.to_shader_type();
    //         info!("A: Terrain [0..10] {:?}", &heights_a[0..10]);
    //     });

    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.height_b.clone()))
    //     .observe(|event: On<ReadbackComplete>| {
    //         let data: Vec<f32> = event.to_shader_type();
    //         info!("B: heights[0..10] {:?}", &data[0..10]);
    //     });

    // // TODO: stream_a is also the output buffer half the time
    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.stream_a.clone()))
    //     .observe(print_output_stream_buffer::<'A'>);

    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.stream_b.clone()))
    //     .observe(print_output_stream_buffer::<'B'>);

    // commands
    //     .spawn(Readback::buffer(r_buffer_handles.debug.clone()))
    //     .observe(print_output_stream_buffer::<'D'>);

    commands
        .spawn(Readback::buffer(r_buffer_handles.vertex_positions.clone()))
        .observe(store_gpu_positions);

    // Create the custom material and add it to the materials assets
    let terrain_material_handle = materials.add(shaders::TerrainMaterial {
        height_buffer_handle: r_buffer_handles.height_a.clone(),
        positions_buffer_handle: r_buffer_handles.vertex_positions.clone(),
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
        Transform::default(),
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
    commands.insert_resource(shaders::DepositionUniforms {
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
fn print_output_stream_buffer<const BUFFER_IDENT: char>(
    event: On<ReadbackComplete>,
    mut r_gpu_data: ResMut<GPUData>,
) {
    let stream_data: Vec<f32> = event.to_shader_type();

    let min = stream_data
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&-1.0);
    let max = stream_data
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(&-1.0);

    // r_gpu_data.data = stream_data.clone();
    // TODO: Only do this in "debugging mode" (e.g. #cfg(debug  ), #cfg(debug_assertions)), basically a feature flag
    // if BUFFER_IDENT == 'D' {
    // r_file.data = stream_data.clone();
    // }

    info!(
        "{}: Buffer {:?}, min {}, max {}",
        BUFFER_IDENT,
        &stream_data[0..10],
        min,
        max
    );

    if BUFFER_IDENT == 'B' {
        info!("==================\n");
    }
}

fn store_gpu_positions(event: On<ReadbackComplete>, mut r_gpu_data: ResMut<GPUData>) {
    let stream_data: Vec<Vec3> = event.to_shader_type();
    r_gpu_data.vertex_positions = stream_data.clone();
}

fn write_res_data_to_file(r: Res<GPUData>) -> Result {
    let file = File::create("debug_buffer.txt")?;
    let mut writer = BufWriter::new(file);

    for value in &r.data {
        writeln!(writer, "{}", value)?;
    }

    Ok(())
}

/// Synchronizes the positions read from the GPU onto the CPU
/// Needed, so that Wireframe plugin recomputes the mesh correctly
fn synchronize_gpu_positions(
    mut meshes: ResMut<Assets<Mesh>>,
    q_meshes: Query<&Mesh3d>,
    r_gpu_data: Res<GPUData>,
) {
    let gpu_positions = &r_gpu_data.vertex_positions;

    if gpu_positions.len() == 0 {
        return;
    }

    for mesh in &q_meshes {
        if let Some(mesh) = meshes.get_mut(mesh) {
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                assert_eq!(positions.len(), gpu_positions.len());

                for (i, p) in positions.iter_mut().enumerate() {
                    *p = gpu_positions[i].into();
                }
            }
        }
    }
}
