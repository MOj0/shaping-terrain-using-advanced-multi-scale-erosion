@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var texture: texture_storage_2d<r32uint, read>;
// TODO: ^ Fix the texture format stuff, also this is not needed currently...

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {

    let value = textureLoad(texture, vec2<i32>(i32(global_id.x), 0));

    data[global_id.x] = value.r;
}