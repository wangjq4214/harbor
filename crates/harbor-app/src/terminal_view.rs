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
                        // A draw before admission can already install the new viewport
                        // with an old grid. Compare the prepared projection as well.
                        let projection_grid_changed = renderer.grid_size()
                            != (TerminalSize {
                                rows: term.screen().rows(),
                                cols: term.screen().cols(),
                            });
                        if needs_live_projection(mode, &term, renderer.viewport(), target, &metrics)
                            || projection_grid_changed
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
mod tests;

#[cfg(test)]
mod decoration_tests;
