use bevy::{
    asset::RenderAssetUsages,
    input::common_conditions,
    mesh::VertexAttributeValues,
    mesh::{Indices, Mesh},
    prelude::*,
    render::extract_resource::ExtractResource,
    render::gpu_readback::{Readback, ReadbackComplete},
    render::render_resource::*,
    render::storage::ShaderStorageBuffer,
};
use rand::RngExt;
use std::fs::File;
use std::io::{BufWriter, Write};

use crate::shaders;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GPUData>()
            .init_resource::<TerrainConfig>()
            .add_systems(
                Update,
                init_terrain.run_if(in_state(crate::AppState::GeneratingTerrain)),
            )
            .add_systems(
                Update,
                upsample_terrain.run_if(
                    in_state(crate::AppState::Running)
                        .and(common_conditions::input_just_pressed(KeyCode::KeyU)),
                ),
            )
            .add_systems(
                Update,
                change_terrain.run_if(
                    in_state(crate::AppState::Running)
                        .and(common_conditions::input_just_pressed(KeyCode::KeyG)),
                ),
            )
            .add_systems(
                Update,
                write_res_data_to_file.run_if(common_conditions::input_just_pressed(KeyCode::KeyS)),
            )
            .add_systems(Update, change_terrain_config)
            .add_systems(
                Update,
                synchronize_gpu_positions.run_if(
                    bevy::time::common_conditions::on_timer(std::time::Duration::from_secs_f32(
                        0.35,
                    ))
                    .and(resource_exists_and_equals(crate::DebugConfig {
                        // NOTE: If we add more fields and change their default value, this will not work...
                        is_wireframe_on: true,
                        ..default()
                    })),
                ),
            );
    }
}

#[derive(Resource, Reflect, PartialEq, Clone, Debug, ExtractResource)]
#[reflect(Resource)]
pub struct TerrainConfig {
    pub run_erosion: bool,
    pub run_deposition: bool,
    pub run_thermal: bool,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            run_erosion: false,
            run_deposition: false,
            run_thermal: false,
        }
    }
}

#[derive(Component)]
struct Terrain;

/// Helper resource used for deubgging purposes
/// Also used for getting back heights from the GPU (currently used when upsampling)
#[derive(Resource, Default, Reflect, Debug)]
#[reflect(Resource)]
struct GPUData {
    written_to_file: bool,
    debug_data: Vec<f32>,
    vertex_positions: Vec<Vec3>,
}

impl GPUData {
    fn get_heights(&self) -> Vec<f32> {
        self.vertex_positions
            .iter()
            .map(|vertex| vertex.y)
            .collect()
    }
}

// TODO: Split this function into 2. One should just compute the `heights` and set the resource. Other one should do the rest.
fn init_terrain(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<shaders::ImageHandle>,
    r_shader_config: Res<shaders::ShaderConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<shaders::TerrainMaterial>>,
    mut r_ssbo_handles: ResMut<shaders::ComputeSSBOHandles>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    let Some(image) = images.get_mut(&image_handle.0) else {
        error!("image not yet loaded...");
        return;
    };

    let texture_size = r_shader_config.texture_size;

    // TODO: Parameterize this somehow
    let height_scale = 3000.0; // Source: from the original size in C++ implementation
    let width_scale = 20000.0 / (texture_size - 1) as f32; // Source: from the original size in C++ implementation: celldiagonal = Vector2((b[0] - a[0]) / (nx - 1), (b[1] - a[1]) / (ny - 1));

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

    let (positions, indices) = generate_terrain(&heights, texture_size, width_scale);

    // NOTE: Overwrite the existing dummy handles with ones pointing to the actual data
    r_ssbo_handles.height_a = shaders::prepare_ssbo(&mut buffers, heights.clone());

    // // Print data from the CPU for debugging
    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.height_a.clone()))
    //     .observe(|event: On<ReadbackComplete>| {
    //         let heights_a: Vec<f32> = event.to_shader_type();
    //         info!("A: Terrain [0..10] {:?}", &heights_a[0..10]);
    //     });

    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.height_b.clone()))
    //     .observe(|event: On<ReadbackComplete>| {
    //         let data: Vec<f32> = event.to_shader_type();
    //         info!("B: heights[0..10] {:?}", &data[0..10]);
    //     });

    // // TODO: stream_a is also the output buffer half the time
    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.stream_a.clone()))
    //     .observe(print_output_stream_buffer::<'A'>);

    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.stream_b.clone()))
    //     .observe(print_output_stream_buffer::<'B'>);

    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.debug_a.clone()))
    //     .observe(print_output_stream_buffer::<'1'>);
    // commands
    //     .spawn(Readback::buffer(r_ssbo_handles.debug_b.clone()))
    //     .observe(print_output_stream_buffer::<'2'>);

    commands
        .spawn(Readback::buffer(r_ssbo_handles.vertex_positions.clone()))
        .observe(store_gpu_positions);

    // Create the custom material and add it to the materials assets
    let terrain_material_handle = materials.add(shaders::TerrainMaterial {
        height_buffer_handle: r_ssbo_handles.height_a.clone(),
        positions_buffer_handle: r_ssbo_handles.vertex_positions.clone(),
    });

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    terrain_mesh.insert_indices(Indices::U32(indices));
    terrain_mesh.compute_normals();

    // Spawn the terrain
    commands.spawn((
        Terrain,
        Name::new("terrain"),
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(terrain_material_handle.clone()),
        Transform::default(),
    ));

    // Overwrite the dummy default values for the uniforms
    // TODO: Make this nicer...
    let a = &positions[0];
    let b = &positions[texture_size * texture_size - 1];
    // NOTE: We take the `x` and `z` coordinate here, since that is what forms the surface of the grid, `y` is up
    let a = Vec2::new(a[0], a[2]);
    let b = Vec2::new(b[0], b[2]);
    let cell_size = (b - a) / (texture_size - 1) as f32;

    // Insert uniforms
    commands.insert_resource(shaders::ErosionUniforms {
        nx: texture_size as i32,
        ny: texture_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });
    commands.insert_resource(shaders::DepositionUniforms {
        nx: texture_size as i32,
        ny: texture_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });
    commands.insert_resource(shaders::ThermalUniforms {
        nx: texture_size as i32,
        ny: texture_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });

    info!(?cell_size, ?a, ?b, "inserted uniforms");

    s_next_app_state.set(crate::AppState::Running);
}

// FIXME: Fix bug where terrain moves when upsampled and there is a flickering wall on the side.
fn upsample_terrain(
    s_terrain: Single<Entity, With<Terrain>>,
    r_gpu_data: Res<GPUData>,
    mut commands: Commands,
    mut r_shader_config: ResMut<shaders::ShaderConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<shaders::TerrainMaterial>>,
    mut r_buffer_handles: ResMut<shaders::ComputeSSBOHandles>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
) {
    let old_heights = &r_gpu_data.get_heights();
    let texture_size = r_shader_config.texture_size;

    // let _ = write_vec_to_file("old_heights.txt", old_heights);

    // 2x upsampling
    let double_size = texture_size * 2;
    let heights = resize_heightmap(old_heights, texture_size, double_size);

    // let _ = write_vec_to_file("heights.txt", &heights);

    let width_scale = 20000.0 / (double_size - 1) as f32; // Source: from the original size in C++ implementation: celldiagonal = Vector2((b[0] - a[0]) / (nx - 1), (b[1] - a[1]) / (ny - 1));
    let (positions, indices) = generate_terrain(&heights, double_size, width_scale);

    // NOTE: Overwrite the existing handle with one pointing to updated data
    r_buffer_handles.height_a = shaders::prepare_ssbo(&mut buffers, heights.clone());

    // Reinitialize buffers with new size
    let buffer_size = double_size * double_size;
    r_buffer_handles.height_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.stream_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.stream_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.sed_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.sed_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.debug_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.debug_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_buffer_handles.vertex_positions =
        shaders::prepare_ssbo(&mut buffers, vec![Vec3::ZERO; buffer_size]);

    // Override the observer
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

    commands.entity(s_terrain.into_inner()).insert((
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(terrain_material_handle.clone()),
    ));

    // Overwrite the dummy default values for the uniforms
    // TODO: Make this nicer...
    let a = &positions[0];
    let b = &positions[double_size * double_size - 1];
    // NOTE: We take the `x` and `z` coordinate here, since that is what forms the surface of the grid, `y` is up
    let a = Vec2::new(a[0], a[2]);
    let b = Vec2::new(b[0], b[2]);
    let cell_size = (b - a) / (double_size - 1) as f32;

    // NOTE: Overwrite the dummy parameters
    commands.insert_resource(shaders::ErosionUniforms {
        nx: double_size as i32,
        ny: double_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });
    commands.insert_resource(shaders::DepositionUniforms {
        nx: double_size as i32,
        ny: double_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });
    commands.insert_resource(shaders::ThermalUniforms {
        nx: double_size as i32,
        ny: double_size as i32,
        cell_size,
        a,
        b,
        ..default()
    });

    r_shader_config.texture_size = double_size;

    info!(?cell_size, ?a, ?b, "updated uniforms");
}

fn change_terrain(
    s_terrain: Single<Entity, With<Terrain>>,
    mut commands: Commands,
    r_shader_config: Res<shaders::ShaderConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<shaders::TerrainMaterial>>,
    mut r_ssbo_handles: ResMut<shaders::ComputeSSBOHandles>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
) {
    let texture_size = r_shader_config.texture_size;
    let width_scale = 20000.0 / (texture_size - 1) as f32; // Source: from the original size in C++ implementation: celldiagonal = Vector2((b[0] - a[0]) / (nx - 1), (b[1] - a[1]) / (ny - 1));

    let heights = generate_dummy_heights(texture_size);
    let (positions, indices) = generate_terrain(&heights, texture_size, width_scale);

    // NOTE: Overwrite ssbo handle with the one pointing to the new data
    r_ssbo_handles.height_a = shaders::prepare_ssbo(&mut buffers, heights.clone());

    // Reset the rest of SSBO resource
    let buffer_size = heights.len();
    r_ssbo_handles.height_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.stream_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.stream_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.sed_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.sed_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.debug_a = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);
    r_ssbo_handles.debug_b = shaders::prepare_ssbo(&mut buffers, vec![0.0; buffer_size]);

    // NOTE: We should NOT override the vertex_positions because GPU might be writing into it. Causes weird bugs...
    // r_ssbo_handles.vertex_positions =
    //     shaders::prepare_ssbo(&mut buffers, vec![Vec3::ZERO; buffer_size]);

    // Create the custom material and add it to the materials assets
    let terrain_material_handle = materials.add(shaders::TerrainMaterial {
        height_buffer_handle: r_ssbo_handles.height_a.clone(),
        positions_buffer_handle: r_ssbo_handles.vertex_positions.clone(),
    });

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    terrain_mesh.insert_indices(Indices::U32(indices));
    terrain_mesh.compute_normals();

    commands.entity(s_terrain.into_inner()).insert((
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(terrain_material_handle.clone()),
    ));
}

fn generate_terrain(
    height_data: &Vec<f32>,
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

fn generate_dummy_heights(size: usize) -> Vec<f32> {
    let mut heights = vec![0.0; size * size];
    let mut rng = rand::rng();

    // Generate vertices
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            heights[i] = rng.random_range(300.0..1000.0);
        }
    }

    heights
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
    if BUFFER_IDENT == 'D' {
        r_gpu_data.debug_data = stream_data.clone();
    }

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
    let gpu_event_data: Vec<Vec3> = event.to_shader_type();
    r_gpu_data.vertex_positions = gpu_event_data.clone();
}

fn write_res_data_to_file(r: Res<GPUData>) -> Result {
    write_vec_to_file("debug_buffer.txt", &r.debug_data)
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

            mesh.compute_normals();
        }
    }
}

fn change_terrain_config(
    mut r_terrain_config: ResMut<TerrainConfig>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::KeyE) {
        r_terrain_config.run_erosion = !r_terrain_config.run_erosion;
    }
    if keyboard.just_pressed(KeyCode::KeyD) {
        r_terrain_config.run_deposition = !r_terrain_config.run_deposition;
    }
    if keyboard.just_pressed(KeyCode::KeyT) {
        r_terrain_config.run_thermal = !r_terrain_config.run_thermal;
    }
}

fn sample_bilinear(heights: &[f32], size: usize, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;

    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);

    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    // Access values from flat array
    let v00 = heights[y0 * size + x0];
    let v10 = heights[y0 * size + x1];
    let v01 = heights[y1 * size + x0];
    let v11 = heights[y1 * size + x1];

    // Bilinear interpolation
    let a = v00 * (1.0 - tx) + v10 * tx;
    let b = v01 * (1.0 - tx) + v11 * tx;

    a * (1.0 - ty) + b * ty
}

fn resize_heightmap(heights: &[f32], old_size: usize, new_size: usize) -> Vec<f32> {
    let mut result = vec![0.0; new_size * new_size];

    for y in 0..new_size {
        for x in 0..new_size {
            // Map destination pixel into source space
            let src_x = (x as f32 / (new_size - 1) as f32) * (old_size - 1) as f32;
            let src_y = (y as f32 / (new_size - 1) as f32) * (old_size - 1) as f32;

            result[y * new_size + x] = sample_bilinear(heights, old_size, src_x, src_y);
        }
    }

    result
}

fn write_vec_to_file<T: std::fmt::Display>(filename: &str, vec: &Vec<T>) -> Result {
    let file = File::create(filename)?;
    let mut writer = BufWriter::new(file);

    for value in vec {
        writeln!(writer, "{}", value)?;
    }

    Ok(())
}
