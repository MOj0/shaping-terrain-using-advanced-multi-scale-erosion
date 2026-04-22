use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, Mesh},
    prelude::*,
    render::render_resource::*,
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
    r_buffer_handle: Res<shaders::ShaderStorageBufferHandle>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    let Some(image) = images.get_mut(&image_handle.0) else {
        error!("image not yet loaded...");
        return;
    };

    assert_eq!(image.width(), image.height());

    // Collect the values of the texture into a vector.
    // Range of heights is [0..1]
    let heights: Vec<f32> = match &image.data {
        Some(data) => data
            .iter()
            .step_by(4)
            .map(|texture_value| *texture_value as f32 / 256.0)
            .collect(),
        None => panic!("whoops, no data"),
    };

    let image_size = image.width() as usize;
    let (positions, indices) = generate_terrain(heights, image_size, 0.7, 50.0);
    let cell_size = compute_cell_size(&positions, image_size);

    let buffer_handle = &r_buffer_handle.0;

    // Create the custom material and add it to the materials assets
    let terrain_material_handle = materials.add(shaders::TerrainMaterial {
        buffer_handle: buffer_handle.clone(),
    });

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    terrain_mesh.insert_indices(Indices::U32(indices));
    terrain_mesh.compute_normals();

    // Spawn the plane
    commands.spawn((
        Name::new("plane"),
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(terrain_material_handle.clone()),
        Transform::from_translation(Vec3::new(-4.0, -1.0, 0.0)),
    ));

    // TODO: Figure out updaing the uniform in the compute shader (for some reason it doesn't update it)
    // // Insert the uniform resource
    // commands.insert_resource(ErosionUniforms {
    //     foo: 500,
    //     cell_size,
    // });

    s_next_app_state.set(crate::AppState::Running);
}

fn generate_terrain(
    height_data: Vec<f32>,
    size: usize,
    width_scale: f32,
    height_scale: f32,
) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut positions = Vec::with_capacity(size * size);
    let mut indices = Vec::new();

    // Generate vertices
    for z in 0..size {
        for x in 0..size {
            let i = z * size + x;
            let height = height_data[i];

            positions.push([
                x as f32 * width_scale,
                height * height_scale,
                z as f32 * width_scale,
            ]);
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
    debug!(?a, ?b, ?cell_size_x, ?cell_size_y);

    Vec2::new(cell_size_x, cell_size_y)
}
