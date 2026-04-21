use bevy::{
    asset::RenderAssetUsages,
    ecs::schedule::common_conditions,
    mesh::{Indices, Mesh},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{binding_types, *},
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::{GpuShaderStorageBuffer, ShaderStorageBuffer},
    },
    shader::ShaderRef,
};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ComputeAndVertexPlugin,
            ExtractResourcePlugin::<ShaderStorageBufferHandle>::default(),
            ExtractResourcePlugin::<ErosionUniforms>::default(),
            ExtractResourcePlugin::<ImageHandle>::default(),
            MaterialPlugin::<TerrainMaterial>::default(),
        ))
        .add_message::<ImageLoaded>()
        .add_systems(Startup, terrain_setup)
        .add_systems(
            Update,
            image_loaded_observer.run_if(common_conditions::not(
                common_conditions::resource_exists::<InitMessageSent>,
            )),
        )
        .add_systems(
            Update,
            init_terrain.run_if(common_conditions::on_message::<ImageLoaded>),
        )
        .add_systems(Update, change_erosion_uniform_resource);
    }
}

/// Path to the compute shader
const COMPUTE_SHADER_PATH: &str = "shaders/terrain_compute.wgsl";

/// Path to the render shader
const RENDER_SHADER_PATH: &str = "shaders/terrain_render.wgsl";

/// Size (width/height) of the texture
/// TODO: This shouldn't be hardcoded
const TEXTURE_SIZE: usize = 256;

/// Length of the buffer sent to the GPU
const BUFFER_LEN: usize = TEXTURE_SIZE * TEXTURE_SIZE;

/// Plugin for setting up the render node
struct ComputeAndVertexPlugin;

/// Resource containing the shader bind group
#[derive(Resource)]
struct GpuBufferBindGroup(BindGroup);

/// Compute shader pipeline
#[derive(Resource)]
struct ComputePipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
}

/// Label to identify the node in the render graph
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

/// The node that will execute the compute shader
#[derive(Default)]
struct ComputeNode {}

#[derive(Resource, ExtractResource, Clone)]
struct ImageHandle(Handle<Image>);

#[derive(Message, Default)]
struct ImageLoaded;

#[derive(Resource)]
struct InitMessageSent;

// Holds handle to the SSBO
#[derive(Resource, ExtractResource, Clone)]
struct ShaderStorageBufferHandle(Handle<ShaderStorageBuffer>);

#[derive(Resource, Clone, ExtractResource, ShaderType, Reflect)]
#[reflect(Resource)]
struct ErosionUniforms {
    foo: u32,
    cell_size: Vec2,
}

// This struct defines the data that will be passed to your shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct TerrainMaterial {
    #[storage(0)]
    buffer_handle: Handle<ShaderStorageBuffer>,
}

impl Material for TerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }
}

fn terrain_setup(
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    asset_server: Res<AssetServer>,
) {
    // Create a (storage) buffer with some dummy data
    let buffer: Vec<u32> = (0..BUFFER_LEN as u32).collect();
    let shader_storage_buffer = ShaderStorageBuffer::from(buffer);
    let buffer_handle = buffers.add(shader_storage_buffer);
    commands.insert_resource(ShaderStorageBufferHandle(buffer_handle.clone()));

    let image_handle: Handle<Image> = asset_server.load("heightfields/mountains.png");
    commands.insert_resource(ImageHandle(image_handle));

    // TODO: This is hack to insert a dummy, temporary resource so that `prepare_bind_group` works
    commands.insert_resource(ErosionUniforms {
        foo: 5,
        cell_size: Vec2::ONE,
    })
}

fn image_loaded_observer(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<ImageHandle>,
    mut message_writer: MessageWriter<ImageLoaded>,
) {
    if images.get_mut(&image_handle.0).is_some() {
        message_writer.write_default();
        commands.insert_resource(InitMessageSent);
    } else {
        info!("image not yet loaded...");
    }
}

fn init_terrain(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<ImageHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    r_buffer_handle: Res<ShaderStorageBufferHandle>,
) {
    let Some(image) = images.get_mut(&image_handle.0) else {
        info!("image not yet loaded...");
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
    let terrain_material_handle = materials.add(TerrainMaterial {
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

    // TODO: Is this inserted at all?
    // // Insert the uniform resource
    // commands.insert_resource(ErosionUniforms {
    //     foo: 500,
    //     cell_size,
    // });
}

impl Plugin for ComputeAndVertexPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(
                RenderStartup,
                (
                    Self::init_compute_pipeline,
                    Self::add_compute_render_graph_node,
                ),
            )
            .add_systems(
                Render,
                Self::prepare_bind_group
                    .in_set(RenderSystems::PrepareBindGroups)
                    .run_if(not(resource_exists::<GpuBufferBindGroup>)),
            );
    }
}

impl ComputeAndVertexPlugin {
    fn init_compute_pipeline(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        pipeline_cache: Res<PipelineCache>,
    ) {
        // Make a descriptor for the bind group
        let layout = BindGroupLayoutDescriptor::new(
            "",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<u32>>(false),
                    binding_types::uniform_buffer::<ErosionUniforms>(false),
                ),
            ),
        );

        // Load the shader
        let shader = asset_server.load(COMPUTE_SHADER_PATH);

        // Make a new compute pipeline
        let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("Our pipeline".into()),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            ..default()
        });

        // We will use this when writing the render code in the Render Graph's Node
        commands.insert_resource(ComputePipeline { layout, pipeline });
    }

    fn add_compute_render_graph_node(mut render_graph: ResMut<RenderGraph>) {
        // Add the compute node as a top-level node to the render graph.
        // This means it will only get executed once per frame.
        render_graph.add_node(ComputeNodeLabel, ComputeNode::default());
    }

    fn prepare_bind_group(
        mut commands: Commands,
        r_pipeline: Res<ComputePipeline>,
        r_render_device: Res<RenderDevice>,
        r_pipeline_cache: Res<PipelineCache>,
        r_custom_material_handle: Res<ShaderStorageBufferHandle>,
        r_gpu_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>, // NOTE: GpuShaderStorageBuffer implements the RenderAsset trait
        r_queue: Res<RenderQueue>,
        r_erosion_uniform_buffer: Res<ErosionUniforms>,
    ) {
        // Get the SSBO with the ShaderStorageBufferHandle
        let buffer = r_gpu_buffers.get(&r_custom_material_handle.0).unwrap();

        // TODO: Move this write to an earlier step
        let mut uniform_buffer = UniformBuffer::from(r_erosion_uniform_buffer.into_inner());
        uniform_buffer.write_buffer(&r_render_device, &r_queue);

        let bind_group = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.layout),
            &BindGroupEntries::sequential((
                buffer.buffer.as_entire_buffer_binding(),
                &uniform_buffer,
            )),
        );

        // We will use this when writing the render code in the Render Graph's Node
        commands.insert_resource(GpuBufferBindGroup(bind_group));
    }
}

impl render_graph::Node for ComputeNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();
        let bind_group = world.resource::<GpuBufferBindGroup>();

        if let Some(init_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline) {
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("our pipeline"),
                        ..default()
                    });

            let dispatch_size = (TEXTURE_SIZE / 8) as u32;

            pass.set_bind_group(0, &bind_group.0, &[]);
            pass.set_pipeline(init_pipeline);
            pass.dispatch_workgroups(dispatch_size, dispatch_size, 1);
        }

        Ok(())
    }
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

fn change_erosion_uniform_resource(
    mut r_erosion_uniform: ResMut<ErosionUniforms>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        r_erosion_uniform.foo += 100;
    }
}
