use bevy::{prelude::*, render::render_resource::*, shader::ShaderRef};

const SHADER_ASSET_PATH: &str = "shaders/vertex_displacement.wgsl";

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(MaterialPlugin::<MyVertexMaterial>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, update_time)
        .run();
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct MyVertexMaterial {
    #[uniform(0)]
    pub time: f32,
}

impl Material for MyVertexMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MyVertexMaterial>>,
) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-3.0, 2.5, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: 2000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    // Spawn a sphere with the vertex material
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(64, 32))),
        MeshMaterial3d(materials.add(MyVertexMaterial::default())),
    ));
}

fn update_time(time: Res<Time>, mut materials: ResMut<Assets<MyVertexMaterial>>) {
    for (_, material) in materials.iter_mut() {
        material.time = time.elapsed_secs();
    }
}
