// use bevy::prelude::*;
// use bevy::render::Render;
// use bevy::render::RenderApp;
// use bevy::render::render_graph::RenderGraph;
// use bevy::render::render_resource::*;
// use bevy::render::renderer::RenderDevice;
// use bytemuck::{Pod, Zeroable};

// use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
// use bevy::render::render_resource::binding_types;

// const PARTICLE_COUNT: u32 = 4096;

// fn main() {
//     App::new()
//         .add_plugins(DefaultPlugins)
//         .add_plugins(ParticleRenderPlugin)
//         .add_systems(Startup, setup)
//         .run();
// }
// fn setup(mut commands: Commands) {
//     commands.spawn((Camera3d::default(), Transform::default()));
// }

// pub struct ParticleRenderPlugin;

// impl Plugin for ParticleRenderPlugin {
//     fn build(&self, app: &mut App) {
//         let render_app = app.sub_app_mut(RenderApp);

//         // TODO: Something like this?
//         // app.add_plugins(ExtractResourcePlugin::<TerrainStorageBuffer>::default());

//         render_app
//             .init_resource::<ParticleBuffer>()
//             .init_resource::<ParticleComputePipeline>()
//             .init_resource::<ParticleRenderPipeline>()
//             .add_systems(Render, queue_particles);

//         let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();

//         graph.add_node(ParticleComputeLabel, ParticleComputeNode);

//         graph.add_node_edge(
//             ParticleComputeLabel,
//             bevy::core_pipeline::core_3d::graph::Node3d::MainOpaquePass,
//         );
//     }
// }

// #[repr(C)]
// #[derive(Clone, Copy, Pod, Zeroable)]
// pub struct Particle {
//     pub position: [f32; 4],
// }

// #[derive(Resource, Clone, ExtractResource, ShaderType)]
// pub struct ParticleBuffer {
//     pub buffer: Buffer,
// }

// impl FromWorld for ParticleBuffer {
//     fn from_world(world: &mut World) -> Self {
//         let device = world.resource::<RenderDevice>();

//         let particles = vec![
//             Particle {
//                 position: [0.5, 0.0, 0.0, 1.0]
//             };
//             PARTICLE_COUNT as usize
//         ];

//         let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
//             label: Some("particle_buffer"),
//             contents: bytemuck::cast_slice(&particles),
//             usage: BufferUsages::STORAGE
//                 | BufferUsages::VERTEX
//                 | BufferUsages::COPY_DST
//                 | BufferUsages::COPY_SRC,
//         });

//         Self { buffer }
//     }
// }

// #[derive(Resource)]
// pub struct ParticleComputePipeline {
//     pub pipeline: CachedComputePipelineId,
//     pub layout: BindGroupLayout,
// }

// impl FromWorld for ParticleComputePipeline {
//     fn from_world(world: &mut World) -> Self {
//         let device = world.resource::<RenderDevice>();
//         let pipeline_cache = world.resource::<PipelineCache>();
//         let asset_server = world.resource::<AssetServer>();

//         let shader = asset_server.load("particle_compute.wgsl");

//         let layout = device.create_bind_group_layout(
//             "particle_compute_layout",
//             &[BindGroupLayoutEntry {
//                 binding: 0,
//                 visibility: ShaderStages::COMPUTE,
//                 ty: BindingType::Buffer {
//                     ty: BufferBindingType::Storage { read_only: false },
//                     has_dynamic_offset: false,
//                     min_binding_size: None,
//                 },
//                 count: None,
//             }],
//         );

//         let layout = BindGroupLayoutDescriptor::new(
//             "particle_compute_layout",
//             &BindGroupLayoutEntries::sequential(
//                 ShaderStages::COMPUTE,
//                 (binding_types::storage_buffer,),
//             ),
//         );

//         let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
//             layout: vec![layout.clone()],
//             shader,
//             entry_point: "main".into(),
//             ..default()
//         });

//         Self { pipeline, layout }
//     }
// }

// #[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
// pub struct ParticleComputeLabel;

// #[derive(Default)]
// pub struct ParticleComputeNode;

// impl render_graph::Node for ParticleComputeNode {
//     fn run(
//         &self,
//         _graph: &mut RenderGraphContext,
//         render_context: &mut RenderContext,
//         world: &World,
//     ) -> Result<(), NodeRunError> {
//         let pipeline_cache = world.resource::<PipelineCache>();
//         let pipeline_res = world.resource::<ParticleComputePipeline>();
//         let particle_buffer = world.resource::<ParticleBuffer>();
//         let device = world.resource::<RenderDevice>();

//         let Some(pipeline) = pipeline_cache.get_compute_pipeline(pipeline_res.pipeline) else {
//             return Ok(());
//         };

//         let bind_group = device.create_bind_group(
//             "particle_compute_bg",
//             &pipeline_res.layout,
//             &[BindGroupEntry {
//                 binding: 0,
//                 resource: particle_buffer.buffer.as_entire_binding(),
//             }],
//         );

//         let mut pass = render_context
//             .command_encoder()
//             .begin_compute_pass(&ComputePassDescriptor::default());

//         pass.set_pipeline(pipeline);
//         pass.set_bind_group(0, &bind_group, &[]);
//         pass.dispatch_workgroups(PARTICLE_COUNT / 64, 1, 1);

//         Ok(())
//     }
// }

// #[derive(Resource)]
// pub struct ParticleRenderPipeline {
//     pub mesh_pipeline: MeshPipeline,
//     pub shader: Handle<Shader>,
// }

// impl FromWorld for ParticleRenderPipeline {
//     fn from_world(world: &mut World) -> Self {
//         let mesh_pipeline = world.resource::<MeshPipeline>().clone();
//         let asset_server = world.resource::<AssetServer>();

//         Self {
//             mesh_pipeline,
//             shader: asset_server.load("particle_render.wgsl"),
//         }
//     }
// }

// impl SpecializedMeshPipeline for ParticleRenderPipeline {
//     type Key = MeshPipelineKey;

//     fn specialize(
//         &self,
//         key: Self::Key,
//         layout: &MeshVertexBufferLayoutRef,
//     ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
//         let mut desc = self.mesh_pipeline.specialize(key, layout)?;

//         desc.vertex.shader = self.shader.clone();
//         desc.fragment.as_mut().unwrap().shader = self.shader.clone();

//         Ok(desc)
//     }
// }
