use crate::{config::TEXT_PADDING, font::FontBook, terminal::TerminalSize};

/// Fixed measurements used to map window pixels to terminal cells.
#[derive(Clone, Copy)]
pub(crate) struct TextMetrics {
    pub(crate) cell_width: f32,
    pub(crate) line_height: f32,
    pub(crate) ascent: f32,
}

impl TextMetrics {
    pub(crate) fn new(fonts: &FontBook) -> Self {
        let (cell_width, line_height, ascent) = fonts.terminal_metrics();
        Self {
            cell_width,
            line_height,
            ascent,
        }
    }

    pub(crate) fn terminal_size(self, width: u32, height: u32) -> TerminalSize {
        let text_width = (width as f32 - TEXT_PADDING * 2.0).max(self.cell_width);
        let text_height = (height as f32 - TEXT_PADDING * 2.0).max(self.line_height);

        TerminalSize {
            rows: (text_height / self.line_height).floor().max(1.0) as usize,
            cols: (text_width / self.cell_width).floor().max(1.0) as usize,
        }
    }
}
