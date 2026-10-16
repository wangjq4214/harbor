struct Params {
    crop: vec4<f32>,
    shape: vec4<f32>,
    radii: vec4<f32>,
    settings: vec4<f32>, // physical sigma, scale
    clip_rect0: vec4<f32>,
    clip_radii0: vec4<f32>,
    clip_rect1: vec4<f32>,
    clip_radii1: vec4<f32>,
    clip_meta: vec4<u32>,
};
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var original: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[i], 0.0, 1.0);
}

fn gaussian(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    // Cover every source texel in the three-sigma support. Scaling a fixed
    // sparse kernel with sigma repeats thin text as a comb/grid instead of
    // softening it. Pair adjacent Gaussian taps using hardware linear sampling
    // so each pair needs only one fetch on either side of the center.
    let sigma = max(params.settings.x, 0.001);
    let radius = i32(ceil(sigma * 3.0));
    var sum = textureSampleLevel(source, linear_sampler, uv, 0.0);
    var weight_sum = 1.0;
    for (var i = 1; i <= radius; i += 2) {
        let a = f32(i) / sigma;
        let b = f32(i + 1) / sigma;
        let first = exp(-0.5 * a * a);
        let second = select(0.0, exp(-0.5 * b * b), i + 1 <= radius);
        let weight = first + second;
        if weight > 0.0 {
            let distance = (f32(i) * first + f32(i + 1) * second) / weight;
            let offset = axis * distance / params.crop.zw;
            sum += (textureSampleLevel(source, linear_sampler, uv + offset, 0.0)
                + textureSampleLevel(source, linear_sampler, uv - offset, 0.0)) * weight;
            weight_sum += 2.0 * weight;
        }
    }
    return sum / weight_sum;
}

fn coverage(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>, mode: u32) -> f32 {
    let q = p - rect.xy;
    let right = q.x > rect.z * 0.5;
    let bottom = q.y > rect.w * 0.5;
    let radius = select(select(radii.x, radii.y, right), select(radii.w, radii.z, right), bottom);
    let d = abs(q - rect.zw * 0.5) - rect.zw * 0.5 + vec2<f32>(radius);
    let distance = length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - radius;
    // Derivatives are evaluated uniformly, including hard-edge clips.
    let aa = max(fwidth(distance), 0.0001);
    return select(1.0 - smoothstep(-aa * 0.5, aa * 0.5, distance), select(0.0, 1.0, distance <= 0.0), mode == 1u);
}

@fragment
fn fs_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(p.xy / params.crop.zw, vec2<f32>(1.0, 0.0));
}

@fragment
fn fs_composite(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (p.xy - params.crop.xy) / params.crop.zw;
    let logical = p.xy / params.settings.y;
    var mask = coverage(logical, params.shape, params.radii, 2u);
    if params.clip_meta.x > 0u {
        mask *= coverage(logical, params.clip_rect0, params.clip_radii0, params.clip_meta.y);
    }
    if params.clip_meta.x > 1u {
        mask *= coverage(logical, params.clip_rect1, params.clip_radii1, params.clip_meta.z);
    }
    let before = textureSampleLevel(original, linear_sampler, uv, 0.0);
    // Replace the prior premultiplied pixels, including alpha; source-over would
    // incorrectly retain sharp details and count the old background twice.
    return mix(before, gaussian(uv, vec2<f32>(0.0, 1.0)), mask);
}

@fragment
fn fs_blit(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0);
}
