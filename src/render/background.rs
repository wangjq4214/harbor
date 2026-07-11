use crate::{
    config::TEXT_PADDING,
    render::{
        Component,
        gpu::{self, ColoredVertex, GpuContext},
    },
    terminal::{CellAttrs, Color, Screen},
};

// ── Background shader ─────────────────────────────────────────────────────────

/// Simple untextured shader that renders per-vertex color quads.
const BACKGROUND_SHADER: &str = r#"
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

// ── BackgroundLayer ───────────────────────────────────────────────────────────

/// Draws a solid-color rectangle behind each cell with a non-default background.
/// Rendered before the text layer so glyphs appear on top.
pub(crate) struct Background {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    dirty: bool,
    rows: usize,
    cols: usize,
    cell_width: f32,
    line_height: f32,
}

impl Background {
    /// Creates the background render pipeline and pre-allocates a vertex buffer
    /// for the full grid (rows × cols × 6 vertices).
    pub(crate) fn new(
        gpu: &GpuContext,
        screen: &Screen,
        cell_width: f32,
        line_height: f32,
    ) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());

        let rows = screen.rows();
        let cols = screen.cols();
        let max_vertices = rows * cols * 6;
        let vertex_buffer = gpu::create_colored_vertex_buffer(
            gpu.device(),
            &vec![ColoredVertex::default(); max_vertices.max(1)],
        );

        let mut layer = Self {
            pipeline,
            vertex_buffer,
            dirty: true,
            rows,
            cols,
            cell_width,
            line_height,
        };

        // Build initial vertex data and upload.
        let (surf_w, surf_h) = gpu.surface_size();
        let verts = layer.build_all_vertices(screen, surf_w as f32, surf_h as f32);
        gpu.queue()
            .write_buffer(&layer.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        layer.dirty = false;

        layer
    }

    /// Creates the render pipeline for untextured colored quads.
    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("background shader"),
            source: wgpu::ShaderSource::Wgsl(BACKGROUND_SHADER.into()),
        });

        // No bind group layout needed — no textures or uniforms.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("background pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("background pipeline"),
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

    /// Builds the 6 × cols vertices for one row, using `cell_width` and `line_height`
    /// for positioning. Cells with `bg == Color::Default` (and not inverse) produce
    /// degenerate quads skipped by the rasterizer. Inverse cells use `fg` for the
    /// background rect color.
    pub(crate) fn build_background_row_vertices(
        cell_width: f32,
        line_height: f32,
        row: usize,
        screen: &Screen,
        surf_w: f32,
        surf_h: f32,
    ) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity(screen.cols() * 6);
        for col in 0..screen.cols() {
            let cell = screen.cell(row, col);
            let inverse = cell.attrs.contains(CellAttrs::INVERSE);
            if cell.bg != Color::Default || (inverse && cell.fg != Color::Default) {
                let left = TEXT_PADDING + col as f32 * cell_width;
                let right = TEXT_PADDING + (col + 1) as f32 * cell_width;
                let top = TEXT_PADDING + row as f32 * line_height;
                let bottom = TEXT_PADDING + (row + 1) as f32 * line_height;

                let color = if inverse {
                    cell.fg.to_rgba()
                } else {
                    cell.bg.to_rgba()
                };

                verts.extend_from_slice(&ColoredVertex::from_pixel_rect(
                    left, top, right, bottom, color, surf_w, surf_h,
                ));
            } else {
                // Default background → degenerate quad.
                verts.extend(std::iter::repeat_n(ColoredVertex::default(), 6));
            }
        }
        verts
    }

    /// Builds vertices for every row in the full grid.
    fn build_all_vertices(&self, screen: &Screen, surf_w: f32, surf_h: f32) -> Vec<ColoredVertex> {
        let mut verts = Vec::with_capacity(screen.rows() * screen.cols() * 6);
        for row in 0..screen.rows() {
            verts.extend(Self::build_background_row_vertices(
                self.cell_width,
                self.line_height,
                row,
                screen,
                surf_w,
                surf_h,
            ));
        }
        verts
    }
}

impl Component for Background {
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
                "background layer resize"
            );
            let new_cap = screen.rows() * screen.cols() * 6;
            let old_cap = self.rows * self.cols * 6;
            if new_cap > old_cap {
                self.vertex_buffer = gpu::create_colored_vertex_buffer(
                    gpu.device(),
                    &vec![ColoredVertex::default(); new_cap.max(1)],
                );
            }
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.rows = screen.rows();
            self.cols = screen.cols();
            self.dirty = false;
            return;
        }

        // Dirty check: skip upload if nothing changed.
        let any_dirty_rows = !screen.dirty_rows().is_empty();
        if !self.dirty && !any_dirty_rows {
            return;
        }

        if self.dirty {
            tracing::trace!("rebuilding background draw batch (full)");
            let verts = self.build_all_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        } else {
            tracing::trace!("rebuilding background draw batch (incremental)");
            for row in screen.dirty_rows() {
                let row_verts = Self::build_background_row_vertices(
                    self.cell_width,
                    self.line_height,
                    row,
                    screen,
                    surf_w as f32,
                    surf_h as f32,
                );
                let offset = row * screen.cols() * 6 * std::mem::size_of::<ColoredVertex>();
                gpu.queue().write_buffer(
                    &self.vertex_buffer,
                    offset as u64,
                    bytemuck::cast_slice(&row_verts),
                );
            }
        }

        self.dirty = false;
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let vertex_count = (self.rows * self.cols * 6) as u32;
        if vertex_count > 0 {
            pass.draw(0..vertex_count, 0..1);
        }
    }

    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;

    #[test]
    fn inverse_background_rect_uses_fg_color() {
        let mut terminal = Terminal::new(2, 3);
        terminal.put_str("\x1b[7;31mX\x1b[0m  ");
        let screen = terminal.screen();
        let cell = screen.cell(0, 0);
        assert!(
            cell.attrs.contains(CellAttrs::INVERSE),
            "cell should have INVERSE attr"
        );
        assert_eq!(cell.fg, Color::Named(1), "fg should be red (ANSI 31)");

        let verts = Background::build_background_row_vertices(10.0, 20.0, 0, screen, 800.0, 600.0);

        let expected = Color::Named(1).to_rgba();
        assert_eq!(verts[0].color, expected, "inverse bg rect uses fg color");
    }

    #[test]
    fn sgr_strikethrough_stored() {
        let mut terminal = Terminal::new(2, 6);
        terminal.put_str("\x1b[9mstrike\x1b[0m");
        let screen = terminal.screen();
        assert!(
            screen.cell(0, 0).attrs.contains(CellAttrs::STRIKETHROUGH),
            "cell 0 should have STRIKETHROUGH attr"
        );
    }

    #[test]
    fn sgr_underline_stored() {
        let mut terminal = Terminal::new(2, 6);
        terminal.put_str("\x1b[4munder\x1b[0m");
        let screen = terminal.screen();
        assert!(
            screen.cell(0, 0).attrs.contains(CellAttrs::UNDERLINE),
            "cell 0 should have UNDERLINE attr"
        );
    }
}
