#![allow(unused_imports)]

mod damage;
mod normal_buf;
mod parser;
pub mod render;
mod screen;
pub mod selection_model;
#[cfg(test)]
mod terminal_tests;

// Re-exports for the main crate.
pub use damage::DirtyRange;
pub use normal_buf::NormalBuf;
pub use parser::TerminalParser;
pub use render::{
    Background, Cursor, Decoration, GpuContext, Scrollbar, Selection, SurfaceDisposition,
    SurfaceStatus, Text, UploadMode, UploadPlan, UploadPolicy, surface_disposition,
};
pub use screen::{AltScreenAction, Cell, CellAttrs, Color, CursorShape, Screen, SelectionBounds};
pub use selection_model::{
    AutoScroll, SelectionGranularity, SelectionModel, SelectionOutcome, SelectionRange,
};

pub use harbor_text::{AtlasGlyph, FontBook, TextMetrics, load_system_fonts};
pub use harbor_types::should_confirm_multiline;
pub use harbor_types::{
    CopySelectionResult, InputKey, InputModes, InputModifiers, InputRequest, PasteDisposition,
    RevisionedUpdateReceiver, TerminalCommand, TerminalSize, TerminalSnapshot, TerminalUpdate,
    UpdateDamage, WorkerStatus, safe_preview_line,
};
pub use harbor_widget::scene::primitive::ExternalDrawId;

/// Stateful terminal engine owning screen state, parser, and wgpu rendering.
pub struct Terminal {
    /// Incremental ANSI/VT parser.
    parser: TerminalParser,
    /// Screen (primary buffer; alt screen handled internally via in_alt).
    normal: Screen,
    /// When true, `process_output` skips the scroll-to-bottom snap.
    suppress_scroll_snap: bool,
    /// Identifier for CustomPaint widget delegate drawing.
    draw_id: ExternalDrawId,
    // Render components
    background: Option<Background>,
    text: Option<Text>,
    decoration: Option<Decoration>,
    selection: Option<Selection>,
    cursor: Option<Cursor>,
    scrollbar: Option<Scrollbar>,
}

impl Terminal {
    /// Creates a Terminal with GPU rendering enabled.
    pub fn new(
        size: TerminalSize,
        gpu: &GpuContext,
        font_book: FontBook,
        metrics: TextMetrics,
    ) -> Self {
        let mut term = Self::new_headless(size.rows, size.cols);
        let snap = term.normal.terminal_snapshot();

        let background = Background::new(gpu, &snap, metrics.cell_width, metrics.line_height);
        let text = Text::new(gpu, font_book, metrics, &snap).expect("text renderer init");
        let decoration = Decoration::new(gpu, &snap, metrics);
        let selection = Selection::new(gpu, metrics.cell_width, metrics.line_height);
        let cursor = Cursor::new(gpu, metrics);
        let scrollbar = Scrollbar::new(gpu, &snap);

        term.background = Some(background);
        term.text = Some(text);
        term.decoration = Some(decoration);
        term.selection = Some(selection);
        term.cursor = Some(cursor);
        term.scrollbar = Some(scrollbar);

        term
    }

    /// Creates a headless Terminal without GPU rendering resources (for testing / parser use).
    pub fn new_headless(rows: usize, cols: usize) -> Self {
        Self {
            parser: TerminalParser::default(),
            normal: Screen::new(rows, cols),
            suppress_scroll_snap: false,
            draw_id: 1,
            background: None,
            text: None,
            decoration: None,
            selection: None,
            cursor: None,
            scrollbar: None,
        }
    }

    /// Fixed identifier linking this Terminal to its CustomPaint widget.
    pub fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }

    /// Prepares GPU resources for all render components.
    pub fn prepare(&mut self, gpu: &GpuContext, damage: Option<&UpdateDamage>) {
        let snap = self.normal.terminal_snapshot();
        if let (
            Some(background),
            Some(text),
            Some(decoration),
            Some(selection),
            Some(cursor),
            Some(scrollbar),
        ) = (
            &mut self.background,
            &mut self.text,
            &mut self.decoration,
            &mut self.selection,
            &mut self.cursor,
            &mut self.scrollbar,
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
                background.prepare_with_dirty(gpu, &snap, dirty_ranges);
                text.prepare_with_dirty(gpu, &snap, dirty_ranges);
                decoration.prepare_with_dirty(gpu, &snap, dirty_ranges);
            } else {
                background.prepare(gpu, Some(&snap));
                text.prepare(gpu, Some(&snap));
                decoration.prepare(gpu, Some(&snap));
            }
            selection.prepare(gpu, Some(&snap));
            cursor.prepare(gpu, Some(&snap));
            scrollbar.prepare(gpu, Some(&snap));
        }
    }

    /// Coordinates prepare + draw for all components during widget external draw pass.
    pub fn render(
        &mut self,
        draw_id: ExternalDrawId,
        _rect: harbor_widget::layout::Rect,
        pass: &mut wgpu::RenderPass,
        gpu: &GpuContext,
    ) {
        if draw_id != self.draw_id {
            return;
        }
        self.prepare(gpu, None);
        if let (
            Some(background),
            Some(text),
            Some(decoration),
            Some(selection),
            Some(cursor),
            Some(scrollbar),
        ) = (
            &self.background,
            &self.text,
            &self.decoration,
            &self.selection,
            &self.cursor,
            &self.scrollbar,
        ) {
            background.draw(pass);
            text.draw(pass);
            decoration.draw(pass);
            selection.draw(pass);
            cursor.draw(pass);
            scrollbar.draw(pass);
        }
    }

    /// Resizes the terminal grid and updates render component dimensions.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.normal.resize(rows, cols);
        self.suppress_scroll_snap = false;
    }

    pub fn resize_gpu(&mut self, size: TerminalSize, gpu: &GpuContext) {
        self.resize(size.rows, size.cols);
        if let Some(bg) = &mut self.background {
            bg.resize(gpu, (0, 0));
        }
        if let Some(text) = &mut self.text {
            text.resize(gpu, (0, 0));
        }
        if let Some(dec) = &mut self.decoration {
            dec.resize(gpu, (0, 0));
        }
        if let Some(sel) = &mut self.selection {
            sel.resize(gpu, (0, 0));
        }
        if let Some(cur) = &mut self.cursor {
            cur.resize(gpu, (0, 0));
        }
        if let Some(sb) = &mut self.scrollbar {
            sb.resize(gpu, (0, 0));
        }
    }

    pub fn terminal_size(&self, gpu: &GpuContext) -> TerminalSize {
        if let Some(text) = &self.text {
            text.terminal_size(gpu)
        } else {
            TerminalSize {
                rows: self.normal.rows(),
                cols: self.normal.cols(),
            }
        }
    }

    pub fn text_metrics(&self) -> Option<&TextMetrics> {
        self.text.as_ref().map(|t| t.metrics())
    }

    pub fn text_glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        self.text.as_ref().and_then(|t| t.glyph(ch))
    }

    pub fn text_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.text.as_ref().map(|t| t.text_bind_group())
    }

    pub fn text_bind_group_layout(&self) -> Option<&wgpu::BindGroupLayout> {
        self.text.as_ref().map(|t| t.text_bind_group_layout())
    }

    pub fn ensure_glyphs(&mut self, text: &str, gpu: &GpuContext) {
        if let Some(t) = &mut self.text {
            t.ensure_glyphs(text, gpu);
        }
    }

    pub fn put_str(&mut self, text: &str) {
        self.put_bytes(text.as_bytes());
    }

    /// Feeds raw PTY bytes through the streaming parser.
    pub fn put_bytes(&mut self, bytes: &[u8]) {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let result = {
                let parser = &mut self.parser;
                parser.put_bytes(&mut self.normal, remaining)
            };
            remaining = &remaining[result.consumed..];
            if let Some(action) = result.alt_request {
                self.normal.take_alt_request();
                self.suppress_scroll_snap = false;
                match action {
                    AltScreenAction::Enter => self.normal.enter_alt(),
                    AltScreenAction::Exit => self.normal.exit_alt(),
                }
            }
        }
    }

    /// Feeds raw PTY bytes into the terminal parser.
    pub fn process_output(&mut self, output: &[u8]) {
        if output.is_empty() {
            tracing::trace!("ignored empty pty output chunk");
            return;
        }
        if !self.normal.is_alt() && !self.suppress_scroll_snap {
            self.normal.scroll_to_bottom();
        }
        self.put_bytes(output);
    }

    /// Returns the renderable screen snapshot owned by this terminal.
    pub fn screen(&self) -> &Screen {
        &self.normal
    }

    /// Returns the GPU-independent terminal state for the UI/update contract.
    pub fn snapshot(&self) -> TerminalSnapshot {
        self.normal.terminal_snapshot()
    }

    /// Mutable screen access for tests.
    pub fn screen_mut(&mut self) -> &mut Screen {
        &mut self.normal
    }

    /// Resets the screen's dirty-row tracking.
    pub fn clear_screen_dirty(&mut self) {
        self.normal.clear_dirty();
    }

    pub fn row_text(&self, row: usize) -> String {
        self.screen().row_text(row)
    }

    /// Resizes the terminal grid without GPU resources. Returns true if size changed.
    pub fn resize_if_changed(&mut self, new_size: TerminalSize) -> bool {
        let current = TerminalSize {
            rows: self.screen().rows(),
            cols: self.screen().cols(),
        };

        if new_size != current {
            self.resize(new_size.rows, new_size.cols);
            true
        } else {
            false
        }
    }

    pub fn scroll_viewport_up(&mut self, n: usize) {
        self.normal.scroll_up(n);
    }

    pub fn scroll_viewport_down(&mut self, n: usize) {
        self.normal.scroll_down(n);
    }

    pub fn scroll_viewport_to_top(&mut self) {
        let scroll_count = self.normal.scroll_count();
        self.normal.scroll_up(scroll_count);
    }

    pub fn scroll_viewport_to_bottom(&mut self) {
        self.normal.scroll_to_bottom();
    }

    pub fn is_alt_screen(&self) -> bool {
        self.normal.is_alt()
    }

    pub fn set_suppress_scroll_snap(&mut self, suppress: bool) {
        self.suppress_scroll_snap = suppress;
    }
}
