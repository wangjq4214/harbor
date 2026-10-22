use super::gpu::{ColoredVertex, TerminalGpuAccess, VertexBuffer};
use crate::{RenderViewport, read_model::CellSpan};
use std::sync::Arc;

/// Prepared current-geometry overlay, independent of terminal cells/SGR/selection.
pub(super) struct Highlights {
    pipeline: Arc<wgpu::RenderPipeline>,
    quads: VertexBuffer<ColoredVertex>,
    spans: Vec<CellSpan>,
    viewport: Option<RenderViewport>,
}

impl Highlights {
    pub fn new(gpu: TerminalGpuAccess<'_>, pipeline: Arc<wgpu::RenderPipeline>) -> Self {
        Self {
            pipeline,
            quads: VertexBuffer::new(gpu.device()),
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
        self.quads.reserve(gpu.device(), vertices.len().max(64));
        self.quads.write(gpu, &vertices);
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.quads.draw(pass, &self.pipeline, None);
    }
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
