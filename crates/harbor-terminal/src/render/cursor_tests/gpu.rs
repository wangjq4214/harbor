// The `gpu::` module name keeps these in the renderer's explicit GPU test recipe.
mod gpu {
    use super::super::*;
    use crate::Terminal;
    use crate::render::gpu::TexturedVertex;

    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 32;
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    fn metrics() -> TextMetrics {
        // Cursor::new retains this public argument even though projection uses the viewport.
        TextMetrics {
            cell_width: 8.0,
            line_height: 10.0,
            ascent: 8.0,
            underline_position: 9.0,
            underline_thickness: 1.0,
            strikethrough_position: 5.0,
            strikethrough_thickness: 1.0,
        }
    }

    fn viewport(width: f32, height: f32, origin: (f32, f32)) -> RenderViewport {
        RenderViewport {
            cell_width: width,
            line_height: height,
            padding: 1.0,
            allocation_origin: origin,
            allocation_size: (WIDTH, HEIGHT),
            surface_size: (WIDTH, HEIGHT),
        }
    }

    fn snapshot() -> TerminalSnapshot {
        let mut snap = Terminal::new_headless(3, 4).snapshot();
        snap.cursor_x = 1;
        snap.cursor_y = 1;
        snap.cursor_visible = true;
        snap.cursor_blink = true;
        snap.cursor_shape = CursorShape::Block;
        snap
    }

    fn raster(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        clear: wgpu::Color,
        draw: impl FnOnce(&mut wgpu::RenderPass<'_>),
    ) -> Vec<u8> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cursor readback"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cursor readback buffer"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cursor readback pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            draw(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
        buffer.unmap();
        pixels
    }

    fn cursor_pixels(device: &wgpu::Device, queue: &wgpu::Queue, cursor: &Cursor) -> Vec<u8> {
        raster(device, queue, FORMAT, wgpu::Color::TRANSPARENT, |pass| {
            cursor.draw(pass);
        })
    }

    fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        let offset = (y * WIDTH as usize + x) * 4;
        pixels[offset..offset + 4].try_into().unwrap()
    }

    fn assert_pixel_near(actual: [u8; 4], expected: [u8; 4]) {
        for (a, e) in actual.into_iter().zip(expected) {
            assert!(
                a.abs_diff(e) <= 1,
                "pixel {actual:?} differs from {expected:?}"
            );
        }
    }

    fn assert_blank(pixels: &[u8]) {
        assert!(pixels.iter().all(|byte| *byte == 0));
    }

    // A test-only reference to the original textured cursor pipeline. Exact whole-image
    // comparisons catch changes to triangle ordering, fractional coverage, blending,
    // channel order, or sRGB conversion when the production pipeline changes.
    const LEGACY_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
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

    fn legacy_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("legacy cursor shader"),
            source: wgpu::ShaderSource::Wgsl(LEGACY_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("legacy cursor pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("legacy cursor pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TexturedVertex::layout())],
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

    fn legacy_vertices(
        snap: &TerminalSnapshot,
        view: &RenderViewport,
        color: Rgba,
    ) -> [TexturedVertex; 6] {
        let (left, top) = view.cell_pos(snap.cursor_y, snap.cursor_x);
        let right = left + view.cell_width;
        let bottom = top + view.line_height;
        let (left, top, right, bottom) = match snap.cursor_shape {
            CursorShape::Block => (left, top, right, bottom),
            CursorShape::Underline => (
                left,
                bottom - (view.line_height * 0.1).max(2.0),
                right,
                bottom,
            ),
            CursorShape::Bar => (left, top, left + (view.cell_width * 0.15).max(2.0), bottom),
        };
        let (surf_w, surf_h) = view.surface_dimensions();
        TexturedVertex::from_pixel_rect(
            left,
            top,
            right,
            bottom,
            0.0,
            0.0,
            1.0,
            1.0,
            color.components(),
            surf_w,
            surf_h,
        )
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn cursor_shapes_draw_exact_rectangles_with_straight_alpha() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = TerminalGpuAccess::new(&device, &queue, FORMAT);
        let view = viewport(8.0, 10.0, (3.0, 2.0));
        let mut snap = snapshot();
        let mut cursor = Cursor::new(gpu, metrics(), Rgba::new(1.0, 0.25, 0.0, 0.5));
        for (shape, rect) in [
            (CursorShape::Block, (12, 13, 20, 23)),
            (CursorShape::Underline, (12, 21, 20, 23)),
            (CursorShape::Bar, (12, 13, 14, 23)),
        ] {
            snap.cursor_shape = shape;
            cursor.prepare(gpu, Some(&snap), &view, false, true);
            let pixels = cursor_pixels(&device, &queue, &cursor);
            for y in 0..HEIGHT as usize {
                for x in 0..WIDTH as usize {
                    let actual = pixel(&pixels, x, y);
                    if x >= rect.0 && x < rect.2 && y >= rect.1 && y < rect.3 {
                        assert_pixel_near(actual, [128, 32, 0, 128]);
                    } else {
                        assert_eq!(actual, [0; 4], "{shape:?} at ({x}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn cursor_matches_legacy_pixels_across_formats_fractional_geometry_and_backgrounds() {
        let (device, queue) = crate::test_gpu::device();
        let color = Rgba::new(0.79, 0.21, 0.43, 0.37);
        let views = [
            viewport(13.5, 15.25, (2.25, 1.75)),
            viewport(20.0, 24.0, (3.25, 1.75)),
        ];
        let backgrounds = [
            wgpu::Color::TRANSPARENT,
            wgpu::Color {
                r: 0.13,
                g: 0.42,
                b: 0.27,
                a: 0.3,
            },
        ];
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let gpu = TerminalGpuAccess::new(&device, &queue, format);
            let mut cursor = Cursor::new(gpu, metrics(), color);
            let pipeline = legacy_pipeline(&device, format);
            let mut snap = snapshot();
            snap.cursor_y = 0;
            for view in views {
                for shape in [CursorShape::Block, CursorShape::Underline, CursorShape::Bar] {
                    snap.cursor_shape = shape;
                    cursor.invalidate_projection();
                    cursor.prepare(gpu, Some(&snap), &view, false, true);
                    let vertices = legacy_vertices(&snap, &view, color);
                    let vertex_buffer =
                        crate::render::gpu::create_vertex_buffer(&device, &vertices);
                    for clear in backgrounds {
                        let actual =
                            raster(&device, &queue, format, clear, |pass| cursor.draw(pass));
                        let legacy = raster(&device, &queue, format, clear, |pass| {
                            pass.set_pipeline(&pipeline);
                            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            pass.draw(0..6, 0..1);
                        });
                        assert_eq!(actual, legacy, "{format:?}, {shape:?}, {view:?}, {clear:?}");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn overlapping_cursors_preserve_draw_order_and_alpha_composition() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = TerminalGpuAccess::new(&device, &queue, FORMAT);
        let view = viewport(8.0, 10.0, (3.0, 2.0));
        let snap = snapshot();
        let mut red = Cursor::new(gpu, metrics(), Rgba::new(1.0, 0.0, 0.0, 0.5));
        let mut blue = Cursor::new(gpu, metrics(), Rgba::new(0.0, 0.0, 1.0, 0.5));
        red.prepare(gpu, Some(&snap), &view, false, true);
        blue.prepare(gpu, Some(&snap), &view, false, true);
        let red_then_blue = raster(&device, &queue, FORMAT, wgpu::Color::TRANSPARENT, |pass| {
            red.draw(pass);
            blue.draw(pass);
        });
        let blue_then_red = raster(&device, &queue, FORMAT, wgpu::Color::TRANSPARENT, |pass| {
            blue.draw(pass);
            red.draw(pass);
        });
        assert_pixel_near(pixel(&red_then_blue, 13, 14), [64, 0, 128, 191]);
        assert_pixel_near(pixel(&blue_then_red, 13, 14), [128, 0, 64, 191]);
        assert_eq!(pixel(&red_then_blue, 0, 0), [0; 4]);
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn cursor_visibility_blink_preedit_and_bounds_hide_and_reappear() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = TerminalGpuAccess::new(&device, &queue, FORMAT);
        let view = viewport(8.0, 10.0, (3.0, 2.0));
        let mut snap = snapshot();
        let mut cursor = Cursor::new(gpu, metrics(), Rgba::new(0.8, 0.2, 0.4, 0.5));
        cursor.prepare(gpu, Some(&snap), &view, false, true);
        let visible = cursor_pixels(&device, &queue, &cursor);
        assert_ne!(pixel(&visible, 13, 14), [0; 4]);

        cursor.prepare(gpu, Some(&snap), &view, true, true);
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        assert!(!cursor.is_dirty());
        cursor.prepare(gpu, Some(&snap), &view, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), visible);

        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        cursor.prepare(gpu, Some(&snap), &view, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), visible);

        snap.cursor_blink = false;
        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), visible);
        snap.cursor_visible = false;
        cursor.prepare(gpu, Some(&snap), &view, false, true);
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        snap.cursor_visible = true;
        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), visible);

        snap.cursor_x = snap.cols;
        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        // The same position reappears when the snapshot expands: an invisible
        // or off-snapshot cursor was not cached as the last drawn state.
        snap.cols += 1;
        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_ne!(
            pixel(&cursor_pixels(&device, &queue, &cursor), 37, 14),
            [0; 4]
        );
        snap.cursor_x = 1;
        cursor.prepare(gpu, Some(&snap), &view, false, false);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), visible);
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn cursor_retains_early_returns_dirty_state_and_draw_after_invalidation() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = TerminalGpuAccess::new(&device, &queue, FORMAT);
        let view = viewport(8.0, 10.0, (3.0, 2.0));
        let mut moved = view;
        moved.allocation_origin = (11.0, 2.0);
        let snap = snapshot();
        let mut cursor = Cursor::new(gpu, metrics(), Rgba::new(1.0, 0.0, 0.0, 0.5));

        assert!(cursor.is_dirty());
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        cursor.prepare(gpu, None, &view, false, true);
        assert!(cursor.is_dirty(), "prepare(None) does not commit");
        cursor.commit_frame();
        assert!(!cursor.is_dirty());
        cursor.prepare(gpu, None, &view, false, true);
        assert!(!cursor.is_dirty());
        cursor.prepare(gpu, Some(&snap), &view, false, true);
        let original = cursor_pixels(&device, &queue, &cursor);
        assert_ne!(pixel(&original, 13, 14), [0; 4]);

        // Even after invalidation, draw still uses the previous vertices. A
        // public commit can cancel the invalidation without rewriting them.
        cursor.invalidate_projection();
        assert!(cursor.is_dirty());
        assert_eq!(cursor_pixels(&device, &queue, &cursor), original);
        cursor.commit_frame();
        assert!(!cursor.is_dirty());
        let mut narrower = snap.clone();
        narrower.cols = snap.cursor_x;
        cursor.prepare(gpu, Some(&narrower), &moved, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), original);
        let mut shorter = snap.clone();
        shorter.rows = snap.cursor_y;
        cursor.prepare(gpu, Some(&shorter), &moved, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), original);

        cursor.invalidate_projection();
        assert_eq!(cursor_pixels(&device, &queue, &cursor), original);
        cursor.prepare(gpu, Some(&narrower), &moved, false, true);
        assert!(!cursor.is_dirty());
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        cursor.prepare(gpu, Some(&snap), &moved, false, true);
        let relocated = cursor_pixels(&device, &queue, &cursor);
        assert_ne!(relocated, original);
        assert_eq!(pixel(&relocated, 13, 14), [0; 4]);
        assert_ne!(pixel(&relocated, 21, 14), [0; 4]);

        // Color changes only upload after prepare; an explicit commit keeps
        // the old pixels if the cached cursor state is otherwise unchanged.
        cursor.set_color(Rgba::new(0.0, 0.0, 1.0, 0.5));
        assert!(cursor.is_dirty());
        assert_eq!(cursor_pixels(&device, &queue, &cursor), relocated);
        cursor.commit_frame();
        cursor.prepare(gpu, Some(&snap), &moved, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), relocated);
        cursor.invalidate_projection();
        cursor.prepare(gpu, Some(&snap), &moved, false, true);
        let recolored = cursor_pixels(&device, &queue, &cursor);
        assert_ne!(recolored, relocated);
        assert_pixel_near(pixel(&recolored, 21, 14), [0, 0, 128, 128]);

        cursor.invalidate_projection();
        cursor.prepare(gpu, None, &moved, false, true);
        assert!(
            cursor.is_dirty(),
            "prepare(None) preserves a pending invalidation"
        );
        assert_blank(&cursor_pixels(&device, &queue, &cursor));
        cursor.commit_frame();
        cursor.prepare(gpu, Some(&snap), &moved, false, true);
        assert_eq!(cursor_pixels(&device, &queue, &cursor), recolored);
        assert!(!cursor.is_dirty());
    }
}
