#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read_write> heights: array<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) normal: vec3<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // Read data from SSBO
    let height = heights[vertex.vertex_index];
    
    let local_position = vertex.position + vertex.normal * height;
    // let local_position = vertex.position + vertex.normal * -0.6;
    // let local_position = vertex.position + vec3<f32>(height, 0.0, 0.0);
    // let local_position = vertex.position;

    var world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4(local_position, 1.0));
    out.clip_position = position_world_to_clip(out.world_position.xyz);

    out.normal = vertex.normal;

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4(0.2 * (vec3(3.0) + 2.0 * in.normal), 1.0);
}