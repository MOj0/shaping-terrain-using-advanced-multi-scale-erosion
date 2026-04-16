#import bevy_pbr::mesh_functions
#import bevy_pbr::view_transformations::position_world_to_clip


@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var <uniform> time: f32;


struct VertexInput {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
}

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
    time: f32,
) -> vec3<f32> {
    let noise_pos = position * 1.7 + vec3<f32>(time * 0.2);
    let noise = noise3d(noise_pos) * 2.0 - 1.0; // -1 to 1

    return position + normal * noise;
}


@vertex
fn vertex(in: VertexInput) -> VertexOutput{
    var out: VertexOutput;

    let model = mesh_functions::get_world_from_local(in.instance_index);

    let local_position = apply_noise_displacement(in.position,in.normal,time);
    let world_position = mesh_functions::mesh_position_local_to_world(
            model,
            vec4<f32>(local_position, 1.0)
        );

    out.clip_position = position_world_to_clip(world_position.xyz);
    out.normal = in.normal;

    return out;
}


@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32>{
    let blue_shade = vec3<f32>(0.1, 0.1, in.normal.y * 3.0);

    return vec4<f32>(max(vec3<f32>(0.1, 0.05, 0.2), blue_shade), 1.0);
}