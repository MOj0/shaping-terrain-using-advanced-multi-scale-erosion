@group(0) @binding(0) var<storage, read_write> in_terrain: array<f32>;
@group(0) @binding(1) var<storage, read_write> out_terrain : array<f32>;

@group(0) @binding(2) var<storage, read_write> out_debug : array<f32>;

@group(0) @binding(3) var<uniform> params: ThermalUniforms;


struct ThermalUniforms {
    nx: i32,
    ny: i32,
    a: vec2<f32>,
    b: vec2<f32>,
    cell_size: vec2<f32>,

    eps: f32,
    tan_threshold_angle: f32,
    noisified_angle: i32,
    noise_min: f32,
    noise_max: f32,
    noise_wavelength: f32,
    use_threshold_map: i32,

    debug: f32,
};


const next8 : array<vec2<i32>, 8> = array<vec2<i32>, 8>(
    vec2<i32>(0, 1), vec2<i32>(1, 1), vec2<i32>(1, 0), vec2<i32>(1, -1),
    vec2<i32>(0, -1), vec2<i32>(-1, -1), vec2<i32>(-1, 0), vec2<i32>(-1, 1)
);


fn mod289_3(x: vec3<f32>) -> vec3<f32> {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn mod289_4(x: vec4<f32>) -> vec4<f32> {
    return x - floor(x * (1.0 / 289.0)) * 289.0;
}

fn permute(x: vec4<f32>) -> vec4<f32> {
    return mod289_4(((x * 34.0) + 1.0) * x);
}

fn taylorInvSqrt(r: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(1.79284291400159) - 0.85373472095314 * r;
}

// 3D simplex noise
fn snoise(v: vec3<f32>) -> f32 {
    let C = vec2<f32>(1.0 / 6.0, 1.0 / 3.0);
    let D = vec4<f32>(0.0, 0.5, 1.0, 2.0);

    var i = floor(v + dot(v, vec3<f32>(C.y)));
    var x0 = v - i + dot(i, vec3<f32>(C.x));

    let g = step(x0.yzx, x0.xyz);
    let l = vec3<f32>(1.0) - g;
    let i1 = min(g, l.zxy);
    let i2 = max(g, l.zxy);

    let x1 = x0 - i1 + vec3<f32>(C.x);
    let x2 = x0 - i2 + vec3<f32>(C.y);
    let x3 = x0 - vec3<f32>(D.y);

    i = mod289_3(i);

    let p = permute(
        permute(
            permute(i.z + vec4<f32>(0.0, i1.z, i2.z, 1.0))
            + i.y + vec4<f32>(0.0, i1.y, i2.y, 1.0)
        )
        + i.x + vec4<f32>(0.0, i1.x, i2.x, 1.0)
    );

    let n_ = 0.142857142857;
    let ns = n_ * D.wyz - D.xzx;

    let j = p - 49.0 * floor(p * ns.z * ns.z);

    let x_ = floor(j * ns.z);
    let y_ = floor(j - 7.0 * x_);

    let x = x_ * ns.x + ns.yyyy;
    let y = y_ * ns.x + ns.yyyy;
    let h = vec4<f32>(1.0) - abs(x) - abs(y);

    let b0 = vec4<f32>(x.xy, y.xy);
    let b1 = vec4<f32>(x.zw, y.zw);

    let s0 = floor(b0) * 2.0 + 1.0;
    let s1 = floor(b1) * 2.0 + 1.0;
    let sh = -step(h, vec4<f32>(0.0));

    let a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    let a1 = b1.xzyw + s1.xzyw * sh.zzww;

    var p0 = vec3<f32>(a0.xy, h.x);
    var p1 = vec3<f32>(a0.zw, h.y);
    var p2 = vec3<f32>(a1.xy, h.z);
    var p3 = vec3<f32>(a1.zw, h.w);

    let norm = taylorInvSqrt(vec4<f32>(
        dot(p0, p0), dot(p1, p1), dot(p2, p2), dot(p3, p3)
    ));

    p0 *= norm.x;
    p1 *= norm.y;
    p2 *= norm.z;
    p3 *= norm.w;

    var m = max(
        vec4<f32>(0.6) - vec4<f32>(
            dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)
        ),
        vec4<f32>(0.0)
    );

    m = m * m;

    return 42.0 * dot(
        m * m,
        vec4<f32>(
            dot(p0, x0), dot(p1, x1), dot(p2, x2), dot(p3, x3)
        )
    );
}



fn toIndex1D(i: i32, j: i32) -> i32 {
    return i + params.nx * j;
}

fn toIndex1D_v(p: vec2<i32>) -> i32 {
    return p.x + params.nx * p.y;
}

fn arrayPoint(p: vec2<i32>) -> vec2<f32> {
    return params.a + vec2<f32>(p) * params.cell_size;
}

fn height(p: vec2<i32>) -> f32 {
	return in_terrain[toIndex1D_v(p)];
}

fn point(p: vec2<i32>) -> vec3<f32> {
	return vec3(arrayPoint(p), height(p));
}


@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
    let p = arrayPoint(vec2(x, y));

    var distances: array<f32, 9>;
    var samples: array<f32, 9>;

    for (var i = 0; i < 3; i++) {
        for (var j = 0; j < 3; j++) {
            var tap = vec2(x, y) + vec2(i, j) - vec2(1, 1);

            tap = (tap + vec2(params.nx, params.ny)) % vec2(params.nx, params.ny);

            let idx = i * 3 + j;
            samples[idx] = height(tap);
            distances[idx] = length(p - arrayPoint(tap));
        }
    }

    var tanAngle = params.tan_threshold_angle;

    if (params.noisified_angle != 0) {
        let pt = point(vec2(x, y));
        let t = snoise(pt * params.noise_wavelength) * 0.5 + 0.5;
        tanAngle = mix(params.noise_min, params.noise_max, t);
    }

    let z = height(vec2(x, y));

    var receiveMul: f32 = 0.0;
    var distributeMul: f32 = 0.0;

    for (var i = 0; i < 9; i++) {
        let d = distances[i];
        var zd = samples[i] - z;

        if (zd / d > tanAngle) {
            receiveMul += 1.0;
        }

        zd = z - samples[i];

        if (zd / d > tanAngle) {
            distributeMul += 1.0;
        }
    }

    let matter = params.eps * params.cell_size.x * params.cell_size.y;

    out_terrain[id] = in_terrain[id] + matter * (receiveMul - distributeMul);
}