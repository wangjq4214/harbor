use crate::{BoxConstraints, Color, LegacyPaintContext, PaintContext, Rect, Widget};
use harbor_config::FONT_SIZE;
use harbor_gpu::gpu::{self, TexturedVertex};
use harbor_render::RenderEnvironment;
use harbor_types::RgbaColor;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub size: f32,
    pub line_height: f32,
    pub bold: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color([1.0, 1.0, 1.0, 1.0]),
            size: 14.0,
            line_height: 20.0,
            bold: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub content: String,
    pub style: TextStyle,
    pub wrap: bool,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
            wrap: false,
        }
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    fn cell_width(&self, environment: RenderEnvironment) -> f32 {
        (environment.text_metrics().cell_width * self.style.size / FONT_SIZE).max(1.0)
    }

    fn columns(&self, environment: RenderEnvironment, width: f32) -> usize {
        if self.wrap {
            (width / self.cell_width(environment)).floor().max(1.0) as usize
        } else {
            usize::MAX
        }
    }

    fn sync_lines(&self, state: &mut TextState, columns: usize) {
        if state.content == self.content && state.columns == columns && state.wrap == self.wrap {
            return;
        }
        state.content.clone_from(&self.content);
        state.columns = columns;
        state.wrap = self.wrap;
        state.lines.clear();
        for line in self.content.lines() {
            if !self.wrap {
                state.lines.push(line.to_owned());
                continue;
            }
            let mut chunk = String::with_capacity(columns);
            for ch in line.chars() {
                chunk.push(ch);
                if chunk.chars().count() == columns {
                    state.lines.push(std::mem::take(&mut chunk));
                    chunk = String::with_capacity(columns);
                }
            }
            if !chunk.is_empty() || line.is_empty() {
                state.lines.push(chunk);
            }
        }
        if state.lines.is_empty() {
            state.lines.push(String::new());
        }
    }
}

pub struct TextState {
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: usize,
    vertex_count: u32,
    content: String,
    columns: usize,
    wrap: bool,
    lines: Vec<String>,
}

impl<A> Widget<A> for Text {
    type State = TextState;

    fn create_state(&self) -> Self::State {
        TextState {
            vertex_buffer: None,
            vertex_capacity: 0,
            vertex_count: 0,
            content: String::new(),
            columns: 0,
            wrap: false,
            lines: Vec::new(),
        }
    }

    fn layout(
        &self,
        state: &mut Self::State,
        environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        let estimated_cell_width = self.cell_width(environment);
        let columns = self.columns(environment, constraints.max_width);
        self.sync_lines(state, columns);
        let longest_line = state
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let (width, height) = constraints.constrain(
            longest_line * estimated_cell_width,
            state.lines.len().max(1) as f32 * self.style.line_height,
        );
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    fn paint<'a>(&'a self, state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        let bounds = context.bounds();
        let columns = self.columns(context.environment(), bounds.width);
        self.sync_lines(state, columns);
        for (row, line) in state.lines.iter().enumerate() {
            context.draw_text(
                (bounds.x, bounds.y + row as f32 * self.style.line_height),
                line,
                RgbaColor(self.style.color.0),
                self.style.size,
                self.style.line_height,
                self.style.bold,
            );
        }
    }

    fn legacy_paint<'pass>(
        &self,
        state: &mut Self::State,
        context: LegacyPaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        context
            .text
            .ensure_glyphs(&self.content, context.gpu.device(), context.gpu.queue());
        let metrics = *context.text.metrics();
        let scale = self.style.size / FONT_SIZE;
        let cell_width = metrics.cell_width * scale;
        let ascent = metrics.ascent * scale;
        let columns = if self.wrap {
            (context.bounds.width / cell_width).floor().max(1.0) as usize
        } else {
            usize::MAX
        };
        self.sync_lines(state, columns);
        let mut vertices = Vec::with_capacity(self.content.chars().count() * 6);

        for (row, line) in state.lines.iter().enumerate() {
            let baseline = context.bounds.y + row as f32 * self.style.line_height + ascent.ceil();
            for (column, ch) in line.chars().enumerate() {
                let Some(glyph) = context.text.glyph(ch).copied() else {
                    continue;
                };
                if glyph.width == 0 || glyph.height == 0 {
                    continue;
                }
                let left =
                    context.bounds.x + column as f32 * cell_width + glyph.xmin as f32 * scale;
                let bottom = baseline - glyph.ymin as f32 * scale;
                let top = bottom - glyph.height as f32 * scale;
                let right = left + glyph.width as f32 * scale;
                vertices.extend_from_slice(&TexturedVertex::from_pixel_rect(
                    left,
                    top,
                    right,
                    bottom,
                    glyph.uv.left,
                    glyph.uv.top,
                    glyph.uv.right,
                    glyph.uv.bottom,
                    self.style.color.0,
                    context.gpu.surface_size().0 as f32,
                    context.gpu.surface_size().1 as f32,
                ));
            }
        }

        state.vertex_count = vertices.len() as u32;
        if state.vertex_count == 0 {
            return;
        }
        if vertices.len() > state.vertex_capacity {
            state.vertex_buffer = Some(gpu::create_vertex_buffer(
                context.gpu.device(),
                &vec![TexturedVertex::default(); vertices.len()],
            ));
            state.vertex_capacity = vertices.len();
        }
        let Some(vertex_buffer) = state.vertex_buffer.as_ref() else {
            return;
        };
        context
            .gpu
            .queue()
            .write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        pass.set_pipeline(context.text.text_pipeline());
        pass.set_bind_group(0, context.text.text_bind_group(), &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..state.vertex_count, 0..1);
    }
}
