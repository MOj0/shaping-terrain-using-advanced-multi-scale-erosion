use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{binding_types::storage_buffer, *},
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
    shader::PipelineCacheError,
};
use std::borrow::Cow;

const SHADER_ASSET_PATH: &str = "shaders/terrain.wgsl";

const GRID_WIDTH: usize = 128;
const GRID_HEIGHT: usize = 128;

const DISPLAY_FACTOR: u32 = 4;
const SIZE: UVec2 = UVec2::new(1280 / DISPLAY_FACTOR, 720 / DISPLAY_FACTOR);
const WORKGROUP_SIZE: u32 = 8;

#[derive(Resource, Clone, ExtractResource, ShaderType)]
struct TerrainStorageBuffer {
    vertices: [Vec3; GRID_WIDTH * GRID_HEIGHT],
}

#[derive(Resource)]
struct TerrainPipeline {
    texture_bind_group_layout: BindGroupLayoutDescriptor,
    init_pipeline: CachedComputePipelineId,
}

#[derive(Resource)]
struct TerrainBindGroups([BindGroup; 1]);

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct TerrainLabel;

enum TerrainState {
    Loading,
    Init,
}

struct TerrainNode {
    state: TerrainState,
}

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(terrain_plugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            print_terrain_storage_buffer.run_if(
                bevy::input::common_conditions::input_just_pressed(KeyCode::KeyB),
            ),
        )
        .run();
}

fn terrain_plugin(app: &mut App) {
    // Extract the game of life image resource from the main world into the render world
    // for operation on by the compute shader and display on the sprite.
    app.add_plugins(ExtractResourcePlugin::<TerrainStorageBuffer>::default());

    let render_app = app.sub_app_mut(RenderApp);
    render_app
        .add_systems(RenderStartup, init_terrain_pipeline)
        .add_systems(
            Render,
            prepare_bind_group.in_set(RenderSystems::PrepareBindGroups),
        );

    let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
    render_graph.add_node(TerrainLabel, TerrainNode::default());
    render_graph.add_node_edge(TerrainLabel, bevy::render::graph::CameraDriverLabel);
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut vertices = [Vec3::ZERO; GRID_WIDTH * GRID_HEIGHT];
    let vertex_count = vertices.len();

    let mut counter = 0;
    for z in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let xf = x as f32;
            let zf = z as f32;

            vertices[counter] = Vec3::new(xf, 0.0, zf);
            counter += 1;
        }
    }

    commands.insert_resource(TerrainStorageBuffer { vertices });

    let indices = generate_grid_indices(GRID_WIDTH as u32, GRID_HEIGHT as u32);

    let mut terrain_mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );

    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::from(vertices));

    terrain_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; vertex_count]);

    // TODO: Why do we need this?
    terrain_mesh.insert_indices(Indices::U32(indices));

    commands.spawn((
        Mesh3d(meshes.add(terrain_mesh)),
        MeshMaterial3d(materials.add(StandardMaterial::from_color(Color::srgb(0.3, 0.7, 0.3)))),
    ));

    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(64.0, 50.0, 140.0).looking_at(Vec3::new(64.0, 0.0, 64.0), Vec3::Y),
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.9, 0.9, 0.9),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.0, -0.5, 0.0)),
    ));
}

fn generate_grid_indices(width: u32, height: u32) -> Vec<u32> {
    let mut indices = Vec::new();

    for z in 0..height - 1 {
        for x in 0..width - 1 {
            let i = z * width + x;

            indices.extend_from_slice(&[i, i + width, i + 1, i + 1, i + width, i + width + 1]);
        }
    }

    indices
}

fn print_terrain_storage_buffer(r_terrain_storage_buffer: Res<TerrainStorageBuffer>) {
    info!("{:?}", r_terrain_storage_buffer.into_inner().vertices[0]);
}

fn init_terrain_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let texture_bind_group_layout = BindGroupLayoutDescriptor::new(
        "TerrainLabel",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (storage_buffer::<TerrainStorageBuffer>(false),),
        ),
    );
    let shader = asset_server.load(SHADER_ASSET_PATH);
    let init_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![texture_bind_group_layout.clone()],
        shader: shader.clone(),
        entry_point: Some(Cow::from("init")),
        ..default()
    });

    commands.insert_resource(TerrainPipeline {
        texture_bind_group_layout,
        init_pipeline,
    });
}

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<TerrainPipeline>,
    r_terrain_storage_buffer: Res<TerrainStorageBuffer>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
) {
    let mut terrain_storage_buffer = StorageBuffer::from(r_terrain_storage_buffer.into_inner());
    terrain_storage_buffer.add_usages(BufferUsages::COPY_SRC);
    terrain_storage_buffer.write_buffer(&render_device, &queue);

    let bind_group_0 = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.texture_bind_group_layout),
        &BindGroupEntries::sequential((&terrain_storage_buffer,)),
    );

    commands.insert_resource(TerrainBindGroups([bind_group_0]));
}

impl Default for TerrainNode {
    fn default() -> Self {
        Self {
            state: TerrainState::Loading,
        }
    }
}

impl render_graph::Node for TerrainNode {
    fn update(&mut self, world: &mut World) {
        let pipeline = world.resource::<TerrainPipeline>();
        let pipeline_cache = world.resource::<PipelineCache>();

        // if the corresponding pipeline has loaded, transition to the next stage
        match self.state {
            TerrainState::Loading => {
                match pipeline_cache.get_compute_pipeline_state(pipeline.init_pipeline) {
                    CachedPipelineState::Ok(_) => {
                        self.state = TerrainState::Init;
                    }
                    // If the shader hasn't loaded yet, just wait.
                    CachedPipelineState::Err(PipelineCacheError::ShaderNotLoaded(_)) => {}
                    CachedPipelineState::Err(err) => {
                        panic!("Initializing assets/{SHADER_ASSET_PATH}:\n{err}")
                    }
                    _ => {}
                }
            }
            TerrainState::Init => {}
        }
    }

    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let bind_groups = &world.resource::<TerrainBindGroups>().0;
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<TerrainPipeline>();

        let mut pass = render_context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor::default());

        // select the pipeline based on the current state
        match self.state {
            TerrainState::Loading => {}
            TerrainState::Init => {
                let init_pipeline = pipeline_cache
                    .get_compute_pipeline(pipeline.init_pipeline)
                    .unwrap();
                pass.set_bind_group(0, &bind_groups[0], &[]);
                pass.set_pipeline(init_pipeline);
                pass.dispatch_workgroups(SIZE.x / WORKGROUP_SIZE, SIZE.y / WORKGROUP_SIZE, 1);
            }
        }

        Ok(())
    }
}
