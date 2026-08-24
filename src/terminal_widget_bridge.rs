//! Root-level Component adapting widget paint and input to terminal boundaries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use harbor_terminal::{
    RenderTarget, Terminal, TerminalEvent, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent,
    TerminalModifiers, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
};
use harbor_widget::input::event::{
    FocusEvent, Key, KeyboardEvent, Modifiers, PointerButton, PointerPhase, UiEvent,
};
use harbor_widget::input::event_ctx::EventHandled;
use harbor_widget::scene::primitive::{
    ExternalDrawContext, ExternalDrawFn, ExternalDrawId, ExternalDrawMode, ExternalScheduleDemand,
    ExternalScheduleFn,
};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::custom_paint::{CustomPaint, ExternalInputFn};

use crate::app::current_gpu;

/// Default external-draw identifier matching the previous terminal-owned constant.
const DEFAULT_DRAW_ID: ExternalDrawId = 1;

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

/// Maps a widget [`UiEvent`] onto the terminal-owned [`TerminalEvent`] vocabulary.
pub(crate) fn terminal_event_from_ui_event(event: UiEvent) -> TerminalEvent {
    match event {
        UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) => {
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                key: map_key(key),
                modifiers: map_modifiers(modifiers),
            })
        }
        UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }) => {
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
                key: map_key(key),
                modifiers: map_modifiers(modifiers),
            })
        }
        UiEvent::Keyboard(KeyboardEvent::Ime(text)) => {
            TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime(text))
        }
        UiEvent::Pointer(pointer) => TerminalEvent::Pointer(
            TerminalPointerEvent::new(
                (pointer.position.x, pointer.position.y),
                map_pointer_phase(pointer.phase),
                map_pointer_button(pointer.button),
                pointer.pointer_id,
            )
            .with_modifiers(map_modifiers(pointer.modifiers)),
        ),
        UiEvent::Focus(FocusEvent::Gained) => TerminalEvent::Focus(TerminalFocusEvent::Gained),
        UiEvent::Focus(FocusEvent::Lost) => TerminalEvent::Focus(TerminalFocusEvent::Lost),
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

/// Component that owns the widget draw id and embeds a shared [`Terminal`] via [`CustomPaint`].
pub struct TerminalWidgetBridge {
    draw_id: ExternalDrawId,
    handler: Arc<ExternalDrawFn<'static>>,
    schedule: Arc<ExternalScheduleFn>,
    on_input: Arc<ExternalInputFn>,
}

impl TerminalWidgetBridge {
    /// Creates a bridge that paints and receives input for `terminal`.
    pub fn new(terminal: Arc<Mutex<Terminal>>, gate_active: Arc<AtomicBool>) -> Self {
        let draw_id = DEFAULT_DRAW_ID;
        let draw_terminal = Arc::clone(&terminal);
        // ExternalDrawFn is Arc-typed; the closure captures UI-thread Terminal.
        #[allow(clippy::arc_with_non_send_sync)]
        let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |id, context, pass, mode| {
            dispatch_matched_draw(draw_id, id, context, |target| {
                current_gpu(|gpu| {
                    if let Ok(mut term) = draw_terminal.lock() {
                        match mode {
                            ExternalDrawMode::Live => term.render(target, pass, gpu),
                            ExternalDrawMode::Retain => term.draw_retained(target, pass, gpu),
                        }
                    }
                });
            });
        });

        let schedule_terminal = Arc::clone(&terminal);
        #[allow(clippy::arc_with_non_send_sync)]
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |id, now| {
            schedule_demand_for_terminal(draw_id, id, &schedule_terminal, now)
        });

        let input_gate = Arc::clone(&gate_active);
        let input_terminal = Arc::clone(&terminal);
        #[allow(clippy::arc_with_non_send_sync)]
        let on_input: Arc<ExternalInputFn> = Arc::new(move |event, ctx| {
            if gate_suppresses_event(input_gate.load(Ordering::Acquire), event) {
                return EventHandled::Handled;
            }

            let wheel = is_terminal_wheel(event);
            let key_wakes = wakes_redraw_for_routed_input(event);
            let mapped = terminal_event_from_ui_event(event.clone());

            let mut offset_before = None;
            if let Ok(mut term) = input_terminal.lock() {
                if wheel {
                    offset_before = Some(term.screen().view_offset());
                }
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

        Self {
            draw_id,
            handler,
            schedule,
            on_input,
        }
    }

    /// Widget-facing external draw identifier owned by this bridge.
    pub fn draw_id(&self) -> ExternalDrawId {
        self.draw_id
    }
}

impl Component for TerminalWidgetBridge {
    fn build(&self, cx: &mut BuildCx) -> View {
        CustomPaint::new(self.draw_id())
            .handler(Arc::clone(&self.handler))
            .schedule(Arc::clone(&self.schedule))
            .on_input(Arc::clone(&self.on_input))
            .build(cx)
    }
}

/// Maps terminal Frame Demand into the widget schedule contract for a matched id.
pub(crate) fn schedule_demand_for_terminal(
    owned_id: ExternalDrawId,
    invoked_id: ExternalDrawId,
    terminal: &Mutex<Terminal>,
    now: std::time::Instant,
) -> ExternalScheduleDemand {
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
    use harbor_widget::winit::WinitAdapter;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    fn context(logical: Rect, physical: (u32, u32), scale: f32) -> ExternalDrawContext {
        ExternalDrawContext::new(logical, Viewport::new(physical.0, physical.1, scale))
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
    fn should_expose_default_draw_id() {
        // Arrange
        let terminal = headless_terminal(24, 80);
        let gate = Arc::new(AtomicBool::new(false));

        // Act
        let bridge = TerminalWidgetBridge::new(terminal, gate);

        // Assert
        assert_eq!(bridge.draw_id(), DEFAULT_DRAW_ID);
    }

    #[test]
    fn should_reuse_cached_handler_arc_when_built_multiple_times() {
        // Arrange: handler is created once in `new` and cloned into each build.
        let terminal = headless_terminal(24, 80);
        let bridge = TerminalWidgetBridge::new(terminal, Arc::new(AtomicBool::new(false)));
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
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
                key: TerminalKey::Enter,
                modifiers: TerminalModifiers {
                    ctrl: true,
                    ..TerminalModifiers::default()
                },
            })
        );
    }

    #[test]
    fn should_map_ime_and_focus_and_wheel() {
        // Arrange / Act / Assert
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Keyboard(KeyboardEvent::Ime("你好".into()))),
            TerminalEvent::Keyboard(TerminalKeyboardEvent::Ime("你好".into()))
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Gained)),
            TerminalEvent::Focus(TerminalFocusEvent::Gained)
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Pointer(PointerEvent::new(
                Point::new(1.0, 2.0),
                PointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
                PointerButton::Left,
                3,
            ))),
            TerminalEvent::Pointer(TerminalPointerEvent::new(
                (1.0, 2.0),
                TerminalPointerPhase::WheelLine { dx: 0.0, dy: -1.0 },
                TerminalPointerButton::Left,
                3,
            ))
        );
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
            TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyUp {
                key: TerminalKey::Escape,
                modifiers: TerminalModifiers {
                    alt: true,
                    ..TerminalModifiers::default()
                },
            })
        );
        assert_eq!(
            terminal_event_from_ui_event(UiEvent::Focus(FocusEvent::Lost)),
            TerminalEvent::Focus(TerminalFocusEvent::Lost)
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
        let bridge = TerminalWidgetBridge::new(terminal, gate);
        let mut rt = harbor_widget::runtime::Runtime::new();
        rt.set_root(bridge);
        rt.update(std::time::Instant::now());
        assert!(rt.focus_first_focusable());
        let _ = rt.drain_external_input();
        rt
    }

    #[test]
    fn should_scroll_viewport_when_gate_open_and_page_up_delivered() {
        // Arrange
        let terminal = headless_terminal(8, 40);
        seed_scrollback(&mut terminal.lock().unwrap());
        let gate = Arc::new(AtomicBool::new(false));
        let mut rt = runtime_with_bridge(Arc::clone(&terminal), Arc::clone(&gate));
        let offset_before = terminal.lock().unwrap().screen().view_offset();

        // Act
        let effects = rt.dispatch(
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::PageUp,
                modifiers: Modifiers::default(),
            }),
            std::time::Instant::now(),
        );

        // Assert: open-gate delivery scrolls; KeyDown invalidates paint.
        assert!(terminal.lock().unwrap().screen().view_offset() > offset_before);
        assert!(effects.request_redraw);
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
        let effects = rt.dispatch(
            UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::PageUp,
                modifiers: Modifiers::default(),
            }),
            std::time::Instant::now(),
        );

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
        let effects = rt.dispatch(
            UiEvent::Pointer(PointerEvent::new(
                Point::new(10.0, 10.0),
                PointerPhase::WheelLine { dx: 0.0, dy: 2.0 },
                PointerButton::Left,
                0,
            )),
            std::time::Instant::now(),
        );

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
        // Arrange — headless Terminal has no Cursor/renderer
        let terminal = headless_terminal(2, 4);
        let now = std::time::Instant::now();

        // Act
        let demand = schedule_demand_for_terminal(1, 1, &terminal, now);

        // Assert
        assert_eq!(demand, ExternalScheduleDemand::empty());
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

        // Arrange — fake schedule via CustomPaint-equivalent bridge registration
        // is exercised through Runtime with a scripted provider in widget tests;
        // here verify bridge root registers a schedule that headless empties.
        let terminal = headless_terminal(2, 4);
        let mut rt = runtime_with_bridge(terminal, Arc::new(AtomicBool::new(false)));
        let now = std::time::Instant::now();

        // Act — clean idle turn after initial build consumed dirty work
        let _ = rt.update(now);
        let idle = rt.update(now + Duration::from_millis(1));

        // Assert — headless demand yields no WaitUntil / no blink Poll
        assert!(!idle.request_redraw);
        assert_ne!(idle.control_flow, Some(ControlFlowEffect::Poll));
        assert!(idle.control_flow.is_none() || idle.control_flow == Some(ControlFlowEffect::Wait));
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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);

        // Act
        let idle = adapter.about_to_wait(&mut rt, now, None);

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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let _ = adapter.about_to_wait(&mut rt, now, None);

        // Act
        let due = adapter.about_to_wait(&mut rt, now + Duration::from_millis(100), None);

        // Assert — recovery live-commits without resetting nested DECRQM Set
        assert!(due.request_redraw);
        assert!(due.force_present);
        assert!(due.has_deferred_externals);
        assert_eq!(due.control_flow, Some(ControlFlowEffect::Wait));
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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);

        // Act
        let idle = adapter.about_to_wait(&mut rt, std::time::Instant::now(), None);

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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let armed = adapter.about_to_wait(&mut rt, now, None);
        assert!(armed.has_deferred_externals);
        assert_eq!(
            armed.control_flow,
            Some(ControlFlowEffect::WaitUntil(
                now + Duration::from_millis(100)
            ))
        );

        // Act
        terminal.lock().unwrap().process_output(b"\x1bc");
        let idle = adapter.about_to_wait(&mut rt, now, None);

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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let _ = adapter.about_to_wait(&mut rt, now, None);

        // Act
        terminal.lock().unwrap().process_output(b"\x1b[!p");
        let idle = adapter.about_to_wait(&mut rt, now, None);

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
        let mut adapter = WinitAdapter::with_surface(800, 600, 1.0);
        let armed = adapter.about_to_wait(&mut rt, now, None);
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
        let idle = adapter.about_to_wait(&mut rt, now, None);

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
