
@group(0) @binding(0) var<storage, read_write> in_terrain: array<f32>;
@group(0) @binding(1) var<storage, read_write> out_terrain : array<f32>;

@group(0) @binding(2) var<storage, read_write> in_stream : array<f32>;
@group(0) @binding(3) var<storage, read_write> out_stream : array<f32>;

@group(0) @binding(4) var<storage, read_write> in_hardness : array<f32>;

@group(0) @binding(5) var<uniform> params: ErosionUniforms;


struct ErosionUniforms {
    nx: i32,
    ny: i32,
    a: vec2<f32>,
    b: vec2<f32>,
    cell_size: vec2<f32>,
    flow_p: f32,
    k: f32,
    p_sa: f32,
    p_sl: f32,
    dt: f32,
    max_spe: f32,

    debug: f32,
};


const next8 : array<vec2<i32>, 8> = array<vec2<i32>, 8>(
    vec2<i32>(0, 1), vec2<i32>(1, 1), vec2<i32>(1, 0), vec2<i32>(1, -1),
    vec2<i32>(0, -1), vec2<i32>(-1, -1), vec2<i32>(-1, 0), vec2<i32>(-1, 1)
);


fn toIndex1D(i: i32, j: i32) -> i32 {
    return i + params.nx * j;
}

fn toIndex1D_v(p: vec2<i32>) -> i32 {
    return p.x + params.nx * p.y;
}

fn arrayPoint(p: vec2<i32>) -> vec2<f32> {
    return params.a + vec2<f32>(p) * params.cell_size;
}

fn slope(p: vec2<i32>, q: vec2<i32>) -> f32 {
    if (p.x < 0 || p.x >= params.nx || p.y < 0 || p.y >= params.ny) { return 0.0; }
    if (q.x < 0 || q.x >= params.nx || q.y < 0 || q.y >= params.ny) { return 0.0; }
    if (all(p == q)) { return 0.0; }

    let index_p = toIndex1D_v(p);
    let index_q = toIndex1D_v(q);
    let d = length(arrayPoint(q) - arrayPoint(p));
    return (in_terrain[index_q] - in_terrain[index_p]) / d;
}

fn streamAt(p: vec2<i32>) -> f32 {
    if (p.x < 0 || p.x >= params.nx || p.y < 0 || p.y >= params.ny) {
        return 0.0;
    }
    return in_stream[toIndex1D_v(p)];
}

fn getFlowSteepest(p: vec2<i32>) -> vec2<i32> {
    var d = vec2<i32>(0, 0);
    var maxSlope = 0.0;

    for (var i = 0; i < 8; i++) {
        let ss = slope(p + next8[i], p);
        if (ss > maxSlope) {
            maxSlope = ss;
            d = next8[i];
        }
    }
    return d;
}

fn getFlowWeighted(p: vec2<i32>) -> array<f32, 8> {
    var sn: array<f32, 8>;
    var slopeSum = 0.0;

    for (var i = 0; i < 8; i++) {
        let slope_i = slope(p + next8[i], p);
        if (slope_i > 0.0) {
            sn[i] = pow(abs(slope_i), params.flow_p);
            slopeSum += sn[i];
        } else {
            sn[i] = -1.0;
        }
    }

    if (slopeSum < 0.00001) {
        slopeSum = 1.0;
    }

    for (var i = 0; i < 8; i++) {
        sn[i] = sn[i] / slopeSum;
    }

    return sn;
}

fn computeIncomingFlowWeighted(p: vec2<i32>) -> f32 {
    var stream = 0.0;

    for (var i = 0; i < 8; i++) {
        let q = p + next8[i];
        let sn = getFlowWeighted(q);
        let ss = sn[(i + 4) % 8];

        if (ss > 0.0) {
            stream += ss * streamAt(q);
        }
    }

    return stream;
}



@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
    let x = i32(gid.x);
    let y = i32(gid.y);

    if (x < 0 || y < 0 || x >= params.nx || y >= params.ny) {
        return;
    }

    // NOTE: Parameters are not yet loaded
    if length(params.cell_size) <= 0.01 {
        return;
    }

    let id = toIndex1D(x, y);
    let p = vec2<i32>(x, y);

    // Flow accumulation
    var stream = length(params.cell_size);
    stream += computeIncomingFlowWeighted(p);

    // Steepest slope
    let d = getFlowSteepest(p);
    let receiver_height = in_terrain[toIndex1D_v(p + d)];
    let steepest_slope = abs(slope(p + d, p));

    // Stream power
    var spe = pow(stream, params.p_sa) *
              clamp(pow(steepest_slope, params.p_sl), 0.0, 1.0);

    spe = clamp(spe, 0.0, params.max_spe);
    spe *= params.k;

    // Height update
    let old_height = in_terrain[id];
    var new_height = old_height - params.dt * spe;
    new_height = max(new_height, receiver_height);

    out_terrain[id] = new_height;
    out_stream[id] = stream;

    /// DEBUG
    // out_stream[id] = stream;
    // out_stream[id] = steepest_slope;
    // out_stream[id] = receiver_height;
    // out_stream[id] = f32(d[1]);
    // out_stream[id] = stream;
    // out_stream[id] += 1.0;
    // out_stream[id] = in_stream[id] + 1.0;

    // out_terrain[id] = params.debug;
    // out_terrain[id] = in_terrain[id] - 0.1;
    // out_terrain[id] = in_terrain[id] + 1.0;
}
