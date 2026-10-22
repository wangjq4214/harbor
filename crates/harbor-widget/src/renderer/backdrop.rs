//! Encoder-level local filters. Backgrounds are captured at their paint-order
//! boundary, never by replaying external callbacks or sampling an active attachment.
use super::{Viewport, pack_active_clips};
use crate::{
    BorderRadius, ClipBehavior,
    layout::Point,
    scene::{
        SceneItem,
        primitive::{ExternalDrawContext, Primitive, ScissorRect},
    },
};
use bytemuck::{Pod, Zeroable};
use hashbrown::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    crop: [f32; 4],
    shape: [f32; 4],
    radii: [f32; 4],
    settings: [f32; 4],
    material: [f32; 4], // physical fill sigma, signed frost mix, highlight, shade
    tone: [f32; 4],
    clip_rect0: [f32; 4],
    clip_radii0: [f32; 4],
    clip_rect1: [f32; 4],
    clip_radii1: [f32; 4],
    clip_meta: [u32; 4],
}

struct Region {
    crop: ScissorRect,
    destination: ScissorRect,
    params: Params,
}

impl Region {
    fn new(item: &SceneItem, viewport: &Viewport) -> Option<Self> {
        let (rect, sigma, corner_radii, refraction, material) = match item.primitive {
            Primitive::BackdropBlur {
                rect,
                sigma,
                corner_radii,
            } => (
                rect,
                sigma,
                corner_radii,
                crate::EdgeRefraction::default(),
                None,
            ),
            Primitive::BackdropRefract {
                rect,
                sigma,
                corner_radii,
                refraction,
            } => (rect, sigma, corner_radii, refraction, None),
            Primitive::BackdropMaterial {
                rect,
                sigma,
                corner_radii,
                refraction,
                material,
            } => (rect, sigma, corner_radii, refraction, Some(material)),
            _ => return None,
        };
        if !viewport.is_drawable()
            || !viewport.scale_factor.is_finite()
            || viewport.scale_factor <= 0.0
            || !sigma.is_finite()
            || sigma < 0.0
            || (material.is_none() && sigma == 0.0 && !refraction.enabled())
        {
            return None;
        }
        let radius = BorderRadius::only(
            corner_radii[0],
            corner_radii[1],
            corner_radii[2],
            corner_radii[3],
        )
        .ok()?;
        // RoundedClip validates endpoints as well as radii and dimensions.
        let shape =
            crate::scene::clip::RoundedClip::new(rect, radius, ClipBehavior::AntiAlias).ok()?;
        let size = rect.size();
        let frost = material.map_or([0.0; 2], |m| m.frost());
        let rim = material.map_or([0.0; 2], |m| m.rim());
        let fill_sigma = (f64::from(frost[0]) * f64::from(viewport.scale_factor)).min(f64::from(
            viewport.physical_size.0.max(viewport.physical_size.1),
        )) as f32;
        if !size.width.is_finite() || !size.height.is_finite() {
            return None;
        }
        let mut destination = ExternalDrawContext::compute_scissor(
            rect,
            viewport.scale_factor,
            viewport.physical_size,
        );
        for clip in &item.clips {
            if clip.behavior() != ClipBehavior::None {
                destination = destination.intersect(&ExternalDrawContext::compute_scissor(
                    clip.rect(),
                    viewport.scale_factor,
                    viewport.physical_size,
                ));
            }
        }
        if destination.is_empty() {
            return None;
        }
        let sigma = (f64::from(sigma.min(64.0)) * f64::from(viewport.scale_factor)).min(f64::from(
            viewport.physical_size.0.max(viewport.physical_size.1),
        )) as f32;
        // Expand the sampled crop, not the destination. Clamp before integer
        // arithmetic so even extreme finite scales cannot overflow dimensions.
        let displacement = f64::from(refraction.displacement()) * f64::from(viewport.scale_factor);
        let support = (f64::from(sigma.max(fill_sigma)) * 3.0 + displacement)
            .ceil()
            .min(f64::from(u32::MAX)) as u32;
        let x = destination.x.saturating_sub(support);
        let y = destination.y.saturating_sub(support);
        let right = destination
            .x
            .saturating_add(destination.width)
            .saturating_add(support)
            .min(viewport.physical_size.0);
        let bottom = destination
            .y
            .saturating_add(destination.height)
            .saturating_add(support)
            .min(viewport.physical_size.1);
        let crop = ScissorRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        };
        let clips = pack_active_clips(&item.clips, Point::ZERO);
        Some(Self {
            crop,
            destination,
            params: Params {
                crop: [x as f32, y as f32, crop.width as f32, crop.height as f32],
                shape: [
                    rect.min.x,
                    rect.min.y,
                    rect.size().width,
                    rect.size().height,
                ],
                radii: shape.radii().as_array(),
                settings: [
                    sigma,
                    viewport.scale_factor,
                    refraction.width().min(size.width.min(size.height) * 0.5),
                    refraction.displacement(),
                ],
                clip_rect0: clips.clip_rect0,
                clip_radii0: clips.clip_radii0,
                clip_rect1: clips.clip_rect1,
                clip_radii1: clips.clip_radii1,
                material: [fill_sigma, frost[1], rim[0], rim[1]],
                tone: material.map_or([0.0, 1.0, 0.0, 1.0], |m| m.tone()),
                clip_meta: [
                    clips.clip_meta[0],
                    clips.clip_meta[1],
                    clips.clip_meta[2],
                    u32::from(material.is_some()),
                ],
            },
        })
    }
}

struct FrostTarget {
    horizontal: wgpu::TextureView,
    vertical: wgpu::TextureView,
    horizontal_group: wgpu::BindGroup,
    vertical_group: wgpu::BindGroup,
}

struct FilterTarget {
    size: (u32, u32),
    snapshot: wgpu::Texture,
    horizontal: wgpu::TextureView,
    frost: Option<FrostTarget>,
    params: wgpu::Buffer,
    horizontal_group: wgpu::BindGroup,
    composite_group: wgpu::BindGroup,
    region: Region,
}

pub(crate) struct BackdropRenderer {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    horizontal: wgpu::RenderPipeline,
    fill_horizontal: wgpu::RenderPipeline,
    fill_vertical: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    frame: Option<FrameTarget>,
    filters: HashMap<u64, FilterTarget>,
}

struct FrameTarget {
    size: (u32, u32),
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    blit_group: wgpu::BindGroup,
}

impl BackdropRenderer {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("local backdrop layout"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("local backdrop pipelines"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("local backdrop shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let horizontal = pipeline("fs_horizontal");
        let composite = pipeline("fs_composite");
        let blit = pipeline("fs_blit");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("local backdrop clamp sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            format,
            layout,
            sampler,
            horizontal,
            fill_horizontal: pipeline("fs_fill_horizontal"),
            fill_vertical: pipeline("fs_fill_vertical"),
            composite,
            blit,
            frame: None,
            filters: HashMap::new(),
        }
    }

    fn texture(
        &self,
        device: &wgpu::Device,
        size: (u32, u32),
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("retained local backdrop target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage,
            view_formats: &[],
        })
    }

    fn group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        original: &wgpu::TextureView,
        fill: &wgpu::TextureView,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retained local backdrop bindings"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(fill),
                },
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(original),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: &Viewport,
        items: &[SceneItem],
    ) {
        let regions: HashMap<_, _> = items
            .iter()
            .filter_map(|item| Region::new(item, viewport).map(|r| (item.id, r)))
            .collect();
        self.filters.retain(|id, _| regions.contains_key(id));
        if regions.is_empty() {
            self.frame = None;
            return;
        }
        if self
            .frame
            .as_ref()
            .is_none_or(|f| f.size != viewport.physical_size)
        {
            let texture = self.texture(
                device,
                viewport.physical_size,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let view = texture.create_view(&Default::default());
            let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("backdrop blit params"),
                contents: bytemuck::bytes_of(&Params::zeroed()),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let blit_group = self.group(device, &view, &view, &view, &params);
            self.frame = Some(FrameTarget {
                size: viewport.physical_size,
                texture,
                view,
                blit_group,
            });
        }
        for (id, mut region) in regions {
            if self.format.is_srgb() {
                region.params.clip_meta[3] |= 2;
            }
            let size = (region.crop.width, region.crop.height);
            let needs_frost = region.params.material[0] > 0.0 && region.params.material[1] != 0.0;
            if self
                .filters
                .get(&id)
                .is_none_or(|f| f.size != size || f.frost.is_some() != needs_frost)
            {
                let snapshot = self.texture(
                    device,
                    size,
                    wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                );
                let snapshot_view = snapshot.create_view(&Default::default());
                let horizontal = self
                    .texture(
                        device,
                        size,
                        wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                    )
                    .create_view(&Default::default());
                let params = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("per-filter backdrop params"),
                    size: std::mem::size_of::<Params>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let horizontal_group = self.group(
                    device,
                    &snapshot_view,
                    &snapshot_view,
                    &snapshot_view,
                    &params,
                );
                let frost = needs_frost.then(|| {
                    let view = || {
                        self.texture(
                            device,
                            size,
                            wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                        )
                        .create_view(&Default::default())
                    };
                    let fill_h = view();
                    let fill_v = view();
                    FrostTarget {
                        horizontal_group: self.group(
                            device,
                            &snapshot_view,
                            &snapshot_view,
                            &snapshot_view,
                            &params,
                        ),
                        vertical_group: self.group(
                            device,
                            &fill_h,
                            &snapshot_view,
                            &snapshot_view,
                            &params,
                        ),
                        horizontal: fill_h,
                        vertical: fill_v,
                    }
                });
                let fill = frost.as_ref().map_or(&snapshot_view, |f| &f.vertical);
                let composite_group =
                    self.group(device, &horizontal, &snapshot_view, fill, &params);
                self.filters.insert(
                    id,
                    FilterTarget {
                        size,
                        snapshot,
                        horizontal,
                        frost,
                        params,
                        horizontal_group,
                        composite_group,
                        region,
                    },
                );
            } else {
                self.filters.get_mut(&id).unwrap().region = region;
            }
            let filter = &self.filters[&id];
            queue.write_buffer(&filter.params, 0, bytemuck::bytes_of(&filter.region.params));
        }
    }

    pub(crate) fn view(&self) -> Option<&wgpu::TextureView> {
        self.frame.as_ref().map(|f| &f.view)
    }

    pub(crate) fn filter(&self, encoder: &mut wgpu::CommandEncoder, id: u64) {
        let (Some(frame), Some(filter)) = (&self.frame, self.filters.get(&id)) else {
            return;
        };
        let crop = filter.region.crop;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: crop.x,
                    y: crop.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &filter.snapshot,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: crop.width,
                height: crop.height,
                depth_or_array_layers: 1,
            },
        );
        {
            let mut pass = begin_pass(
                encoder,
                &filter.horizontal,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
            pass.set_pipeline(&self.horizontal);
            pass.set_bind_group(0, &filter.horizontal_group, &[]);
            pass.draw(0..3, 0..1);
        }
        if let Some(frost) = &filter.frost {
            for (view, pipeline, group) in [
                (
                    &frost.horizontal,
                    &self.fill_horizontal,
                    &frost.horizontal_group,
                ),
                (&frost.vertical, &self.fill_vertical, &frost.vertical_group),
            ] {
                let mut pass =
                    begin_pass(encoder, view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        let mut pass = begin_pass(encoder, &frame.view, wgpu::LoadOp::Load);
        let scissor = filter.region.destination;
        pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        pass.set_pipeline(&self.composite);
        pass.set_bind_group(0, &filter.composite_group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub(crate) fn present(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let Some(frame) = &self.frame else { return };
        let mut pass = begin_pass(
            encoder,
            target,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        pass.set_pipeline(&self.blit);
        pass.set_bind_group(0, &frame.blit_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub(crate) fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("widget scene/filter pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Rect, Size};
    #[test]
    fn material_support_uses_broader_sigma_and_never_expands_the_clip_destination() {
        let item = SceneItem {
            id: 1,
            paint_order: 0,
            clips: vec![],
            primitive: Primitive::BackdropMaterial {
                rect: Rect::from_min_size(Point::new(100.0, 100.0), Size::new(20.0, 10.0)),
                sigma: 2.0,
                corner_radii: [2.0; 4],
                refraction: crate::EdgeRefraction::new(64.0, 4.0).unwrap(),
                material: crate::BackdropMaterial::new(
                    10.0,
                    -0.3,
                    [0.0, 1.0, 0.0, 1.0],
                    [0.3, 0.2],
                )
                .unwrap(),
            },
        };
        for scale in [1.0, 1.5, 2.0] {
            let r = Region::new(&item, &Viewport::new(400, 400, scale)).unwrap();
            assert_eq!(r.destination.x, (100.0 * scale) as u32);
            assert_eq!(r.crop.x, (66.0 * scale) as u32);
            assert_eq!(r.params.material, [10.0 * scale, -0.3, 0.3, 0.2]);
            assert_eq!(r.params.settings, [2.0 * scale, scale, 5.0, 4.0]);
            assert_eq!(r.params.clip_meta[3], 1);
        }
    }

    #[test]
    fn refraction_expands_sample_support_only_and_clamps_small_shape_band() {
        let item = SceneItem {
            id: 1,
            paint_order: 0,
            clips: vec![],
            primitive: Primitive::BackdropRefract {
                rect: Rect::from_min_size(Point::new(30.0, 30.0), Size::new(20.0, 10.0)),
                sigma: 2.0,
                corner_radii: [2.0; 4],
                refraction: crate::EdgeRefraction::new(64.0, 4.0).unwrap(),
            },
        };
        for scale in [1.0, 1.5, 2.0] {
            let region = Region::new(&item, &Viewport::new(200, 200, scale)).unwrap();
            assert_eq!(region.destination.x, (30.0 * scale) as u32);
            assert_eq!(region.crop.x, (20.0 * scale) as u32);
            assert_eq!(region.params.settings, [2.0 * scale, scale, 5.0, 4.0]);
        }
    }

    #[test]
    fn bounds_sample_support_without_expanding_destination() {
        let item = SceneItem {
            id: 1,
            paint_order: 0,
            clips: vec![],
            primitive: Primitive::BackdropBlur {
                rect: Rect::from_min_size(Point::new(2.0, 4.0), Size::new(20.0, 10.0)),
                sigma: 3.0,
                corner_radii: [2.0; 4],
            },
        };
        let region = Region::new(&item, &Viewport::new(100, 80, 2.0)).unwrap();
        assert_eq!(
            (
                region.destination.x,
                region.destination.y,
                region.destination.width,
                region.destination.height
            ),
            (4, 8, 40, 20)
        );
        assert_eq!(
            (
                region.crop.x,
                region.crop.y,
                region.crop.width,
                region.crop.height
            ),
            (0, 0, 62, 46)
        );
        assert!(Region::new(&item, &Viewport::new(0, 80, 1.0)).is_none());
    }

    #[test]
    fn invalid_and_offscreen_regions_allocate_no_targets() {
        let viewport = Viewport::new(32, 32, 1.5);
        for (origin, size, sigma) in [
            (Point::ZERO, Size::ZERO, 2.0),
            (Point::new(-50.0, -50.0), Size::new(2.0, 2.0), 2.0),
            (Point::ZERO, Size::new(20.0, 20.0), f32::NAN),
            (Point::ZERO, Size::new(20.0, 20.0), -1.0),
            (Point::new(f32::NAN, 0.0), Size::new(20.0, 20.0), 2.0),
            (
                Point::new(-f32::MAX, 0.0),
                Size::new(f32::INFINITY, 20.0),
                2.0,
            ),
        ] {
            let item = SceneItem {
                id: 1,
                paint_order: 0,
                clips: vec![],
                primitive: Primitive::BackdropBlur {
                    rect: Rect::from_min_size(origin, size),
                    sigma,
                    corner_radii: [0.0; 4],
                },
            };
            assert!(Region::new(&item, &viewport).is_none());
        }
    }
}
