use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin, FrameTimeGraphConfig};
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
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
            bevy_egui::EguiPlugin::default(),
            WorldInspectorPlugin::default().run_if(
                bevy::input::common_conditions::input_toggle_active(false, KeyCode::Tab),
            ),
            shaders::ShaderPlugin,
            terrain::TerrainPlugin,
            camera::CameraOrbitPlugin,
        ))
        .init_state::<AppState>()
        .insert_resource(ClearColor(Color::srgb_u8(102, 178, 212)))
        .add_systems(Startup, (setup, set_window_maximized))
        .run();
}

/// Enum specifying in what state the application is
#[derive(States, Clone, Copy, Default, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    LoadingAssets,
    GeneratingTerrain,
    Running,
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
