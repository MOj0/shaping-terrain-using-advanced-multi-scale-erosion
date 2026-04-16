mod compute_mesh;
mod cube_colors;
mod particles_wip;
mod random_colors;
mod terrain;

use bevy::prelude::*;

fn main() {
    // terrain::run();
    // cube_colors::run();
    // random_colors::run();
    compute_mesh::run();

    // App::new()
    //     .add_plugins(DefaultPlugins)
    //     .add_systems(Startup, logging_shenanigans)
    //     .run();
}

fn logging_shenanigans() {
    let a = 1.0;
    let b = 2;
    let c = 123;

    info!(?a, %a, ?a, ?a, "processing mesh");

    info!(
        target: "a different target in the logging system",
        ip = ?a,
        b,
        ?c,
    );
}
