//! Declarative terminal widget and its private visual layers.

mod background;
mod decoration;
mod font;
mod metrics;
mod text;

use std::sync::Arc;

pub use font::{FontBook, load_system_fonts};
pub use metrics::TextMetrics;
pub use text::{AtlasGlyph, TextResources};

use self::{background::Background, decoration::Decoration};
use crate::{BoxConstraints, Key, LegacyPaintContext, Rect, Widget, WidgetEventResult};
use harbor_render::RenderEnvironment;
use harbor_types::{RenderSnapshot, TerminalSize};

/// A terminal viewport configuration. The shell owns the terminal session and
/// supplies a render snapshot; this widget retains only GPU and interaction state.
#[derive(Clone)]
pub struct Terminal {
    key: Key,
    snapshot: Option<Arc<RenderSnapshot>>,
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
    background: Option<Background>,
    decoration: Option<Decoration>,
}

impl Terminal {
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            snapshot: None,
        }
    }

    pub fn with_snapshot(key: Key, snapshot: Arc<RenderSnapshot>) -> Self {
        Self {
            key,
            snapshot: Some(snapshot),
        }
    }

    pub fn with_render_snapshot(&self, snapshot: Arc<RenderSnapshot>) -> Self {
        Self::with_snapshot(self.key, snapshot)
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

impl Widget<TerminalIntent> for Terminal {
    type State = TerminalState;

    fn create_state(&self) -> Self::State {
        TerminalState {
            bounds: Rect::default(),
            background: None,
            decoration: None,
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

    fn legacy_paint<'pass>(
        &self,
        state: &mut Self::State,
        context: LegacyPaintContext<'_>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let metrics = *context.text.metrics();
        let background = state.background.get_or_insert_with(|| {
            Background::new(
                context.gpu,
                snapshot,
                metrics.cell_width,
                metrics.line_height,
            )
        });
        background.prepare_with_dirty(context.gpu, snapshot, &snapshot.dirty_ranges);
        background.draw(pass);

        context
            .text
            .prepare_with_dirty(context.gpu, snapshot, &snapshot.dirty_ranges);
        context.text.draw(pass);

        let decoration = state
            .decoration
            .get_or_insert_with(|| Decoration::new(context.gpu, snapshot, metrics));
        decoration.prepare_with_dirty(context.gpu, snapshot, &snapshot.dirty_ranges);
        decoration.draw(pass);
    }
}
