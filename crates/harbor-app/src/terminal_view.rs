//! Root-level Component adapting widget paint and input to terminal boundaries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use harbor_terminal::{
    Preedit, RenderTarget, RenderViewport, Terminal, TerminalEvent, TerminalFocusEvent,
    TerminalGpuAccess, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
    TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase, TerminalRenderPipeline,
    TerminalSize, TextMetrics,
};
use harbor_widget::effects::ImeEffect;
use harbor_widget::input::event::{
    FocusEvent, Key, KeyboardEvent, Modifiers, PointerButton, PointerPhase, UiEvent,
};
use harbor_widget::input::event_ctx::EventHandled;
use harbor_widget::scene::primitive::{
    ExternalDrawContext, ExternalDrawFn, ExternalDrawId, ExternalDrawMode, ExternalImeFn,
    ExternalScheduleDemand, ExternalScheduleFn,
};
#[cfg(test)]
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::external_surface::{ExternalInputFn, ExternalSurface};

use harbor_widget::layout::{Point, Rect};
use harbor_widget::renderer::Viewport;
use harbor_widget::scene::primitive::Color;
use harbor_widget::{BorderRadius, BoxDecoration, BoxShadow, ClipBehavior, DecoratedBox};

/// Converts widget external-draw geometry into a terminal-owned [`RenderTarget`].
pub(crate) fn render_target_from_context(context: &ExternalDrawContext) -> RenderTarget {
    let (origin_x, origin_y, alloc_w, alloc_h) = context.physical_allocation();
    RenderTarget::new_with_scale(
        (origin_x, origin_y),
        (alloc_w, alloc_h),
        context.surface_size(),
        context.scale_factor(),
    )
}

/// Converts a final logical terminal-panel allocation into its PTY grid.
///
/// Invalid or non-drawable geometry is rejected before `RenderViewport` applies its minimum
/// grid clamp, so minimizing a window cannot emit a synthetic resize.
pub fn terminal_size_from_allocation(
    logical_rect: Rect,
    scale_factor: f32,
    surface_size: (u32, u32),
    metrics: &TextMetrics,
) -> Option<TerminalSize> {
    let coordinates = [
        logical_rect.min.x,
        logical_rect.min.y,
        logical_rect.max.x,
        logical_rect.max.y,
    ];
    if !coordinates.into_iter().all(f32::is_finite)
        || logical_rect.max.x <= logical_rect.min.x
        || logical_rect.max.y <= logical_rect.min.y
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || surface_size.0 == 0
        || surface_size.1 == 0
        || !metrics.cell_width.is_finite()
        || metrics.cell_width <= 0.0
        || !metrics.line_height.is_finite()
        || metrics.line_height <= 0.0
    {
        return None;
    }

    let context = ExternalDrawContext::new(
        logical_rect,
        Viewport::new(surface_size.0, surface_size.1, scale_factor),
    );
    let target = render_target_from_context(&context);
    if target.allocation_size.0 == 0 || target.allocation_size.1 == 0 {
        return None;
    }
    Some(RenderViewport::from_target(target, metrics).compute_grid_size())
}

/// Invokes `draw` only when the Runtime-supplied id matches the bridge-owned id.
pub(crate) fn dispatch_matched_draw(
    owned_id: impl Into<ExternalDrawId>,
    invoked_id: impl Into<ExternalDrawId>,
    context: &ExternalDrawContext,
    draw: impl FnOnce(RenderTarget),
) {
    let owned_id = owned_id.into();
    let invoked_id = invoked_id.into();
    if invoked_id != owned_id {
        return;
    }
    draw(render_target_from_context(context));
}

/// Retained frames replay buffers unless their placement no longer matches the engine.
fn needs_live_projection(
    mode: ExternalDrawMode,
    terminal: &Terminal,
    viewport: RenderViewport,
    target: RenderTarget,
    metrics: &TextMetrics,
) -> bool {
    mode == ExternalDrawMode::Live
        || terminal.projection_requires_full_update()
        || terminal.retained_geometry_changed(viewport, target, metrics)
}

/// Maps a widget [`UiEvent`] onto the terminal-owned [`TerminalEvent`] vocabulary.
///
/// Pointer-boundary events are consumed by widget-level mouse regions and have no terminal
/// equivalent, so they are deliberately not bridged to the terminal engine.
pub(crate) fn terminal_event_from_ui_event(event: UiEvent) -> Option<TerminalEvent> {
    match event {
        UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) => {
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                key: map_key(key),
                modifiers: map_modifiers(modifiers),
            }))
        }
        UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }) => {
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
                key: map_key(key),
                modifiers: map_modifiers(modifiers),
            }))
        }
        UiEvent::ImePreedit(preedit) => Some(TerminalEvent::Preedit(Preedit::new(
            preedit.text,
            preedit.cursor_range,
        ))),
        UiEvent::Keyboard(KeyboardEvent::Ime(text)) => {
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(text)))
        }
        UiEvent::Pointer(pointer) => Some(TerminalEvent::Pointer(
            TerminalPointerEvent::new(
                (pointer.position.x, pointer.position.y),
                map_pointer_phase(pointer.phase),
                map_pointer_button(pointer.button),
                pointer.pointer_id,
            )
            .with_modifiers(map_modifiers(pointer.modifiers)),
        )),
        UiEvent::PointerBoundary(_) => None,
        UiEvent::Clipboard { .. } => None,
        UiEvent::Focus(FocusEvent::Gained | FocusEvent::GainedVisible) => {
            Some(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        }
        UiEvent::Focus(FocusEvent::Lost) => Some(TerminalEvent::Focus(TerminalFocusEvent::Lost)),
        UiEvent::Focus(FocusEvent::VisibilityChanged(_)) => None,
    }
}

fn map_key(key: Key) -> TerminalKey {
    match key {
        Key::Tab => TerminalKey::Tab,
        Key::Enter => TerminalKey::Enter,
        Key::Space => TerminalKey::Space,
        Key::Escape => TerminalKey::Escape,
        Key::Backspace => TerminalKey::Backspace,
        Key::Insert => TerminalKey::Insert,
        Key::Delete => TerminalKey::Delete,
        Key::F1 => TerminalKey::F1,
        Key::F2 => TerminalKey::F2,
        Key::F3 => TerminalKey::F3,
        Key::F4 => TerminalKey::F4,
        Key::F5 => TerminalKey::F5,
        Key::F6 => TerminalKey::F6,
        Key::F7 => TerminalKey::F7,
        Key::F8 => TerminalKey::F8,
        Key::F9 => TerminalKey::F9,
        Key::F10 => TerminalKey::F10,
        Key::F11 => TerminalKey::F11,
        Key::F12 => TerminalKey::F12,
        Key::ArrowUp => TerminalKey::ArrowUp,
        Key::ArrowDown => TerminalKey::ArrowDown,
        Key::ArrowLeft => TerminalKey::ArrowLeft,
        Key::ArrowRight => TerminalKey::ArrowRight,
        Key::Home => TerminalKey::Home,
        Key::End => TerminalKey::End,
        Key::PageUp => TerminalKey::PageUp,
        Key::PageDown => TerminalKey::PageDown,
        Key::NumpadCharacter(c) => TerminalKey::NumpadCharacter(c),
        Key::NumpadEnter => TerminalKey::NumpadEnter,
        Key::Character(c) => TerminalKey::Character(c),
    }
}

fn map_modifiers(modifiers: Modifiers) -> TerminalModifiers {
    TerminalModifiers {
        shift: modifiers.shift,
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        meta: modifiers.meta,
    }
}

fn map_pointer_phase(phase: PointerPhase) -> TerminalPointerPhase {
    match phase {
        PointerPhase::Down => TerminalPointerPhase::Down,
        PointerPhase::Move => TerminalPointerPhase::Move,
        PointerPhase::Up => TerminalPointerPhase::Up,
        PointerPhase::Cancel => TerminalPointerPhase::Cancel,
        PointerPhase::WheelLine { dx, dy } => TerminalPointerPhase::WheelLine { dx, dy },
        PointerPhase::WheelPixel { dx, dy } => TerminalPointerPhase::WheelPixel { dx, dy },
    }
}

fn map_pointer_button(button: PointerButton) -> TerminalPointerButton {
    match button {
        PointerButton::Left => TerminalPointerButton::Left,
        PointerButton::Right => TerminalPointerButton::Right,
        PointerButton::Middle => TerminalPointerButton::Middle,
    }
}

fn forward_hyperlink_activation(uri: Option<String>, callback: &(dyn Fn(String) + Send + Sync)) {
    if let Some(uri) = uri {
        callback(uri);
    }
}

fn is_terminal_wheel(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::Pointer(pointer)
            if matches!(
                pointer.phase,
                PointerPhase::WheelLine { .. } | PointerPhase::WheelPixel { .. }
            )
    )
}

/// Returns true when the Host gate should suppress delivery of this event.
pub(crate) fn gate_suppresses_event(gate_active: bool, event: &UiEvent) -> bool {
    gate_active && !is_terminal_wheel(event)
}

fn wakes_redraw_for_routed_input(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Keyboard(KeyboardEvent::KeyDown { .. }))
}
/// CPU cell projection is independent of the renderer's pixel/GPU highlight cache.
/// All inputs read by project_content_ranges are qualified here; publication invalidates
/// the key so even different proofs for identical logical positions are revalidated.
#[derive(Default)]
struct DecorationProjection {
    ranges: Vec<harbor_terminal::read_model::ContentRange>,
    key: Option<DecorationProjectionKey>,
    spans: Vec<harbor_terminal::read_model::CellSpan>,
    #[cfg(test)]
    projections: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecorationProjectionKey {
    version: harbor_terminal::read_model::ContentVersion,
    history_start: u64,
    scroll_count: usize,
    view_offset: usize,
    rows: usize,
    cols: usize,
}

impl DecorationProjection {
    fn set_ranges(&mut self, ranges: Vec<harbor_terminal::read_model::ContentRange>) {
        self.ranges = ranges;
        self.key = None;
    }

    fn project(&mut self, terminal: &Terminal) -> &[harbor_terminal::read_model::CellSpan] {
        let screen = terminal.screen();
        let key = DecorationProjectionKey {
            version: terminal.content_version(),
            history_start: screen.history_start(),
            scroll_count: screen.scroll_count(),
            view_offset: screen.view_offset(),
            rows: screen.rows(),
            cols: screen.cols(),
        };
        if self.key != Some(key) {
            self.spans = terminal.project_content_ranges(&self.ranges);
            self.key = Some(key);
            #[cfg(test)]
            {
                self.projections += 1;
            }
        }
        &self.spans
    }
}

/// Component that owns the widget draw id and embeds a shared [`Terminal`] via [`ExternalSurface`].
#[derive(Clone)]
pub struct TerminalWidgetBridge {
    decorations: Arc<Mutex<DecorationProjection>>,
    #[cfg(test)]
    alt_screen_queries: Arc<std::sync::atomic::AtomicUsize>,
    draw_id: ExternalDrawId,
    terminal: Arc<Mutex<Terminal>>,
    handler: Arc<ExternalDrawFn<'static>>,
    schedule: Arc<ExternalScheduleFn>,
    ime: Arc<ExternalImeFn>,
    on_input: Arc<ExternalInputFn>,
    on_unmount: Arc<harbor_widget::widgets::external_surface::ExternalUnmountFn>,
    renderer: Option<Arc<Mutex<TerminalRenderPipeline>>>,
}

impl TerminalWidgetBridge {
    #[allow(dead_code)]
    /// Creates a headless bridge for input/scheduling without a GPU projection.
    /// Host transaction access; the renderer remains shared with this bridge's draw/IME providers.
    pub fn renderer(&self) -> Option<Arc<Mutex<TerminalRenderPipeline>>> {
        self.renderer.clone()
    }
    pub fn new(
        draw_id: impl Into<ExternalDrawId>,
        terminal: Arc<Mutex<Terminal>>,
        gate_active: Arc<AtomicBool>,
    ) -> Self {
        Self::new_internal(
            draw_id.into(),
            terminal,
            None,
            gate_active,
            Arc::new(|_| {}),
        )
    }

    /// Product bridge with one concrete GPU projection for this session.
    /// The widget clones only share this renderer; they never own another session.
    pub fn new_rendered_with_hyperlink_activation(
        draw_id: impl Into<ExternalDrawId>,
        terminal: Arc<Mutex<Terminal>>,
        renderer: TerminalRenderPipeline,
        gate_active: Arc<AtomicBool>,
        activate_hyperlink: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        #[allow(clippy::arc_with_non_send_sync)]
        let renderer = Arc::new(Mutex::new(renderer));
        Self::new_internal(
            draw_id.into(),
            terminal,
            Some(renderer),
            gate_active,
            activate_hyperlink,
        )
    }

    fn new_internal(
        draw_id: ExternalDrawId,
        terminal: Arc<Mutex<Terminal>>,
        renderer: Option<Arc<Mutex<TerminalRenderPipeline>>>,
        gate_active: Arc<AtomicBool>,
        activate_hyperlink: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        let decorations = Arc::new(Mutex::new(DecorationProjection::default()));
        let draw_decorations = Arc::clone(&decorations);
        let draw_terminal = Arc::clone(&terminal);
        let draw_renderer = renderer.clone();
        // ExternalDrawFn is UI-thread local; both resources have this tab's lifetime.
        #[allow(clippy::arc_with_non_send_sync)]
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(
            move |id, context, external_gpu, pass, mode| {
                dispatch_matched_draw(draw_id, id, context, |target| {
                    let Some(renderer) = &draw_renderer else {
                        // Headless bridges used by tests and the HMR host have no GPU projection.
                        return;
                    };
                    if let Ok(mut term) = draw_terminal.lock() {
                        let Ok(mut renderer) = renderer.lock() else {
                            return;
                        };
                        let gpu = TerminalGpuAccess::new(
                            external_gpu.device(),
                            external_gpu.queue(),
                            external_gpu.target_format(),
                        );
                        if renderer.raster_scale() != target.scale_factor {
                            let resource_update = term.read_update(std::time::Instant::now());
                            match renderer.sync_raster_scale(
                                gpu,
                                target.scale_factor,
                                &resource_update,
                            ) {
                                Ok(true) => term.invalidate_update(),
                                Ok(false) => {}
                                Err(error) => {
                                    tracing::warn!(%error, "terminal DPI resource preparation failed; update retained");
                                    return;
                                }
                            }
                        }
                        let metrics = *renderer.metrics();
                        if needs_live_projection(mode, &term, renderer.viewport(), target, &metrics)
                        {
                            let now = std::time::Instant::now();
                            let (viewport, grid_changed) =
                                term.prepare_render_frame(target, &metrics, now);
                            let update = term.read_update(now);
                            if !renderer.sync_viewport(viewport, grid_changed)
                                || !renderer.prepare(gpu, &update, term.blink_visible_at(now))
                            {
                                tracing::warn!(
                                    "terminal projection preparation rejected; update retained"
                                );
                                return;
                            }
                            let mut decorations = draw_decorations.lock().expect("decoration lock");
                            renderer.prepare_highlights(gpu, decorations.project(&term));
                            renderer.draw(pass);
                            term.acknowledge_update(&update);
                        } else {
                            let mut decorations = draw_decorations.lock().expect("decoration lock");
                            renderer.prepare_highlights(gpu, decorations.project(&term));
                            renderer.draw(pass);
                        }
                    }
                });
            },
        );

        let schedule_terminal = Arc::clone(&terminal);
        #[allow(clippy::arc_with_non_send_sync)]
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |id, now| {
            schedule_demand_for_terminal(draw_id, id, &schedule_terminal, now)
        });

        let ime_terminal = Arc::clone(&terminal);
        let ime_renderer = renderer.clone();
        #[allow(clippy::arc_with_non_send_sync)]
        let ime: Arc<ExternalImeFn> = Arc::new(move |id, context| {
            if id != draw_id {
                return None;
            }
            let target = render_target_from_context(context);
            let mut effect = ImeEffect::set_allowed(true);
            let position = ime_renderer.as_ref().and_then(|renderer| {
                let term = ime_terminal.lock().ok()?;
                let renderer = renderer.lock().ok()?;
                term.ime_candidate_position_with_metrics(target, renderer.metrics())
            });
            if let Some((x, y)) = position {
                effect.position =
                    Some(Point::new(x / target.scale_factor, y / target.scale_factor));
            }
            Some(effect)
        });

        let input_terminal = Arc::clone(&terminal);
        #[allow(clippy::arc_with_non_send_sync)]
        let on_input: Arc<ExternalInputFn> = Arc::new(move |event, ctx| {
            if gate_suppresses_event(gate_active.load(Ordering::Acquire), event) {
                return EventHandled::Handled;
            }

            let wheel = is_terminal_wheel(event);
            let key_wakes = wakes_redraw_for_routed_input(event);
            let Some(mapped) = terminal_event_from_ui_event(event.clone()) else {
                return EventHandled::Ignored;
            };

            if let Ok(mut term) = input_terminal.lock() {
                let offset_before = wheel.then(|| term.screen().view_offset());
                match term.handle_event_with_outcome(mapped) {
                    Ok(outcome) => {
                        if let Some(pointer_id) = outcome.capture_pointer {
                            ctx.capture_pointer(pointer_id);
                        }
                        if let Some(pointer_id) = outcome.release_pointer {
                            ctx.release_pointer(pointer_id);
                        }
                        if let Some(text) = outcome.clipboard_text {
                            ctx.write_clipboard(text);
                        }
                        forward_hyperlink_activation(
                            outcome.hyperlink_activation,
                            activate_hyperlink.as_ref(),
                        );
                        let offset_moved = offset_before
                            .is_some_and(|before| before != term.screen().view_offset());
                        if key_wakes || offset_moved || outcome.redraw {
                            ctx.invalidate_paint();
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            error = %format_args!("{error:#}"),
                            "failed to write terminal input"
                        );
                    }
                }
            }
            EventHandled::Handled
        });

        let unmount_terminal = Arc::clone(&terminal);
        #[allow(clippy::arc_with_non_send_sync)]
        let on_unmount: Arc<harbor_widget::widgets::external_surface::ExternalUnmountFn> =
            Arc::new(move || {
                if let Ok(mut term) = unmount_terminal.lock() {
                    term.clear_preedit();
                }
            });

        Self {
            decorations,
            #[cfg(test)]
            alt_screen_queries: Arc::default(),
            draw_id,
            renderer,
            terminal,
            handler,
            schedule,
            ime,
            on_input,
            on_unmount,
        }
    }

    #[cfg(test)]
    pub(crate) fn decoration_ranges(&self) -> Vec<harbor_terminal::read_model::ContentRange> {
        self.decorations.lock().unwrap().ranges.clone()
    }

    #[cfg(test)]
    pub(crate) fn alt_screen_queries(&self) -> usize {
        self.alt_screen_queries.load(Ordering::Relaxed)
    }
    /// Application-published proven ranges, never physical coordinates or query policy.
    pub fn set_decorations(&self, ranges: Vec<harbor_terminal::read_model::ContentRange>) {
        self.decorations
            .lock()
            .expect("decoration lock")
            .set_ranges(ranges);
    }

    /// Widget-facing external draw identifier owned by this bridge.
    pub fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }

    /// Reports whether the active selection would produce non-empty copied text.
    pub fn has_non_empty_selection(&self) -> bool {
        self.terminal
            .lock()
            .is_ok_and(|terminal| terminal.has_non_empty_selection())
    }

    /// Reports whether terminal input currently targets the alternate screen.
    pub fn is_alt_screen(&self) -> bool {
        #[cfg(test)]
        self.alt_screen_queries.fetch_add(1, Ordering::Relaxed);
        self.terminal.lock().is_ok_and(|mut terminal| {
            // Preserve fresh mode ingestion without copying the visible grid.
            terminal.drain_pty();
            terminal.is_alt_screen()
        })
    }
}

/// Creates the leaf widget that renders a terminal bridge without an extra component wrapper.
pub(crate) fn terminal_widget(bridge: TerminalWidgetBridge) -> ExternalSurface {
    ExternalSurface::new(bridge.draw_id())
        .on_draw(Arc::clone(&bridge.handler))
        .schedule_with(Arc::clone(&bridge.schedule))
        .ime_with(Arc::clone(&bridge.ime))
        .on_input(Arc::clone(&bridge.on_input))
        .on_unmount(Arc::clone(&bridge.on_unmount))
}

/// Maps terminal Frame Demand into the widget schedule contract for a matched id.
pub(crate) fn schedule_demand_for_terminal(
    owned_id: impl Into<ExternalDrawId>,
    invoked_id: impl Into<ExternalDrawId>,
    terminal: &Mutex<Terminal>,
    now: std::time::Instant,
) -> ExternalScheduleDemand {
    let owned_id = owned_id.into();
    let invoked_id = invoked_id.into();
    if invoked_id != owned_id {
        return ExternalScheduleDemand::empty();
    }
    let Ok(mut term) = terminal.lock() else {
        return ExternalScheduleDemand::empty();
    };
    let demand = term.frame_demand(now);
    ExternalScheduleDemand {
        redraw_now: demand.redraw_now,
        deadline: demand.deadline,
        ordinary_present_eligible: demand.ordinary_present_eligible,
    }
}

/// Product appearance for the main Harbor terminal.
pub(crate) struct TerminalDecorationPreset;

impl TerminalDecorationPreset {
    pub(crate) fn decoration() -> BoxDecoration {
        let shadow = BoxShadow::new()
            .try_color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.10,
            })
            .expect("product shadow color is finite")
            .try_offset(Point::new(0.0, 1.0))
            .expect("product shadow offset is finite")
            .try_blur_radius(1.0)
            .expect("product shadow blur is finite and non-negative");
        BoxDecoration::new()
            .border_radius(
                BorderRadius::all(8.0).expect("product radius is finite and non-negative"),
            )
            .shadow(shadow)
    }

    pub(crate) fn container() -> DecoratedBox {
        DecoratedBox::new(Self::decoration()).clip_behavior(ClipBehavior::AntiAlias)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::input::event::{
        Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
        UiEvent,
    };
    use harbor_widget::layout::{Point, Rect, Size};
    use harbor_widget::renderer::Viewport;
    use harbor_widget::scene::primitive::ExternalDrawContext;
    use harbor_widget::winit::WindowPresenter;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    fn decoration_range(
        terminal: &Terminal,
        row: usize,
        units: std::ops::Range<usize>,
    ) -> harbor_terminal::read_model::ContentRange {
        use harbor_terminal::read_model::{ReadBudget, ReadScope};
        terminal
            .read_content(
                ReadScope::Visible {
                    start_row: row,
                    row_count: 1,
                },
                ReadBudget::default(),
            )
            .unwrap()
            .fragments[0]
            .range(units)
            .unwrap()
    }

    fn assert_cached_projection(cache: &mut DecorationProjection, terminal: &Terminal) {
        let expected = terminal.project_content_ranges(&cache.ranges);
        assert_eq!(cache.project(terminal), expected);
        let count = cache.projections;
        assert_eq!(cache.project(terminal), expected);
        assert_eq!(
            cache.projections, count,
            "unchanged frames must not reproject"
        );
    }

    #[test]
    fn decoration_projection_caches_and_invalidates_viewport_and_publication() {
        use harbor_terminal::read_model::CellSpan;
        let mut terminal = Terminal::new_headless(2, 8);
        terminal.process_output("ab界\r\nnext".as_bytes());
        let range = decoration_range(&terminal, 0, 2..3);
        let mut cache = DecorationProjection::default();
        cache.set_ranges(vec![range.clone()]);
        assert_eq!(
            cache.project(&terminal),
            [CellSpan {
                row: 0,
                start_col: 2,
                end_col: 4
            }]
        );
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(cache.projections, 1);

        // Unrelated row output revalidates proofs but keeps the original match.
        terminal.process_output(b"!");
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(cache.projections, 2);
        assert!(!cache.spans.is_empty());

        terminal.process_output(b"\r\nlast");
        assert_cached_projection(&mut cache, &terminal);
        assert!(cache.spans.is_empty(), "match is now above the viewport");
        let version = terminal.content_version();
        terminal.scroll_viewport_up(1);
        assert_eq!(
            terminal.content_version(),
            version,
            "scrolling is not a semantic edit"
        );
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(cache.spans[0].row, 0);
        let count = cache.projections;
        terminal.scroll_viewport_to_bottom();
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(cache.projections, count + 1);
        assert!(cache.spans.is_empty());

        terminal.scroll_viewport_up(1);
        assert_cached_projection(&mut cache, &terminal);
        cache.set_ranges(vec![decoration_range(&terminal, 1, 0..2)]);
        assert_cached_projection(&mut cache, &terminal);
        assert_eq!(
            cache.spans,
            [CellSpan {
                row: 1,
                start_col: 0,
                end_col: 2
            }]
        );
        cache.set_ranges(vec![]);
        assert_cached_projection(&mut cache, &terminal);
        assert!(
            cache.spans.is_empty(),
            "query cancellation removes cached spans"
        );
    }

    #[test]
    fn decoration_projection_never_reuses_stale_proofs_across_content_lifetimes() {
        // Overwrite, erase, reset, alternate entry, resize/reflow and eviction must
        // all replace a populated cache with fresh (empty) geometry.
        for change in 0..7 {
            let mut terminal = Terminal::new_headless(2, 8);
            terminal.process_output(b"needle");
            let range = decoration_range(&terminal, 0, 0..6);
            let mut cache = DecorationProjection::default();
            cache.set_ranges(vec![range]);
            assert_cached_projection(&mut cache, &terminal);
            assert!(!cache.spans.is_empty());
            match change {
                0 => terminal.process_output(b"\rxxxxxx"),
                1 => terminal.process_output(b"\r\x1b[2K"),
                2 => terminal.process_output(b"\x1bcneedle"),
                3 => terminal.process_output(b"\x1b[?1049hneedle"),
                4 => {
                    terminal
                        .try_resize_if_changed(TerminalSize { rows: 2, cols: 4 })
                        .unwrap();
                }
                5 => {
                    terminal
                        .try_resize_if_changed(TerminalSize { rows: 3, cols: 8 })
                        .unwrap();
                }
                6 => terminal.process_output("other\r\n".repeat(1100).as_bytes()),
                _ => unreachable!(),
            }
            assert_cached_projection(&mut cache, &terminal);
            assert_eq!(
                cache.projections, 2,
                "change {change} must miss the CPU cache"
            );
            assert!(
                cache.spans.is_empty(),
                "change {change} retained stale geometry"
            );
            if change == 3 {
                terminal.process_output(b"\x1b[?1049l");
                assert_cached_projection(&mut cache, &terminal);
                // The restored primary's unchanged proof can legitimately be visible again.
                assert_eq!(cache.projections, 3);
            }
        }

        let mut original = Terminal::new_headless(2, 8);
        original.process_output(b"needle");
        let mut cache = DecorationProjection::default();
        cache.set_ranges(vec![decoration_range(&original, 0, 0..6)]);
        assert!(!cache.project(&original).is_empty());
        let mut replacement = Terminal::new_headless(2, 8);
        replacement.process_output(b"needle");
        assert_cached_projection(&mut cache, &replacement);
        assert!(
            cache.spans.is_empty(),
            "same text in a new session is not the same proof"
        );
    }

    #[test]
    fn bridge_alternate_screen_query_drains_queued_output_without_snapshot_or_acknowledgement() {
        struct QueuedReader(std::sync::mpsc::Receiver<Vec<u8>>);
        impl std::io::Read for QueuedReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let Ok(bytes) = self.0.recv() else {
                    return Ok(0);
                };
                assert!(bytes.len() <= buf.len());
                buf[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
        }
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        let (wake_tx, wake_rx) = std::sync::mpsc::channel();
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            2,
            8,
            QueuedReader(output_rx),
            std::io::sink(),
            move || wake_tx.send(()).is_ok(),
        )));
        let bridge =
            TerminalWidgetBridge::new(1, terminal.clone(), Arc::new(AtomicBool::new(false)));
        for (bytes, expected) in [
            (b"\x1b[?1049h".to_vec(), true),
            (b"\x1b[?1049l".to_vec(), false),
        ] {
            output_tx.send(bytes).unwrap();
            wake_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            assert_eq!(terminal.lock().unwrap().is_alt_screen(), !expected);
            assert_eq!(
                bridge.is_alt_screen(),
                expected,
                "availability must ingest queued modes"
            );
            assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
            assert_eq!(bridge.is_alt_screen(), expected);
        }
        drop(output_tx);
    }

    fn context(logical: Rect, physical: (u32, u32), scale: f32) -> ExternalDrawContext {
        ExternalDrawContext::new(logical, Viewport::new(physical.0, physical.1, scale))
    }

    fn metrics() -> TextMetrics {
        TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 16.0,
            underline_thickness: 2.0,
            strikethrough_position: 10.0,
            strikethrough_thickness: 2.0,
        }
    }

    #[test]
    fn skipped_widget_frame_preserves_update_until_live_projection_and_rechecks_dpi() {
        let now = std::time::Instant::now();
        let metrics = metrics();
        let target = RenderTarget::new((0.0, 0.0), (100, 40), (100, 40));
        let viewport = RenderViewport::from_target(target, &metrics);
        let size = viewport.compute_grid_size();
        let mut terminal = Terminal::new_headless(size.rows, size.cols);
        assert!(terminal.acknowledge_update(&terminal.read_update(now)));
        terminal.process_output(b"hi");
        assert!(!needs_live_projection(
            ExternalDrawMode::Retain,
            &terminal,
            viewport,
            target,
            &metrics,
        ));
        assert_eq!(terminal.read_update(now).snapshot.cell_char(0, 0), 'h');
        assert!(!terminal.read_update(now).snapshot.dirty_ranges.is_empty());
        assert!(needs_live_projection(
            ExternalDrawMode::Live,
            &terminal,
            viewport,
            target,
            &metrics,
        ));
        let scale_only = RenderTarget::new_with_scale((0.0, 0.0), (100, 40), (100, 40), 2.0);
        assert!(needs_live_projection(
            ExternalDrawMode::Retain,
            &terminal,
            viewport,
            scale_only,
            &metrics,
        ));
        let (same_viewport, changed) = terminal.prepare_render_frame(scale_only, &metrics, now);
        assert!(!changed);
        assert_eq!(same_viewport, viewport);
        assert!(!needs_live_projection(
            ExternalDrawMode::Retain,
            &terminal,
            viewport,
            scale_only,
            &metrics,
        ));
        let dpi_target = RenderTarget::new_with_scale((0.0, 0.0), (200, 80), (200, 80), 2.0);
        assert!(needs_live_projection(
            ExternalDrawMode::Retain,
            &terminal,
            viewport,
            dpi_target,
            &metrics,
        ));
        let (new_viewport, _) = terminal.prepare_render_frame(dpi_target, &metrics, now);
        assert!(terminal.acknowledge_update(&terminal.read_update(now)));
        assert!(!needs_live_projection(
            ExternalDrawMode::Retain,
            &terminal,
            new_viewport,
            dpi_target,
            &metrics,
        ));
    }

    #[test]
    fn rendered_bridge_replays_hidden_output_and_retained_resize_after_skipped_frames() {
        assert_rendered_bridge_replays_hidden_output_and_retained_resize(None);
        // Exercise the geometry that previously exposed styled scroll-fill tails
        // on CI, independently of the machine's selected system font.
        assert_rendered_bridge_replays_hidden_output_and_retained_resize(Some(TextMetrics {
            line_height: 30.0,
            ..metrics()
        }));
    }

    fn assert_rendered_bridge_replays_hidden_output_and_retained_resize(
        geometry_metrics: Option<TextMetrics>,
    ) {
        use harbor_widget::scene::primitive::ExternalDrawGpu;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            }))
        else {
            eprintln!("SKIP: no GPU adapter for rendered terminal bridge test");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("terminal bridge test"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .expect("test GPU device");
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let gpu = TerminalGpuAccess::new(&device, &queue, format);
        let fonts = harbor_terminal::load_system_fonts(&harbor_config::FontSettings::default())
            .expect("test system fonts");
        let metrics = geometry_metrics
            .unwrap_or_else(|| TextMetrics::from_font_metrics(fonts.font_metrics()));
        let terminal = headless_terminal(4, 8);
        let renderer = TerminalRenderPipeline::new(
            gpu,
            (160, 120),
            fonts,
            metrics,
            &terminal
                .lock()
                .unwrap()
                .read_update(std::time::Instant::now()),
        )
        .expect("terminal renderer");
        let weak = Arc::downgrade(&terminal);
        let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
            73,
            Arc::clone(&terminal),
            renderer,
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        );
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terminal bridge target"),
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let initial = context(
            Rect::from_min_size(Point::ZERO, Size::new(160.0, 120.0)),
            (256, 256),
            1.0,
        );
        let resized = context(
            Rect::from_min_size(Point::ZERO, Size::new(192.0, 128.0)),
            (256, 256),
            1.0,
        );
        let draw = |ctx: &ExternalDrawContext, id: u64, mode| {
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("terminal bridge pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                (bridge.handler)(
                    id.into(),
                    ctx,
                    ExternalDrawGpu::new(&device, &queue, format),
                    &mut pass,
                    mode,
                );
            }
            queue.submit([encoder.finish()]);
        };
        draw(&initial, 73, ExternalDrawMode::Live);
        terminal.lock().unwrap().process_output(b"ab");
        let range = decoration_range(&terminal.lock().unwrap(), 0, 0..2);
        bridge.set_decorations(vec![range]);
        draw(&initial, 73, ExternalDrawMode::Live);
        assert!(!bridge.decorations.lock().unwrap().spans.is_empty());
        let projections = bridge.decorations.lock().unwrap().projections;
        for mode in [
            ExternalDrawMode::Retain,
            ExternalDrawMode::Live,
            ExternalDrawMode::Retain,
        ] {
            draw(&initial, 73, mode);
            assert_eq!(bridge.decorations.lock().unwrap().projections, projections);
        }
        terminal.lock().unwrap().process_output(b"\rxx");
        draw(&initial, 73, ExternalDrawMode::Retain);
        assert!(bridge.decorations.lock().unwrap().spans.is_empty());
        assert_eq!(
            bridge.decorations.lock().unwrap().projections,
            projections + 1
        );
        draw(&initial, 73, ExternalDrawMode::Live);
        assert_eq!(
            bridge.decorations.lock().unwrap().projections,
            projections + 1
        );
        bridge.set_decorations(vec![]);
        terminal
            .lock()
            .unwrap()
            .handle_event(TerminalEvent::Preedit(Preedit::new("x", None)))
            .unwrap();
        assert!(
            (bridge.ime)(73.into(), &initial)
                .unwrap()
                .position
                .is_some()
        );
        assert!((bridge.ime)(99.into(), &initial).is_none());
        terminal.lock().unwrap().clear_preedit();
        terminal.lock().unwrap().process_output(b"hidden");
        draw(&initial, 73, ExternalDrawMode::Retain);
        draw(&initial, 99, ExternalDrawMode::Live);
        assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        draw(&initial, 73, ExternalDrawMode::Live);
        assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        // Rejected targets must not acknowledge damage. Recovery consumes it
        // only after the renderer accepts the viewport and prepares every layer.
        for rejected in [
            context(
                Rect::from_min_size(Point::ZERO, Size::new(0.0, 120.0)),
                (256, 256),
                1.0,
            ),
            context(
                Rect::from_min_size(Point::ZERO, Size::new(160.0, 120.0)),
                (0, 256),
                1.0,
            ),
        ] {
            terminal
                .lock()
                .unwrap()
                .process_output(b"\x1b[4;9;53mrejected");
            draw(&rejected, 73, ExternalDrawMode::Live);
            assert!(!terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
            draw(&initial, 73, ExternalDrawMode::Live);
            assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        }
        // The remaining checks exercise successful resize/draw coordination, not
        // the model's rejection of a cursor pushed into history. Styled scroll
        // fills can leave meaningful blanks after the cursor; at tiny font-
        // dependent grids those blanks wrap below it and make a shrink invalid.
        // Reset the pen and erase only that suffix, preserving the printed text
        // and its decorations already exercised by the rejected-target checks.
        terminal.lock().unwrap().process_output(b"\x1b[0m\x1b[0J");
        let scaled = context(
            Rect::from_min_size(Point::ZERO, Size::new(80.0, 60.0)),
            (256, 256),
            2.0,
        );
        terminal.lock().unwrap().process_output(b"dpi");
        draw(&scaled, 73, ExternalDrawMode::Retain);
        assert!(terminal.lock().unwrap().snapshot().dirty_ranges.is_empty());
        let expected_scaled =
            RenderViewport::from_target(render_target_from_context(&scaled), &metrics)
                .compute_grid_size();
        let scaled_snapshot = terminal.lock().unwrap().snapshot();
        assert_eq!(
            (scaled_snapshot.rows, scaled_snapshot.cols),
            (expected_scaled.rows, expected_scaled.cols),
            "host DPI must preserve main's physical font metrics"
        );
        draw(&resized, 73, ExternalDrawMode::Retain);
        let expected = RenderViewport::from_target(render_target_from_context(&resized), &metrics)
            .compute_grid_size();
        let snapshot = terminal.lock().unwrap().snapshot();
        assert_eq!(
            (snapshot.rows, snapshot.cols),
            (expected.rows, expected.cols)
        );
        assert!(snapshot.dirty_ranges.is_empty());
        // The product coordinator broadcasts sizes using startup metrics before the
        // bridge draws. Both phases must agree, including after a skipped draw/DPI change.
        for scale in [1.0, 1.25, 1.5, 2.0, 1.5, 1.0] {
            for (width, height) in [(80.0, 60.0), (96.0, 64.0), (80.0, 60.0)] {
                let rect = Rect::from_min_size(Point::ZERO, Size::new(width, height));
                let ctx = context(rect, (256, 256), scale);
                let size = terminal_size_from_allocation(rect, scale, (256, 256), &metrics)
                    .expect("drawable allocation");
                terminal
                    .lock()
                    .unwrap()
                    .try_resize_if_changed(size)
                    .unwrap();
                let before_draw = terminal.lock().unwrap().snapshot();
                draw(&ctx, 99, ExternalDrawMode::Live); // unmatched/skipped frame
                draw(&ctx, 73, ExternalDrawMode::Retain);
                let after_draw = terminal.lock().unwrap().snapshot();
                assert_eq!(
                    (after_draw.rows, after_draw.cols),
                    (size.rows, size.cols),
                    "allocation and drawing disagree at scale {scale}"
                );
                assert_eq!(
                    after_draw.cells, before_draw.cells,
                    "draw must not reflow again"
                );
                assert!(after_draw.dirty_ranges.is_empty());
            }
        }
        drop(bridge);
        drop(terminal);
        assert!(weak.upgrade().is_none());
    }

    #[allow(clippy::arc_with_non_send_sync)]
    fn headless_terminal(rows: usize, cols: usize) -> Arc<Mutex<Terminal>> {
        Arc::new(Mutex::new(Terminal::new_headless(rows, cols)))
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
    fn terminal_size_uses_external_rounding_and_shared_grid_rules() {
        let metrics = metrics();
        let rect = Rect::from_min_size(Point::new(0.4, 0.6), Size::new(100.2, 50.4));
        let size = terminal_size_from_allocation(rect, 1.5, (1200, 900), &metrics).unwrap();
        let physical_width = 151.0_f32;
        let physical_height = 77.0_f32;
        let padding = 2.0 * harbor_config::TEXT_PADDING;
        assert_eq!(
            size.cols,
            ((physical_width - padding) / 10.0).floor() as usize
        );
        assert_eq!(
            size.rows,
            ((physical_height - padding) / 20.0).floor() as usize
        );
    }

    #[test]
    fn terminal_size_uses_only_surface_visible_allocation() {
        let metrics = metrics();
        let clipped = Rect::from_min_size(Point::new(80.0, 40.0), Size::new(120.0, 120.0));
        let context = context(clipped, (140, 120), 1.0);
        let target = render_target_from_context(&context);
        assert_eq!(target.allocation_origin, (80.0, 40.0));
        assert_eq!(target.allocation_size, (60, 80));
        assert_eq!(
            terminal_size_from_allocation(clipped, 1.0, (140, 120), &metrics),
            Some(TerminalSize { rows: 2, cols: 2 })
        );

        let outside = Rect::from_min_size(Point::new(150.0, 40.0), Size::new(120.0, 120.0));
        assert_eq!(
            terminal_size_from_allocation(outside, 1.0, (140, 120), &metrics),
            None
        );
    }

    #[test]
    fn terminal_size_matches_supported_two_column_minimum() {
        let metrics = metrics();
        let tiny = Rect::from_min_size(Point::ZERO, Size::new(35.0, 35.0));
        let size = terminal_size_from_allocation(tiny, 1.0, (140, 120), &metrics);
        assert_eq!(size, Some(TerminalSize { rows: 1, cols: 2 }));
        let mut terminal = Terminal::new_headless(4, 20);
        assert!(terminal.resize_if_changed(size.unwrap()));
        assert_eq!((terminal.screen().rows(), terminal.screen().cols()), (1, 2));
    }

    #[test]
    fn terminal_size_rejects_non_drawable_and_invalid_geometry() {
        let metrics = metrics();
        let valid = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        assert_eq!(
            terminal_size_from_allocation(valid, 1.0, (0, 600), &metrics),
            None
        );
        assert_eq!(
            terminal_size_from_allocation(
                Rect::from_min_size(Point::ZERO, Size::ZERO),
                1.0,
                (800, 600),
                &metrics,
            ),
            None
        );
        assert_eq!(
            terminal_size_from_allocation(valid, f32::NAN, (800, 600), &metrics),
            None
        );
    }

    #[test]
    fn should_use_caller_provided_draw_id() {
        let terminal = headless_terminal(24, 80);
        let gate = Arc::new(AtomicBool::new(false));

        let bridge = TerminalWidgetBridge::new(41, terminal, gate);

        assert_eq!(bridge.draw_id(), 41);
    }

    #[test]
    fn should_forward_one_hyperlink_activation_to_injected_callback() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let callback_received = Arc::clone(&received);
        let callback: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |uri| {
            callback_received.lock().unwrap().push(uri);
        });

        forward_hyperlink_activation(Some("https://example.test".to_owned()), callback.as_ref());
        forward_hyperlink_activation(None, callback.as_ref());

        assert_eq!(
            received.lock().unwrap().as_slice(),
            ["https://example.test"]
        );
    }

    #[test]
    fn should_reuse_cached_handler_arc_when_building_multiple_widgets() {
        // Arrange: handler is created once in `new` and cloned into each widget.
        let terminal = headless_terminal(24, 80);
        let bridge = TerminalWidgetBridge::new(42, terminal, Arc::new(AtomicBool::new(false)));
        let cached = Arc::clone(&bridge.handler);
        assert_eq!(Arc::strong_count(&cached), 2);

        // Act
        let widget_a = terminal_widget(bridge.clone());
        let count_after_first = Arc::strong_count(&cached);

        let widget_b = terminal_widget(bridge.clone());
        let count_after_second = Arc::strong_count(&cached);

        // Assert: each widget clones the same Arc (not a freshly allocated handler).
        assert!(Arc::ptr_eq(&cached, &bridge.handler));
        assert!(count_after_first > 2);
        assert!(count_after_second > count_after_first);

        drop(widget_a);
        drop(widget_b);
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
        dispatch_matched_draw(41, 42, &ctx, |_| {
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
        dispatch_matched_draw(41, 41, &ctx, |target| {
            drawn.set(Some(target));
        });
        // Assert
        let target = drawn.get().expect("draw invoked");
        assert_eq!(target.allocation_origin, (10.0, 5.0));
        assert_eq!(target.allocation_size, (200, 100));
        assert_eq!(target.surface_size, (800, 600));
    }

    #[test]
    fn should_map_keyboard_key_down_with_modifiers() {
        // Arrange
        let event = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        });

        // Act
        let mapped = terminal_event_from_ui_event(event);

        // Assert
        assert_eq!(
            mapped,
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                key: TerminalKey::Enter,
                modifiers: TerminalModifiers {
                    ctrl: true,
                    ..TerminalModifiers::default()
                },
            }))
        );
    }

    #[test]
    fn should_map_ime_and_focus_and_wheel() {
        // Arrange / Act / Assert
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Keyboard(KeyboardEvent::Ime("你好".into()))),
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(
                "你好".into()
            )))
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::ImePreedit(
                harbor_widget::input::event::ImePreedit::new("ni", Some((2, 2))),
            )),
            Some(TerminalEvent::Preedit(Preedit::new("ni", Some((2, 2)))))
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Gained)),
            Some(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        );

        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::GainedVisible)),
            Some(TerminalEvent::Focus(TerminalFocusEvent::Gained))
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Pointer(PointerEvent::new(
                Point::new(1.0, 2.0),
                PointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
                PointerButton::Left,
                3,
            ))),
            Some(TerminalEvent::Pointer(TerminalPointerEvent::new(
                (1.0, 2.0),
                TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
                TerminalPointerButton::Left,
                3,
            )))
        );
    }

    #[test]
    fn should_preserve_both_wheel_axes_and_units_at_terminal_bridge() {
        for (ui_phase, terminal_phase) in [
            (
                PointerPhase::WheelLine { dx: 1.5, dy: -2.0 },
                TerminalPointerPhase::WheelLine { dx: 1.5, dy: -2.0 },
            ),
            (
                PointerPhase::WheelPixel { dx: -0.25, dy: 3.5 },
                TerminalPointerPhase::WheelPixel { dx: -0.25, dy: 3.5 },
            ),
        ] {
            assert_eq!(
                terminal_event_from_ui_event(UiEvent::Pointer(PointerEvent::new(
                    Point::new(1.0, 2.0),
                    ui_phase,
                    PointerButton::Left,
                    3
                ))),
                Some(TerminalEvent::Pointer(TerminalPointerEvent::new(
                    (1.0, 2.0),
                    terminal_phase,
                    TerminalPointerButton::Left,
                    3
                )))
            );
        }
    }

    #[test]
    fn should_suppress_non_wheel_when_gate_active() {
        let key = UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        });
        let wheel = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
            PointerButton::Left,
            0,
        ));
        let move_event = UiEvent::Pointer(PointerEvent::new(
            Point::ZERO,
            PointerPhase::Move,
            PointerButton::Left,
            0,
        ));

        assert!(!gate_suppresses_event(false, &key));
        assert!(gate_suppresses_event(true, &key));
        assert!(!gate_suppresses_event(true, &wheel));
        assert!(gate_suppresses_event(true, &move_event));
    }

    #[test]
    fn should_map_key_up_and_focus_lost() {
        // Arrange / Act / Assert
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Keyboard(KeyboardEvent::KeyUp {
                key: WidgetKey::Escape,
                modifiers: Modifiers {
                    alt: true,
                    ..Modifiers::default()
                },
            })),
            Some(TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
                key: TerminalKey::Escape,
                modifiers: TerminalModifiers {
                    alt: true,
                    ..TerminalModifiers::default()
                },
            }))
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Lost)),
            Some(TerminalEvent::Focus(TerminalFocusEvent::Lost))
        );
    }

    fn seed_scrollback(terminal: &mut Terminal) {
        for _ in 0..40 {
            terminal.process_output(b"line\r\n");
        }
    }

    fn runtime_with_bridge(
        terminal: Arc<Mutex<Terminal>>,
        gate: Arc<AtomicBool>,
    ) -> harbor_widget::runtime::Runtime {
        let bridge = TerminalWidgetBridge::new(1, terminal, gate);
        let mut rt = harbor_widget::runtime::Runtime::new();
        rt.set_root(terminal_widget(bridge));
        rt.update(std::time::Instant::now());
        assert!(rt.focus_first_focusable());
        let _ = rt.drain_external_input();
        rt
    }

    #[test]
    fn should_leave_scrollback_unchanged_when_page_up_reaches_terminal_bridge() {
        // Arrange
        let terminal = headless_terminal(8, 40);
        seed_scrollback(&mut terminal.lock().unwrap());
        let gate = Arc::new(AtomicBool::new(false));
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
        let offset_before = terminal.lock().unwrap().screen().view_offset();

        // Act
        let effects = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::PageUp,
            modifiers: Modifiers::default(),
        }));

        // Assert: application scroll interception has migrated out of terminal input.
        assert_eq!(
            terminal.lock().unwrap().screen().view_offset(),
            offset_before
        );
        assert!(!effects.request_redraw);
        assert!(rt.drain_external_input().is_empty());
    }

    #[test]
    fn should_not_scroll_when_gate_suppresses_keydown() {
        // Arrange
        let terminal = headless_terminal(8, 40);
        seed_scrollback(&mut terminal.lock().unwrap());
        let gate = Arc::new(AtomicBool::new(true));
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
        let offset_before = terminal.lock().unwrap().screen().view_offset();

        // Act
        let effects = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::PageUp,
            modifiers: Modifiers::default(),
        }));

        // Assert: gated non-wheel is swallowed without delivery or redraw wake.
        assert_eq!(
            terminal.lock().unwrap().screen().view_offset(),
            offset_before
        );
        assert!(!effects.request_redraw);
    }

    #[test]
    fn should_scroll_when_gate_allows_wheel() {
        // Arrange
        let terminal = headless_terminal(8, 40);
        seed_scrollback(&mut terminal.lock().unwrap());
        let gate = Arc::new(AtomicBool::new(true));
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
        let offset_before = terminal.lock().unwrap().screen().view_offset();

        // Act
        let effects = rt.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(10.0, 10.0),
            PointerPhase::WheelLine { dx: 0.0, dy: 2.0 },
            PointerButton::Left,
            0,
        )));

        // Assert: gated wheel still delivers and wakes when the viewport moves.
        assert!(terminal.lock().unwrap().screen().view_offset() > offset_before);
        assert!(effects.request_redraw);
    }

    #[test]
    fn should_return_empty_schedule_demand_for_mismatched_draw_id() {
        // Arrange
        let terminal = headless_terminal(2, 4);
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 99, &terminal, now);

        // Assert
        assert_eq!(demand, ExternalScheduleDemand::empty());
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_map_headless_frame_demand_to_empty_schedule_demand() {
        // Arrange — headless engine still owns cursor blink timing.
        let terminal = headless_terminal(2, 4);
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert!(!demand.redraw_now);
        assert!(demand.deadline.is_some());
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_copy_frame_demand_fields_when_draw_id_matches() {
        // Arrange
        let terminal = headless_terminal(2, 4);
        let now = std::time::Instant::now();
        let expected = terminal.lock().unwrap().frame_demand(now);

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert_eq!(demand.redraw_now, expected.redraw_now);
        assert_eq!(demand.deadline, expected.deadline);
        assert_eq!(
            demand.ordinary_present_eligible,
            expected.ordinary_present_eligible
        );
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_return_empty_schedule_demand_when_terminal_mutex_is_poisoned() {
        // Arrange — poison on this thread (Terminal is !Send)
        let terminal = headless_terminal(2, 4);
        let now = std::time::Instant::now();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = terminal.lock().unwrap();
            panic!("poison schedule lock");
        }));
        assert!(poisoned.is_err());

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert_eq!(demand, ExternalScheduleDemand::empty());
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_emit_wait_until_from_runtime_when_bridge_registers_schedule() {
        use harbor_widget::effects::ControlFlowEffect;
        use std::time::Duration;

        // Arrange — fake schedule via ExternalSurface-equivalent bridge registration
        // is exercised through Runtime with a scripted provider in widget tests;
        // here verify bridge root registers a schedule that headless empties.
        let terminal = headless_terminal(2, 4);
        let mut rt = runtime_with_bridge(terminal, Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();

        // Act — clean idle turn after initial build consumed dirty work
        let _ = rt.update(now);
        let idle = rt.update(now + Duration::from_millis(1));

        // Assert — blink timing is scheduled even for a headless engine.
        assert!(!idle.request_redraw);
        assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
        assert!(matches!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(_))
        ));
    }

    #[test]
    fn should_copy_ineligible_demand_when_synchronized_output_is_enabled() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert!(!demand.ordinary_present_eligible);
        assert!(!demand.redraw_now);
        assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
    }

    #[test]
    fn should_copy_eligible_demand_when_matching_2026_disable_has_been_applied() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026hhello\x1b[?2026l");
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert!(demand.ordinary_present_eligible);
        assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
    }

    #[test]
    fn should_return_empty_eligible_demand_when_draw_id_mismatches_ineligible_terminal() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 99, &terminal, now);

        // Assert — mismatch ignores the ineligible terminal snapshot
        assert_eq!(demand, ExternalScheduleDemand::empty());
        assert!(demand.ordinary_present_eligible);
    }

    #[test]
    fn should_skip_ordinary_redraw_when_about_to_wait_while_2026_is_enabled() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();
        let _ = rt.update(now);
        let mut presenter = WindowPresenter::new(800, 600, 1.0);

        // Act
        let idle = presenter.about_to_wait(&mut rt, now, None);

        // Assert
        assert!(!idle.request_redraw);
        assert!(idle.ordinary_present_eligible);
        assert!(idle.has_deferred_externals);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_force_commit_when_about_to_wait_after_recovery_while_2026_is_nested() {
        use harbor_widget::effects::ControlFlowEffect;
        use std::time::Duration;

        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026h\x1b[?2026hhello");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();
        let _ = rt.update(now);
        let mut presenter = WindowPresenter::new(800, 600, 1.0);
        let _ = presenter.about_to_wait(&mut rt, now, None);

        // Act
        let due = presenter.about_to_wait(&mut rt, now + Duration::from_millis(100), None);

        // Assert — recovery live-commits without resetting nested DECRQM Set
        assert!(due.request_redraw);
        assert!(due.force_present);
        assert!(due.has_deferred_externals);
        assert!(matches!(
            due.control_flow,
            Some(ControlFlowEffect::WaitUntil(_))
        ));
        assert!(
            !terminal
                .lock()
                .unwrap()
                .frame_demand(now)
                .ordinary_present_eligible
        );
        assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
    }

    #[test]
    fn should_report_eligible_effects_when_about_to_wait_after_matching_disable() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026hhello\x1b[?2026l");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let mut presenter = WindowPresenter::new(800, 600, 1.0);

        // Act
        let idle = presenter.about_to_wait(&mut rt, std::time::Instant::now(), None);

        // Assert
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.force_present);
    }

    #[test]
    fn should_stay_eligible_when_trailing_disables_precede_later_output() {
        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026l\x1b[?2026llater");
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert!(demand.ordinary_present_eligible);
        assert!(terminal.lock().unwrap().row_text(0).contains("later"));
    }

    #[test]
    fn should_cancel_deferred_externals_when_ris_clears_nested_2026() {
        use harbor_widget::effects::ControlFlowEffect;
        use std::time::Duration;

        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026h\x1b[?2026hhello");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();
        let _ = rt.update(now);
        let mut presenter = WindowPresenter::new(800, 600, 1.0);
        let armed = presenter.about_to_wait(&mut rt, now, None);
        assert!(armed.has_deferred_externals);
        assert_eq!(
            armed.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );

        // Act
        terminal.lock().unwrap().process_output(b"\x1bc");
        let idle = presenter.about_to_wait(&mut rt, now, None);

        // Assert
        assert!(idle.ordinary_present_eligible);
        assert!(!idle.has_deferred_externals);
        assert!(!idle.force_present);
        assert!(idle.request_redraw);
        assert_ne!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );
        assert!(
            terminal
                .lock()
                .unwrap()
                .frame_demand(now)
                .ordinary_present_eligible
        );
    }

    #[test]
    fn should_keep_deferred_externals_when_decstr_leaves_nested_2026() {
        use harbor_widget::effects::ControlFlowEffect;
        use std::time::Duration;

        // Arrange
        let terminal = headless_terminal(2, 20);
        terminal.lock().unwrap().process_output(b"\x1b[?2026hhello");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();
        let _ = rt.update(now);
        let mut presenter = WindowPresenter::new(800, 600, 1.0);
        let _ = presenter.about_to_wait(&mut rt, now, None);

        // Act
        terminal.lock().unwrap().process_output(b"\x1b[!p");
        let idle = presenter.about_to_wait(&mut rt, now, None);

        // Assert
        assert!(idle.has_deferred_externals);
        assert!(!idle.force_present);
        assert_eq!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );
        assert!(
            !terminal
                .lock()
                .unwrap()
                .frame_demand(now)
                .ordinary_present_eligible
        );
        assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
    }

    #[test]
    fn should_cancel_deferred_externals_when_pty_eof_clears_nested_2026() {
        use harbor_widget::effects::ControlFlowEffect;
        use std::io::Read;
        use std::time::Duration;

        struct PermitEofReader {
            permit: std::sync::mpsc::Receiver<()>,
            exited: std::sync::mpsc::Sender<()>,
        }
        impl Read for PermitEofReader {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                let _ = self.permit.recv();
                Ok(0)
            }
        }
        impl Drop for PermitEofReader {
            fn drop(&mut self) {
                let _ = self.exited.send(());
            }
        }

        let (permit_tx, permit_rx) = std::sync::mpsc::channel();
        let (exited_tx, exited_rx) = std::sync::mpsc::channel();
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            2,
            20,
            PermitEofReader {
                permit: permit_rx,
                exited: exited_tx,
            },
            std::io::sink(),
            || true,
        )));
        terminal
            .lock()
            .unwrap()
            .process_output(b"\x1b[?2026h\x1b[?2026hhello");
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();
        let _ = rt.update(now);
        let mut presenter = WindowPresenter::new(800, 600, 1.0);
        let armed = presenter.about_to_wait(&mut rt, now, None);
        assert!(armed.has_deferred_externals);
        assert_eq!(
            armed.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );

        permit_tx.send(()).expect("reader should still be waiting");
        exited_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("reader must drop before drain can observe disconnect");
        let idle = presenter.about_to_wait(&mut rt, now, None);

        assert!(idle.ordinary_present_eligible);
        assert!(!idle.has_deferred_externals);
        assert!(!idle.force_present);
        assert!(idle.request_redraw);
        assert_ne!(
            idle.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );
        assert!(
            terminal
                .lock()
                .unwrap()
                .frame_demand(now)
                .ordinary_present_eligible
        );
        assert!(terminal.lock().unwrap().row_text(0).contains("hello"));
    }
}

#[cfg(test)]
mod decoration_tests {
    use super::*;
    use harbor_widget::layout::{Point, Rect, Size};
    use harbor_widget::renderer::Viewport;
    use harbor_widget::runtime::Runtime;
    use harbor_widget::scene::SceneItem;
    use harbor_widget::scene::primitive::{Color, Primitive};
    use harbor_widget::widgets::external_surface::ExternalSurface;
    use harbor_widget::widgets::padding::Padding;
    use harbor_widget::{
        BorderRadius, ClipBehavior, ControlFlowEffect, DecoratedBox, RuntimeEffects,
    };
    use std::any::TypeId;
    use std::time::Instant;

    #[derive(Clone)]
    struct TestMainTerminalRoot<C: Clone> {
        root: Padding,
        child: C,
    }

    impl<C: Component + Clone + 'static> Component for TestMainTerminalRoot<C> {
        fn build(&self, _cx: &mut BuildCx) -> View {
            harbor_widget::view! { self.root.clone() => {
                TerminalDecorationPreset::container() => {
                    self.child.clone();
                }
            }
            }
        }
    }

    fn test_main_terminal_root(
        backdrop_available: bool,
        child: impl Component + Clone + 'static,
    ) -> impl Component {
        TestMainTerminalRoot {
            root: crate::workspace::root_padding(
                backdrop_available,
                harbor_config::WindowBackdropStyle::default().fallback,
            ),
            child,
        }
    }

    fn mounted_main_root(viewport: Option<Viewport>) -> (Runtime, RuntimeEffects) {
        let mut runtime = Runtime::new();
        if let Some(viewport) = viewport {
            runtime.set_viewport(viewport);
        }
        runtime.set_root(test_main_terminal_root(false, ExternalSurface::new(1)));
        let effects = runtime.update(Instant::now());
        (runtime, effects)
    }

    fn fiber_chain(runtime: &Runtime) -> (TypeId, Rect, TypeId, Rect, TypeId, Rect) {
        let padding_id = runtime.root_id().expect("root fiber");
        let padding = runtime.arena().get(padding_id).expect("padding fiber");
        let padding_type = padding.widget_type();
        let padding_rect = padding.layout_rect().expect("padding layout");
        let decorated_id = padding.children()[0];
        let decorated = runtime.arena().get(decorated_id).expect("decorated fiber");
        let decorated_type = decorated.widget_type();
        let decorated_rect = decorated.layout_rect().expect("decorated layout");
        let external_id = decorated.children()[0];
        let external = runtime.arena().get(external_id).expect("external fiber");
        (
            padding_type,
            padding_rect,
            decorated_type,
            decorated_rect,
            external.widget_type(),
            external.layout_rect().expect("external layout"),
        )
    }

    fn painted_items(runtime: &Runtime) -> Vec<SceneItem> {
        let mut items = runtime
            .pending_delta()
            .cloned()
            .expect("mounted root produces a scene delta")
            .added;
        items.sort_by_key(|item| item.paint_order);
        items
    }

    fn assert_opaque_fallback_color(color: Color) {
        for channel in [color.r, color.g, color.b] {
            let srgb = if channel <= 0.0031308 {
                channel * 12.92
            } else {
                1.055 * channel.powf(1.0 / 2.4) - 0.055
            };
            assert_eq!((srgb * 255.0).round() as u8, 0x1E);
        }
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn should_expose_product_decoration_values() {
        let wrapped = TerminalDecorationPreset::container();

        assert_eq!(wrapped.get_clip_behavior(), ClipBehavior::AntiAlias);
        let decoration = wrapped.get_decoration();
        assert_eq!(decoration.color(), None);
        assert_eq!(decoration.border_value(), None);
        let radii = decoration
            .border_radius_value()
            .expect("product radius is present")
            .as_array();
        assert_eq!(radii, [8.0, 8.0, 8.0, 8.0]);
        let shadows = decoration.shadows();
        assert_eq!(shadows.len(), 1);
        let shadow = shadows[0];
        assert_eq!(shadow.color().a, 0.10);
        assert_eq!(shadow.offset(), Point::new(0.0, 1.0));
        assert_eq!(shadow.blur_radius(), 1.0);
        assert_eq!(shadow.spread_radius(), 0.0);
    }

    #[test]
    fn should_apply_four_dp_inset_with_opaque_fallback_when_no_backdrop() {
        let root = crate::workspace::root_padding(
            false,
            harbor_config::WindowBackdropStyle::default().fallback,
        );
        assert_eq!(root.get_insets().top, 4.0);
        assert_eq!(root.get_insets().right, 4.0);
        assert_eq!(root.get_insets().bottom, 4.0);
        assert_eq!(root.get_insets().left, 4.0);
        let mut runtime = Runtime::new();
        runtime.set_root(root);
        runtime.update(Instant::now());
        let items = painted_items(&runtime);
        let [item] = items.as_slice() else {
            panic!("one opaque root background quad");
        };
        let Primitive::Quad { color, .. } = item.primitive else {
            panic!("opaque root background quad");
        };
        assert_opaque_fallback_color(color);
    }

    #[test]
    fn should_omit_root_background_when_backdrop_is_available() {
        let root = crate::workspace::root_padding(
            true,
            harbor_config::WindowBackdropStyle::default().fallback,
        );
        let mut runtime = Runtime::new();
        runtime.set_root(root);
        runtime.update(Instant::now());
        assert!(
            runtime
                .pending_delta()
                .is_none_or(|delta| delta.added.is_empty())
        );
    }

    #[test]
    fn should_emit_no_root_quad_and_keep_inset_when_backdrop_is_available() {
        let mut runtime = Runtime::new();
        runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
        let _effects = runtime.update(Instant::now());
        let items = painted_items(&runtime);

        assert!(
            !items
                .iter()
                .any(|item| matches!(item.primitive, Primitive::Quad { .. })),
            "backdrop-aware root must not emit a base Quad"
        );

        let (_, _padding_rect, _, decorated_rect, _, external_rect) = fiber_chain(&runtime);
        let expected_child = Rect::from_min_size(Point::new(4.0, 4.0), Size::new(792.0, 592.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);
    }

    #[test]
    fn should_paint_dark_fallback_across_viewport_when_backdrop_is_unavailable() {
        for (width, height) in [(800, 600), (40, 40), (10, 10)] {
            let viewport = Viewport::new(width, height, 1.0);
            let expected_rect = Rect::from_min_size(Point::new(0.0, 0.0), viewport.logical_size);
            let (runtime, _) = mounted_main_root(Some(viewport));
            let items = painted_items(&runtime);
            let first = items.first().expect("opaque root emits its fallback");
            let Primitive::Quad { rect, color, .. } = &first.primitive else {
                panic!("opaque fallback must paint before the shadow and terminal");
            };
            assert_eq!(*rect, expected_rect);
            assert_opaque_fallback_color(*color);
            assert!(
                first.clips.is_empty(),
                "fallback must fill the rounded cutouts"
            );
        }
    }

    #[test]
    fn should_preserve_decoration_paint_when_backdrop_is_available() {
        for (width, height) in [(800, 600), (40, 40)] {
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(width, height, 1.0));
            runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
            runtime.update(Instant::now());
            let items = painted_items(&runtime);

            assert_eq!(items.len(), 2);
            let expected_child = Rect::from_min_size(
                Point::new(4.0, 4.0),
                Size::new(width as f32 - 8.0, height as f32 - 8.0),
            );
            let Primitive::OuterShadow {
                color,
                blur_radius,
                occluder_rect,
                ..
            } = &items[0].primitive
            else {
                panic!("shadow must paint before the terminal");
            };
            assert_eq!(color.a, 0.10);
            assert_eq!(*blur_radius, 1.0);
            assert_eq!(*occluder_rect, expected_child);
            assert!(matches!(
                items[1].primitive,
                Primitive::External { draw, rect } if draw == 1 && rect == expected_child
            ));
            let clip = items[1]
                .clips
                .last()
                .expect("terminal keeps its rounded clip");
            assert_eq!(clip.behavior(), ClipBehavior::AntiAlias);
            let radius = 8.0;
            assert_eq!(clip.radii().as_array(), [radius; 4]);
        }
    }

    #[test]
    fn should_omit_base_fill_when_backdrop_viewport_has_no_content_space() {
        for (width, height) in [(0, 0), (4, 4), (8, 8)] {
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(width, height, 1.0));
            runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
            let effects = runtime.update(Instant::now());
            let items = painted_items(&runtime);

            assert!(
                !items
                    .iter()
                    .any(|item| matches!(item.primitive, Primitive::Quad { .. }))
            );
            for item in &items {
                if let Primitive::External { rect, .. } = item.primitive {
                    assert_eq!(rect.size(), Size::ZERO);
                }
            }
            assert!(!matches!(
                effects.control_flow,
                Some(ControlFlowEffect::WaitUntil(_))
            ));
        }
    }

    #[test]
    fn should_layout_and_paint_preset_tree_when_runtime_uses_default_viewport() {
        let (runtime, _effects) = mounted_main_root(None);
        let (
            padding_type,
            padding_rect,
            decorated_type,
            decorated_rect,
            external_type,
            external_rect,
        ) = fiber_chain(&runtime);
        let items = painted_items(&runtime);

        // The root Fiber belongs to the mounted description component (ADR 0079);
        // the concrete Padding materializes onto it without an extra layer.
        assert_eq!(
            padding_type,
            TypeId::of::<TestMainTerminalRoot<ExternalSurface>>()
        );
        assert_eq!(decorated_type, TypeId::of::<DecoratedBox>());
        assert_eq!(external_type, TypeId::of::<ExternalSurface>());
        assert_eq!(padding_rect.min, Point::new(0.0, 0.0));
        assert_eq!(padding_rect.size(), Size::new(800.0, 600.0));
        let expected_child = Rect::from_min_size(Point::new(4.0, 4.0), Size::new(792.0, 592.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);

        let mut primitives = items.iter().map(|item| &item.primitive);
        let first = primitives
            .find(|primitive| {
                matches!(
                    primitive,
                    Primitive::Quad { .. }
                        | Primitive::OuterShadow { .. }
                        | Primitive::External { .. }
                        | Primitive::RoundedQuad { .. }
                        | Primitive::RoundedBorder { .. }
                )
            })
            .expect("decoration or external primitive");
        let Primitive::Quad { color, .. } = first else {
            panic!("first matching primitive must be the opaque base Quad")
        };
        assert_eq!(color.a, 1.0);
        let shadow_item = items
            .iter()
            .find(|item| matches!(item.primitive, Primitive::OuterShadow { .. }))
            .expect("decoration emits OuterShadow");
        let Primitive::OuterShadow {
            color,
            blur_radius,
            occluder_rect,
            ..
        } = &shadow_item.primitive
        else {
            unreachable!();
        };
        assert_eq!(color.a, 0.10);
        assert_eq!(*blur_radius, 1.0);
        assert_eq!(*occluder_rect, expected_child);
        assert!(
            !items
                .iter()
                .any(|item| matches!(item.primitive, Primitive::RoundedQuad { .. })),
            "no-fill decoration must not emit RoundedQuad"
        );
        assert!(
            !items
                .iter()
                .any(|item| matches!(item.primitive, Primitive::RoundedBorder { .. })),
            "no-border decoration must not emit RoundedBorder"
        );

        let external = items
            .iter()
            .find(|item| matches!(item.primitive, Primitive::External { .. }))
            .expect("ExternalSurface emits External");
        assert!(matches!(
            external.primitive,
            Primitive::External {
                draw,
                rect
            } if draw == 1 && rect == expected_child
        ));
        assert!(!external.clips.is_empty());
        let innermost = external.clips.last().expect("innermost clip");
        assert_eq!(innermost.behavior(), ClipBehavior::AntiAlias);
        assert_eq!(innermost.radii().as_array(), [8.0, 8.0, 8.0, 8.0]);
    }

    #[test]
    fn should_keep_zero_child_size_without_repeating_redraw_when_viewport_is_zero() {
        let viewport = Viewport::new(0, 0, 1.0);
        let (runtime, effects) = mounted_main_root(Some(viewport));
        let (
            _padding_type,
            padding_rect,
            _decorated_type,
            decorated_rect,
            _external_type,
            external_rect,
        ) = fiber_chain(&runtime);

        assert_eq!(padding_rect.min, Point::new(0.0, 0.0));
        assert_eq!(decorated_rect.size(), Size::ZERO);
        assert_eq!(external_rect.size(), Size::ZERO);
        assert!(!matches!(
            effects.control_flow,
            Some(ControlFlowEffect::WaitUntil(_))
        ));
    }

    #[test]
    fn should_normalize_clip_radii_to_child_size_when_viewport_is_smaller_than_radius() {
        let viewport = Viewport::new(12, 12, 1.0);
        let logical = viewport.logical_size;
        let (runtime, _effects) = mounted_main_root(Some(viewport));
        let (
            _padding_type,
            _padding_rect,
            _decorated_type,
            decorated_rect,
            _external_type,
            external_rect,
        ) = fiber_chain(&runtime);
        let items = painted_items(&runtime);

        let expected_child = Rect::from_min_size(
            Point::new(4.0, 4.0),
            Size::new(logical.width - 8.0, logical.height - 8.0),
        );
        assert_eq!(decorated_rect.min, Point::new(4.0, 4.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);

        let external = items
            .iter()
            .find(|item| matches!(item.primitive, Primitive::External { .. }))
            .expect("ExternalSurface emits External");
        let innermost = external
            .clips
            .last()
            .expect("External carries a child clip");
        let expected_radii = BorderRadius::all(8.0)
            .expect("ticket radius is finite")
            .normalize(expected_child.size())
            .expect("child size is finite")
            .as_array();
        assert_eq!(innermost.radii().as_array(), expected_radii);
    }
}
