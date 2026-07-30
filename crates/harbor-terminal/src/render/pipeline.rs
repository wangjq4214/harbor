use crate::damage::DirtyRange;
use crate::render::{Background, Cursor, Decoration, GpuContext, Scrollbar, Selection, Text};
use harbor_text::{AtlasGlyph, FontBook, TextMetrics};
use harbor_types::{TerminalSize, TerminalSnapshot, UpdateDamage};

/// Encapsulates the GPU rendering pipeline components for the terminal.
pub struct TerminalRenderPipeline {
    pub background: Background,
    pub text: Text,
    pub decoration: Decoration,
    pub selection: Selection,
    pub cursor: Cursor,
    pub scrollbar: Scrollbar,
}

impl TerminalRenderPipeline {
    pub fn new(
        gpu: &GpuContext,
        font_book: FontBook,
        metrics: TextMetrics,
        snap: &TerminalSnapshot,
    ) -> anyhow::Result<Self> {
        let background = Background::new(gpu, snap, metrics.cell_width, metrics.line_height);
        let text = Text::new(gpu, font_book, metrics, snap)?;
        let decoration = Decoration::new(gpu, snap, metrics);
        let selection = Selection::new(gpu, metrics.cell_width, metrics.line_height);
        let cursor = Cursor::new(gpu, metrics);
        let scrollbar = Scrollbar::new(gpu, snap);

        Ok(Self {
            background,
            text,
            decoration,
            selection,
            cursor,
            scrollbar,
        })
    }

    pub fn prepare(
        &mut self,
        gpu: &GpuContext,
        snap: &TerminalSnapshot,
        damage: Option<&UpdateDamage>,
    ) {
        if let Some(damage) = damage {
            let full_ranges;
            let dirty_ranges = match damage {
                UpdateDamage::Ranges(ranges) => ranges,
                UpdateDamage::FullUpload => {
                    full_ranges = (0..snap.rows)
                        .map(|row| DirtyRange {
                            row,
                            start_col: 0,
                            end_col: snap.cols,
                        })
                        .collect::<Vec<_>>();
                    &full_ranges
                }
            };
            self.background.prepare_with_dirty(gpu, snap, dirty_ranges);
            self.text.prepare_with_dirty(gpu, snap, dirty_ranges);
            self.decoration.prepare_with_dirty(gpu, snap, dirty_ranges);
        } else {
            self.background.prepare(gpu, Some(snap));
            self.text.prepare(gpu, Some(snap));
            self.decoration.prepare(gpu, Some(snap));
        }
        self.selection.prepare(gpu, Some(snap));
        self.cursor.prepare(gpu, Some(snap));
        self.scrollbar.prepare(gpu, Some(snap));
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass) {
        self.background.draw(pass);
        self.text.draw(pass);
        self.decoration.draw(pass);
        self.selection.draw(pass);
        self.cursor.draw(pass);
        self.scrollbar.draw(pass);
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.background.resize(gpu, (0, 0));
        self.text.resize(gpu, (0, 0));
        self.decoration.resize(gpu, (0, 0));
        self.selection.resize(gpu, (0, 0));
        self.cursor.resize(gpu, (0, 0));
        self.scrollbar.resize(gpu, (0, 0));
    }

    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        self.text.terminal_size(gpu)
    }

    pub fn metrics(&self) -> &TextMetrics {
        self.text.metrics()
    }

    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.text.glyph(ch)
    }

    pub fn text_bind_group(&self) -> &wgpu::BindGroup {
        self.text.text_bind_group()
    }

    pub fn text_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.text.text_bind_group_layout()
    }

    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        self.text.ensure_glyphs(text, gpu);
    }
}
