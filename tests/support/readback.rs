//! Test-only offscreen targets and synchronous readback.
//!
//! Only single-layer, single-sample RGBA8/BGRA8 (linear or sRGB) targets are
//! supported. Bytes stay in texture order: RGBA for Rgba8*, BGRA for Bgra8*.
//! No swizzle, gamma conversion or alpha conversion is performed.

#[derive(Clone, Copy, Debug)]
pub enum Rows {
    /// Return exactly width * height * 4 bytes, stripping copy-row padding.
    Tight,
    /// Return height rows with this explicit byte stride, including padding.
    /// The stride must fit width * 4 bytes and meet wgpu copy alignment.
    Padded(u32),
}

impl Rows {
    fn copy_stride(self, width: u32) -> u32 {
        let tight = width.checked_mul(4).expect("readback row size overflow");
        match self {
            Self::Tight => tight
                .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                .checked_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                .expect("readback stride overflow"),
            Self::Padded(stride) => {
                assert!(
                    stride >= tight,
                    "readback stride is smaller than a pixel row"
                );
                assert_eq!(stride % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
                stride
            }
        }
    }
}

fn assert_format(format: wgpu::TextureFormat) {
    assert!(
        matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ),
        "unsupported readback format: {format:?}"
    );
}

pub fn target(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    assert_format(format);
    assert!(width > 0 && height > 0, "readback target must be nonempty");
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Append a full mip-zero copy to the caller's encoder and submit it.
/// Wait for mapping to succeed, copy into owned bytes, then release the mapping
/// before unmapping. Adapter, polling and mapping failures are never skipped.
pub fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    rows: Rows,
) -> Vec<u8> {
    assert_format(texture.format());
    assert_eq!(texture.dimension(), wgpu::TextureDimension::D2);
    assert_eq!(texture.depth_or_array_layers(), 1);
    assert_eq!(texture.sample_count(), 1);
    let width = texture.width();
    let height = texture.height();
    let stride = rows.copy_stride(width);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test texture readback"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (send, receive) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        send.send(result).expect("readback receiver dropped");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("readback GPU poll failed");
    receive
        .recv()
        .expect("readback callback was not delivered")
        .expect("readback mapping failed");
    let mapped = slice
        .get_mapped_range()
        .expect("readback buffer not mapped");
    let pixels = match rows {
        Rows::Tight => mapped
            .chunks_exact(stride as usize)
            .flat_map(|row| row[..(width * 4) as usize].iter().copied())
            .collect(),
        Rows::Padded(_) => mapped.to_vec(),
    };
    drop(mapped);
    buffer.unmap();
    pixels
}
