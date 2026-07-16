//! Declarative terminal widget and its private visual projection.

mod font;
mod metrics;
mod text;

use std::sync::Arc;

pub use font::{FontBook, load_system_fonts};
pub use metrics::TextMetrics;
pub use text::{AtlasGlyph, TextResources};

use crate::{BoxConstraints, CustomPainter, Key, Rect, Widget, WidgetEventResult};
use harbor_config::{
    SCROLLBAR_COLOR, SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH,
    SELECTION_COLOR, TEXT_PADDING,
};
use harbor_render::{
    Glyph, GlyphPatch, GlyphSlot, PaintContext, RectPatch, RectSlot, RenderEnvironment,
    RenderIdentity,
};
use harbor_types::{
    CellAttrs, CursorShape, DirtyRange, RenderSnapshot, RgbaColor, SelectionBounds, TerminalSize,
};

/// Shell-owned transient state required to project terminal visuals.
#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalVisualState {
    pub selection: Option<SelectionBounds>,
    pub cursor_visible: bool,
    pub scrollbar_visible: bool,
}

/// A terminal viewport configuration. The shell owns the terminal session and
/// interaction state; this widget only projects supplied snapshots to commands.
#[derive(Clone)]
pub struct Terminal {
    key: Key,
    snapshot: Option<Arc<RenderSnapshot>>,
    visual_state: TerminalVisualState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalScroll {
    Lines(isize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalIntent {
    Resize(TerminalSize),
    Scroll(TerminalScroll),
}

pub struct TerminalState {
    bounds: Rect,
}

impl Terminal {
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            snapshot: None,
            visual_state: TerminalVisualState {
                selection: None,
                cursor_visible: false,
                scrollbar_visible: false,
            },
        }
    }

    pub fn with_snapshot(key: Key, snapshot: Arc<RenderSnapshot>) -> Self {
        Self::with_snapshot_and_visual(key, snapshot, TerminalVisualState::default())
    }

    pub fn with_snapshot_and_visual(
        key: Key,
        snapshot: Arc<RenderSnapshot>,
        visual_state: TerminalVisualState,
    ) -> Self {
        Self {
            key,
            snapshot: Some(snapshot),
            visual_state,
        }
    }

    pub fn with_render_snapshot(&self, snapshot: Arc<RenderSnapshot>) -> Self {
        Self::with_snapshot_and_visual(self.key, snapshot, self.visual_state)
    }

    pub fn with_visual_state(&self, visual_state: TerminalVisualState) -> Self {
        Self {
            key: self.key,
            snapshot: self.snapshot.clone(),
            visual_state,
        }
    }

    pub fn resize_intent(&self, bounds: Rect, cell_width: f32, line_height: f32) -> TerminalIntent {
        let cols = (bounds.width / cell_width).floor().max(1.0) as usize;
        let rows = (bounds.height / line_height).floor().max(1.0) as usize;
        TerminalIntent::Resize(TerminalSize { rows, cols })
    }

    pub fn event_intent(&self, event: &winit::event::WindowEvent) -> Option<TerminalIntent> {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_alt)
        {
            return None;
        }
        let winit::event::WindowEvent::MouseWheel { delta, .. } = event else {
            return None;
        };
        let lines = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => (*y * 3.0) as isize,
            winit::event::MouseScrollDelta::PixelDelta(position) => (position.y / 20.0) as isize,
        };
        (lines != 0).then_some(TerminalIntent::Scroll(TerminalScroll::Lines(lines)))
    }
}

impl CustomPainter for Terminal {
    fn paint<'a>(&'a self, context: &mut PaintContext<'a>) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let metrics = context.environment().text_metrics();
        let identity = RenderIdentity::new(self.key.0);
        let slots = snapshot.rows * snapshot.cols;
        let dirty_ranges = &snapshot.dirty_ranges;

        context.with_identity(identity, |context| {
            let background_identity = RenderIdentity::new(self.key.0 ^ 0xBACC_6001);
            context.with_identity(background_identity, |context| {
                context.draw_rect_patch(RectPatch {
                    identity: background_identity,
                    slots: slots * 3,
                    updates: background_updates(
                        snapshot,
                        dirty_ranges,
                        metrics.cell_width,
                        metrics.line_height,
                    ),
                });
            });
            let glyph_identity = RenderIdentity::new(self.key.0 ^ 0x6179_7068);
            context.with_identity(glyph_identity, |context| {
                context.draw_glyph_patch(GlyphPatch {
                    identity: glyph_identity,
                    slots,
                    updates: glyph_updates(
                        snapshot,
                        dirty_ranges,
                        metrics.cell_width,
                        metrics.line_height,
                    ),
                });
            });
            let decoration_identity = RenderIdentity::new(self.key.0 ^ 0xDEC0_A710);
            context.with_identity(decoration_identity, |context| {
                context.draw_rect_patch(RectPatch {
                    identity: decoration_identity,
                    slots: slots * 3,
                    updates: decoration_updates(
                        snapshot,
                        dirty_ranges,
                        metrics.cell_width,
                        metrics.line_height,
                    ),
                });
            });
            for rect in selection_rects(
                snapshot,
                self.visual_state.selection,
                metrics.cell_width,
                metrics.line_height,
            ) {
                context.fill_rect(rect, RgbaColor(SELECTION_COLOR));
            }
            if self.visual_state.cursor_visible
                && snapshot.cursor_visible
                && let Some(rect) = cursor_rect(snapshot, metrics.cell_width, metrics.line_height)
            {
                context.fill_rect(rect, RgbaColor::WHITE);
            }
            if self.visual_state.scrollbar_visible
                && let Some(rect) = scrollbar_rect(snapshot, context.bounds())
            {
                context.fill_rect(rect, RgbaColor(SCROLLBAR_COLOR));
            }
        });
    }
}

impl Widget<TerminalIntent> for Terminal {
    type State = TerminalState;

    fn create_state(&self) -> Self::State {
        TerminalState {
            bounds: Rect::default(),
        }
    }

    fn key(&self) -> Option<Key> {
        Some(self.key)
    }

    fn layout(
        &self,
        state: &mut Self::State,
        _environment: RenderEnvironment,
        constraints: BoxConstraints,
    ) -> Rect {
        state.bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: constraints.max_width,
            height: constraints.max_height,
        };
        state.bounds
    }

    fn event(
        &self,
        _state: &mut Self::State,
        event: &winit::event::WindowEvent,
        _bounds: Rect,
    ) -> WidgetEventResult<TerminalIntent> {
        self.event_intent(event)
            .map_or(WidgetEventResult::Ignored, WidgetEventResult::Intent)
    }

    fn paint<'a>(&'a self, _state: &'a mut Self::State, context: &mut PaintContext<'a>) {
        CustomPainter::paint(self, context);
    }
}

fn background_updates(
    snapshot: &RenderSnapshot,
    dirty_ranges: &[DirtyRange],
    cell_width: f32,
    line_height: f32,
) -> Vec<RectSlot> {
    dirty_cells(snapshot, dirty_ranges)
        .map(|(row, col)| {
            let cell = snapshot.cell(row, col);
            let inverse = cell.attrs.contains(CellAttrs::INVERSE);
            let color = if inverse { cell.fg } else { cell.bg };
            let rect = (cell.bg != harbor_types::Color::Default
                || (inverse && cell.fg != harbor_types::Color::Default))
                .then_some(cell_rect(row, col, cell_width, line_height));
            RectSlot {
                slot: (row * snapshot.cols + col) * 3,
                rect,
                color: RgbaColor(color.to_rgba()),
            }
        })
        .collect()
}

fn glyph_updates(
    snapshot: &RenderSnapshot,
    dirty_ranges: &[DirtyRange],
    cell_width: f32,
    line_height: f32,
) -> Vec<GlyphSlot> {
    dirty_cells(snapshot, dirty_ranges)
        .map(|(row, col)| {
            let cell = snapshot.cell(row, col);
            let glyph = (cell.ch != ' ').then(|| {
                let bounds = cell_rect(row, col, cell_width, line_height);
                Glyph {
                    character: cell.ch,
                    bounds,
                    color: RgbaColor(text::glyph_color(cell.fg, cell.bg, cell.attrs)),
                }
            });
            GlyphSlot {
                slot: row * snapshot.cols + col,
                glyph,
            }
        })
        .collect()
}

fn decoration_updates(
    snapshot: &RenderSnapshot,
    dirty_ranges: &[DirtyRange],
    cell_width: f32,
    line_height: f32,
) -> Vec<RectSlot> {
    dirty_cells(snapshot, dirty_ranges)
        .flat_map(|(row, col)| {
            let cell = snapshot.cell(row, col);
            let base_slot = (row * snapshot.cols + col) * 3;
            let cell_rect = cell_rect(row, col, cell_width, line_height);
            let color = RgbaColor(cell.fg.to_rgba());
            let underline = (cell.attrs.contains(CellAttrs::UNDERLINE) && cell.ch != ' ')
                .then_some(Rect {
                    x: cell_rect.x,
                    y: cell_rect.y + line_height * 0.8,
                    width: cell_rect.width,
                    height: 1.5,
                });
            let strikethrough = (cell.attrs.contains(CellAttrs::STRIKETHROUGH) && cell.ch != ' ')
                .then_some(Rect {
                    x: cell_rect.x,
                    y: cell_rect.y + line_height * 0.45,
                    width: cell_rect.width,
                    height: 1.5,
                });
            [
                RectSlot {
                    slot: base_slot + 1,
                    rect: underline,
                    color,
                },
                RectSlot {
                    slot: base_slot + 2,
                    rect: strikethrough,
                    color,
                },
            ]
        })
        .collect()
}

fn dirty_cells<'a>(
    snapshot: &'a RenderSnapshot,
    dirty_ranges: &'a [DirtyRange],
) -> impl Iterator<Item = (usize, usize)> + 'a {
    dirty_ranges.iter().flat_map(|range| {
        (range.row < snapshot.rows)
            .then_some(range.start_col.min(snapshot.cols)..range.end_col.min(snapshot.cols))
            .into_iter()
            .flatten()
            .map(move |col| (range.row, col))
    })
}

fn cell_rect(row: usize, col: usize, cell_width: f32, line_height: f32) -> Rect {
    Rect {
        x: TEXT_PADDING + col as f32 * cell_width,
        y: TEXT_PADDING + row as f32 * line_height,
        width: cell_width,
        height: line_height,
    }
}

fn selection_rects(
    snapshot: &RenderSnapshot,
    selection: Option<SelectionBounds>,
    cell_width: f32,
    line_height: f32,
) -> Vec<Rect> {
    let Some(selection) = selection else {
        return Vec::new();
    };
    let view_start =
        snapshot.history_start + snapshot.scroll_count.saturating_sub(snapshot.view_offset) as u64;
    let view_end = view_start + snapshot.rows.saturating_sub(1) as u64;
    let start = selection.start_row.max(view_start);
    let end = selection.end_row.min(view_end);
    (start <= end)
        .then(|| {
            (start..=end)
                .flat_map(|generation| {
                    let row = (generation - view_start) as usize;
                    let first = if generation == selection.start_row {
                        selection.start_col
                    } else {
                        0
                    };
                    let last = if generation == selection.end_row {
                        selection.end_col
                    } else {
                        snapshot.cols.saturating_sub(1)
                    };
                    (first.min(snapshot.cols)..=last.min(snapshot.cols.saturating_sub(1)))
                        .map(move |col| cell_rect(row, col, cell_width, line_height))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cursor_rect(snapshot: &RenderSnapshot, cell_width: f32, line_height: f32) -> Option<Rect> {
    (snapshot.cursor_y < snapshot.rows && snapshot.cursor_x < snapshot.cols).then(|| {
        let mut rect = cell_rect(
            snapshot.cursor_y,
            snapshot.cursor_x,
            cell_width,
            line_height,
        );
        match snapshot.cursor_shape {
            CursorShape::Block => {}
            CursorShape::Underline => {
                rect.y += line_height - (line_height * 0.1).max(2.0);
                rect.height = (line_height * 0.1).max(2.0);
            }
            CursorShape::Bar => rect.width = (cell_width * 0.15).max(2.0),
        }
        rect
    })
}

fn scrollbar_rect(snapshot: &RenderSnapshot, bounds: Rect) -> Option<Rect> {
    if snapshot.is_alt || snapshot.scroll_count == 0 {
        return None;
    }
    let visible_height = bounds.height - TEXT_PADDING * 2.0;
    let total = snapshot.scroll_count + snapshot.rows;
    let thumb_height =
        (snapshot.rows as f32 / total as f32 * visible_height).max(SCROLLBAR_MIN_THUMB_HEIGHT);
    let track_height = visible_height - thumb_height;
    let offset = snapshot.view_offset as f32 / snapshot.scroll_count as f32;
    Some(Rect {
        x: bounds.x + bounds.width - SCROLLBAR_MARGIN - SCROLLBAR_WIDTH,
        y: bounds.y + TEXT_PADDING + (1.0 - offset.clamp(0.0, 1.0)) * track_height,
        width: SCROLLBAR_WIDTH,
        height: thumb_height,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{Terminal, TerminalVisualState};
    use crate::{BoxConstraints, Key, Widget, WidgetRuntime};
    use harbor_render::{PaintCommand, PaintContext, RenderEnvironment};
    use harbor_types::{Cell, CursorShape, DirtyRange, RenderSnapshot};

    fn snapshot() -> RenderSnapshot {
        let mut cell = Cell::default();
        cell.ch = 'A';
        RenderSnapshot {
            rows: 1,
            cols: 1,
            cells: vec![cell],
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            cursor_blink: false,
            cursor_shape: CursorShape::Block,
            scroll_count: 0,
            view_offset: 0,
            history_start: 0,
            is_alt: false,
            dirty_ranges: vec![DirtyRange {
                row: 0,
                start_col: 0,
                end_col: 1,
            }],
        }
    }

    #[test]
    fn terminal_projects_dirty_cells_and_visual_state_as_ordered_commands() {
        let terminal = Terminal::with_snapshot_and_visual(
            Key(7),
            Arc::new(snapshot()),
            TerminalVisualState {
                cursor_visible: true,
                ..TerminalVisualState::default()
            },
        );
        let environment = RenderEnvironment::new(80.0, 40.0, 1.0);
        let mut runtime = WidgetRuntime::new(&terminal);
        let bounds = runtime.layout(&terminal, environment, BoxConstraints::tight(80.0, 40.0));
        let mut context = PaintContext::new(environment, bounds);

        runtime.paint(&terminal, &mut context);

        let commands = context.finish();
        assert!(matches!(
            commands.as_slice(),
            [
                PaintCommand::RectPatch { .. },
                PaintCommand::GlyphPatch { .. },
                PaintCommand::RectPatch { .. },
                PaintCommand::FillRect { .. }
            ]
        ));
        let (
            PaintCommand::RectPatch {
                patch: background, ..
            },
            PaintCommand::RectPatch {
                patch: decoration, ..
            },
        ) = (&commands[0], &commands[2])
        else {
            unreachable!("command order asserted above");
        };
        assert_ne!(background.identity, decoration.identity);
    }
}
