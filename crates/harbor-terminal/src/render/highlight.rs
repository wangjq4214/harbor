use super::gpu::{self, ColoredVertex, TerminalGpuAccess};
use crate::{RenderViewport, read_model::CellSpan};
use std::sync::Arc;

/// Prepared current-geometry overlay, independent of terminal cells/SGR/selection.
pub(super) struct Highlights {
    pipeline: Arc<wgpu::RenderPipeline>,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
    spans: Vec<CellSpan>,
    viewport: Option<RenderViewport>,
}

impl Highlights {
    pub fn new(gpu: TerminalGpuAccess<'_>, pipeline: Arc<wgpu::RenderPipeline>) -> Self {
        Self {
            pipeline,
            buffer: gpu::create_colored_vertex_buffer(gpu.device(), &[]),
            capacity: 0,
            count: 0,
            spans: Vec::new(),
            viewport: None,
        }
    }

    pub fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        spans: &[CellSpan],
        viewport: RenderViewport,
    ) {
        if self.spans == spans && self.viewport == Some(viewport) {
            return;
        }
        self.spans = spans.to_vec();
        self.viewport = Some(viewport);
        let (sw, sh) = viewport.surface_dimensions();
        let mut vertices = Vec::with_capacity(spans.len() * 6);
        for span in spans {
            let (left, top, _, bottom) = viewport.cell_bounds(span.row, span.start_col);
            let (_, _, right, _) = viewport.cell_bounds(span.row, span.end_col - 1);
            vertices.extend_from_slice(&ColoredVertex::from_pixel_rect(
                left,
                top,
                right,
                bottom,
                [1.0, 0.65, 0.05, 0.3],
                sw,
                sh,
            ));
        }
        if vertices.len() > self.capacity {
            self.capacity = vertices.len().max(64);
            self.buffer = gpu::create_colored_vertex_buffer(
                gpu.device(),
                &vec![ColoredVertex::default(); self.capacity],
            );
        }
        if !vertices.is_empty() {
            gpu.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&vertices));
        }
        self.count = vertices.len() as u32;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..self.count, 0..1);
        }
    }
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
