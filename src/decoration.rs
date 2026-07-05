use crate::{
    config::TEXT_PADDING,
    gpu::{self, ColoredVertex, GpuContext},
    metrics::TextMetrics,
    render::Layer,
    terminal::CellAttrs,
    terminal::Screen,
};

// ── Decoration shader ─────────────────────────────────────────────────────────

/// Simple untextured shader that renders per-vertex color quads (identical to
/// BackgroundLayer's shader, duplicated per "no shared GPU objects" convention).
const DECORATION_SHADER: &str = r#"
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

// ── Vertex builders (free fn, testable without GPU handles) ───────────────────

/// Builds underline vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub(crate) fn build_underline_vertices(
    cell_width: f32,
    line_height: f32,
    underline_pos: f32,
    underline_thickness: f32,
    screen: &Screen,
    surf_w: f32,
    surf_h: f32,
) -> Vec<ColoredVertex> {
    let mut verts = Vec::with_capacity(screen.rows() * screen.cols() * 6);
    for row in 0..screen.rows() {
        let cell_top = TEXT_PADDING + row as f32 * line_height;
        let u_top = cell_top + underline_pos;
        let u_bottom = u_top + underline_thickness;
        for col in 0..screen.cols() {
            let cell = screen.cell(row, col);
            if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
                let color = cell.fg.to_rgba();
                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, u_top, right, u_bottom, color, surf_w, surf_h,
                ));
            } else {
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
    }
    verts
}

/// Builds strikethrough vertices for every row.
/// Returns one `ColoredVertex` per grid cell (degenerate for cells without decoration).
pub(crate) fn build_strikethrough_vertices(
    cell_width: f32,
    line_height: f32,
    strikethrough_pos: f32,
    strikethrough_thickness: f32,
    screen: &Screen,
    surf_w: f32,
    surf_h: f32,
) -> Vec<ColoredVertex> {
    let mut verts = Vec::with_capacity(screen.rows() * screen.cols() * 6);
    for row in 0..screen.rows() {
        let cell_top = TEXT_PADDING + row as f32 * line_height;
        let s_top = cell_top + strikethrough_pos - strikethrough_thickness / 2.0;
        let s_bottom = s_top + strikethrough_thickness;
        for col in 0..screen.cols() {
            let cell = screen.cell(row, col);
            if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
                let color = cell.fg.to_rgba();
                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, s_top, right, s_bottom, color, surf_w, surf_h,
                ));
            } else {
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
    }
    verts
}

// ── DecorationLayer ───────────────────────────────────────────────────────────

/// Draws underline and strikethrough decorations on top of text and cursor.
/// Uses two separate vertex buffers (one per decoration type) because they
/// need separate draw calls.
pub(crate) struct DecorationLayer {
    pipeline: wgpu::RenderPipeline,
    underline_buffer: wgpu::Buffer,
    strikethrough_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
    cell_width: f32,
    line_height: f32,
    underline_pos: f32,
    underline_thickness: f32,
    strikethrough_pos: f32,
    strikethrough_thickness: f32,
}

impl DecorationLayer {
    /// Creates the decoration render pipeline and pre-allocates vertex buffers
    /// for the full grid (rows × cols × 6 vertices each).
    pub(crate) fn new(gpu: &GpuContext, screen: &Screen, metrics: TextMetrics) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());

        let rows = screen.rows();
        let cols = screen.cols();
        let max_vertices = rows * cols * 6;
        let empty = vec![ColoredVertex::default(); max_vertices.max(1)];
        let underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
        let strikethrough_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);

        let mut layer = Self {
            pipeline,
            underline_buffer,
            strikethrough_buffer,
            dirty: true,
            rows,
            cols,
            cell_width: metrics.cell_width,
            line_height: metrics.line_height,
            underline_pos: metrics.underline_position,
            underline_thickness: metrics.underline_thickness,
            strikethrough_pos: metrics.strikethrough_position,
            strikethrough_thickness: metrics.strikethrough_thickness,
        };

        // Build initial vertex data and upload.
        let (surf_w, surf_h) = gpu.surface_size();
        let u = build_underline_vertices(
            layer.cell_width,
            layer.line_height,
            layer.underline_pos,
            layer.underline_thickness,
            screen,
            surf_w as f32,
            surf_h as f32,
        );
        let s = build_strikethrough_vertices(
            layer.cell_width,
            layer.line_height,
            layer.strikethrough_pos,
            layer.strikethrough_thickness,
            screen,
            surf_w as f32,
            surf_h as f32,
        );
        gpu.queue()
            .write_buffer(&layer.underline_buffer, 0, bytemuck::cast_slice(&u));
        gpu.queue()
            .write_buffer(&layer.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
        layer.dirty = false;

        layer
    }

    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("decoration shader"),
            source: wgpu::ShaderSource::Wgsl(DECORATION_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("decoration pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("decoration pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(ColoredVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
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
            multiview_mask: None,
            cache: None,
        })
    }

    /// Forces a full vertex rebuild on the next `prepare` (called after resize).
    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl Layer for DecorationLayer {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        let Some(screen) = screen else {
            return;
        };
        let (surf_w, surf_h) = gpu.surface_size();

        // Detect resize: dimensions changed → reallocate and full rebuild.
        if screen.rows() != self.rows || screen.cols() != self.cols {
            tracing::trace!(
                rows = screen.rows(),
                cols = screen.cols(),
                "decoration layer resize"
            );
            let new_cap = screen.rows() * screen.cols() * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                let empty = vec![ColoredVertex::default(); new_cap.max(1)];
                self.underline_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
                self.strikethrough_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &empty);
            }
            let u = build_underline_vertices(
                self.cell_width,
                self.line_height,
                self.underline_pos,
                self.underline_thickness,
                screen,
                surf_w as f32,
                surf_h as f32,
            );
            let s = build_strikethrough_vertices(
                self.cell_width,
                self.line_height,
                self.strikethrough_pos,
                self.strikethrough_thickness,
                screen,
                surf_w as f32,
                surf_h as f32,
            );
            gpu.queue()
                .write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.queue()
                .write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
            self.rows = screen.rows();
            self.cols = screen.cols();
            self.dirty = false;
            return;
        }

        let any_dirty_rows = screen.dirty_rows().next().is_some();
        if !self.dirty && !any_dirty_rows {
            return;
        }

        if self.dirty {
            tracing::trace!("rebuilding decoration draw batch (full)");
            let u = build_underline_vertices(
                self.cell_width,
                self.line_height,
                self.underline_pos,
                self.underline_thickness,
                screen,
                surf_w as f32,
                surf_h as f32,
            );
            let s = build_strikethrough_vertices(
                self.cell_width,
                self.line_height,
                self.strikethrough_pos,
                self.strikethrough_thickness,
                screen,
                surf_w as f32,
                surf_h as f32,
            );
            gpu.queue()
                .write_buffer(&self.underline_buffer, 0, bytemuck::cast_slice(&u));
            gpu.queue()
                .write_buffer(&self.strikethrough_buffer, 0, bytemuck::cast_slice(&s));
        } else {
            tracing::trace!("rebuilding decoration draw batch (incremental)");
            for row in screen.dirty_rows() {
                let cell_top = TEXT_PADDING + row as f32 * self.line_height;
                let u_top = cell_top + self.underline_pos;
                let u_bottom = u_top + self.underline_thickness;
                let s_top = cell_top + self.strikethrough_pos - self.strikethrough_thickness / 2.0;
                let s_bottom = s_top + self.strikethrough_thickness;

                let mut u_row = Vec::with_capacity(screen.cols() * 6);
                let mut s_row = Vec::with_capacity(screen.cols() * 6);
                for col in 0..screen.cols() {
                    let cell = screen.cell(row, col);
                    if cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ' {
                        let left = TEXT_PADDING + col as f32 * self.cell_width;
                        let right = TEXT_PADDING + (col + 1) as f32 * self.cell_width;
                        let color = cell.fg.to_rgba();
                        u_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left,
                            u_top,
                            right,
                            u_bottom,
                            color,
                            surf_w as f32,
                            surf_h as f32,
                        ));
                    } else {
                        u_row.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
                    }

                    if cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ' {
                        let left = TEXT_PADDING + col as f32 * self.cell_width;
                        let right = TEXT_PADDING + (col + 1) as f32 * self.cell_width;
                        let color = cell.fg.to_rgba();
                        s_row.extend_from_slice(&ColoredVertex::from_pixel_rect(
                            left,
                            s_top,
                            right,
                            s_bottom,
                            color,
                            surf_w as f32,
                            surf_h as f32,
                        ));
                    } else {
                        s_row.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
                    }
                }

                let stride = screen.cols() * 6 * std::mem::size_of::<ColoredVertex>();
                let offset = (row * stride) as u64;
                gpu.queue().write_buffer(
                    &self.underline_buffer,
                    offset,
                    bytemuck::cast_slice(&u_row),
                );
                gpu.queue().write_buffer(
                    &self.strikethrough_buffer,
                    offset,
                    bytemuck::cast_slice(&s_row),
                );
            }
        }

        self.dirty = false;
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.underline_buffer.slice(..));
        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
        pass.set_vertex_buffer(0, self.strikethrough_buffer.slice(..));
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Color;

    fn test_screen(rows: usize, cols: usize) -> Screen {
        Screen::new(rows, cols)
    }

    #[test]
    fn underline_vertices_for_cell_with_attr() {
        let mut screen = test_screen(2, 3);
        // Set up extended RGB fg + underline.
        screen.set_sgr(&[Some(38), Some(2), Some(200), Some(50), Some(0)]); // fg = RGB(200,50,0)
        screen.set_sgr(&[Some(4)]); // underline on
        screen.write_char('a');
        screen.set_sgr(&[Some(0)]); // reset
        screen.write_char(' ');
        screen.write_char(' ');

        let verts = build_underline_vertices(10.0, 20.0, 18.0, 1.5, &screen, 800.0, 600.0);

        // Cell 0 (underline) should have non-zero color matching fg.
        assert_ne!(
            verts[0].color, [0.0; 4],
            "underline cell should have non-zero color"
        );
        let expected = Color::Rgb(200, 50, 0).to_rgba();
        assert_eq!(verts[0].color, expected, "underline color should match fg");

        // Cells 1-2 (no underline) should be degenerate.
        for i in 1..3 {
            let idx = i * 6;
            assert_eq!(verts[idx].color, [0.0; 4], "cell {i} should be degenerate");
        }
    }

    #[test]
    fn strikethrough_vertices_for_cell_with_attr() {
        let mut screen = test_screen(2, 2);
        screen.set_sgr(&[Some(38), Some(2), Some(0), Some(200), Some(50)]); // fg = RGB(0,200,50)
        screen.set_sgr(&[Some(9)]); // strikethrough on
        screen.write_char('x');
        screen.set_sgr(&[Some(0)]); // reset
        screen.write_char(' ');

        let verts = build_strikethrough_vertices(10.0, 20.0, 9.0, 1.5, &screen, 800.0, 600.0);

        assert_ne!(
            verts[0].color, [0.0; 4],
            "strikethrough cell should have non-zero color"
        );
        let expected = Color::Rgb(0, 200, 50).to_rgba();
        assert_eq!(
            verts[0].color, expected,
            "strikethrough color should match fg"
        );
        // Cell 1 should be degenerate.
        assert_eq!(verts[6].color, [0.0; 4], "cell 1 should be degenerate");
    }

    #[test]
    fn no_decoration_for_default_cell() {
        let screen = test_screen(1, 3);

        let u = build_underline_vertices(10.0, 20.0, 18.0, 1.5, &screen, 800.0, 600.0);
        let s = build_strikethrough_vertices(10.0, 20.0, 9.0, 1.5, &screen, 800.0, 600.0);

        for (i, v) in u.iter().enumerate() {
            assert_eq!(
                v.color, [0.0; 4],
                "default cell underline vertex {i} should be degenerate"
            );
        }
        for (i, v) in s.iter().enumerate() {
            assert_eq!(
                v.color, [0.0; 4],
                "default cell strikethrough vertex {i} should be degenerate"
            );
        }
    }

    #[test]
    fn no_decoration_for_blank_cell() {
        let mut screen = test_screen(1, 2);
        // Cell 0: underline attr but blank char.
        screen.set_sgr(&[Some(4)]);
        screen.write_char(' ');
        // Cell 1: strikethrough attr but blank char.
        screen.set_sgr(&[Some(0)]);
        screen.set_sgr(&[Some(9)]);
        screen.write_char(' ');

        let u = build_underline_vertices(10.0, 20.0, 18.0, 1.5, &screen, 800.0, 600.0);
        let s = build_strikethrough_vertices(10.0, 20.0, 9.0, 1.5, &screen, 800.0, 600.0);

        for v in &u {
            assert_eq!(
                v.color, [0.0; 4],
                "blank+underline cell should be degenerate"
            );
        }
        for v in &s {
            assert_eq!(
                v.color, [0.0; 4],
                "blank+strikethrough cell should be degenerate"
            );
        }
    }
}
