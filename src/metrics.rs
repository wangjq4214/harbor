use crate::{config::TEXT_PADDING, font::FontBook, terminal::TerminalSize};

/// Fixed measurements used to map window pixels to terminal cells.
#[derive(Clone, Copy)]
pub(crate) struct TextMetrics {
    pub(crate) cell_width: f32,
    pub(crate) line_height: f32,
    pub(crate) ascent: f32,
    /// Distance from cell top to underline top edge (px).
    pub(crate) underline_position: f32,
    pub(crate) underline_thickness: f32,
    /// Distance from cell top to strikethrough center (px).
    pub(crate) strikethrough_position: f32,
    pub(crate) strikethrough_thickness: f32,
}

impl TextMetrics {
    pub(crate) fn new(fonts: &FontBook) -> Self {
        let (cell_width, line_height, ascent) = fonts.terminal_metrics();
        let (underline_position, strikethrough_position) = fonts
            .primary_horizontal_line_metrics(crate::config::FONT_SIZE)
            .map(|lm| {
                let d = lm.descent.abs();
                (line_height - d + 1.0, (line_height - d) * 0.45)
            })
            .unwrap_or((line_height * 0.8, line_height * 0.45));

        Self {
            cell_width,
            line_height,
            ascent,
            underline_position,
            underline_thickness: 1.5,
            strikethrough_position,
            strikethrough_thickness: 1.5,
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
