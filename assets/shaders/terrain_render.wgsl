#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read_write> buffer_data: array<u32>;

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

// Simple hash function for noise
fn hash(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
}

fn noise3d(p: vec3<f32>) -> f32 {
    let ip = floor(p);
    var fp = fract(p);
    fp = fp * fp * (3.0 - 2.0 * fp); // Smoothstep

    let n = ip.x + ip.y * 57.0 + ip.z * 113.0;
    
    return mix(
        mix(
            mix(hash(n), hash(n + 1.0), fp.x),
            mix(hash(n + 57.0), hash(n + 58.0), fp.x),
            fp.y
        ),
        mix(
            mix(hash(n + 113.0), hash(n + 114.0), fp.x),
            mix(hash(n + 170.0), hash(n + 171.0), fp.x),
            fp.y
        ),
        fp.z
    );
}

fn apply_noise_displacement(
    position: vec3<f32>,
    normal: vec3<f32>,
) -> vec3<f32> {
    let noise = noise3d(position) * 0.8; // 0 to 1

    return position + normal * noise;
}


@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // Read data from SSBO
    let height = f32(buffer_data[vertex.vertex_index]) * 0.05;
    
    // let local_position = apply_noise_displacement(vertex.position, vertex.normal);
    let local_position = vertex.position + vertex.normal * height;

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