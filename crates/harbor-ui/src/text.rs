use crate::{BoxConstraints, Color, PaintContext, Rect, Widget};
use harbor_config::FONT_SIZE;
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
    content: String,
    columns: usize,
    wrap: bool,
    lines: Vec<String>,
}

impl<A> Widget<A> for Text {
    type State = TextState;

    fn create_state(&self) -> Self::State {
        TextState {
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
}
