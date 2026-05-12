use bevy::{
    input::common_conditions,
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
            init_shader_resources.run_if(in_state(crate::AppState::InitShaderResources)),
        )
        .add_systems(
            Update,
            change_erosion_uniform_resource.run_if(in_state(crate::AppState::Running)),
        )
        .add_systems(
            Update,
            toggle_compute_pipeline.run_if(
                in_state(crate::AppState::Running)
                    .and(common_conditions::input_just_pressed(KeyCode::KeyC)),
            ),
        );
    }
}

/// Path to the erosion compute shader
const COMPUTE_EROSION_SHADER_PATH: &str = "shaders/erosion.wgsl";
/// Path to the deposition compute shader
const COMPUTE_DEPOSITION_SHADER_PATH: &str = "shaders/deposition.wgsl";
/// Path to the thermal compute shader
const COMPUTE_THERMAL_SHADER_PATH: &str = "shaders/thermal.wgsl";

/// Path to the render shader
const RENDER_SHADER_PATH: &str = "shaders/terrain_render.wgsl";

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

    pub debug_a: Handle<ShaderStorageBuffer>,
    pub debug_b: Handle<ShaderStorageBuffer>,

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
            nx: 0,                 //NOTE: This will be overwritten by the actual texture size
            ny: 0,                 //NOTE: This will be overwritten by the actual texture size
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
            nx: 0,                 //NOTE: This will be overwritten by the actual texture size
            ny: 0,                 //NOTE: This will be overwritten by the actual texture size
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

#[derive(Resource, Clone, ExtractResource, ShaderType, Reflect)]
#[reflect(Resource)]
pub struct ThermalUniforms {
    pub nx: i32,
    pub ny: i32,
    // NOTE: Opposite corners of the grid
    pub a: Vec2, // TODO: Rename to something like grid_corner1
    pub b: Vec2, // TODO: Rename to something like grid_corner2
    pub cell_size: Vec2,

    pub eps: f32,
    pub tan_threshold_angle: f32,
    pub noisified_angle: i32,
    pub noise_min: f32,
    pub noise_max: f32,
    pub noise_wavelength: f32,
    pub use_threshold_map: i32,

    pub debug: f32,
}

impl Default for ThermalUniforms {
    fn default() -> Self {
        Self {
            nx: 0,                 //NOTE: This will be overwritten by the actual texture size
            ny: 0,                 //NOTE: This will be overwritten by the actual texture size
            a: Vec2::ZERO, //NOTE: This will be overwritten by the actual corners of the grid
            b: Vec2::ONE,  //NOTE: This will be overwritten by the actual corners of the grid
            cell_size: Vec2::ZERO, // NOTE: This will be overwritten by the actual size of the cell

            eps: 0.00005,
            tan_threshold_angle: 0.57,
            noisified_angle: 1,
            noise_min: 0.9,
            noise_max: 1.4,
            noise_wavelength: 0.0023,
            use_threshold_map: 0,

            debug: 1.0,
        }
    }
}

//////////

/// Plugin for setting up the render node
/// TODO: Possibly move to a separate file
struct ComputeShaderPipelinePlugin;

/// Resource containing bind groups for compute shaders
/// Each compute shader uses double buffering
#[derive(Resource)]
struct ComputeBindGroups {
    erosion: [BindGroup; 2],
    deposition: [BindGroup; 2],
    thermal: [BindGroup; 2],
}

/// Compute shader pipeline
#[derive(Resource)]
struct ComputePipeline {
    erosion_layout: BindGroupLayoutDescriptor,
    erosion_pipeline: CachedComputePipelineId,
    deposition_layout: BindGroupLayoutDescriptor,
    deposition_pipeline: CachedComputePipelineId,
    thermal_layout: BindGroupLayoutDescriptor,
    thermal_pipeline: CachedComputePipelineId,
}

/// Label to identify the node in the render graph
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

/// The node that will execute the compute shader
/// NOTE: Idea: we can make a compute node for each compute stage (erosion, deposition, ...)
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

#[derive(Resource, Reflect, Clone, ExtractResource)]
#[reflect(Resource)]
pub struct ShaderConfig {
    pub run_compute: bool,
    pub texture_size: usize,
}

fn toggle_compute_pipeline(mut r_shader_config: ResMut<ShaderConfig>) {
    r_shader_config.run_compute = !r_shader_config.run_compute;
}

/// Sets up everything related to shaders
fn shader_setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Load in the heightfield texture
    let texture_handle: Handle<Image> = asset_server.load("heightfields/mountains.png");
    commands.insert_resource(ImageHandle(texture_handle));
}

/// Waits until the texture asset is loaded and changes the app state
fn image_loaded_observer(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    image_handle: Res<ImageHandle>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    if let Some(image) = images.get_mut(&image_handle.0) {
        assert_eq!(image.width(), image.height());

        let texture_size = image.width() as usize;
        commands.insert_resource(ShaderConfig {
            run_compute: true,
            texture_size,
        });

        s_next_app_state.set(crate::AppState::InitShaderResources);
        info!("Texture loaded!");
    } else {
        info!("Texture not yet loaded...");
    };
}

fn init_shader_resources(
    mut commands: Commands,
    r_shader_config: Res<ShaderConfig>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut s_next_app_state: ResMut<NextState<crate::AppState>>,
) {
    let buffer_size = r_shader_config.texture_size * r_shader_config.texture_size;

    info!("buffer_size: {}", buffer_size);

    // Prepare SSBOs for the compute shader
    commands.insert_resource(ComputeSSBOHandles {
        height_a: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        height_b: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        stream_a: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        stream_b: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        sed_a: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        sed_b: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),

        debug_a: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),
        debug_b: prepare_ssbo(&mut buffers, vec![0.0; buffer_size]),

        vertex_positions: prepare_ssbo(&mut buffers, vec![Vec3::ZERO; buffer_size]),
    });

    s_next_app_state.set(crate::AppState::GeneratingTerrain);
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

#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
struct ComputeShaderConfig {
    run_erosion: bool,
}

impl Plugin for ComputeShaderPipelinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // NOTE: Extract all these resources from the Main World to the Render World
            ExtractResourcePlugin::<ShaderConfig>::default(),
            ExtractResourcePlugin::<ComputeSSBOHandles>::default(),
            ExtractResourcePlugin::<ErosionUniforms>::default(),
            ExtractResourcePlugin::<DepositionUniforms>::default(),
            ExtractResourcePlugin::<ThermalUniforms>::default(),
            ExtractResourcePlugin::<ImageHandle>::default(),
            ExtractResourcePlugin::<crate::terrain::TerrainConfig>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<ComputeShaderConfig>()
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
        let erosion_layout = BindGroupLayoutDescriptor::new(
            "erosion",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_stream
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_stream
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_debug
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_debug
                    binding_types::uniform_buffer::<ErosionUniforms>(false),
                ),
            ),
        );

        let deposition_layout = BindGroupLayoutDescriptor::new(
            "deposition",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_stream
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_stream
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_sed
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_sed
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_debug
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_debug
                    binding_types::uniform_buffer::<DepositionUniforms>(false),
                ),
            ),
        );

        let thermal_layout = BindGroupLayoutDescriptor::new(
            "thermal",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_terrain
                    binding_types::storage_buffer::<Vec<f32>>(false), // in_debug
                    binding_types::storage_buffer::<Vec<f32>>(false), // out_debug
                    binding_types::uniform_buffer::<ThermalUniforms>(false),
                ),
            ),
        );

        let erosion_shader = asset_server.load(COMPUTE_EROSION_SHADER_PATH);
        let deposition_shader = asset_server.load(COMPUTE_DEPOSITION_SHADER_PATH);
        let thermal_shader = asset_server.load(COMPUTE_THERMAL_SHADER_PATH);

        let erosion_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("Erosion".into()),
            layout: vec![erosion_layout.clone()],
            shader: erosion_shader.clone(),
            ..default()
        });

        let deposition_pipeline =
            pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("Deposition".into()),
                layout: vec![deposition_layout.clone()],
                shader: deposition_shader.clone(),
                ..default()
            });

        let thermal_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("Thermal".into()),
            layout: vec![thermal_layout.clone()],
            shader: thermal_shader.clone(),
            ..default()
        });

        // We will use this when writing the render code in the Render Graph's Node
        commands.insert_resource(ComputePipeline {
            erosion_layout,
            erosion_pipeline,
            deposition_layout,
            deposition_pipeline,
            thermal_layout,
            thermal_pipeline,
        });
    }

    /// Creates the bind group according to the layout and loads in the uniform data
    fn prepare_bind_group(
        mut commands: Commands,
        r_pipeline: Res<ComputePipeline>,
        r_render_device: Res<RenderDevice>,
        r_pipeline_cache: Res<PipelineCache>,
        ssbo_handles: If<Res<ComputeSSBOHandles>>,
        r_gpu_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>, // NOTE: GpuShaderStorageBuffer implements the RenderAsset trait
        r_queue: Res<RenderQueue>,
        r_erosion_uniform_buffer: If<Res<ErosionUniforms>>,
        r_deposition_uniform_buffer: If<Res<DepositionUniforms>>,
        r_thermal_uniform_buffer: If<Res<ThermalUniforms>>,
    ) {
        // Get handles to the SSBOs from the ComputeSSBOHandles
        let height_a_buffer = r_gpu_buffers.get(&ssbo_handles.height_a).unwrap();
        let height_b_buffer = r_gpu_buffers.get(&ssbo_handles.height_b).unwrap();
        let stream_a_buffer = r_gpu_buffers.get(&ssbo_handles.stream_a).unwrap();
        let stream_b_buffer = r_gpu_buffers.get(&ssbo_handles.stream_b).unwrap();
        let sed_a_buffer = r_gpu_buffers.get(&ssbo_handles.sed_a).unwrap();
        let sed_b_buffer = r_gpu_buffers.get(&ssbo_handles.sed_b).unwrap();
        let debug_buffer_a = r_gpu_buffers.get(&ssbo_handles.debug_a).unwrap();
        let debug_buffer_b = r_gpu_buffers.get(&ssbo_handles.debug_b).unwrap();

        let mut erosion_uniform_buffer =
            UniformBuffer::from(r_erosion_uniform_buffer.0.into_inner());
        erosion_uniform_buffer.write_buffer(&r_render_device, &r_queue);

        let mut deposition_uniform_buffer =
            UniformBuffer::from(r_deposition_uniform_buffer.0.into_inner());
        deposition_uniform_buffer.write_buffer(&r_render_device, &r_queue);

        let mut thermal_uniform_buffer =
            UniformBuffer::from(r_thermal_uniform_buffer.0.into_inner());
        thermal_uniform_buffer.write_buffer(&r_render_device, &r_queue);

        let erosion_bind_group0 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.erosion_layout),
            &BindGroupEntries::sequential((
                height_a_buffer.buffer.as_entire_buffer_binding(),
                height_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                &erosion_uniform_buffer,
            )),
        );
        let erosion_bind_group1 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.erosion_layout),
            &BindGroupEntries::sequential((
                height_b_buffer.buffer.as_entire_buffer_binding(),
                height_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                &erosion_uniform_buffer,
            )),
        );

        let deposition_bind_group0 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.deposition_layout),
            &BindGroupEntries::sequential((
                height_a_buffer.buffer.as_entire_buffer_binding(),
                height_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                sed_a_buffer.buffer.as_entire_buffer_binding(),
                sed_b_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                &deposition_uniform_buffer,
            )),
        );
        let deposition_bind_group1 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.deposition_layout),
            &BindGroupEntries::sequential((
                height_b_buffer.buffer.as_entire_buffer_binding(),
                height_a_buffer.buffer.as_entire_buffer_binding(),
                stream_b_buffer.buffer.as_entire_buffer_binding(),
                stream_a_buffer.buffer.as_entire_buffer_binding(),
                sed_b_buffer.buffer.as_entire_buffer_binding(),
                sed_a_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                &deposition_uniform_buffer,
            )),
        );

        let thermal_bind_group0 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.thermal_layout),
            &BindGroupEntries::sequential((
                height_a_buffer.buffer.as_entire_buffer_binding(),
                height_b_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                &thermal_uniform_buffer,
            )),
        );
        let thermal_bind_group1 = r_render_device.create_bind_group(
            None,
            &r_pipeline_cache.get_bind_group_layout(&r_pipeline.thermal_layout),
            &BindGroupEntries::sequential((
                height_b_buffer.buffer.as_entire_buffer_binding(),
                height_a_buffer.buffer.as_entire_buffer_binding(),
                debug_buffer_b.buffer.as_entire_buffer_binding(),
                debug_buffer_a.buffer.as_entire_buffer_binding(),
                &thermal_uniform_buffer,
            )),
        );

        commands.insert_resource(ComputeBindGroups {
            erosion: [erosion_bind_group0, erosion_bind_group1],
            deposition: [deposition_bind_group0, deposition_bind_group1],
            thermal: [thermal_bind_group0, thermal_bind_group1],
        });
    }
}

impl render_graph::Node for ComputeNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        let Some(compute_bind_groups) = world.get_resource::<ComputeBindGroups>() else {
            return Ok(());
        };
        let Some(shader_config) = world.get_resource::<ShaderConfig>() else {
            return Ok(());
        };
        let Some(terrain_config) = world.get_resource::<crate::terrain::TerrainConfig>() else {
            return Ok(());
        };

        // Don't run if uniforms are not initialized
        if world.get_resource::<ErosionUniforms>().is_none()
            || world.get_resource::<DepositionUniforms>().is_none()
            || world.get_resource::<ThermalUniforms>().is_none()
        {
            return Ok(());
        }

        if !shader_config.run_compute {
            return Ok(());
        }

        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();

        let dispatch_size = (shader_config.texture_size / 8) as u32;

        let terrain_computer =
            TerrainComputer::new(pipeline, compute_bind_groups, pipeline_cache, dispatch_size);
        let erosion_computer = ErosionComputer(&terrain_computer);
        let deposition_computer = DepositionComputer(&terrain_computer);
        let thermal_computer = ThermalComputer(&terrain_computer);

        let mut pass =
            render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("compute pipeline"),
                    ..default()
                });

        if terrain_config.run_erosion {
            // TODO: Parameterize this loop count
            for i in 0..100 {
                erosion_computer.compute(&mut pass, i % 2);
            }
        }
        if terrain_config.run_thermal {
            // TODO: Parameterize this loop count
            for i in 0..20 {
                thermal_computer.compute(&mut pass, i % 2);
            }
        }
        if terrain_config.run_deposition {
            // TODO: Parameterize this loop count
            for i in 0..50 {
                deposition_computer.compute(&mut pass, i % 2);
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

trait TerrainComputerShader {
    fn compute(&self, pass: &mut ComputePass, bind_group_index: usize);
}

struct TerrainComputer<'a> {
    pipeline: &'a ComputePipeline,
    compute_bind_groups: &'a ComputeBindGroups,
    pipeline_cache: &'a PipelineCache,
    dispatch_size: u32,
}

#[derive(Deref)]
struct ErosionComputer<'a>(&'a TerrainComputer<'a>);

#[derive(Deref)]
struct DepositionComputer<'a>(&'a TerrainComputer<'a>);
#[derive(Deref)]
struct ThermalComputer<'a>(&'a TerrainComputer<'a>);

impl<'a> TerrainComputer<'a> {
    fn new(
        pipeline: &'a ComputePipeline,
        compute_bind_groups: &'a ComputeBindGroups,
        pipeline_cache: &'a PipelineCache,
        dispatch_size: u32,
    ) -> Self {
        TerrainComputer {
            pipeline,
            compute_bind_groups,
            pipeline_cache,
            dispatch_size,
        }
    }
}

impl<'a> TerrainComputerShader for ErosionComputer<'a> {
    fn compute(&self, pass: &mut ComputePass, bind_group_index: usize) {
        let Some(erosion_pipeline) = self
            .pipeline_cache
            .get_compute_pipeline(self.pipeline.erosion_pipeline)
        else {
            return;
        };

        pass.set_bind_group(0, &self.compute_bind_groups.erosion[bind_group_index], &[]);
        pass.set_pipeline(erosion_pipeline);
        pass.dispatch_workgroups(self.dispatch_size, self.dispatch_size, 1);
    }
}

impl<'a> TerrainComputerShader for DepositionComputer<'a> {
    fn compute(&self, pass: &mut ComputePass, bind_group_index: usize) {
        let Some(deposition_pipeline) = self
            .pipeline_cache
            .get_compute_pipeline(self.pipeline.deposition_pipeline)
        else {
            return;
        };

        pass.set_bind_group(
            0,
            &self.compute_bind_groups.deposition[bind_group_index],
            &[],
        );
        pass.set_pipeline(deposition_pipeline);
        pass.dispatch_workgroups(self.dispatch_size, self.dispatch_size, 1);
    }
}

impl<'a> TerrainComputerShader for ThermalComputer<'a> {
    fn compute(&self, pass: &mut ComputePass, bind_group_index: usize) {
        let Some(thermal_pipeline) = self
            .pipeline_cache
            .get_compute_pipeline(self.pipeline.thermal_pipeline)
        else {
            return;
        };

        pass.set_bind_group(0, &self.compute_bind_groups.thermal[bind_group_index], &[]);
        pass.set_pipeline(thermal_pipeline);
        pass.dispatch_workgroups(self.dispatch_size, self.dispatch_size, 1);
    }
}
