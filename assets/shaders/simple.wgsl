@group(0) @binding(0) var output: texture_storage_2d<rgba32float, write>;

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

@compute @workgroup_size(8, 8, 1)
fn init(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let location = vec2<i32>(i32(invocation_id.x), i32(invocation_id.y));

    let randomNumber1 = randomFloat((invocation_id.z << 16u) | invocation_id.x);
    let randomNumber2 = randomFloat((invocation_id.x << 16u) | invocation_id.y);
    let randomNumber3 = randomFloat((invocation_id.y << 16u) | invocation_id.z);
    let color = vec4(randomNumber1, randomNumber2, randomNumber3, 1.0);

    textureStore(output, location, color);
}
