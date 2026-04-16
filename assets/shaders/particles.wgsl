struct Particle {
    position: vec4<f32>,
};

// TODO: Check if group is correct
@group(0) @binding(0)
var<storage, read> particles: array<Particle>;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) idx: u32) -> VertexOut {
    var out: VertexOut;
    out.clip = particles[idx].position;
    return out;
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}