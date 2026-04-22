use bevy::{
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

pub struct ShaderPlugin;

impl Plugin for ShaderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ComputeShaderPipelinePlugin,
            ExtractResourcePlugin::<ShaderStorageBufferHandle>::default(),
            ExtractResourcePlugin::<ErosionUniforms>::default(),
            ExtractResourcePlugin::<ImageHandle>::default(),
            MaterialPlugin::<TerrainMaterial>::default(),
        ))
        .add_systems(Startup, shader_setup)
        .add_systems(
            Update,
            image_loaded_observer.run_if(in_state(crate::AppState::LoadingAssets)),
        )
        .add_systems(
            Update,
            change_erosion_uniform_resource.run_if(in_state(crate::AppState::Running)),
        );
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

//////////
/// TODO: Figure out where to put these structs
#[derive(Resource, ExtractResource, Clone, Deref)]
pub struct ImageHandle(pub Handle<Image>);

// Holds handle to the SSBO
#[derive(Resource, ExtractResource, Clone)]
pub struct ShaderStorageBufferHandle(pub Handle<ShaderStorageBuffer>);

// This struct defines the data that will be passed to your shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[storage(0)]
    pub buffer_handle: Handle<ShaderStorageBuffer>,
}

//////////

/// Plugin for setting up the render node
/// TODO: Possibly move to a separate file
struct ComputeShaderPipelinePlugin;

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

#[derive(Resource, Clone, ExtractResource, ShaderType, Reflect)]
#[reflect(Resource)]
struct ErosionUniforms {
    foo: u32,
    cell_size: Vec2,
}

impl Material for TerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }
}

/// Sets up everything related to shaders
fn shader_setup(
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    asset_server: Res<AssetServer>,
) {
    // Create a (storage) buffer with some dummy data
    let buffer: Vec<u32> = (0..BUFFER_LEN as u32).collect();
    let shader_storage_buffer = ShaderStorageBuffer::from(buffer);
    let buffer_handle = buffers.add(shader_storage_buffer);
    commands.insert_resource(ShaderStorageBufferHandle(buffer_handle.clone()));

    let texture_handle: Handle<Image> = asset_server.load("heightfields/mountains.png");
    commands.insert_resource(ImageHandle(texture_handle));

    // TODO: This is hack to insert a dummy, temporary resource so that `prepare_bind_group` works
    commands.insert_resource(ErosionUniforms {
        foo: 5,
        cell_size: Vec2::ONE,
    });
}

/// Waits until the texture asset is loaded and changes the app state
fn image_loaded_observer(
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<ImageHandle>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    if images.get_mut(&image_handle.0).is_some() {
        s_next_app_state.set(crate::AppState::GeneratingTerrain);
    } else {
        info!("image not yet loaded...");
    }
}

fn change_erosion_uniform_resource(
    mut r_erosion_uniform: ResMut<ErosionUniforms>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        r_erosion_uniform.foo += 100;
    }
}

impl Plugin for ComputeShaderPipelinePlugin {
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

impl ComputeShaderPipelinePlugin {
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

        // Load the compute shader
        let compute_shader = asset_server.load(COMPUTE_SHADER_PATH);

        // Make a new compute pipeline
        let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("Our pipeline".into()),
            layout: vec![layout.clone()],
            shader: compute_shader.clone(),
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

        // TODO: Move this write to an earlier step?
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
