use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui;

pub struct CameraOrbitPlugin;

impl Plugin for CameraOrbitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraSettings>().add_systems(
            Update,
            (
                orbit.run_if(bevy::input::common_conditions::input_pressed(
                    MouseButton::Left,
                )),
                pan.run_if(bevy::input::common_conditions::input_pressed(
                    MouseButton::Right,
                )),
                zoom,
            )
                .run_if(not(bevy_egui::input::egui_wants_any_pointer_input)),
        );
    }
}

#[derive(Debug, Resource, Reflect)]
#[reflect(Resource)]
struct CameraSettings {
    pub orbit_distance: f32,
    pub pitch_speed: f32,
    pub yaw_speed: f32,
    pub target: Vec3,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            orbit_distance: 80.0,
            pitch_speed: 0.003,
            yaw_speed: 0.004,
            target: Vec3::new(2000.0, 4400.0, 18000.0),
        }
    }
}

fn orbit(
    mut camera: Single<&mut Transform, With<Camera>>,
    camera_settings: Res<CameraSettings>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    let delta = -mouse_motion.delta; // Invert the delta for something more sensible

    // Mouse motion is one of the few inputs that should not be multiplied by delta time,
    // as we are already receiving the full movement since the last frame was rendered. Multiplying
    // by delta time here would make the movement slower that it should be.
    let delta_pitch = delta.y * camera_settings.pitch_speed;
    let delta_yaw = delta.x * camera_settings.yaw_speed;

    // Obtain the existing pitch, yaw, and roll values from the transform.
    let (yaw, pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

    let yaw = yaw + delta_yaw;
    let pitch = pitch + delta_pitch;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);

    // Adjust the translation to maintain the correct orientation toward the orbit target.
    // In our example it's a static target, but this could easily be customized.
    camera.translation = camera_settings.target - camera.forward() * camera_settings.orbit_distance;
}

fn pan(
    mut camera: Single<&mut Transform, With<Camera>>,
    mut camera_settings: ResMut<CameraSettings>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    // Get delta
    let delta = mouse_motion.delta * 50.0;

    // Update the orbit distance in settings resource
    camera_settings.target += camera.rotation * Vec3::new(-delta.x, delta.y, 0.0);

    // Update camera's translation
    camera.translation = camera_settings.target - camera.forward() * camera_settings.orbit_distance;
}

fn zoom(
    mut camera: Single<&mut Transform, With<Camera>>,
    mut camera_settings: ResMut<CameraSettings>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
) {
    // Get scroll delta, also invert
    let scroll = -mouse_scroll.delta.y * 200.0;

    // Update the orbit distance in settings resource
    camera_settings.orbit_distance += scroll;

    // Update camera's translation
    camera.translation = camera_settings.target - camera.forward() * camera_settings.orbit_distance;
}
