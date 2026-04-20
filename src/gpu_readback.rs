//! Simple example demonstrating the use of the [`Readback`] component to read back data from the GPU
//! using both a storage buffer and texture.

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
};

/// This example uses a shader source file from the assets subdirectory
const SHADER_ASSET_PATH: &str = "shaders/gpu_readback.wgsl";

// The length of the buffer sent to the gpu
const BUFFER_LEN: usize = 16;

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            GpuReadbackPlugin,
            ExtractResourcePlugin::<ReadbackBuffer>::default(),
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, setup)
        .run();
}

// We need a plugin to organize all the systems and render node required for this example
struct GpuReadbackPlugin;

impl Plugin for GpuReadbackPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(
                RenderStartup,
                (init_compute_pipeline, Self::add_compute_render_graph_node),
            )
            .add_systems(
                Render,
                prepare_bind_group
                    .in_set(RenderSystems::PrepareBindGroups)
                    // We don't need to recreate the bind group every frame
                    .run_if(not(resource_exists::<GpuBufferBindGroup>)),
            );
    }
}

impl GpuReadbackPlugin {
    fn add_compute_render_graph_node(mut render_graph: ResMut<RenderGraph>) {
        // Add the compute node as a top-level node to the render graph. This means it will only execute
        // once per frame. Normally, adding a node would use the `RenderGraphApp::add_render_graph_node`
        // method, but it does not allow adding as a top-level node.
        render_graph.add_node(ComputeNodeLabel, ComputeNode::default());
        // NOTE: How come we don't add any egdes? I assume this is meant by a "top-level node".
    }
}

#[derive(Resource, ExtractResource, Clone)]
struct ReadbackBuffer(Handle<ShaderStorageBuffer>);

fn setup(mut commands: Commands, mut buffers: ResMut<Assets<ShaderStorageBuffer>>) {
    // Create a storage buffer with some data
    let buffer: Vec<u32> = (0..BUFFER_LEN as u32).collect();
    let mut buffer = ShaderStorageBuffer::from(buffer);
    // We need to enable the COPY_SRC usage so we can copy the buffer to the cpu
    buffer.buffer_description.usage |= BufferUsages::COPY_SRC;
    let buffer = buffers.add(buffer);

    // Spawn the readback components. For each frame, the data will be read back from the GPU
    // asynchronously and trigger the `ReadbackComplete` event on this entity. Despawn the entity
    // to stop reading back the data.
    commands
        .spawn(Readback::buffer(buffer.clone()))
        .observe(|event: On<ReadbackComplete>| {
            // This matches the type which was used to create the `ShaderStorageBuffer` above,
            // and is a convenient way to interpret the data.
            let data: Vec<u32> = event.to_shader_type();
            info!("Buffer {:?}", data);
        });

    // This is just a simple way to pass the buffer handle to the render app for our compute node
    commands.insert_resource(ReadbackBuffer(buffer));
}

#[derive(Resource)]
struct GpuBufferBindGroup(BindGroup);

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<ComputePipeline>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    buffer: Res<ReadbackBuffer>,
    buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
) {
    let buffer = buffers.get(&buffer.0).unwrap();
    let bind_group = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::single(buffer.buffer.as_entire_buffer_binding()),
    );
    commands.insert_resource(GpuBufferBindGroup(bind_group));
}

#[derive(Resource)]
struct ComputePipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline: CachedComputePipelineId,
}

fn init_compute_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "",
        &BindGroupLayoutEntries::single(ShaderStages::COMPUTE, storage_buffer::<Vec<u32>>(false)),
    );
    let shader = asset_server.load(SHADER_ASSET_PATH);
    let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("GPU readback compute shader".into()),
        layout: vec![layout.clone()],
        shader: shader.clone(),
        ..default()
    });
    commands.insert_resource(ComputePipeline { layout, pipeline });
}

/// Label to identify the node in the render graph
#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

/// The node that will execute the compute shader
#[derive(Default)]
struct ComputeNode {}

impl render_graph::Node for ComputeNode {
    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();
        let bind_group = world.resource::<GpuBufferBindGroup>();

        if let Some(init_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline) {
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("GPU readback compute pass"),
                        ..default()
                    });

            pass.set_bind_group(0, &bind_group.0, &[]);
            pass.set_pipeline(init_pipeline);
            pass.dispatch_workgroups(BUFFER_LEN as u32, 1, 1);
        }
        Ok(())
    }
}
