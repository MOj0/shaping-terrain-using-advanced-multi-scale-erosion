use bevy::color::palettes::css::*;
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
use bevy::input::common_conditions;
use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui;
use bevy_inspector_egui::quick::WorldInspectorPlugin;

mod camera;
mod shaders;
mod terrain;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(bevy::log::LogPlugin {
                level: bevy::log::Level::INFO,
                ..default()
            }),
            FpsOverlayPlugin {
                config: FpsOverlayConfig {
                    frame_time_graph_config: FrameTimeGraphConfig {
                        enabled: false,
                        ..default()
                    },
                    ..default()
                },
            },
            WireframePlugin::default(),
            bevy_egui::EguiPlugin::default(),
            WorldInspectorPlugin::default()
                .run_if(common_conditions::input_toggle_active(false, KeyCode::Tab)),
            shaders::ShaderPlugin,
            terrain::TerrainPlugin,
            camera::CameraOrbitPlugin,
        ))
        .init_state::<AppState>()
        .init_resource::<DebugConfig>()
        .insert_resource(ClearColor(Color::srgb_u8(102, 178, 212)))
        .insert_resource(WireframeConfig {
            global: false,
            default_color: BLACK.into(),
        })
        .add_systems(Startup, (setup, set_window_maximized))
        .add_systems(
            Update,
            toggle_wireframe.run_if(common_conditions::input_just_pressed(KeyCode::KeyW)),
        )
        .add_systems(
            Update,
            update_wireframe_config.run_if(
                bevy::ecs::schedule::common_conditions::resource_exists_and_changed::<DebugConfig>,
            ),
        )
        .run();
}

/// Enum specifying in what state the application is
#[derive(States, Clone, Copy, Default, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    LoadingAssets,
    InitShaderResources,
    GeneratingTerrain,
    Running,
}

#[derive(Resource, Reflect, PartialEq, Debug)]
#[reflect(Resource)]
pub struct DebugConfig {
    is_wireframe_on: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            is_wireframe_on: false,
        }
    }
}

fn setup(mut commands: Commands) {
    // Camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 5.0, 50.0).looking_at(Vec3::ZERO, Vec3::Y),
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

fn set_window_maximized(mut q_windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>) {
    for mut window in q_windows.iter_mut() {
        window.set_maximized(true);
    }
}

fn toggle_wireframe(mut r_terrain_config: ResMut<DebugConfig>) {
    r_terrain_config.is_wireframe_on = !r_terrain_config.is_wireframe_on;
}

fn update_wireframe_config(
    r_terrain_config: ResMut<DebugConfig>,
    mut r_wireframe_config: ResMut<WireframeConfig>,
) {
    r_wireframe_config.global = r_terrain_config.is_wireframe_on;
}
