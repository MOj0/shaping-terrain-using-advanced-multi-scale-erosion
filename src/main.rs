mod compute_and_vertex;
mod compute_mesh;
mod cube_colors;
mod gpu_readback;
mod particles_wip;
mod random_colors;
mod storage_buffer;
mod terrain;
mod vertex_displacement;

use bevy::prelude::*;

fn main() {
    terrain::run();
    // cube_colors::run();
    // random_colors::run();
    // compute_mesh::run();
    // vertex_displacement::run();
    // storage_buffer::run();
    // gpu_readback::run();
    // compute_and_vertex::run();

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
