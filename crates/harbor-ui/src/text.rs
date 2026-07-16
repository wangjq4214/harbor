use crate::{BoxConstraints, Color, Rect, Widget};

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
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
        }
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

impl<A> Widget<A> for Text {
    type State = ();

    fn create_state(&self) -> Self::State {}

    fn layout(&self, _state: &mut Self::State, constraints: BoxConstraints) -> Rect {
        let (line_count, longest_line) = self.content.lines().fold((0usize, 0usize), |(count, longest), line| {
            (count + 1, longest.max(line.chars().count()))
        });
        let line_count = line_count.max(1) as f32;
        let longest_line = longest_line as f32;
        let (width, height) = constraints.constrain(
            longest_line * self.style.size * 0.6,
            line_count * self.style.line_height,
        );
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }
}
