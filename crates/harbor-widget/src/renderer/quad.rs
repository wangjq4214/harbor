use crate::layout::Rect;
use crate::renderer::{Viewport, finite_difference, pack_active_clips};
use crate::scene::clip::RoundedClip;
use crate::scene::primitive::Primitive;
use crate::scene::{SceneDelta, SceneItem};
use bytemuck::Zeroable;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const QUAD_SHADER: &str = r#"
struct GradientPaint {
    endpoints: vec4<f32>,
    colors: array<vec4<f32>, 8>, // premultiplied linear RGBA
    offsets: array<vec4<f32>, 2>,
    stop_info: vec4<u32>, // stop count; zero selects the legacy solid path
}
@group(0) @binding(0) var<storage, read> paints: array<GradientPaint>;

fn gradient_offset(paint: GradientPaint, index: u32) -> f32 {
    return paint.offsets[index / 4u][index % 4u];
}

fn paint_color(local: vec2<f32>, size: vec2<f32>, solid: vec4<f32>, index: u32) -> vec4<f32> {
    let paint = paints[index];
    if paint.stop_info.x == 0u { return solid; }
    let direction = paint.endpoints.zw - paint.endpoints.xy;
    let t = dot(local / size - paint.endpoints.xy, direction) / dot(direction, direction);
    var color = paint.colors[paint.stop_info.x - 1u];
    if t <= gradient_offset(paint, 0u) {
        color = paint.colors[0u];
    } else {
        for (var i = 1u; i < paint.stop_info.x; i += 1u) {
            let offset = gradient_offset(paint, i);
            if t <= offset {
                let before = gradient_offset(paint, i - 1u);
                color = mix(paint.colors[i - 1u], paint.colors[i], clamp((t - before) / (offset - before), 0.0, 1.0));
                break;
            }
        }
    }
    // The existing pipeline uses straight-alpha blending. Unpremultiply only
    // after interpolation; transparent stop RGB must not bleed into the ramp.
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb / color.a, color.a);
}

struct VertexInput {
    @location(0) pos: vec2<f32>,
}

struct InstanceInput {
    @location(1) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radii: vec4<f32>, // top-left, top-right, bottom-right, bottom-left
    @location(4) border_width: f32,
    @location(5) logical_size: vec2<f32>,
    @location(6) shape_rect: vec4<f32>, // origin x/y and size w/h within the instance
    @location(7) blur_radius: f32,
    @location(8) mode: u32, // 0 = ordinary quad/border, 1 = outer shadow, 2 = dest-in clip mask, 3.. = Lucide icons, 65536 = gradient
    @location(9) clip_rect0: vec4<f32>, // local origin x/y and size w/h
    @location(10) clip_radii0: vec4<f32>,
    @location(11) clip_rect1: vec4<f32>,
    @location(12) clip_radii1: vec4<f32>,
    @location(13) clip_meta: vec4<u32>, // count, behavior0, behavior1, paint slot
    @location(14) occluder_rect: vec4<f32>,
    @location(15) occluder_radii: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) shape_origin: vec2<f32>,
    @location(6) shape_size: vec2<f32>,
    @location(7) blur_radius: f32,
    @interpolate(flat) @location(8) mode: u32,
    @location(9) clip_rect0: vec4<f32>,
    @location(10) clip_radii0: vec4<f32>,
    @location(11) clip_rect1: vec4<f32>,
    @location(12) clip_radii1: vec4<f32>,
    @interpolate(flat) @location(13) clip_meta: vec4<u32>,
    @location(14) occluder_rect: vec4<f32>,
    @location(15) occluder_radii: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    let unit = vert.pos + vec2<f32>(0.5, 0.5);
    let x = inst.rect.x + unit.x * inst.rect.z;
    let y = inst.rect.y + unit.y * inst.rect.w;
    return VertexOutput(
        vec4<f32>(x, y, 0.0, 1.0),
        inst.color,
        unit * inst.logical_size,
        inst.logical_size,
        inst.radii,
        inst.border_width,
        inst.shape_rect.xy,
        inst.shape_rect.zw,
        inst.blur_radius,
        inst.mode,
        inst.clip_rect0,
        inst.clip_radii0,
        inst.clip_rect1,
        inst.clip_radii1,
        inst.clip_meta,
        inst.occluder_rect,
        inst.occluder_radii,
    );
}

fn rounded_box_distance(point: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    // Select the radius by the point's quadrant, then use the standard
    // rounded-rectangle SDF. In particular, the max(q, 0) term keeps the
    // middle of each edge straight instead of measuring it from a corner.
    let half_size = size * 0.5;
    let centered = point - half_size;
    var radius = radii.x; // top-left
    if centered.x >= 0.0 {
        radius = select(radii.y, radii.z, centered.y >= 0.0);
    } else if centered.y >= 0.0 {
        radius = radii.w; // bottom-left
    }
    let q = abs(centered) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn stable_distance_aa(distance: f32, point: vec2<f32>) -> f32 {
    // `fwidth(distance)` is undefined at the two-triangle quad's internal
    // primitive boundary on some backends. A signed-distance field is
    // 1-Lipschitz, so its filter width cannot legitimately exceed the summed
    // derivatives of its linearly interpolated coordinates.
    let coordinate_width = max(fwidth(point.x) + fwidth(point.y), 0.001);
    return clamp(fwidth(distance), 0.001, coordinate_width);
}

// Precompiled Lucide SVG subset (assets/lucide). One distance union per icon
// keeps translucent intersections/round joins from accumulating alpha.
fn segment_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / dot(ab, ab), 0.0, 1.0);
    return length(p - a - ab * t) - 1.0;
}

fn lucide_distance(p: vec2<f32>, mode: u32) -> f32 {
    if mode == 3u {
        return min(
            segment_distance(p, vec2<f32>(12.0, 19.0), vec2<f32>(20.0, 19.0)),
            min(segment_distance(p, vec2<f32>(4.0, 17.0), vec2<f32>(10.0, 11.0)),
                segment_distance(p, vec2<f32>(10.0, 11.0), vec2<f32>(4.0, 5.0)))
        );
    }
    if mode == 4u {
        return min(segment_distance(p, vec2<f32>(5.0, 12.0), vec2<f32>(19.0, 12.0)),
                   segment_distance(p, vec2<f32>(12.0, 5.0), vec2<f32>(12.0, 19.0)));
    }
    if mode == 5u {
        return min(segment_distance(p, vec2<f32>(18.0, 6.0), vec2<f32>(6.0, 18.0)),
                   segment_distance(p, vec2<f32>(6.0, 6.0), vec2<f32>(18.0, 18.0)));
    }
    if mode == 7u {
        return min(segment_distance(p, vec2<f32>(18.0, 15.0), vec2<f32>(12.0, 9.0)),
                   segment_distance(p, vec2<f32>(12.0, 9.0), vec2<f32>(6.0, 15.0)));
    }
    if mode == 8u {
        return min(segment_distance(p, vec2<f32>(6.0, 9.0), vec2<f32>(12.0, 15.0)),
                   segment_distance(p, vec2<f32>(12.0, 15.0), vec2<f32>(18.0, 9.0)));
    }
    // Lucide dot's r=1, stroke-width=2 circle, filled at its zero-radius center.
    return length(p - vec2<f32>(12.0)) - 2.0;
}
fn rounded_clip_coverage(
    point: vec2<f32>,
    rect: vec4<f32>,
    radii: vec4<f32>,
    behavior: u32,
) -> f32 {
    if behavior == 0u {
        return 1.0;
    }
    if behavior == 3u {
        return 0.0;
    }
    let distance = rounded_box_distance(point - rect.xy, rect.zw, radii);
    if behavior == 1u {
        return select(1.0, 0.0, distance > 0.0);
    }
    let aa = stable_distance_aa(distance, point);
    return 1.0 - smoothstep(-aa, aa, distance);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.size.x <= 0.0 || in.size.y <= 0.0 {
        discard;
    }
    var clip_alpha = 1.0;
    if in.clip_meta.x > 0u {
        clip_alpha *= rounded_clip_coverage(in.local, in.clip_rect0, in.clip_radii0, in.clip_meta.y);
    }
    if in.clip_meta.x > 1u {
        clip_alpha *= rounded_clip_coverage(in.local, in.clip_rect1, in.clip_radii1, in.clip_meta.z);
    }
    if in.mode == 2u {
        return vec4<f32>(clip_alpha);
    }
    if clip_alpha <= 0.0 {
        discard;
    }
    if in.mode >= 3u && in.mode < 65536u {
        // Fit a square viewBox, preserving aspect ratio for direct primitives too.
        let side = min(in.size.x, in.size.y);
        let p = (in.local - (in.size - vec2<f32>(side)) * 0.5) * (24.0 / side);
        let distance = lucide_distance(p, in.mode);
        // One physical pixel of coverage, not smoothstep's two-pixel halo.
        // Coordinate derivatives stay stable where min/abs distance gradients
        // cancel across thin strokes, joins, or the quad's triangle boundary.
        let pixel_width = max(max(fwidth(p.x), fwidth(p.y)), 0.001);
        let coverage = clamp(0.5 - distance / pixel_width, 0.0, 1.0);
        return vec4<f32>(in.color.rgb, in.color.a * coverage * clip_alpha);
    }
    if in.mode == 1u {
        let distance = rounded_box_distance(
            in.local - in.shape_origin,
            in.shape_size,
            in.radii,
        );
        var coverage = 1.0;
        if in.blur_radius <= 0.0 {
            let aa = stable_distance_aa(distance, in.local);
            coverage = 1.0 - smoothstep(-aa, aa, distance);
        } else {
            let blur_extent = max(in.blur_radius * 3.0, 0.001);
            coverage = 1.0 - smoothstep(0.0, blur_extent, distance);
        }
        let occluder_distance = rounded_box_distance(
            in.local - in.occluder_rect.xy,
            in.occluder_rect.zw,
            in.occluder_radii,
        );
        let occluder_aa = stable_distance_aa(occluder_distance, in.local);
        let exterior = smoothstep(-occluder_aa, occluder_aa, occluder_distance);
        coverage = min(coverage, exterior);
        if coverage <= 0.0 {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * coverage * clip_alpha);
    }

    var color = in.color;
    if in.mode == 65536u {
        color = paint_color(in.local, in.size, in.color, in.clip_meta.w);
    }
    let outer_distance = rounded_box_distance(in.local, in.size, in.radii);
    // Derivatives express the signed-distance transition in framebuffer
    // pixels, so the antialiasing width follows both DPI and perspective.
    let outer_aa = stable_distance_aa(outer_distance, in.local);
    let outer_alpha = 1.0 - smoothstep(-outer_aa, outer_aa, outer_distance);
    if in.border_width <= 0.0 {
        return vec4<f32>(color.rgb, color.a * outer_alpha * clip_alpha);
    }

    let inset_size = max(in.size - vec2<f32>(2.0 * in.border_width), vec2<f32>(0.0));
    if inset_size.x <= 0.0 || inset_size.y <= 0.0 {
        return vec4<f32>(color.rgb, color.a * outer_alpha * clip_alpha);
    }
    let inner_radii = max(in.radii - vec4<f32>(in.border_width), vec4<f32>(0.0));
        let inner_distance = rounded_box_distance(
        in.local - vec2<f32>(in.border_width),
        inset_size,
        inner_radii,
    );
    let inner_aa = stable_distance_aa(inner_distance, in.local);
    let inner_alpha = 1.0 - smoothstep(-inner_aa, inner_aa, inner_distance);
    return vec4<f32>(color.rgb, color.a * outer_alpha * (1.0 - inner_alpha) * clip_alpha);
}
"#;

/// Storage layout matches GradientPaint in WGSL (all members 16-byte aligned).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GradientPaint {
    endpoints: [f32; 4],
    colors: [[f32; 4]; 8],
    offsets: [[f32; 4]; 2],
    meta: [u32; 4],
}

impl GradientPaint {
    fn for_primitive(primitive: &Primitive) -> Self {
        let mut data = Self::zeroed();
        let paint = match primitive {
            Primitive::PaintedQuad { paint, .. } | Primitive::PaintedBorder { paint, .. } => paint,
            _ => return data,
        };
        if let crate::Paint::LinearGradient(gradient) = paint {
            data.endpoints = [
                gradient.start().x,
                gradient.start().y,
                gradient.end().x,
                gradient.end().y,
            ];
            for (i, stop) in gradient.stops().iter().enumerate() {
                let c = stop.color();
                data.colors[i] = [c.r * c.a, c.g * c.a, c.b * c.a, c.a];
                data.offsets[i / 4][i % 4] = stop.offset();
            }
            data.meta[0] = gradient.stops().len() as u32;
        }
        data
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadInstance {
    rect: [f32; 4],
    color: [f32; 4],
    /// Corner order is top-left, top-right, bottom-right, bottom-left.
    radii: [f32; 4],
    border_width: f32,
    logical_size: [f32; 2],
    shape_rect: [f32; 4],
    blur_radius: f32,
    mode: u32,
    clip_rect0: [f32; 4],
    clip_radii0: [f32; 4],
    clip_rect1: [f32; 4],
    clip_radii1: [f32; 4],
    clip_meta: [u32; 4],
    occluder_rect: [f32; 4],
    occluder_radii: [f32; 4],
    _pad: [u32; 3],
}

const CLIP_MASK_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::SrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Instanced GPU renderer for legacy scalar and independently rounded quads.
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    clip_mask_pipeline: wgpu::RenderPipeline,
    paint_layout: wgpu::BindGroupLayout,
    paint_buffer: wgpu::Buffer,
    paint_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    device: wgpu::Device,
    instance_buffer: wgpu::Buffer,
    clip_mask_instance_buffer: wgpu::Buffer,
    clip_mask_capacity: u32,
    instance_count: u32,
    instance_capacity: u32,
    free_slots: Vec<u32>,
    id_to_slot: std::collections::BTreeMap<u64, u32>,
    slot_to_id: std::collections::BTreeMap<u32, u64>,
}

impl QuadRenderer {
    /// Test-only handle snapshots prove retention, not just stable scene IDs.
    #[cfg(test)]
    pub(crate) fn resource_handles(&self) -> ([wgpu::RenderPipeline; 2], [wgpu::Buffer; 5]) {
        (
            [self.pipeline.clone(), self.clip_mask_pipeline.clone()],
            [
                self.vertex_buffer.clone(),
                self.index_buffer.clone(),
                self.instance_buffer.clone(),
                self.clip_mask_instance_buffer.clone(),
                self.paint_buffer.clone(),
            ],
        )
    }

    fn create_paint_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-retained-gradient-paints"),
            size: capacity as u64 * std::mem::size_of::<GradientPaint>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    fn create_paint_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("widget-gradient-paints"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let vertices: [[f32; 2]; 4] = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-quad-vertex"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("widget-quad-index"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let instance_capacity = 256;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-instance"),
            size: instance_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("widget-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(QUAD_SHADER)),
        });
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 2 * std::mem::size_of::<f32>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };
        let instance_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 52,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 60,
                    shader_location: 6,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 76,
                    shader_location: 7,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 80,
                    shader_location: 8,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 84,
                    shader_location: 9,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 100,
                    shader_location: 10,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 116,
                    shader_location: 11,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 132,
                    shader_location: 12,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32x4,
                    offset: 148,
                    shader_location: 13,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 164,
                    shader_location: 14,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 180,
                    shader_location: 15,
                },
            ],
        };
        let paint_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("widget-gradient-paint-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<GradientPaint>() as u64
                    ),
                },
                count: None,
            }],
        });
        let paint_buffer = Self::create_paint_buffer(device, instance_capacity);
        let paint_group = Self::create_paint_group(device, &paint_layout, &paint_buffer);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("widget-quad-layout"),
            bind_group_layouts: &[Some(&paint_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("widget-quad-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(vertex_buffer_layout.clone()),
                    Some(instance_buffer_layout.clone()),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let clip_mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("widget-quad-clip-mask-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_buffer_layout), Some(instance_buffer_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(CLIP_MASK_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let clip_mask_capacity = 8;
        let clip_mask_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-clip-mask-instance"),
            size: clip_mask_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            clip_mask_pipeline,
            paint_layout,
            paint_buffer,
            paint_group,
            vertex_buffer,
            index_buffer,
            device: device.clone(),
            instance_buffer,
            clip_mask_instance_buffer,
            clip_mask_capacity,
            instance_count: 0,
            instance_capacity,
            free_slots: Vec::new(),
            id_to_slot: std::collections::BTreeMap::new(),
            slot_to_id: std::collections::BTreeMap::new(),
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, delta: &SceneDelta, viewport: &Viewport) {
        for id in &delta.removed {
            if let Some(slot) = self.id_to_slot.remove(id) {
                self.slot_to_id.remove(&slot);
                self.free_slots.push(slot);
                self.write_instance(
                    queue,
                    slot,
                    (QuadInstance::zeroed(), GradientPaint::zeroed()),
                );
            }
        }
        for item in &delta.added {
            if !Self::is_quad_primitive(&item.primitive) {
                continue;
            }
            let Some(slot) = self.allocate_slot(queue) else {
                continue;
            };
            self.id_to_slot.insert(item.id, slot);
            self.slot_to_id.insert(slot, item.id);
            self.write_instance(queue, slot, self.item_to_instance(item, viewport));
        }
        for item in &delta.modified {
            if Self::is_quad_primitive(&item.primitive) {
                if let Some(slot) = self.id_to_slot.get(&item.id).copied() {
                    self.write_instance(queue, slot, self.item_to_instance(item, viewport));
                } else if let Some(slot) = self.allocate_slot(queue) {
                    self.id_to_slot.insert(item.id, slot);
                    self.slot_to_id.insert(slot, item.id);
                    self.write_instance(queue, slot, self.item_to_instance(item, viewport));
                }
            } else if let Some(slot) = self.id_to_slot.remove(&item.id) {
                self.slot_to_id.remove(&slot);
                self.free_slots.push(slot);
                self.write_instance(
                    queue,
                    slot,
                    (QuadInstance::zeroed(), GradientPaint::zeroed()),
                );
            }
        }
    }

    /// Re-uploads retained instances when viewport conversion changes without
    /// changing renderer-neutral scene primitives.
    pub fn refresh_viewport(&self, queue: &wgpu::Queue, items: &[SceneItem], viewport: &Viewport) {
        for item in items {
            if let Some(slot) = self.id_to_slot.get(&item.id).copied()
                && Self::is_quad_primitive(&item.primitive)
            {
                self.write_instance(queue, slot, self.item_to_instance(item, viewport));
            }
        }
    }

    pub fn encode<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let count = self.instance_count.min(self.instance_capacity);
        if count != 0 {
            self.encode_impl(pass, 0, count);
        }
    }

    pub fn encode_range<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, start: u32, count: u32) {
        let capacity = self.instance_count.min(self.instance_capacity);
        if count != 0 && start < capacity {
            self.encode_impl(pass, start, count.min(capacity - start));
        }
    }

    fn encode_impl<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, start: u32, count: u32) {
        pass.set_bind_group(0, &self.paint_group, &[]);
        let end = start.saturating_add(count).min(self.instance_capacity);
        if start >= end {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, start..end);
    }

    /// Uploads one dest-in instance per masked external before the paint walk.
    ///
    /// Queue writes complete before the submitted pass runs, so in-pass reuse of
    /// a single slot would apply the last instance to every mask draw.
    pub(crate) fn upload_clip_masks(
        &mut self,
        queue: &wgpu::Queue,
        masks: &[(&[RoundedClip], Rect)],
        viewport: &Viewport,
    ) {
        if masks.is_empty() {
            return;
        }
        self.ensure_clip_mask_capacity(masks.len() as u32);
        let instances: Vec<QuadInstance> = masks
            .iter()
            .map(|(clips, rect)| Self::clip_mask_instance(clips, *rect, viewport))
            .collect();
        queue.write_buffer(
            &self.clip_mask_instance_buffer,
            0,
            bytemuck::cast_slice(&instances),
        );
    }

    pub(crate) fn encode_clip_mask<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, slot: u32) {
        pass.set_pipeline(&self.clip_mask_pipeline);
        pass.set_bind_group(0, &self.paint_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.clip_mask_instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, slot..slot + 1);
    }

    fn clip_mask_instance(clips: &[RoundedClip], rect: Rect, viewport: &Viewport) -> QuadInstance {
        let size = rect.size();
        let packed = pack_active_clips(clips, rect.min);
        QuadInstance {
            rect: viewport.dp_rect_to_ndc(&rect),
            color: [1.0, 1.0, 1.0, 1.0],
            radii: [0.0; 4],
            border_width: 0.0,
            logical_size: [size.width, size.height],
            shape_rect: [0.0, 0.0, size.width, size.height],
            blur_radius: 0.0,
            mode: 2,
            clip_rect0: packed.clip_rect0,
            clip_radii0: packed.clip_radii0,
            clip_rect1: packed.clip_rect1,
            clip_radii1: packed.clip_radii1,
            clip_meta: packed.clip_meta,
            occluder_rect: [0.0; 4],
            occluder_radii: [0.0; 4],
            _pad: [0; 3],
        }
    }

    fn ensure_clip_mask_capacity(&mut self, required: u32) {
        if required <= self.clip_mask_capacity {
            return;
        }
        let new_capacity = self.clip_mask_capacity.saturating_mul(2).max(required);
        self.clip_mask_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-clip-mask-instance"),
            size: new_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.clip_mask_capacity = new_capacity;
    }

    fn is_quad_primitive(primitive: &Primitive) -> bool {
        matches!(
            primitive,
            Primitive::Quad { .. }
                | Primitive::Border { .. }
                | Primitive::RoundedQuad { .. }
                | Primitive::RoundedBorder { .. }
                | Primitive::PaintedQuad { .. }
                | Primitive::PaintedBorder { .. }
                | Primitive::OuterShadow { .. }
                | Primitive::Icon { .. }
        )
    }

    fn item_to_instance(
        &self,
        item: &SceneItem,
        viewport: &Viewport,
    ) -> (QuadInstance, GradientPaint) {
        let (
            rect,
            shape_rect,
            occluder_rect,
            color,
            radii,
            occluder_radii,
            border_width,
            blur_radius,
            mode,
        ) = match &item.primitive {
            Primitive::Icon { icon, rect, color } => (
                *rect,
                *rect,
                *rect,
                *color,
                [0.0; 4],
                [0.0; 4],
                0.0,
                0.0,
                icon.render_mode(),
            ),
            Primitive::Quad {
                rect,
                color,
                corner_radius,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                [*corner_radius; 4],
                [0.0; 4],
                0.0,
                0.0,
                0,
            ),
            Primitive::Border {
                rect,
                width,
                color,
                corner_radius,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                [*corner_radius; 4],
                [0.0; 4],
                *width,
                0.0,
                0,
            ),
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                *corner_radii,
                [0.0; 4],
                0.0,
                0.0,
                0,
            ),
            Primitive::PaintedQuad {
                rect,
                paint,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                paint.first_color(),
                *corner_radii,
                [0.0; 4],
                0.0,
                0.0,
                if matches!(paint, crate::Paint::LinearGradient(_)) {
                    65536
                } else {
                    0
                },
            ),
            Primitive::PaintedBorder {
                rect,
                width,
                paint,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                paint.first_color(),
                *corner_radii,
                [0.0; 4],
                *width,
                0.0,
                if matches!(paint, crate::Paint::LinearGradient(_)) {
                    65536
                } else {
                    0
                },
            ),
            Primitive::RoundedBorder {
                rect,
                width,
                color,
                corner_radii,
            } => (
                *rect,
                *rect,
                *rect,
                *color,
                *corner_radii,
                [0.0; 4],
                *width,
                0.0,
                0,
            ),
            Primitive::OuterShadow {
                rect,
                shape_rect,
                occluder_rect,
                color,
                corner_radii,
                occluder_radii,
                blur_radius,
            } => (
                *rect,
                *shape_rect,
                *occluder_rect,
                *color,
                *corner_radii,
                *occluder_radii,
                0.0,
                *blur_radius,
                1,
            ),
            _ => return (QuadInstance::zeroed(), GradientPaint::zeroed()),
        };
        let size = rect.size();
        let shape_size = shape_rect.size();
        let packed = pack_active_clips(&item.clips, rect.min);
        (
            QuadInstance {
                rect: viewport.dp_rect_to_ndc(&rect),
                color: color.to_array(),
                radii,
                border_width,
                logical_size: [size.width, size.height],
                shape_rect: [
                    finite_difference(shape_rect.min.x, rect.min.x),
                    finite_difference(shape_rect.min.y, rect.min.y),
                    shape_size.width,
                    shape_size.height,
                ],
                blur_radius,
                mode,
                clip_rect0: packed.clip_rect0,
                clip_radii0: packed.clip_radii0,
                clip_rect1: packed.clip_rect1,
                clip_radii1: packed.clip_radii1,
                clip_meta: packed.clip_meta,
                occluder_rect: [
                    finite_difference(occluder_rect.min.x, rect.min.x),
                    finite_difference(occluder_rect.min.y, rect.min.y),
                    finite_difference(occluder_rect.max.x, occluder_rect.min.x),
                    finite_difference(occluder_rect.max.y, occluder_rect.min.y),
                ],
                occluder_radii,
                _pad: [0; 3],
            },
            GradientPaint::for_primitive(&item.primitive),
        )
    }

    fn write_instance(
        &self,
        queue: &wgpu::Queue,
        slot: u32,
        (mut instance, paint): (QuadInstance, GradientPaint),
    ) {
        instance.clip_meta[3] = slot;
        queue.write_buffer(
            &self.paint_buffer,
            slot as u64 * std::mem::size_of::<GradientPaint>() as u64,
            bytemuck::bytes_of(&paint),
        );
        queue.write_buffer(
            &self.instance_buffer,
            slot as u64 * std::mem::size_of::<QuadInstance>() as u64,
            bytemuck::bytes_of(&instance),
        );
    }

    pub fn slot_of(&self, id: u64) -> Option<u32> {
        self.id_to_slot.get(&id).copied()
    }

    fn allocate_slot(&mut self, queue: &wgpu::Queue) -> Option<u32> {
        if let Some(slot) = self.free_slots.pop() {
            return Some(slot);
        }
        if self.instance_count == self.instance_capacity {
            self.grow_instance_buffer(queue, self.instance_count.checked_add(1)?);
        }
        let slot = self.instance_count;
        self.instance_count += 1;
        Some(slot)
    }

    fn grow_instance_buffer(&mut self, queue: &wgpu::Queue, required_capacity: u32) {
        debug_assert!(required_capacity > self.instance_capacity);
        let new_capacity = self
            .instance_capacity
            .saturating_mul(2)
            .max(required_capacity);
        assert!(new_capacity > self.instance_capacity);

        let new_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget-quad-instance-grown"),
            size: new_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let new_paints = Self::create_paint_buffer(&self.device, new_capacity);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("widget-quad-instance-grow"),
            });
        encoder.copy_buffer_to_buffer(
            &self.instance_buffer,
            0,
            &new_buffer,
            0,
            self.instance_capacity as u64 * std::mem::size_of::<QuadInstance>() as u64,
        );
        encoder.copy_buffer_to_buffer(
            &self.paint_buffer,
            0,
            &new_paints,
            0,
            self.instance_capacity as u64 * std::mem::size_of::<GradientPaint>() as u64,
        );
        queue.submit(Some(encoder.finish()));
        self.instance_buffer = new_buffer;
        self.paint_buffer = new_paints;
        self.paint_group =
            Self::create_paint_group(&self.device, &self.paint_layout, &self.paint_buffer);
        self.instance_capacity = new_capacity;
    }
}
