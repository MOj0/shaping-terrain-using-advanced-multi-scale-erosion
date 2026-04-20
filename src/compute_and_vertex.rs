use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{Readback, ReadbackComplete},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{binding_types::storage_buffer, *},
        renderer::{RenderContext, RenderDevice},
        storage::{GpuShaderStorageBuffer, ShaderStorageBuffer},
    },
    shader::ShaderRef,
};

/// Path to the compute shader
const COMPUTE_SHADER_PATH: &str = "shaders/compute_and_vertex.wgsl";

/// Path to the render shader
const RENDER_SHADER_PATH: &str = "shaders/compute_and_vertex_render.wgsl";

/// Length of the buffer sent to the GPU
const BUFFER_LEN: usize = 3;

/// Plugin for setting up the render node
struct GpuReadbackPlugin;

/// Resource containing the handle to our buffer
#[derive(Resource, ExtractResource, Clone)]
struct ReadbackBuffer(Handle<ShaderStorageBuffer>);

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

// NOTE: This is only for updating this on the CPU...
// // Holds handles to the custom materials
// #[derive(Resource)]
// struct CustomMaterialHandle(Handle<MyVertexMaterial>);

// This struct defines the data that will be passed to your shader
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct MyVertexMaterial {
    #[storage(0)]
    buffer_handle: Handle<ShaderStorageBuffer>,
}

impl Material for MyVertexMaterial {
    fn vertex_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        RENDER_SHADER_PATH.into()
    }
}

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            GpuReadbackPlugin,
            ExtractResourcePlugin::<ReadbackBuffer>::default(),
            MaterialPlugin::<MyVertexMaterial>::default(),
        ))
        .insert_resource(ClearColor(Color::srgb_u8(102, 178, 212)))
        .add_systems(Startup, (setup, shader_setup))
        .run();
}

fn setup(mut commands: Commands) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 4.0, 0.0),
    ));
}

fn shader_setup(
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MyVertexMaterial>>,
) {
    // Create a (storage) buffer with some dummy data
    let buffer: Vec<u32> = (0..BUFFER_LEN as u32).collect();

    let mut shader_storage_buffer = ShaderStorageBuffer::from(buffer);
    // COPY_SRC -> can copy the buffer back to the source (CPU)
    shader_storage_buffer.buffer_description.usage |= BufferUsages::COPY_SRC;

    let buffer_handle = buffers.add(shader_storage_buffer);

    let my_vertex_material_handle = MyVertexMaterial {
        buffer_handle: buffer_handle.clone(),
    };

    // Component which will print the data once it is available for the CPU
    commands
        .spawn(Readback::buffer(buffer_handle.clone()))
        .observe(|event: On<ReadbackComplete>| {
            // Do something with the data on the CPU
            let data: Vec<u32> = event.to_shader_type();
            info!("Buffer {:?}", data);
        });

    // We will use this when preparing the bind group
    commands.insert_resource(ReadbackBuffer(buffer_handle.clone()));

    // Spawn a sphere with MyVertexMaterial
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(64, 32))),
        MeshMaterial3d(materials.add(my_vertex_material_handle)),
    ));
}

impl Plugin for GpuReadbackPlugin {
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

impl GpuReadbackPlugin {
    fn init_compute_pipeline(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        pipeline_cache: Res<PipelineCache>,
    ) {
        // Make a descriptor for the bind group
        let layout = BindGroupLayoutDescriptor::new(
            "",
            &BindGroupLayoutEntries::single(
                ShaderStages::COMPUTE,
                storage_buffer::<Vec<u32>>(false),
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
        pipeline: Res<ComputePipeline>,
        render_device: Res<RenderDevice>,
        pipeline_cache: Res<PipelineCache>,
        buffer: Res<ReadbackBuffer>,
        buffers: Res<RenderAssets<GpuShaderStorageBuffer>>, // NOTE: GpuShaderStorageBuffer implements the RenderAsset trait
    ) {
        let buffer = buffers.get(&buffer.0).unwrap();
        let bind_group = render_device.create_bind_group(
            None,
            &pipeline_cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::single(buffer.buffer.as_entire_buffer_binding()),
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

            pass.set_bind_group(0, &bind_group.0, &[]);
            pass.set_pipeline(init_pipeline);
            pass.dispatch_workgroups(BUFFER_LEN as u32, 1, 1);
        }

        Ok(())
    }
}
