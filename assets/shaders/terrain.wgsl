struct Vertex {
    position: vec3<f32>,
};

struct TerrainBuffer {
    vertices: array<Vertex>,
};

@group(0) @binding(0)
var<storage, read_write> terrain: TerrainBuffer;


const GRID_WIDTH: u32 = 128u;
const GRID_HEIGHT: u32 = 128u;
const SCALE: f32 = 1.0;
const HEIGHT_SCALE: f32 = 4.0;


fn hash(value: u32) -> u32 {
    var state = value;
    state = state ^ 2747636419u;
    state = state * 2654435769u;
    state = state ^ (state >> 16u);
    state = state * 2654435769u;
    state = state ^ (state >> 16u);
    state = state * 2654435769u;
    return state;
}

fn randomFloat(value: u32) -> f32 {
        return f32(hash(value)) / 4294967295.0;
}

fn terrain_height(x: f32, z: f32) -> f32 {
    return sin(x * 0.1) * cos(z * 0.1) * HEIGHT_SCALE;
}

@compute @workgroup_size(8, 8, 1)
fn init(@builtin(global_invocation_id) id: vec3<u32>) {
    // let x = id.x;
    // let z = id.y; // Take z as y, because of Bevy's 3D coordinate system?

    // if (x >= GRID_WIDTH || z >= GRID_HEIGHT) {
    //     return;
    // }

    // let index = z * GRID_WIDTH + x;

    // let world_x = f32(x) * SCALE;
    // let world_z = f32(z) * SCALE;
    // let y = terrain_height(world_x, world_z);

    // terrain.vertices[index].position = vec3<f32>(
    //     world_x,
    //     y,
    //     world_z
    // );

    terrain.vertices[0].position = vec3<f32>(
        1.0, 2.0, 3.0
    );
}
