//! Root-level Component adapting widget external paint to the terminal render boundary.

use std::sync::{Arc, Mutex};

use harbor_terminal::{RenderTarget, Terminal};
use harbor_widget::scene::primitive::{ExternalDrawContext, ExternalDrawFn, ExternalDrawId};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::custom_paint::CustomPaint;

use crate::app::current_gpu;

/// Default external-draw identifier matching the previous terminal-owned constant.
const DEFAULT_DRAW_ID: ExternalDrawId = 1;

/// Converts widget external-draw geometry into a terminal-owned [`RenderTarget`].
pub(crate) fn render_target_from_context(context: &ExternalDrawContext) -> RenderTarget {
    let (origin_x, origin_y, alloc_w, alloc_h) = context.physical_allocation();
    RenderTarget::new(
        (origin_x, origin_y),
        (alloc_w, alloc_h),
        context.surface_size(),
    )
}

/// Invokes `draw` only when the Runtime-supplied id matches the bridge-owned id.
pub(crate) fn dispatch_matched_draw(
    owned_id: ExternalDrawId,
    invoked_id: ExternalDrawId,
    context: &ExternalDrawContext,
    draw: impl FnOnce(RenderTarget),
) {
    if invoked_id != owned_id {
        return;
    }
    draw(render_target_from_context(context));
}

/// Component that owns the widget draw id and embeds a shared [`Terminal`] via [`CustomPaint`].
pub struct TerminalWidgetBridge {
    draw_id: ExternalDrawId,
    handler: Arc<ExternalDrawFn<'static>>,
}

impl TerminalWidgetBridge {
    /// Creates a bridge that paints `terminal` under the default draw identifier.
    pub fn new(terminal: Arc<Mutex<Terminal>>) -> Self {
        let draw_id = DEFAULT_DRAW_ID;
        // ExternalDrawFn is Arc-typed; the closure captures UI-thread Terminal.
        #[allow(clippy::arc_with_non_send_sync)]
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |id, context, pass| {
            dispatch_matched_draw(draw_id, id, context, |target| {
                current_gpu(|gpu| {
                    if let Ok(mut term) = terminal.lock() {
                        term.render(target, pass, gpu);
                    }
                });
            });
        });
        Self { draw_id, handler }
    }

    /// Widget-facing external draw identifier owned by this bridge.
    pub fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }
}

impl Component for TerminalWidgetBridge {
    fn build(&self, cx: &mut BuildCx) -> View {
        CustomPaint::new(self.draw_id)
            .handler(Arc::clone(&self.handler))
            .build(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::layout::{Point, Rect, Size};
    use harbor_widget::renderer::Viewport;
    use harbor_widget::scene::primitive::ExternalDrawContext;
    use std::cell::Cell;

    fn context(logical: Rect, physical: (u32, u32), scale: f32) -> ExternalDrawContext {
        ExternalDrawContext::new(logical, Viewport::new(physical.0, physical.1, scale))
    }

    #[test]
    fn should_map_1x_scale_physical_allocation_to_render_target() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
            (800, 600),
            1.0,
        );

        // Act
        let target = render_target_from_context(&ctx);

        // Assert
        assert_eq!(target.allocation_origin, (10.0, 5.0));
        assert_eq!(target.allocation_size, (200, 100));
        assert_eq!(target.surface_size, (800, 600));
    }

    #[test]
    fn should_map_2x_scale_physical_allocation_to_render_target() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
            (1600, 1200),
            2.0,
        );

        // Act
        let target = render_target_from_context(&ctx);

        // Assert: logical 200×100 at 2× → physical 400×200
        assert_eq!(target.allocation_origin, (20.0, 10.0));
        assert_eq!(target.allocation_size, (400, 200));
        assert_eq!(target.surface_size, (1600, 1200));
    }

    #[test]
    fn should_preserve_zero_size_physical_allocation() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::new(0.0, 0.0), Size::new(0.0, 0.0)),
            (800, 600),
            1.0,
        );

        // Act
        let target = render_target_from_context(&ctx);

        // Assert
        assert_eq!(target.allocation_origin, (0.0, 0.0));
        assert_eq!(target.allocation_size, (0, 0));
        assert_eq!(target.surface_size, (800, 600));
    }

    #[test]
    fn should_round_fractional_logical_allocation_when_mapping_to_render_target() {
        // Arrange: floor origin, ceil far edge (via ExternalDrawContext::physical_allocation)
        let ctx = context(
            Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4)),
            (800, 600),
            1.0,
        );

        // Act
        let target = render_target_from_context(&ctx);

        // Assert: floor(0.4)=0, ceil(100.6)=101; floor(0.6)=0, ceil(51.0)=51
        assert_eq!(target.allocation_origin, (0.0, 0.0));
        assert_eq!(target.allocation_size, (101, 51));
        assert_eq!(target.surface_size, (800, 600));
    }

    #[test]
    fn should_round_fractional_logical_allocation_at_2x_scale() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4)),
            (1600, 1200),
            2.0,
        );

        // Act
        let target = render_target_from_context(&ctx);

        // Assert: floor(0.8)=0, ceil(201.2)=202; floor(1.2)=1, ceil(102.0)=102
        assert_eq!(target.allocation_origin, (0.0, 1.0));
        assert_eq!(target.allocation_size, (202, 101));
        assert_eq!(target.surface_size, (1600, 1200));
    }

    #[test]
    fn should_expose_default_draw_id() {
        // Arrange
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));

        // Act
        let bridge = TerminalWidgetBridge::new(terminal);

        // Assert
        assert_eq!(bridge.draw_id(), DEFAULT_DRAW_ID);
    }

    #[test]
    fn should_reuse_cached_handler_arc_when_built_multiple_times() {
        // Arrange: handler is created once in `new` and cloned into each build.
        // Registration itself is covered by CustomPaint unit tests; here we only
        // prove the bridge reuses one Arc across rebuilds (no widget API widen).
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(24, 80)));
        let bridge = TerminalWidgetBridge::new(terminal);
        let cached = Arc::clone(&bridge.handler);
        assert_eq!(Arc::strong_count(&cached), 2);

        // Act
        let mut cx_a = BuildCx::stub();
        let view_a = bridge.build(&mut cx_a);
        let count_after_first = Arc::strong_count(&cached);

        let mut cx_b = BuildCx::stub();
        let view_b = bridge.build(&mut cx_b);
        let count_after_second = Arc::strong_count(&cached);

        // Assert: each build clones the same Arc (not a freshly allocated handler).
        assert!(Arc::ptr_eq(&cached, &bridge.handler));
        assert!(count_after_first > 2);
        assert!(count_after_second > count_after_first);

        drop(view_a);
        drop(cx_a);
        drop(view_b);
        drop(cx_b);
        assert_eq!(Arc::strong_count(&cached), 2);
    }

    #[test]
    fn should_skip_draw_when_external_draw_id_mismatches() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::ZERO, Size::new(100.0, 100.0)),
            (800, 600),
            1.0,
        );
        let called = Cell::new(false);

        // Act
        dispatch_matched_draw(DEFAULT_DRAW_ID, DEFAULT_DRAW_ID + 1, &ctx, |_| {
            called.set(true);
        });

        // Assert
        assert!(!called.get());
    }

    #[test]
    fn should_draw_when_external_draw_id_matches() {
        // Arrange
        let ctx = context(
            Rect::from_min_size(Point::new(10.0, 5.0), Size::new(200.0, 100.0)),
            (800, 600),
            1.0,
        );
        let drawn = Cell::new(None);

        // Act
        dispatch_matched_draw(DEFAULT_DRAW_ID, DEFAULT_DRAW_ID, &ctx, |target| {
            drawn.set(Some(target));
        });

        // Assert
        let target = drawn.get().expect("draw invoked");
        assert_eq!(target.allocation_origin, (10.0, 5.0));
        assert_eq!(target.allocation_size, (200, 100));
        assert_eq!(target.surface_size, (800, 600));
    }
}
