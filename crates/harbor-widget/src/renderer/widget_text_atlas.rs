//! Widget-owned glyph atlas and GPU texture resources.

use harbor_text::atlas::MAX_ATLAS_SIZE;
use harbor_text::{AtlasGlyph, FontBook, GlyphAtlas, GlyphKey, RasterizeResult};
use wgpu::util::DeviceExt;

/// Glyph atlas shared by Widget runtimes on the UI/render thread.
///
/// The texture and bind group remain stable for the lifetime of the atlas. The
/// revision changes only when an atlas repack invalidates previously issued UVs.
pub struct WidgetTextAtlas {
    fonts: FontBook,
    atlas: GlyphAtlas,
    texture: wgpu::Texture,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    revision: u64,
}

impl WidgetTextAtlas {
    /// Creates an empty Widget atlas and its fixed-size GPU texture.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, fonts: FontBook) -> Self {
        let atlas = GlyphAtlas::new();
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("widget glyph atlas bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("widget glyph atlas texture"),
                size: wgpu::Extent3d {
                    width: MAX_ATLAS_SIZE,
                    height: MAX_ATLAS_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            atlas.pixels(),
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("widget glyph atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("widget glyph atlas bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            fonts,
            atlas,
            texture,
            bind_group_layout,
            bind_group,
            revision: 0,
        }
    }

    /// Ensures every non-space character from the supplied retained text exists.
    /// Returns the current UV-layout revision.
    pub fn ensure_scene_text<'a>(
        &mut self,
        texts: impl IntoIterator<Item = &'a str>,
        queue: &wgpu::Queue,
    ) -> u64 {
        let chars = collect_unique_chars(texts);
        let result = self.atlas.rasterize_new(&self.fonts, &chars);
        match upload_kind(&result) {
            AtlasUploadKind::None => {}
            AtlasUploadKind::Incremental => self.upload_glyphs(queue, &result.new_keys),
            AtlasUploadKind::Full => {
                self.upload_full(queue);
                self.revision = self
                    .revision
                    .checked_add(1)
                    .expect("widget glyph atlas revision overflow");
            }
        }
        self.revision
    }

    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.atlas.glyph_by_char(ch)
    }

    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn upload_full(&self, queue: &wgpu::Queue) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            self.atlas.pixels(),
            wgpu::TexelCopyBufferLayout {
                bytes_per_row: Some(MAX_ATLAS_SIZE),
                rows_per_image: Some(MAX_ATLAS_SIZE),
                offset: 0,
            },
            wgpu::Extent3d {
                width: MAX_ATLAS_SIZE,
                height: MAX_ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_glyphs(&self, queue: &wgpu::Queue, new_keys: &[GlyphKey]) {
        for key in new_keys {
            let Some(glyph) = self.atlas.glyph(*key) else {
                continue;
            };
            if glyph.width == 0 || glyph.height == 0 {
                continue;
            }

            let padded_bytes_per_row = glyph.width.div_ceil(256) * 256;
            let mut tile_data = vec![0u8; (padded_bytes_per_row * glyph.height) as usize];
            let pixels = self.atlas.pixels();
            for row in 0..glyph.height {
                let src_offset = ((glyph.atlas_y + row) * MAX_ATLAS_SIZE + glyph.atlas_x) as usize;
                let dst_offset = (row * padded_bytes_per_row) as usize;
                tile_data[dst_offset..dst_offset + glyph.width as usize]
                    .copy_from_slice(&pixels[src_offset..src_offset + glyph.width as usize]);
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: glyph.atlas_x,
                        y: glyph.atlas_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &tile_data,
                wgpu::TexelCopyBufferLayout {
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(glyph.height),
                    offset: 0,
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn collect_unique_chars<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<char> {
    let mut chars: Vec<char> = texts
        .into_iter()
        .flat_map(str::chars)
        .filter(|ch| *ch != ' ')
        .collect();
    chars.sort_unstable();
    chars.dedup();
    chars
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtlasUploadKind {
    None,
    Incremental,
    Full,
}

fn upload_kind(result: &RasterizeResult) -> AtlasUploadKind {
    if result.new_keys.is_empty() {
        AtlasUploadKind::None
    } else if result.evicted {
        AtlasUploadKind::Full
    } else {
        AtlasUploadKind::Incremental
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_text_collection_deduplicates_and_skips_spaces() {
        assert_eq!(
            collect_unique_chars(["B A", "AB", "中中"]),
            vec!['A', 'B', '中']
        );
    }

    #[test]
    fn upload_kind_distinguishes_noop_incremental_and_repack() {
        assert_eq!(
            upload_kind(&RasterizeResult {
                new_keys: Vec::new(),
                evicted: false,
            }),
            AtlasUploadKind::None
        );
        let key = GlyphKey::new(
            harbor_text::FaceId::PRIMARY,
            harbor_text::GlyphId::new(1),
            harbor_text::FontSize::new(12.0).unwrap(),
            harbor_text::FontStyle::REGULAR,
        );
        assert_eq!(
            upload_kind(&RasterizeResult {
                new_keys: vec![key],
                evicted: false,
            }),
            AtlasUploadKind::Incremental
        );
        assert_eq!(
            upload_kind(&RasterizeResult {
                new_keys: vec![key],
                evicted: true,
            }),
            AtlasUploadKind::Full
        );
    }
}
