// Nonlinear refraction, rim response and frost/tone adapted from Glass HQ
// liquid-glass a4331da89ee47bcaaef3276de522257f37b0ee26 (MIT).
// Copyright (c) 2026 Glass HQ. See ../../README.md for acknowledgement and license.
// Native rounded geometry, premultiplied composition and legacy path are Harbor's.
struct Params {
    crop: vec4<f32>,
    shape: vec4<f32>,
    radii: vec4<f32>,
    settings: vec4<f32>, // physical sigma, scale, logical edge width/displacement
    material: vec4<f32>, // physical fill sigma, signed frost mix, highlight/shade
    tone: vec4<f32>, // straight RGB offset, slope, quadratic, saturation
    clip_rect0: vec4<f32>,
    clip_radii0: vec4<f32>,
    clip_rect1: vec4<f32>,
    clip_radii1: vec4<f32>,
    clip_meta: vec4<u32>, // count, modes, flags (1 material, 2 sRGB attachment)
};
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var original: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var fill: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[i], 0.0, 1.0);
}

fn gaussian(uv: vec2<f32>, axis: vec2<f32>, requested_sigma: f32) -> vec4<f32> {
    // Pair adjacent taps, covering every texel of the three-sigma support.
    let sigma = max(requested_sigma, 0.001);
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
    let aa = max(fwidth(distance), 0.0001);
    return select(1.0 - smoothstep(-aa * 0.5, aa * 0.5, distance), select(0.0, 1.0, distance <= 0.0), mode == 1u);
}

@fragment
fn fs_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(p.xy / params.crop.zw, vec2<f32>(1.0, 0.0), params.settings.x);
}
@fragment
fn fs_fill_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(p.xy / params.crop.zw, vec2<f32>(1.0, 0.0), params.material.x);
}
@fragment
fn fs_fill_vertical(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(p.xy / params.crop.zw, vec2<f32>(0.0, 1.0), params.material.x);
}

// Signed rounded-rectangle distance + outward normal in logical coordinates.
// Preserve the old circular corners; no web outline tessellation is necessary.
fn edge_field(p: vec2<f32>) -> vec3<f32> {
    let local = p - params.shape.xy - params.shape.zw * 0.5;
    let right = local.x > 0.0;
    let bottom = local.y > 0.0;
    let radius = select(select(params.radii.x, params.radii.y, right),
        select(params.radii.w, params.radii.z, right), bottom);
    let d = abs(local) - params.shape.zw * 0.5 + vec2<f32>(radius);
    let outside = max(d, vec2<f32>(0.0));
    let distance = length(outside) + min(max(d.x, d.y), 0.0) - radius;
    var normal = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), d.x > d.y);
    if length(outside) > 0.0001 { normal = normalize(outside); }
    normal *= sign(local);
    return vec3<f32>(distance, normal);
}

fn edge_offset(field: vec3<f32>) -> vec2<f32> {
    let width = params.settings.z;
    let strength = params.settings.w;
    if width <= 0.0 || strength <= 0.0 { return vec2<f32>(0.0); }
    let depth = max(-field.x, 0.0);
    var falloff = 1.0 - smoothstep(0.0, width, depth);
    if (params.clip_meta.w & 1u) != 0u {
        // Glass HQ's nonlinear lens profile, rescaled to the bounded edge band.
        let t = clamp(depth / width, 0.0, 1.0);
        falloff = 1.0 - clamp(sqrt((2.0 - t) * t), 0.0, 1.0);
    }
    return -field.yz * strength * falloff;
}

fn straight(color: vec4<f32>) -> vec3<f32> {
    return color.rgb / max(color.a, 0.00001);
}
fn to_srgb(v: vec3<f32>) -> vec3<f32> {
    return select(12.92 * v, 1.055 * pow(max(v, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, v > vec3<f32>(0.0031308));
}
fn to_linear(v: vec3<f32>) -> vec3<f32> {
    return select(v / 12.92, pow((v + 0.055) / 1.055, vec3<f32>(2.4)), v > vec3<f32>(0.04045));
}

fn material_color(base: vec4<f32>, broad: vec4<f32>) -> vec4<f32> {
    var rgb = straight(base);
    var wide = straight(broad);
    let srgb_attachment = (params.clip_meta.w & 2u) != 0u;
    if srgb_attachment { rgb = to_srgb(rgb); wide = to_srgb(wide); }
    let frosted = select(min(rgb, wide), max(rgb, wide), params.material.y >= 0.0);
    rgb = mix(rgb, frosted, abs(params.material.y));
    rgb = clamp(params.tone.x + params.tone.y * rgb + params.tone.z * rgb * rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let luminance = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = clamp(mix(vec3<f32>(luminance), rgb, params.tone.w), vec3<f32>(0.0), vec3<f32>(1.0));
    if srgb_attachment { rgb = to_linear(rgb); }
    // Color transfer never manufactures opacity in transparent source pixels.
    return vec4<f32>(rgb * base.a, base.a);
}

fn rim_color(field: vec3<f32>, aa: f32) -> vec4<f32> {
    let d = field.x;
    let n = field.yz;
    let spread = 1.08;
    let directional = pow(clamp((abs(n.y) - cos(spread)) / (1.0 - cos(spread)), 0.0, 1.0), 1.25);
    let diffuse_dir = pow(clamp((abs(n.y) - cos(spread * 0.65)) / (1.0 - cos(spread * 0.65)), 0.0, 1.0), 1.25);
    let stroke = clamp((d + 0.9) / aa + 0.5, 0.0, 1.0);
    var specular = directional * stroke;
    specular /= 1.0 + 0.5 * (1.0 - specular);
    let diffuse = diffuse_dir * 0.18 * pow(1.0 - clamp(-d / 4.5, 0.0, 1.0), 2.0);
    let highlight = clamp((specular + diffuse * 0.4) * params.material.z, 0.0, 1.0);
    // Keep the dark rim inside the native bounds (no outside SVG map margin),
    // including the first covered texel at 2x DPI.
    let adjusted = d + 0.25;
    let falloff = 1.0 - clamp(-adjusted / 0.5, 0.0, 1.0);
    let ramp = mix(select(0.0, 1.0, falloff > 0.0), falloff, 0.75);
    let outline = ramp * clamp((adjusted + 0.5) / aa + 0.5, 0.0, 1.0) * clamp(-adjusted / aa + 0.5, 0.0, 1.0);
    let side_spread = 0.98;
    let weights = clamp((vec2<f32>(n.x, -n.x) - cos(side_spread)) / (1.0 - cos(side_spread)), vec2<f32>(0.0), vec2<f32>(1.0)) * outline;
    let shade = dot(weights / (vec2<f32>(1.0) + 0.4 * (vec2<f32>(1.0) - weights)), vec2<f32>(1.0)) * params.material.w;
    // Premultiplied white light over a black rim. Coverage is applied once below.
    return vec4<f32>(vec3<f32>(highlight), highlight + shade * (1.0 - highlight));
}

@fragment
fn fs_composite(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (p.xy - params.crop.xy) / params.crop.zw;
    let logical = p.xy / params.settings.y;
    let field = edge_field(logical);
    let aa = max(fwidth(field.x), 1.0 / params.settings.y);
    var mask = coverage(logical, params.shape, params.radii, 2u);
    if params.clip_meta.x > 0u {
        mask *= coverage(logical, params.clip_rect0, params.clip_radii0, params.clip_meta.y);
    }
    if params.clip_meta.x > 1u {
        mask *= coverage(logical, params.clip_rect1, params.clip_radii1, params.clip_meta.z);
    }
    let before = textureSampleLevel(original, linear_sampler, uv, 0.0);
    let refracted_uv = uv + edge_offset(field) * params.settings.y / params.crop.zw;
    var color = gaussian(refracted_uv, vec2<f32>(0.0, 1.0), params.settings.x);
    if (params.clip_meta.w & 1u) != 0u {
        let broad = textureSampleLevel(fill, linear_sampler, refracted_uv, 0.0);
        color = material_color(color, broad);
        let rim = rim_color(field, aa);
        color = rim + color * (1.0 - rim.a);
    }
    // Replace prior premultiplied pixels; source-over would count the backdrop twice.
    return mix(before, color, mask);
}

@fragment
fn fs_blit(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(p.xy), 0);
}
