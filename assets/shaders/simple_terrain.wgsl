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

fn terrain_height(x: f32, z: f32) -> f32 {
    return sin(x * 0.1) * cos(z * 0.1) * HEIGHT_SCALE;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = id.x;
    let z = id.y;

    if (x >= GRID_WIDTH || z >= GRID_HEIGHT) {
        return;
    }

    let index = z * GRID_WIDTH + x;

    let world_x = f32(x) * SCALE;
    let world_z = f32(z) * SCALE;
    let y = terrain_height(world_x, world_z);

    terrain.vertices[index].position = vec3<f32>(
        world_x,
        y,
        world_z
    );
}