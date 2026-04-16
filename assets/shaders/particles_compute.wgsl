struct Particle {
    // NOTE: Assumption: position is in clip space,
    //  because they are only forwarded in the vertex shader
    position: vec4<f32>,
};

// TODO: Check if group is correct
@group(0) @binding(0)
var<storage, read_write> particles: array<Particle>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    
    let x = particles[i].position.x;

    particles[i].position.y = sin(x * 8.0);
}