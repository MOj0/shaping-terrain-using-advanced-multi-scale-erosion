#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read_write> buffer_data: array<u32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let a = buffer_data[0];
    let b = buffer_data[1];
    let c = buffer_data[2];
    let local_position = vertex.position + vec3<f32>(f32(a), f32(a), 2.0 * -f32(a));

    var world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(local_position, 1.0));
    out.clip_position = position_world_to_clip(out.world_position.xyz);

    out.color = vec4<f32>(vertex.normal, 1.0);

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}