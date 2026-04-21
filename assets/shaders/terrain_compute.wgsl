@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<uniform> erosion_uniforms: ErosionUniforms;

struct ErosionUniforms {
    foo: u32,
    cell_size: vec2<f32>
}


@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let id = to_index_1d(x, y);

    data[id] = erosion_uniforms.foo;
}


fn to_index_1d(x: u32, y: u32) -> u32 {
    return x + 256u * y;
}


