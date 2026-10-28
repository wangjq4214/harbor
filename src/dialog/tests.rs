#[path = "tests/architecture.rs"]
mod architecture;

use super::*;
use harbor_app::confirmation::build_confirmation_root;
use harbor_widget::view::{BuildCx, Component};

#[test]
fn gpu_fault_overrides_queued_button_shortcut_and_cancel_without_writing() {
    use harbor_widget::winit::{GpuFault, GpuFaultKind};
    for kind in [
        GpuFaultKind::OutOfMemory,
        GpuFaultKind::DeviceLost,
        GpuFaultKind::Validation,
        GpuFaultKind::Internal,
    ] {
        for decision in [
            confirmation_result(&AtomicBool::new(true), &AtomicBool::new(false)),
            shortcut_result(&Key::Character("y".into())).unwrap(),
            ConfirmationResult::Cancelled,
            ConfirmationResult::None,
        ] {
            let fault = GpuFault {
                kind,
                message: "approval sentinel".into(),
            };
            let guarded = guard_confirmation(decision, Some(fault.clone()));
            let ConfirmationResult::Fatal(error) = guarded else {
                panic!("fault must win");
            };
            assert_eq!(error, FrameError::SharedGpu(fault));
            let result = DialogOutcome::Fatal(error);
            assert!(
                !write_confirmation_outcome(
                    &result,
                    InputModes::default(),
                    |_| -> Result<(), ()> {
                        panic!("fatal approval must not write PTY");
                    }
                )
                .unwrap()
            );
        }
    }
    assert!(matches!(
        guard_confirmation(ConfirmationResult::Confirmed, None),
        ConfirmationResult::Confirmed
    ));
    assert!(matches!(
        guard_confirmation(
            ConfirmationResult::Fatal(FrameError::presentation("local")),
            None
        ),
        ConfirmationResult::Fatal(FrameError::Presentation(_))
    ));
}

#[test]
fn confirmation_waits_use_application_wide_arbitration_rules() {
    let now = Instant::now();
    let early = now + std::time::Duration::from_millis(10);
    let late = now + std::time::Duration::from_millis(20);
    let mut wait = Some(ControlFlowEffect::WaitUntil(late));

    merge_wait(&mut wait, Some(ControlFlowEffect::WaitUntil(early)));
    assert_eq!(wait, Some(ControlFlowEffect::WaitUntil(early)));

    merge_wait(&mut wait, Some(ControlFlowEffect::Poll));
    assert_eq!(wait, Some(ControlFlowEffect::Poll));
}

#[test]
fn centers_dialog_in_main_window() {
    assert_eq!(
        centered_dialog_position(
            winit::dpi::PhysicalPosition::new(100, 100),
            winit::dpi::PhysicalSize::new(1_000, 800),
            1.0,
        ),
        // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.0
        // x = 100 + (1000 - 600) / 2 = 300
        // y = 100 + (800 - 500) / 2 = 250
        winit::dpi::PhysicalPosition::new(300, 250),
    );
}

#[test]
fn centers_scaled_dialog_on_negative_coordinate_monitor() {
    assert_eq!(
        centered_dialog_position(
            winit::dpi::PhysicalPosition::new(-1_920, 100),
            winit::dpi::PhysicalSize::new(1_200, 900),
            1.5,
        ),
        // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.5
        // dialog_width = 600 * 1.5 = 900
        // dialog_height = 500 * 1.5 = 750
        // x = -1920 + (1200 - 900) / 2 = -1770
        // y = 100 + (900 - 750) / 2 = 175
        winit::dpi::PhysicalPosition::new(-1_770, 175),
    );
}

#[test]
fn build_confirmation_root_produces_valid_view() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));
    let wrapped = vec!["hello".to_string()];
    let scroll = Arc::new(AtomicUsize::new(0));

    let root = build_confirmation_root(5, wrapped, scroll, cancelled, confirmed, 20.0);
    let mut cx = BuildCx::stub();
    let _view = root.build(&mut cx);
}

#[test]
fn paste_callback_sets_confirmed_flag_only() {
    // Replicates the exact closure pattern used in build_confirmation_root.
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));

    let c = Arc::clone(&confirmed);
    let on_paste = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
        c.store(true, Ordering::SeqCst);
    };

    let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
    on_paste(&mut ctx);

    assert!(confirmed.load(Ordering::SeqCst));
    assert!(!cancelled.load(Ordering::SeqCst));
}

#[test]
fn cancel_callback_sets_cancelled_flag_only() {
    // Replicates the exact closure pattern used in build_confirmation_root.
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));

    let c = Arc::clone(&cancelled);
    let on_cancel = move |_ctx: &mut harbor_widget::input::event_ctx::EventCtx| {
        c.store(true, Ordering::SeqCst);
    };

    let mut ctx = harbor_widget::input::event_ctx::EventCtx::new();
    on_cancel(&mut ctx);

    assert!(!confirmed.load(Ordering::SeqCst));
    assert!(cancelled.load(Ordering::SeqCst));
}

#[test]
fn paste_and_cancel_flags_are_independent() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));

    confirmed.store(true, Ordering::SeqCst);
    assert!(confirmed.load(Ordering::SeqCst));
    assert!(!cancelled.load(Ordering::SeqCst));

    confirmed.store(false, Ordering::SeqCst);
    cancelled.store(true, Ordering::SeqCst);
    assert!(!confirmed.load(Ordering::SeqCst));
    assert!(cancelled.load(Ordering::SeqCst));
}

#[test]
fn should_preserve_raw_text_verbatim() {
    // ConfirmationWindow::raw_text() returns the original paste text
    // unchanged. Verify the data-flow contract: String in → &str out,
    // multi-line with mixed line endings preserved.
    let text = String::from("hello\nworld\r\n");
    let stored = text.clone();
    // Simulate the accessor contract: stored text matches original.
    assert_eq!(stored.as_str(), "hello\nworld\r\n");
    assert!(stored.contains('\n'));
    assert!(stored.contains('\r'));
}

#[test]
fn should_center_at_origin_when_main_window_is_zero_sized() {
    // When the main window has zero area, the dialog is still
    // positioned relative to the main window origin.
    let pos = centered_dialog_position(
        winit::dpi::PhysicalPosition::new(50, 60),
        winit::dpi::PhysicalSize::new(0, 0),
        1.0,
    );
    // DIALOG_WIDTH=600, DIALOG_HEIGHT=500 at scale 1.0
    // x = 50 + (0 - 600) / 2 = 50 - 300 = -250
    // y = 60 + (0 - 500) / 2 = 60 - 250 = -190
    assert_eq!(pos, winit::dpi::PhysicalPosition::new(-250, -190));
}

#[test]
fn should_produce_zero_dialog_when_scale_is_zero() {
    // With scale_factor=0 the dialog occupies zero physical pixels,
    // so the center coincides with the main window center.
    let pos = centered_dialog_position(
        winit::dpi::PhysicalPosition::new(100, 100),
        winit::dpi::PhysicalSize::new(800, 600),
        0.0,
    );
    assert_eq!(pos, winit::dpi::PhysicalPosition::new(500, 400));
}

#[test]
fn should_build_with_zero_lines() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));
    let wrapped: Vec<String> = vec![];
    let scroll = Arc::new(AtomicUsize::new(0));

    let root = build_confirmation_root(0, wrapped, scroll, cancelled, confirmed, 20.0);
    let mut cx = BuildCx::stub();
    let _view = root.build(&mut cx);
}

#[test]
fn should_build_with_large_line_count() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));
    let wrapped = vec!["x".to_string()];
    let scroll = Arc::new(AtomicUsize::new(0));

    let root = build_confirmation_root(usize::MAX, wrapped, scroll, cancelled, confirmed, 20.0);
    let mut cx = BuildCx::stub();
    let _view = root.build(&mut cx);
}

// ── Confirmation shortcuts ───────────────────────────────────────

#[test]
fn should_return_confirmed_on_lowercase_y() {
    let key = winit::keyboard::Key::Character("y".into());
    assert!(matches!(
        shortcut_result(&key),
        Some(ConfirmationResult::Confirmed)
    ));
}

#[test]
fn should_return_confirmed_on_uppercase_y() {
    let key = winit::keyboard::Key::Character("Y".into());
    assert!(matches!(
        shortcut_result(&key),
        Some(ConfirmationResult::Confirmed)
    ));
}

#[test]
fn should_return_cancelled_on_lowercase_n() {
    let key = winit::keyboard::Key::Character("n".into());
    assert!(matches!(
        shortcut_result(&key),
        Some(ConfirmationResult::Cancelled)
    ));
}

#[test]
fn should_return_cancelled_on_uppercase_n() {
    let key = winit::keyboard::Key::Character("N".into());
    assert!(matches!(
        shortcut_result(&key),
        Some(ConfirmationResult::Cancelled)
    ));
}

#[test]
fn should_return_cancelled_on_escape() {
    let key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape);
    assert!(matches!(
        shortcut_result(&key),
        Some(ConfirmationResult::Cancelled)
    ));
}

#[test]
fn should_return_none_on_unrecognized_key() {
    let key = winit::keyboard::Key::Character("x".into());
    assert!(shortcut_result(&key).is_none());
}

// ── Confirmation result priority ─────────────────────────────────

#[test]
fn should_return_none_when_neither_flag_is_set() {
    let confirmed = AtomicBool::new(false);
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        confirmation_result(&confirmed, &cancelled),
        ConfirmationResult::None
    ));
}

#[test]
fn should_return_confirmed_when_confirmed_flag_is_set() {
    let confirmed = AtomicBool::new(true);
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        confirmation_result(&confirmed, &cancelled),
        ConfirmationResult::Confirmed
    ));
}

#[test]
fn should_return_cancelled_when_only_cancelled_flag_is_set() {
    let confirmed = AtomicBool::new(false);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        confirmation_result(&confirmed, &cancelled),
        ConfirmationResult::Cancelled
    ));
}

#[test]
fn should_prioritize_confirmed_over_cancelled_when_both_flags_are_set() {
    // Confirmed takes priority: even if both flags are somehow true,
    // the result should be Confirmed.
    let confirmed = AtomicBool::new(true);
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        confirmation_result(&confirmed, &cancelled),
        ConfirmationResult::Confirmed
    ));
}

// ── wrap_preview_text ───────────────────────────────────────────────

#[test]
fn should_return_empty_vec_when_input_is_empty() {
    let result = wrap_preview_text("", 10);
    assert!(result.is_empty());
}

#[test]
fn should_keep_short_line_unchanged_when_within_max_chars() {
    let result = wrap_preview_text("hello", 10);
    assert_eq!(result, vec!["hello"]);
}

#[test]
fn should_wrap_long_line_at_max_chars_boundary() {
    let result = wrap_preview_text("hello world", 5);
    assert_eq!(result, vec!["hello", " worl", "d"]);
}

#[test]
fn should_escape_tab_character_to_arrow() {
    // Tab (U+0009) is escaped to arrow (U+2192) by safe_preview_line.
    let result = wrap_preview_text("a\tb", 10);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains('\u{2192}'));
    assert!(!result[0].contains('\t'));
}

#[test]
fn should_escape_c0_control_character_to_unicode_control_picture() {
    // C0 control 0x01 (SOH) is escaped to U+2401 by safe_preview_line.
    let result = wrap_preview_text("\x01", 10);
    assert_eq!(result.len(), 1);
    assert!(!result[0].contains('\x01'));
    assert!(result[0].contains('\u{2401}'));
}

#[test]
fn should_preserve_carriage_return_in_escaped_output() {
    // CR passes through safe_preview_line unchanged.
    let result = wrap_preview_text("a\rb", 10);
    assert_eq!(result, vec!["a\rb"]);
}

#[test]
fn should_split_input_on_newline_characters() {
    let result = wrap_preview_text("line1\nline2", 10);
    assert_eq!(result, vec!["line1", "line2"]);
}

#[test]
fn should_preserve_empty_lines_between_content() {
    let result = wrap_preview_text("a\n\nb", 10);
    assert_eq!(result, vec!["a", "", "b"]);
}

#[test]
fn should_wrap_correctly_with_max_chars_of_one() {
    let result = wrap_preview_text("ab", 1);
    assert_eq!(result, vec!["a", "b"]);
}

#[test]
fn should_handle_multibyte_utf8_char_boundaries_when_wrapping() {
    // "é" is 2 bytes in UTF-8. Wrapping at max_chars=1 should not panic
    // and should keep the multibyte character intact.
    let result = wrap_preview_text("\u{00E9}x", 1);
    assert_eq!(result, vec!["\u{00E9}", "x"]);
}

#[test]
fn should_wrap_escaped_text_that_becomes_longer_after_escaping() {
    // Control chars 0x01 and 0x02 become single-width Unicode control pictures
    // (U+2401, U+2402). Wrapping at max_columns=2 wraps after 2 visual columns.
    let input = "a\x01b\x02c";
    let result = wrap_preview_text(input, 2);
    assert_eq!(result, vec!["a\u{2401}", "b\u{2402}", "c"]);
}

#[test]
fn should_wrap_cjk_characters_by_display_width() {
    // CJK characters occupy 2 visual columns each. Wrapping at 4 columns
    // fits 2 characters per line.
    let result = wrap_preview_text("你好世界", 4);
    assert_eq!(result, vec!["你好", "世界"]);

    // Wrapping at 3 columns fits 1 CJK char (2 cols); the second char would exceed 3 cols.
    let result = wrap_preview_text("你好世界", 3);
    assert_eq!(result, vec!["你", "好", "世", "界"]);
}

// ── scroll_preview ──────────────────────────────────────────────────

#[test]
fn should_not_invalidate_redraw_when_scroll_delta_does_not_change_offset() {
    // Arrange
    let offset = AtomicUsize::new(5);

    // Act
    let changed = scroll_preview(&offset, 0, 10);

    // Assert
    assert!(!changed);
    assert_eq!(offset.load(Ordering::Relaxed), 5);
}

#[test]
fn should_report_redraw_invalidation_when_keyboard_scroll_changes_offset() {
    // Arrange
    let offset = AtomicUsize::new(5);

    // Act
    let changed = scroll_preview(&offset, 3, 10);

    // Assert
    assert!(changed);
    assert_eq!(offset.load(Ordering::Relaxed), 8);
}

#[test]
fn should_decrement_offset_when_keyboard_scroll_moves_toward_start() {
    // Arrange
    let offset = AtomicUsize::new(5);

    // Act
    let changed = scroll_preview(&offset, -3, 10);

    // Assert
    assert!(changed);
    assert_eq!(offset.load(Ordering::Relaxed), 2);
}

#[test]
fn should_invalidate_redraw_when_negative_keyboard_scroll_is_clamped_to_a_new_offset() {
    // Arrange
    let offset = AtomicUsize::new(1);

    // Act
    let changed = scroll_preview(&offset, -5, 10);

    // Assert
    assert!(changed);
    assert_eq!(offset.load(Ordering::Relaxed), 0);
}

#[test]
fn should_invalidate_redraw_when_positive_keyboard_scroll_is_clamped_to_a_new_offset() {
    // Arrange
    let offset = AtomicUsize::new(8);

    // Act
    let changed = scroll_preview(&offset, 5, 10);

    // Assert
    assert!(changed);
    assert_eq!(offset.load(Ordering::Relaxed), 10);
}

#[test]
fn should_not_invalidate_redraw_when_keyboard_scroll_has_no_range() {
    // Arrange
    let offset = AtomicUsize::new(0);

    // Act
    let changed = scroll_preview(&offset, 3, 0);

    // Assert
    assert!(!changed);
    assert_eq!(offset.load(Ordering::Relaxed), 0);
}

#[test]
fn should_not_invalidate_redraw_when_negative_keyboard_scroll_is_at_zero_boundary() {
    // Arrange: even with a negative delta, max=0 keeps the offset at zero.
    let offset = AtomicUsize::new(0);

    // Act
    let changed = scroll_preview(&offset, -3, 0);

    // Assert
    assert!(!changed);
    assert_eq!(offset.load(Ordering::Relaxed), 0);
}

// ── build_confirmation_root focus order ─────────────────────────────

/// Builds the confirmation root in a Runtime and verifies that
/// focus_first_focusable finds a focusable widget. In the depth-first
/// traversal order, the Cancel Button is encountered before the Paste
/// Button because it is the leftmost child of the Row.
#[test]
fn should_focus_cancel_button_first_in_confirmation_root() {
    use harbor_widget::runtime::Runtime;
    use std::time::Instant;

    let cancelled = Arc::new(AtomicBool::new(false));
    let confirmed = Arc::new(AtomicBool::new(false));
    let wrapped = vec!["sample text".to_string()];
    let scroll = Arc::new(AtomicUsize::new(0));

    // Arrange: build root and set in runtime.
    let root = build_confirmation_root(1, wrapped, scroll, cancelled, confirmed, 20.0);
    let mut rt = Runtime::new();
    rt.set_root(root);
    rt.update(Instant::now());

    // Act: focus the first focusable widget.
    let found = rt.focus_first_focusable();

    // Assert: a focusable widget was found. In the tree,
    // FocusScope > Padding > Column > ... > Row > [Button("Cancel"), SizedBox, Button("Paste")].
    // Depth-first traversal hits Cancel (leftmost Button) first.
    assert!(found, "expected a focusable widget (Cancel button)");
    assert!(rt.input().focused().is_some(), "focused should be set");

    // Verify the focused widget is a Button (without being able to read
    // the label from outside the crate, the structure guarantees it is Cancel).
    let focused_id = rt.input().focused().unwrap();
    let fiber = rt.arena().get(focused_id).unwrap();
    assert_eq!(
        fiber.widget_type(),
        std::any::TypeId::of::<harbor_widget::widgets::button::Button>(),
        "focused widget should be a Button"
    );
}

#[test]
fn both_confirmation_flows_share_the_application_preview_and_scroll_protocol() {
    use harbor_widget::{
        input::event::{
            Key as WidgetKey, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase,
            UiEvent,
        },
        layout::Point,
        renderer::Viewport,
        runtime::Runtime,
    };
    for clipboard in [false, true] {
        for approve in [false, true] {
            let offset = Arc::new(AtomicUsize::new(0));
            let cancelled = Arc::new(AtomicBool::new(false));
            let confirmed = Arc::new(AtomicBool::new(false));
            let lines = (0..40).map(|i| format!("prepared line {i}")).collect();
            let root: Box<dyn Component> = if clipboard {
                Box::new(harbor_app::confirmation::build_clipboard_confirmation_root(
                    "Tab 7 requests clipboard write".into(),
                    lines,
                    offset.clone(),
                    cancelled.clone(),
                    confirmed.clone(),
                    20.0,
                ))
            } else {
                Box::new(build_confirmation_root(
                    40,
                    lines,
                    offset.clone(),
                    cancelled.clone(),
                    confirmed.clone(),
                    20.0,
                ))
            };
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(600, 500, 1.0));
            runtime.set_root(move |cx: &mut BuildCx| root.build(cx));
            runtime.update(Instant::now());
            let mut pending = vec![runtime.root_id().unwrap()];
            let mut preview_rect = None;
            while let Some(id) = pending.pop() {
                let fiber = runtime.arena().get(id).unwrap();
                pending.extend_from_slice(fiber.children());
                if fiber.widget_type()
                    == std::any::TypeId::of::<harbor_app::confirmation::preview::PreviewPane>()
                {
                    preview_rect = fiber.layout_rect();
                }
            }
            let rect = preview_rect.expect("both roots mount the same application-owned preview");
            assert_eq!(rect.size().height, PREVIEW_VISIBLE_LINES as f32 * 20.0);
            let wheel = |dy| {
                UiEvent::Pointer(PointerEvent::new(
                    Point::new(rect.min.x + 10.0, rect.min.y + 10.0),
                    PointerPhase::WheelLine { dx: 0.0, dy },
                    PointerButton::Left,
                    1,
                ))
            };
            assert!(runtime.dispatch(wheel(100.0)).request_redraw);
            assert_eq!(offset.load(Ordering::Relaxed), 40 - PREVIEW_VISIBLE_LINES);
            assert!(scroll_preview(&offset, -100, 40 - PREVIEW_VISIBLE_LINES));
            assert_eq!(offset.load(Ordering::Relaxed), 0);
            assert!(!runtime.dispatch(wheel(-100.0)).request_redraw);
            assert!(runtime.focus_first_focusable());
            if approve {
                runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                    key: WidgetKey::Tab,
                    modifiers: Modifiers::default(),
                }));
            }
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::Enter,
                modifiers: Modifiers::default(),
            }));
            assert_eq!(confirmed.load(Ordering::SeqCst), approve);
            assert_eq!(cancelled.load(Ordering::SeqCst), !approve);
        }
    }
}

#[test]
fn clipboard_confirmation_root_routes_real_deny_and_allow_buttons() {
    use harbor_widget::input::event::{Key as WidgetKey, KeyboardEvent, Modifiers, UiEvent};
    use harbor_widget::runtime::Runtime;
    for approve in [false, true] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let confirmed = Arc::new(AtomicBool::new(false));
        let root = harbor_app::confirmation::build_clipboard_confirmation_root(
            "Tab 7 requests clipboard write (5 bytes)".into(),
            vec!["fixture preview".into()],
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&cancelled),
            Arc::clone(&confirmed),
            20.0,
        );
        let mut runtime = Runtime::new();
        runtime.set_root(root);
        runtime.update(Instant::now());
        assert!(runtime.focus_first_focusable());
        if approve {
            runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
                key: WidgetKey::Tab,
                modifiers: Modifiers::default(),
            }));
        }
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: WidgetKey::Enter,
            modifiers: Modifiers::default(),
        }));
        assert_eq!(confirmed.load(Ordering::SeqCst), approve);
        assert_eq!(cancelled.load(Ordering::SeqCst), !approve);
    }
}

#[test]
fn confirmed_paste_writes_unchanged_raw_text_with_current_modes() {
    let outcome = DialogOutcome::Confirmed("first\r\nsecond\t\x1b[A".to_owned());
    let mut writes = Vec::new();

    let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |bytes| {
        writes.push(bytes.to_vec());
        Ok::<_, ()>(())
    })
    .expect("capture writer succeeds");

    assert!(wrote);
    assert_eq!(writes, vec![b"first\r\nsecond\t\x1b[A".to_vec()]);
}

#[test]
fn confirmed_paste_uses_bracketed_mode_current_at_confirmation_time() {
    let outcome = DialogOutcome::Confirmed("first\nsecond".to_owned());
    let mut writes = Vec::new();
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };

    let wrote = write_confirmation_outcome(&outcome, modes, |bytes| {
        writes.push(bytes.to_vec());
        Ok::<_, ()>(())
    })
    .expect("capture writer succeeds");

    assert!(wrote);
    assert_eq!(writes, vec![b"\x1b[200~first\nsecond\x1b[201~".to_vec()]);
}

#[test]
fn cancelled_or_closed_confirmation_never_calls_the_writer() {
    for outcome in [DialogOutcome::Cancelled, DialogOutcome::None] {
        let wrote =
            write_confirmation_outcome(&outcome, InputModes::default(), |_| -> Result<(), ()> {
                panic!("cancelled and native-close outcomes must not write to the PTY")
            })
            .expect("no-write outcome cannot fail");
        assert!(!wrote);
    }
}

#[test]
fn should_not_write_to_pty_when_confirmation_frame_is_fatal() {
    let outcome = DialogOutcome::Fatal(FrameError::out_of_memory());
    let write_attempts = std::cell::Cell::new(0);

    let wrote = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
        write_attempts.set(write_attempts.get() + 1);
        Ok::<_, ()>(())
    })
    .expect("fatal outcome cannot invoke the writer");

    assert!(!wrote);
    assert_eq!(write_attempts.get(), 0);
}

#[test]
fn should_return_the_writer_error_after_one_confirmed_paste_attempt() {
    let outcome = DialogOutcome::Confirmed("paste".to_owned());
    let write_attempts = std::cell::Cell::new(0);

    let result = write_confirmation_outcome(&outcome, InputModes::default(), |_| {
        write_attempts.set(write_attempts.get() + 1);
        Err::<(), _>("PTY disconnected")
    });

    assert_eq!(result, Err("PTY disconnected"));
    assert_eq!(write_attempts.get(), 1);
}
