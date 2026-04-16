use bevy::{
    asset::RenderAssetUsages,
    color::palettes::tailwind::{RED_400, SKY_400},
    mesh::Indices,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        graph::CameraDriverLabel,
        mesh::allocator::MeshAllocator,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{binding_types::*, *},
        renderer::{RenderContext, RenderQueue},
    },
};
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Not;

const SHADER_PATH: &str = "shaders/compute_mesh.wgsl";

pub fn run() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            ComputeShaderMeshGeneratorPlugin,
            ExtractComponentPlugin::<GenerateMesh>::default(),
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, setup)
        .run();
}

// We need a plugin to organize all the systems and render node required for this example
struct ComputeShaderMeshGeneratorPlugin;

impl Plugin for ComputeShaderMeshGeneratorPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<MeshesToProcess>()
            .add_systems(RenderStartup, init_compute_pipeline)
            .add_systems(Render, prepare_chunks);

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(ComputeMeshLabel, ComputeMeshNode::default());
        render_graph.add_node_edge(ComputeMeshLabel, CameraDriverLabel);
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .world_mut()
            .resource_mut::<MeshAllocator>()
            // This allows using the mesh allocator slabs as
            // storage buffers directly in the compute shader.
            // Which means that we can write from our compute
            // shader directly to the allocated mesh slabs.
            .extra_buffer_usages = BufferUsages::STORAGE;
    }
}

/// Holds a handle to the empty mesh that should be filled
/// by the compute shader.
#[derive(Component, ExtractComponent, Clone)]
struct GenerateMesh(Handle<Mesh>);

#[derive(Resource, Default)]
struct MeshesToProcess(Vec<AssetId<Mesh>>);

#[derive(Default)]
struct ComputeMeshNode;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeMeshLabel;

// A uniform that holds the vertex and index offsets
// for the vertex/index mesh_allocator buffer slabs
#[derive(ShaderType)]
struct DataRanges {
    vertex_start: u32,
    vertex_end: u32,
    index_start: u32,
    index_end: u32,
}

#[derive(Resource)]
struct ComputePipeline {
    layout: BindGroupLayoutDescriptor,
    pipeline_id: CachedComputePipelineId,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // some additional scene elements.
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(4.0))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // a truly empty mesh will error if used in Mesh3d
    // so we set up the data to be what we want the compute shader to output
    // We're using 36 indices and 24 vertices which is directly taken from
    // the Bevy Cuboid mesh implementation.
    //
    // We allocate 50 spots for each attribute here because
    // it is *very important* that the amount of data allocated here is
    // *bigger* than (or exactly equal to) the amount of data we intend to
    // write from the compute shader. This amount of data defines how big
    // the buffer we get from the mesh_allocator will be, which in turn
    // defines how big the buffer is when we're in the compute shader.
    //
    // If it turns out you don't need all of the space when the compute shader
    // is writing data, you can write NaN to the rest of the data.
    let empty_mesh = {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.; 3]; 50])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.; 3]; 50])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.; 2]; 50])
        .with_inserted_indices(Indices::U32(vec![0; 50]));

        mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
        mesh
    };

    let handle = meshes.add(empty_mesh);

    // we spawn two "users" of the mesh handle,
    // but only insert `GenerateMesh` on one of them
    // to show that the mesh handle works as usual
    commands.spawn((
        GenerateMesh(handle.clone()),
        Mesh3d(handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: RED_400.into(),
            ..default()
        })),
        Transform::from_xyz(-2.5, 1.5, 0.),
    ));

    commands.spawn((
        Mesh3d(handle),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: SKY_400.into(),
            ..default()
        })),
        Transform::from_xyz(2.5, 1.5, 0.),
    ));
}

fn init_compute_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE, // | ShaderStages::VERTEX // NOTE: Here we can assign multiple stages
            (
                // binding 0: offset buffer
                uniform_buffer::<DataRanges>(false),
                // binding 1: vertices buffer
                storage_buffer::<Vec<u32>>(false),
                // binding 2: indices buffer
                storage_buffer::<Vec<u32>>(false),
            ),
        ),
    );

    let shader = asset_server.load(SHADER_PATH);

    let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("Mesh generation compute shader".into()),
        layout: vec![layout.clone()],
        shader: shader.clone(),
        entry_point: Some(Cow::from("main")),
        ..default()
    });

    commands.insert_resource(ComputePipeline {
        layout,
        pipeline_id: pipeline,
    });
}

/// `processed` is a `HashSet` contains the `AssetId`s that have been
/// processed. We use that to remove `AssetId`s that have already
/// been processed, which means each unique `GenerateMesh` will result
/// in one compute shader mesh generation process instead of generating
/// the mesh every frame.
fn prepare_chunks(
    meshes_to_generate: Query<&GenerateMesh>,
    mut meshes_to_process: ResMut<MeshesToProcess>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<ComputePipeline>,
    mut processed: Local<HashSet<AssetId<Mesh>>>,
) {
    // If the pipeline we queued isn't ready, then meshes
    // won't be processed. So we want to wait until
    // the pipeline is ready before considering any mesh processed.
    if pipeline_cache
        .get_compute_pipeline(pipeline.pipeline_id)
        .is_some()
    {
        // get the AssetId for each Handle<Mesh>
        // which we'll use later to get the relevant buffers
        // from the mesh_allocator
        let mesh_data: Vec<AssetId<Mesh>> = meshes_to_generate
            .iter()
            .filter_map(|gmesh| {
                let id = gmesh.0.id();
                processed.contains(&id).not().then_some(id)
            })
            .collect();

        // Cache any meshes we're going to process this frame
        for id in &mesh_data {
            processed.insert(*id);
        }

        meshes_to_process.0 = mesh_data;
    }
}

impl render_graph::Node for ComputeMeshNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> std::result::Result<(), render_graph::NodeRunError> {
        // Get resources from the world
        let meshes_to_process = world.resource::<MeshesToProcess>();
        let mesh_allocator = world.resource::<MeshAllocator>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<ComputePipeline>();
        let render_queue = world.resource::<RenderQueue>();

        let Some(init_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.pipeline_id) else {
            return Ok(());
        };

        for mesh_id in &meshes_to_process.0 {
            info!(?mesh_id, "processing mesh");

            // the mesh_allocator holds slabs of meshes, so the buffers we get here
            // can contain more data than just the mesh we're asking for.
            // That's why there is a range field.
            // You should *not* touch data in these buffers that is outside of the range.
            let vertex_buffer_slice = mesh_allocator.mesh_vertex_slice(mesh_id).unwrap();
            let index_buffer_slice = mesh_allocator.mesh_index_slice(mesh_id).unwrap();

            let first = DataRanges {
                // there are 8 vertex data values (pos, normal, uv) per vertex
                // and the vertex_buffer_slice.range.start is in "vertex elements"
                // which includes all of that data, so each index is worth 8 indices
                // to our shader code.
                vertex_start: vertex_buffer_slice.range.start * 8,
                vertex_end: vertex_buffer_slice.range.end * 8,
                // but each vertex index is a single value, so the index of the
                // vertex indices is exactly what the value is
                index_start: index_buffer_slice.range.start,
                index_end: index_buffer_slice.range.end,
            };

            let mut indices_uniform = UniformBuffer::from(first);
            indices_uniform.write_buffer(render_context.render_device(), &render_queue);

            // pass in the full mesh_allocator slabs as well as the first index
            // offsets for the vertex and index buffers
            let bind_group = render_context.render_device().create_bind_group(
                None,
                &pipeline_cache.get_bind_group_layout(&pipeline.layout),
                &BindGroupEntries::sequential((
                    &indices_uniform,
                    vertex_buffer_slice.buffer.as_entire_buffer_binding(),
                    index_buffer_slice.buffer.as_entire_buffer_binding(),
                )),
            );

            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("Mesh generation compute pass"),
                        ..default()
                    });

            pass.push_debug_group("compute_mesh"); // NOTE: GPU debugging stuff...

            // NOTE: 0 here is the group(0) in the shader
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_pipeline(init_pipeline);
            // we only dispatch 1,1,1 workgroup here, but a real compute shader
            // would take advantage of more and larger size workgroups
            pass.dispatch_workgroups(1, 1, 1);

            pass.pop_debug_group();
        }

        Ok(())
    }
}
