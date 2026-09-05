use std::cell::RefCell;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
#[cfg(target_os = "windows")]
use windows::core::Interface;
#[cfg(target_os = "windows")]
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use harbor_types::DirtyRange;

/// Upload operation selected for a dirty grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadMode {
    None,
    Incremental,
    Full,
}

/// Pure upload decision, separated from wgpu so it can be tested headlessly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPlan {
    pub mode: UploadMode,
    pub dirty_range_count: usize,
    pub dirty_cells: usize,
    pub dirty_bytes: usize,
    pub full_bytes: usize,
}

/// Chooses full writes when fragmented or broad damage makes them cheaper.
#[derive(Clone, Copy, Debug)]
pub struct UploadPolicy {
    full_upload_ratio: f64,
    max_incremental_ranges: usize,
}

impl Default for UploadPolicy {
    fn default() -> Self {
        Self {
            full_upload_ratio: 0.5,
            max_incremental_ranges: 64,
        }
    }
}

impl UploadPolicy {
    pub fn decide(
        self,
        rows: usize,
        cols: usize,
        bytes_per_cell: usize,
        dirty_ranges: &[DirtyRange],
        force_full: bool,
    ) -> UploadPlan {
        let dirty_cells = dirty_ranges.iter().fold(0usize, |total, range| {
            total.saturating_add(range.end_col.saturating_sub(range.start_col))
        });
        let dirty_bytes = dirty_cells.saturating_mul(bytes_per_cell);
        let full_bytes = rows.saturating_mul(cols).saturating_mul(bytes_per_cell);
        if force_full {
            return UploadPlan {
                mode: UploadMode::Full,
                dirty_range_count: dirty_ranges.len(),
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        if dirty_ranges.is_empty() {
            return UploadPlan {
                mode: UploadMode::None,
                dirty_range_count: 0,
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        let ratio = if full_bytes == 0 {
            1.0
        } else {
            dirty_bytes as f64 / full_bytes as f64
        };
        let mode = if ratio >= self.full_upload_ratio
            || dirty_ranges.len() > self.max_incremental_ranges
        {
            UploadMode::Full
        } else {
            UploadMode::Incremental
        };
        UploadPlan {
            mode,
            dirty_range_count: dirty_ranges.len(),
            dirty_cells,
            dirty_bytes,
            full_bytes,
        }
    }
}

fn selected_backends() -> wgpu::Backends {
    #[cfg(all(feature = "backend-dx12", feature = "backend-vulkan"))]
    {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    }
    #[cfg(all(feature = "backend-dx12", not(feature = "backend-vulkan")))]
    {
        wgpu::Backends::DX12
    }
    #[cfg(all(not(feature = "backend-dx12"), feature = "backend-vulkan"))]
    {
        wgpu::Backends::VULKAN
    }
    #[cfg(all(
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan"),
        target_os = "windows"
    ))]
    {
        // Desktop Acrylic requires a DirectComposition-capable swap chain.
        // wgpu's GL backend exposes only an opaque Win32 surface, so make
        // DX12 the Windows default even when no backend override is enabled.
        wgpu::Backends::DX12
    }
    #[cfg(all(
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan"),
        not(target_os = "windows")
    ))]
    {
        wgpu::Backends::all()
    }
}

/// Picks a compositing-capable alpha mode for the main window surface.
///
/// Preference: `PreMultiplied`, then `PostMultiplied`, then `Auto`.
/// `Opaque` is chosen only when no compositing mode is advertised.
pub fn select_compositing_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode::{Auto, PostMultiplied, PreMultiplied};

    if modes.contains(&PreMultiplied) {
        return PreMultiplied;
    }
    if modes.contains(&PostMultiplied) {
        return PostMultiplied;
    }
    if modes.contains(&Auto) {
        return Auto;
    }
    modes.first().copied().unwrap_or(Auto)
}

/// Returns true only for the alpha mode supported by the terminal's current
/// straight-source blend pipelines. `PostMultiplied`, `Auto`, and `Inherit`
/// are intentionally excluded until a matching pipeline path is implemented.
pub const fn alpha_mode_supports_transparency(mode: wgpu::CompositeAlphaMode) -> bool {
    matches!(mode, wgpu::CompositeAlphaMode::PreMultiplied)
}

#[cfg(test)]
mod surface_tests {
    use super::*;
    use wgpu::CompositeAlphaMode::{Auto, Opaque, PostMultiplied, PreMultiplied};

    #[cfg(all(
        target_os = "windows",
        not(feature = "backend-dx12"),
        not(feature = "backend-vulkan")
    ))]
    #[test]
    fn should_default_to_dx12_on_windows_for_compositor_transparency() {
        assert_eq!(selected_backends(), wgpu::Backends::DX12);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_configure_render_target_as_topmost_when_on_windows() {
        // Arrange
        let expected = true;

        // Act
        let is_topmost = RENDER_TARGET_IS_TOPMOST;

        // Assert
        assert_eq!(is_topmost, expected);
    }

    fn range(row: usize, start_col: usize, end_col: usize) -> DirtyRange {
        DirtyRange {
            row,
            start_col,
            end_col,
        }
    }

    #[test]
    fn should_select_premultiplied_when_all_compositing_modes_present() {
        // Arrange
        let modes = [Opaque, Auto, PostMultiplied, PreMultiplied];

        // Act
        let selected = select_compositing_alpha_mode(&modes);

        // Assert
        assert_eq!(selected, PreMultiplied);
    }

    #[test]
    fn should_select_postmultiplied_when_premultiplied_absent() {
        // Arrange
        let modes = [Opaque, Auto, PostMultiplied];

        // Act
        let selected = select_compositing_alpha_mode(&modes);

        // Assert
        assert_eq!(selected, PostMultiplied);
    }

    #[test]
    fn should_select_auto_when_only_auto_and_opaque() {
        // Arrange
        let modes = [Opaque, Auto];

        // Act
        let selected = select_compositing_alpha_mode(&modes);

        // Assert
        assert_eq!(selected, Auto);
    }

    #[test]
    fn should_select_opaque_when_only_opaque_advertised() {
        // Arrange
        let modes = [Opaque];

        // Act
        let selected = select_compositing_alpha_mode(&modes);

        // Assert
        assert_eq!(selected, Opaque);
    }

    #[test]
    fn should_select_auto_when_modes_empty() {
        // Arrange
        let modes: [wgpu::CompositeAlphaMode; 0] = [];

        // Act
        let selected = select_compositing_alpha_mode(&modes);

        // Assert
        assert_eq!(selected, Auto);
    }

    #[test]
    fn should_only_allow_supported_compositing_mode_for_transparency() {
        assert!(alpha_mode_supports_transparency(PreMultiplied));
        assert!(!alpha_mode_supports_transparency(PostMultiplied));
        assert!(!alpha_mode_supports_transparency(Auto));
        assert!(!alpha_mode_supports_transparency(Opaque));
        assert!(!alpha_mode_supports_transparency(
            wgpu::CompositeAlphaMode::Inherit
        ));
    }

    #[test]
    fn should_never_select_opaque_when_compositing_mode_exists() {
        // Arrange
        let cases = [
            &[PreMultiplied, Opaque][..],
            &[Opaque, PostMultiplied][..],
            &[Auto, Opaque][..],
            &[Opaque, Auto, PostMultiplied, PreMultiplied][..],
        ];

        for modes in cases {
            // Act
            let selected = select_compositing_alpha_mode(modes);

            // Assert
            assert_ne!(selected, Opaque, "modes={modes:?}");
        }
    }

    #[test]
    fn upload_policy_selects_none_incremental_and_full_uploads() {
        let policy = UploadPolicy::default();
        assert_eq!(policy.decide(2, 2, 4, &[], false).mode, UploadMode::None);
        assert_eq!(
            policy.decide(10, 10, 4, &[range(2, 1, 2)], false).mode,
            UploadMode::Incremental
        );
        assert_eq!(
            policy
                .decide(
                    10,
                    10,
                    4,
                    &[
                        range(0, 0, 10),
                        range(1, 0, 10),
                        range(2, 0, 10),
                        range(3, 0, 10),
                        range(4, 0, 10)
                    ],
                    false
                )
                .mode,
            UploadMode::Full
        );
    }

    #[test]
    fn upload_policy_uses_full_upload_for_fragmented_or_forced_damage() {
        let policy = UploadPolicy::default();
        let fragmented = (0..65).map(|row| range(row, 0, 1)).collect::<Vec<_>>();
        assert_eq!(
            policy.decide(100, 100, 4, &fragmented, false).mode,
            UploadMode::Full
        );
        assert_eq!(
            policy.decide(2, 2, 8, &[range(1, 1, 2)], true).mode,
            UploadMode::Full
        );
    }
}

// ── GpuContext ────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
struct CompositionHost {
    device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

#[cfg(target_os = "windows")]
impl CompositionHost {
    fn commit(&self) {
        if let Err(error) = unsafe { self.device.Commit() } {
            tracing::warn!(?error, "failed to commit DirectComposition surface");
        }
    }
}

/// The desktop composition target layer for the renderer surface is always the upper slot (ADR 0028).
#[cfg(target_os = "windows")]
pub const RENDER_TARGET_IS_TOPMOST: bool = true;

#[cfg(target_os = "windows")]
fn create_main_surface(
    instance: &wgpu::Instance,
    window: &Arc<Window>,
    backends: wgpu::Backends,
) -> Result<(wgpu::Surface<'static>, Option<CompositionHost>)> {
    if !backends.contains(wgpu::Backends::DX12) {
        return Ok((
            instance
                .create_surface(Arc::clone(window))
                .context("create surface")?,
            None,
        ));
    }

    let handle = window.window_handle().context("get Win32 window handle")?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        anyhow::bail!("main window is not a Win32 window");
    };

    // A regular HWND swap chain is always reported as opaque by DXGI. Hosting
    // wgpu's swap chain on a DirectComposition visual exposes premultiplied
    // alpha, which lets DWM's Acrylic backdrop remain visible in the client area.
    let device: IDCompositionDevice = unsafe {
        DCompositionCreateDevice(None::<&IDXGIDevice>).context("create DirectComposition device")?
    };
    let target = unsafe {
        device
            .CreateTargetForHwnd(HWND(handle.hwnd.get() as *mut _), RENDER_TARGET_IS_TOPMOST)
            .context("create DirectComposition window target")?
    };
    let visual = unsafe {
        device
            .CreateVisual()
            .context("create DirectComposition visual")?
    };
    unsafe {
        target
            .SetRoot(&visual)
            .context("set DirectComposition root visual")?;
        device.Commit().context("commit DirectComposition tree")?;
    }
    let surface = unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(
                visual.as_raw(),
            ))
            .context("create DirectComposition surface")?
    };

    Ok((
        surface,
        Some(CompositionHost {
            device,
            _target: target,
            _visual: visual,
        }),
    ))
}

/// Shared GPU handles for layers to create and upload resources.
///
/// Fields are private — layers access device/queue/surface through methods only.
pub struct GpuContext {
    /// wgpu instance, kept alive for secondary surface creation (e.g. dialog windows).
    instance: Arc<wgpu::Instance>,
    /// Adapter, kept alive for secondary surface capability queries.
    adapter: wgpu::Adapter,
    /// Keeps the DirectComposition visual tree alive for the main surface.
    #[cfg(target_os = "windows")]
    _composition_host: Option<CompositionHost>,
    /// wgpu surface bound to the main window, provides frame buffers.
    surface: wgpu::Surface<'static>,
    /// Logical GPU device for creating pipelines / textures / buffers.
    device: wgpu::Device,
    /// Command submission queue.
    queue: wgpu::Queue,
    /// Fixed adaptive policy used by cell-grid upload paths.
    upload_policy: UploadPolicy,
    /// Surface configuration (format, size, present mode).
    ///
    /// Interior mutability lets the winit integration reconfigure during a frame
    /// while CustomPaint still borrows this context shared via the Host TLS seam.
    config: RefCell<wgpu::SurfaceConfiguration>,
    /// Shared untextured colored-quad pipeline (background / decoration / selection).
    colored_quad_pipeline: Arc<wgpu::RenderPipeline>,
}

impl GpuContext {
    /// Creates the GPU surface, device, queue, surface configuration, and the
    /// shared colored-quad pipeline from the window.
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        tracing::info!(
            width = size.width,
            height = size.height,
            "creating gpu context"
        );
        let backends = selected_backends();

        let instance = Arc::new(wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        }));
        #[cfg(target_os = "windows")]
        let (surface, composition_host) = create_main_surface(&instance, &window, backends)?;
        #[cfg(not(target_os = "windows"))]
        let surface = instance.create_surface(window).context("create surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .context("request adapter")?;
        let info = adapter.get_info();
        tracing::info!(
            name = %info.name,
            backend = ?info.backend,
            device_type = ?info.device_type,
            "selected gpu adapter"
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                // MemoryUsage: pre-allocate 8 MB device blocks instead of 128 MB.
                // A terminal emitter never allocates large GPU buffers, so the
                // smaller block size is sufficient and avoids unnecessary VRAM
                // reservation at startup.
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                // memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request device")?;

        // Prefer sRGB format so fragment shader colours display correctly.
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: select_compositing_alpha_mode(&capabilities.alpha_modes),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        #[cfg(target_os = "windows")]
        if let Some(host) = composition_host.as_ref() {
            host.commit();
        }

        tracing::info!(
            width = config.width,
            height = config.height,
            ?format,
            "gpu context configured"
        );

        let colored_quad_pipeline = Arc::new(create_colored_quad_pipeline(
            &device,
            config.format,
            "colored-quad pipeline",
        ));

        Ok(Self {
            instance,
            adapter,
            #[cfg(target_os = "windows")]
            _composition_host: composition_host,
            surface,
            device,
            queue,
            upload_policy: UploadPolicy::default(),
            config: RefCell::new(config),
            colored_quad_pipeline,
        })
    }

    /// Frame-scoped borrow of Host-owned presentation resources.
    pub fn borrow_frame(&self) -> (&wgpu::Surface<'static>, &wgpu::Device, &wgpu::Queue) {
        (&self.surface, &self.device, &self.queue)
    }

    /// Updates the Host-owned surface configuration and applies it.
    ///
    /// Called by the winit integration through a frame-scoped configure seam.
    pub fn configure_size(&self, width: u32, height: u32) {
        debug_assert!(
            width > 0 && height > 0,
            "zero-sized configure is refused by the adapter"
        );
        let mut config = self.config.borrow_mut();
        config.width = width;
        config.height = height;
        tracing::debug!(width, height, "configuring surface");
        self.surface.configure(&self.device, &config);
        #[cfg(target_os = "windows")]
        if let Some(host) = self._composition_host.as_ref() {
            host.commit();
        }
    }

    /// Main window surface borrowed by the frame integration.
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// Surface pixel format.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.borrow().format
    }

    /// Configured surface composite alpha mode.
    pub fn alpha_mode(&self) -> wgpu::CompositeAlphaMode {
        self.config.borrow().alpha_mode
    }

    /// Current surface dimensions `(width, height)`.
    pub fn surface_size(&self) -> (u32, u32) {
        let config = self.config.borrow();
        (config.width, config.height)
    }

    pub fn upload_plan(
        &self,
        rows: usize,
        cols: usize,
        bytes_per_cell: usize,
        dirty_ranges: &[DirtyRange],
        force_full: bool,
    ) -> UploadPlan {
        self.upload_policy
            .decide(rows, cols, bytes_per_cell, dirty_ranges, force_full)
    }

    pub fn write_buffer(&self, buffer: &wgpu::Buffer, offset: wgpu::BufferAddress, data: &[u8]) {
        self.queue.write_buffer(buffer, offset, data);
    }
    /// Logical GPU device reference.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Command queue reference.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Shared untextured colored-quad pipeline (background / decoration / selection).
    pub fn colored_quad_pipeline(&self) -> Arc<wgpu::RenderPipeline> {
        Arc::clone(&self.colored_quad_pipeline)
    }

    /// Creates a wgpu surface from an owned window handle, using the same Instance.
    /// The returned surface has a `'static` lifetime and the caller is responsible
    /// for configuring the surface.
    pub fn create_surface(&self, window: Arc<winit::window::Window>) -> wgpu::Surface<'static> {
        self.instance
            .create_surface(window)
            .expect("create dialog surface")
    }

    /// Queries surface capabilities for a new surface, using the stored adapter.
    pub fn surface_capabilities(&self, surface: &wgpu::Surface) -> wgpu::SurfaceCapabilities {
        surface.get_capabilities(&self.adapter)
    }

    // ── startup surface operations ───────────────────────────────────────

    /// Acquires the surface texture, submits a single clear-color render pass,
    /// and presents the frame. No-ops on non-`Success` variants to keep the
    /// startup fast path simple — `Suboptimal` surfaces are intentionally
    /// skipped rather than presented with a size mismatch.
    pub fn clear_surface(&self, color: wgpu::Color) {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            _ => return,
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(output);
    }
}

// ── Shared vertex type ────────────────────────────────────────────────────

/// GPU vertex for textured quads. Replaces both `text::Vertex` and
/// `cursor::CursorVertex`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexturedVertex {
    /// NDC position (x, y), range [-1, 1].
    pub position: [f32; 2],
    /// Texture coordinates (u, v), range [0, 1].
    pub tex_coords: [f32; 2],
    /// Per-vertex RGBA tint, normalized [0, 1]. Glyph shader multiplies
    /// `glyph_alpha * color.a`, using `color.rgb` as the literal color.
    pub color: [f32; 4],
}

impl Default for TexturedVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            tex_coords: [0.0; 2],
            color: [1.0; 4],
        }
    }
}

impl TexturedVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    /// Returns the vertex buffer layout matching `TexturedVertex` memory layout.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds 6 vertices (two triangles) from a pixel-space rect, atlas UV
    /// rect, and tint color, transformed to clip space.
    ///
    /// # Parameters
    /// - `left/top/right/bottom`: pixel-space rectangle
    /// - `uv_l/uv_t/uv_r/uv_b`: atlas sub-region UV rectangle
    /// - `color`: RGBA tint to apply (shader multiplies alpha, uses rgb as literal)
    /// - `surf_w/surf_h`: surface dimensions (for pixel→NDC transform)
    #[allow(clippy::too_many_arguments)]
    pub fn from_pixel_rect(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        uv_l: f32,
        uv_t: f32,
        uv_r: f32,
        uv_b: f32,
        color: [f32; 4],
        surf_w: f32,
        surf_h: f32,
    ) -> [Self; 6] {
        // Pixel → NDC [-1, 1]: linear x mapping, y-flip (screen is y-down, NDC is y-up).
        let ndc_left = left / surf_w * 2.0 - 1.0;
        let ndc_right = right / surf_w * 2.0 - 1.0;
        let ndc_top = 1.0 - top / surf_h * 2.0;
        let ndc_bottom = 1.0 - bottom / surf_h * 2.0;

        // Two triangles forming a quad: TL → BL → BR, TL → BR → TR.
        [
            Self {
                position: [ndc_left, ndc_top],
                tex_coords: [uv_l, uv_t],
                color,
            },
            Self {
                position: [ndc_left, ndc_bottom],
                tex_coords: [uv_l, uv_b],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
                color,
            },
            Self {
                position: [ndc_left, ndc_top],
                tex_coords: [uv_l, uv_t],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                tex_coords: [uv_r, uv_b],
                color,
            },
            Self {
                position: [ndc_right, ndc_top],
                tex_coords: [uv_r, uv_t],
                color,
            },
        ]
    }
}

// ── ColoredVertex ──────────────────────────────────────────────────────────

/// GPU vertex for solid-color quads (background rects, decoration rects).
/// No texture coordinates — color is per-vertex.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColoredVertex {
    /// NDC position (x, y), range [-1, 1].
    pub position: [f32; 2],
    /// Per-vertex RGBA color, normalized [0, 1].
    pub color: [f32; 4],
}

impl Default for ColoredVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 2],
            color: [0.0; 4],
        }
    }
}

impl ColoredVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    /// Returns the vertex buffer layout matching `ColoredVertex` memory layout.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// Builds 6 vertices (two triangles) from a pixel-space rect and a single
    /// color, transformed to clip space.
    #[allow(clippy::too_many_arguments)]
    pub fn from_pixel_rect(
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        color: [f32; 4],
        surf_w: f32,
        surf_h: f32,
    ) -> [Self; 6] {
        let ndc_left = left / surf_w * 2.0 - 1.0;
        let ndc_right = right / surf_w * 2.0 - 1.0;
        let ndc_top = 1.0 - top / surf_h * 2.0;
        let ndc_bottom = 1.0 - bottom / surf_h * 2.0;

        [
            Self {
                position: [ndc_left, ndc_top],
                color,
            },
            Self {
                position: [ndc_left, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_left, ndc_top],
                color,
            },
            Self {
                position: [ndc_right, ndc_bottom],
                color,
            },
            Self {
                position: [ndc_right, ndc_top],
                color,
            },
        ]
    }
}

/// Creates a vertex buffer from a slice of `ColoredVertex`. Uploads one
/// zero vertex when the slice is empty (wgpu requires non-zero buffers);
/// the caller must set `vertex_count` to 0 to skip drawing.
pub fn create_colored_vertex_buffer(
    device: &wgpu::Device,
    vertices: &[ColoredVertex],
) -> wgpu::Buffer {
    let vertices = if vertices.is_empty() {
        &[ColoredVertex::default()]
    } else {
        vertices
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("colored vertex buffer"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

/// WGSL for untextured per-vertex color quads (`ColoredVertex` layout).
const COLORED_QUAD_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

/// Builds the untextured colored-quad pipeline once at `GpuContext` construction.
/// Layers clone the `Arc` instead of creating their own GPU objects.
fn create_colored_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(COLORED_QUAD_SHADER.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(ColoredVertex::layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
    })
}

// ── Shared GPU helpers ────────────────────────────────────────────────────

/// Bind group layout used by the text layer (texture + sampler).
pub fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture bind group layout"),
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
    })
}

/// Creates a vertex buffer from a slice of `TexturedVertex`. Uploads one
/// zero vertex when the slice is empty (wgpu requires non-zero buffers);
/// the caller must set `vertex_count` to 0 to skip drawing.
pub fn create_vertex_buffer(device: &wgpu::Device, vertices: &[TexturedVertex]) -> wgpu::Buffer {
    let vertices = if vertices.is_empty() {
        &[TexturedVertex {
            position: [0.0, 0.0],
            tex_coords: [0.0, 0.0],
            color: [1.0; 4],
        }]
    } else {
        vertices
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vertex buffer"),
        contents: bytemuck::cast_slice(vertices),
        // COPY_DST lets CursorLayer use queue.write_buffer for partial updates.
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}
/// Creates an uninitialized vertex buffer of exactly `vertex_count` vertices.
///
/// This avoids allocating a temporary zero-filled CPU vertex array during resize.
pub fn create_vertex_buffer_sized(device: &wgpu::Device, vertex_count: usize) -> wgpu::Buffer {
    let byte_len = vertex_count
        .checked_mul(std::mem::size_of::<TexturedVertex>())
        .expect("vertex buffer size overflow");
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vertex buffer"),
        size: byte_len.max(std::mem::size_of::<TexturedVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
