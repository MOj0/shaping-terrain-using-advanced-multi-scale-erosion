use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::encase::internal::WriteInto,
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

/// Path to the erosion compute shader
const COMPUTE_EROSION_SHADER_PATH: &str = "shaders/erosion.wgsl";
/// Path to the deposition compute shader
const COMPUTE_DEPOSITION_SHADER_PATH: &str = "shaders/deposition.wgsl";

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

// Holds handles to the SSBOs used by the compute shader
#[derive(Resource, ExtractResource, Clone)]
pub struct ComputeSSBOHandles {
    pub height_a: Handle<ShaderStorageBuffer>,
    pub height_b: Handle<ShaderStorageBuffer>,
    pub stream_a: Handle<ShaderStorageBuffer>,
    pub stream_b: Handle<ShaderStorageBuffer>,
    pub sed_a: Handle<ShaderStorageBuffer>,
    pub sed_b: Handle<ShaderStorageBuffer>,

    pub debug: Handle<ShaderStorageBuffer>,

    pub vertex_positions: Handle<ShaderStorageBuffer>,
}

// This struct defines the data that will be passed to your shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[storage(0)]
    pub height_buffer_handle: Handle<ShaderStorageBuffer>,
    #[storage(1)]
    pub positions_buffer_handle: Handle<ShaderStorageBuffer>,
}

#[derive(Resource, Clone, ExtractResource, ShaderType, Reflect)]
#[reflect(Resource)]
pub struct ErosionUniforms {
    pub nx: i32,
    pub ny: i32,
    // NOTE: Opposite corners of the grid
    pub a: Vec2, // TODO: Rename to something like grid_corner1
    pub b: Vec2, // TODO: Rename to something like grid_corner2
    pub cell_size: Vec2,

    pub flow_p: f32,
    pub k: f32,
    pub p_sa: f32,
    pub p_sl: f32,
    pub dt: f32,
    pub max_spe: f32,

    pub debug: f32,
}

impl Default for ErosionUniforms {
    fn default() -> Self {
        Self {
            nx: TEXTURE_SIZE as i32, // TODO: Should probably not be hardcoded
            ny: TEXTURE_SIZE as i32, // TODO: Should probably not be hardcoded
            a: Vec2::ZERO, //NOTE: This will be overwritten by the actual corners of the grid
            b: Vec2::ONE,  //NOTE: This will be overwritten by the actual corners of the grid
            cell_size: Vec2::ZERO, // NOTE: This will be overwritten by the actual size of the cell
            flow_p: 1.3,
            k: 0.0005,
            p_sa: 0.8,
            p_sl: 2.0,
            dt: 1.0,
            max_spe: 10000.0,

            debug: 1.0,
        }
    }
}

#[derive(Resource, Clone, ExtractResource, ShaderType, Reflect)]
#[reflect(Resource)]
pub struct DepositionUniforms {
    pub nx: i32,
    pub ny: i32,
    // NOTE: Opposite corners of the grid
    pub a: Vec2, // TODO: Rename to something like grid_corner1
    pub b: Vec2, // TODO: Rename to something like grid_corner2
    pub cell_size: Vec2,
    pub deposition_strength: f32,
    pub rain: f32,
    pub flow_p: f32,
    pub p_sa: f32,
    pub p_sl: f32,

    pub debug: f32,
}

impl Default for DepositionUniforms {
    fn default() -> Self {
        Self {
            nx: TEXTURE_SIZE as i32, // TODO: Should probably not be hardcoded
            ny: TEXTURE_SIZE as i32, // TODO: Should probably not be hardcoded
            a: Vec2::ZERO, //NOTE: This will be overwritten by the actual corners of the grid
            b: Vec2::ONE,  //NOTE: This will be overwritten by the actual corners of the grid
            cell_size: Vec2::ZERO, // NOTE: This will be overwritten by the actual size of the cell
            deposition_strength: 1.0,
            rain: 2.6,
            flow_p: 1.3,
            p_sa: 0.8,
            p_sl: 2.0,

            debug: 1.0,
        }
    }
}

//////////

/// Plugin for setting up the render node
/// TODO: Possibly move to a separate file
struct ComputeShaderPipelinePlugin;

/// Resource containing the shader bind groups
/// We need 2 bind groups to do dual buffering
#[derive(Resource)]
struct ComputeBindGroups([BindGroup; 2]);

/// Compute shader pipeline
#[derive(Resource)]
struct ComputePipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline_id: CachedComputePipelineId,
}

/// Label to identify the node in the render graph
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

/// The node that will execute the compute shader
#[derive(Default)]
struct ComputeNode;

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
    // Prepare SSBOs for the compute shader
    commands.insert_resource(ComputeSSBOHandles {
        height_a: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),
        height_b: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),
        stream_a: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),
        stream_b: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),
        sed_a: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),
        sed_b: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),

        debug: prepare_ssbo(&mut buffers, vec![0.0; BUFFER_LEN]),

        vertex_positions: prepare_ssbo(&mut buffers, vec![Vec3::ZERO; BUFFER_LEN]),
    });

    // Load in the heightfield texture
    let texture_handle: Handle<Image> = asset_server.load("heightfields/mountains.png");
    commands.insert_resource(ImageHandle(texture_handle));

    // Insert the uniforms
    commands.insert_resource(ErosionUniforms::default());
    commands.insert_resource(DepositionUniforms::default());
}

/// Waits until the texture asset is loaded and changes the app state
fn image_loaded_observer(
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<ImageHandle>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    if images.get_mut(&image_handle.0).is_some() {
        s_next_app_state.set(crate::AppState::GeneratingTerrain);
        info!("Texture loaded!");
    } else {
        info!("image not yet loaded...");
    }
}

fn change_erosion_uniform_resource(
    mut r_erosion_uniform: ResMut<ErosionUniforms>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        r_erosion_uniform.debug += 1.0;
    }
    if mouse.just_pressed(MouseButton::Right) {
        r_erosion_uniform.debug -= 1.0;
    }
}

impl Plugin for ComputeShaderPipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<ComputeSSBOHandles>::default(),
            ExtractResourcePlugin::<ErosionUniforms>::default(),
            ExtractResourcePlugin::<DepositionUniforms>::default(),
            ExtractResourcePlugin::<ImageHandle>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, Self::init_compute_pipeline)
            .add_systems(
                // NOTE: This is done **every** render frame, in the PrepareBindGroups stage
                Render,
                Self::prepare_bind_group.in_set(RenderSystems::PrepareBindGroups),
            );

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(ComputeNodeLabel, ComputeNode::default());
        render_graph.add_node_edge(ComputeNodeLabel, bevy::render::graph::CameraDriverLabel);
    }
}

impl ComputeShaderPipelinePlugin {
    /// Defines the compute layout and prepares the compute pipeline
    fn init_compute_pipeline(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        pipeline_cache: Res<PipelineCache>,
    ) {
        // Make a descriptor for the erosion shader bind group
        let erosion_compute_layout = BindGroupLayoutDescriptor::new(
            "erosion",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::uniform_buffer::<ErosionUniforms>(false),
                ),
            ),
        );

        let deposition_compute_layout = BindGroupLayoutDescriptor::new(
            "deposition",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::storage_buffer::<Vec<f32>>(false),
                    binding_types::uniform_buffer::<DepositionUniforms>(false),
                ),
            ),
        );

        // Load the compute shader
        let compute_shader = asset_server.load(COMPUTE_DEPOSITION_SHADER_PATH);

        // Make a new compute pipeline
        let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("Our pipeline".into()),
            layout: vec![deposition_compute_layout.clone()],
            shader: compute_shader.clone(),
            ..default()
        });

        // We will use this when writing the render code in the Render Graph's Node
        commands.insert_resource(ComputePipeline {
            layout: deposition_compute_layout,
            pipeline_id: pipeline,
        });
    }

    /// Creates the bind group according to the layout and loads in the uniform data
    fn prepare_bind_group(
        mut commands: Commands,
        r_pipeline: Res<ComputePipeline>,
        r_render_device: Res<RenderDevice>,
        r_pipeline_cache: Res<PipelineCache>,
        ssbo_handles: Res<ComputeSSBOHandles>,
        r_gpu_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>, // NOTE: GpuShaderStorageBuffer implements the RenderAsset trait
        r_queue: Res<RenderQueue>,
        r_erosion_uniform_buffer: Res<ErosionUniforms>,
        r_deposition_uniform_buffer: Res<DepositionUniforms>,
    ) {
        // Get handles to the SSBOs from the ComputeSSBOHandles
        let height_a_buffer = r_gpu_buffers.get(&ssbo_handles.height_a).unwrap();
        let height_b_buffer = r_gpu_buffers.get(&ssbo_handles.height_b).unwrap();
        let stream_a_buffer = r_gpu_buffers.get(&ssbo_handles.stream_a).unwrap();
        let stream_b_buffer = r_gpu_buffers.get(&ssbo_handles.stream_b).unwrap();
        let sed_a_buffer = r_gpu_buffers.get(&ssbo_handles.sed_a).unwrap();
        let sed_b_buffer = r_gpu_buffers.get(&ssbo_handles.sed_b).unwrap();
        let debug_buffer = r_gpu_buffers.get(&ssbo_handles.debug).unwrap();

        // let mut erosion_uniform_buffer = UniformBuffer::from(r_erosion_uniform_buffer.into_inner());
        // erosion_uniform_buffer.write_buffer(&r_render_device, &r_queue);

        let mut deposition_uniform_buffer =
            UniformBuffer::from(r_deposition_uniform_buffer.into_inner());
        deposition_uniform_buffer.write_buffer(&r_render_device, &r_queue);

        // let erosion_bind_group0 = r_render_device.create_bind_group(
        //     None,
        //     &r_pipeline_cache.get_bind_group_layout(&r_pipeline.layout),
        //     &BindGroupEntries::sequential((
        //         height_a_buffer.buffer.as_entire_buffer_binding(),
        //         height_b_buffer.buffer.as_entire_buffer_binding(),
        //         stream_a_buffer.buffer.as_entire_buffer_binding(),
        //         stream_b_buffer.buffer.as_entire_buffer_binding(),
        //         debug_buffer.buffer.as_entire_buffer_binding(),
        //         &deposition_uniform_buffer,
        //     )),
        // );
        // let erosion_bind_group1 = r_render_device.create_bind_group(
        //     None,
        //     &r_pipeline_cache.get_bind_group_layout(&r_pipeline.layout),
        //     &BindGroupEntries::sequential((
        //         height_b_buffer.buffer.as_entire_buffer_binding(),
        //         height_a_buffer.buffer.as_entire_buffer_binding(),
        //         stream_b_buffer.buffer.as_entire_buffer_binding(),
        //         stream_a_buffer.buffer.as_entire_buffer_binding(),
        //         debug_buffer.buffer.as_entire_buffer_binding(),
        //         &deposition_uniform_buffer,
        //     )),
        // );

        let deposition_bind_group0 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.layout),
            &BindGroupEntries::sequential((
                height_a_buffer.buffer.as_entire_buffer_binding(),
                height_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                sed_a_buffer.buffer.as_entire_buffer_binding(),
                sed_b_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer.buffer.as_entire_buffer_binding(),
                &deposition_uniform_buffer,
            )),
        );
        let deposition_bind_group1 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.layout),
            &BindGroupEntries::sequential((
                height_b_buffer.buffer.as_entire_buffer_binding(),
                height_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                sed_b_buffer.buffer.as_entire_buffer_binding(),
                sed_a_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer.buffer.as_entire_buffer_binding(),
                &deposition_uniform_buffer,
            )),
        );

        // We will use this when writing the render code in the Render Graph's Node
        commands.insert_resource(ComputeBindGroups([
            deposition_bind_group0,
            deposition_bind_group1,
        ]));
    }
}

impl render_graph::Node for ComputeNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        let bind_groups = world.resource::<ComputeBindGroups>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();

        if let Some(compute_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline_id) {
            let dispatch_size = (TEXTURE_SIZE / 8) as u32;

            for i in 0..100 {
                let bind_group_index = i % 2;

                let mut pass =
                    render_context
                        .command_encoder()
                        .begin_compute_pass(&ComputePassDescriptor {
                            label: Some("our pipeline"),
                            ..default()
                        });

                pass.set_bind_group(0, &bind_groups.0[bind_group_index], &[]);
                pass.set_pipeline(compute_pipeline);
                pass.dispatch_workgroups(dispatch_size, dispatch_size, 1);
            }
        }

        Ok(())
    }
}

pub fn prepare_ssbo<T: ShaderType + ShaderSize + WriteInto>(
    buffers: &mut ResMut<Assets<ShaderStorageBuffer>>,
    data: Vec<T>,
) -> Handle<ShaderStorageBuffer> {
    let mut buffer = ShaderStorageBuffer::from(data);

    // Used to be able to copy this back to RAM, for debugging purposes
    buffer.buffer_description.usage |= BufferUsages::COPY_SRC;

    buffers.add(buffer)
}
