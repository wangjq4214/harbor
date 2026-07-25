//! Integration tests for TextRenderer — construction with a wgpu Device.
//!
//! These tests require a wgpu Device; they are skipped silently if no GPU
//! adapter is available.

use harbor_widget::renderer::text_renderer::TextRenderer;
use std::sync::atomic::{AtomicBool, Ordering};

// ── GPU helpers ─────────────────────────────────────────────────────────────

/// Tries to create a wgpu Device + Queue. Returns `None` when no adapter is
/// available (headless CI, etc.), so GPU tests degrade gracefully.
fn try_create_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("test-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .ok()
}

/// Creates a tiny 64×64 R8 atlas texture for TextRenderer construction.
fn create_atlas_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-atlas"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Creates a linear atlas sampler.
fn create_atlas_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("test-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    atlas_texture: &wgpu::Texture,
    atlas_sampler: &wgpu::Sampler,
) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("test-text-bind-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
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

    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("test-text-bind-group"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(atlas_sampler),
            },
        ],
    });

    (layout, bind_group)
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// Tracks whether a GPU test actually ran. If no adapter is available, all
/// GPU tests are skipped silently.
static GPU_TESTS_RAN: AtomicBool = AtomicBool::new(false);

#[test]
fn should_construct_text_renderer_with_valid_device() {
    let Some((device, _queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for TextRenderer test");
        return;
    };
    GPU_TESTS_RAN.store(true, Ordering::SeqCst);

    let atlas_texture = create_atlas_texture(&device);
    let atlas_sampler = create_atlas_sampler(&device);
    let (bind_layout, bind_group) = create_bind_group(&device, &atlas_texture, &atlas_sampler);

    // Act: construct the TextRenderer — should not panic
    let _renderer = TextRenderer::new(
        &device,
        wgpu::TextureFormat::Bgra8Unorm,
        &bind_layout,
        &bind_group,
    );

    // If we get here without panic, the test passes.
}

#[test]
fn should_construct_text_renderer_with_rgba8_format() {
    let Some((device, _queue)) = try_create_device() else {
        eprintln!("SKIP: no GPU adapter available for TextRenderer test");
        return;
    };
    GPU_TESTS_RAN.store(true, Ordering::SeqCst);

    let atlas_texture = create_atlas_texture(&device);
    let atlas_sampler = create_atlas_sampler(&device);
    let (bind_layout, bind_group) = create_bind_group(&device, &atlas_texture, &atlas_sampler);

    // Act: construct with a different common format
    let _renderer = TextRenderer::new(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        &bind_layout,
        &bind_group,
    );

    // No panic = pass
}
