//! Mounted palette → sole FIFO → Host reduction seams; native windows remain T0003.
use super::*;
use crate::command::{CommandCompletion, command_registry, palette_command_supported};

fn submit(
    controller: &WorkspaceUiController,
    runtime: &mut Runtime,
    id: &str,
) -> AppCommandRequest {
    open(controller, runtime);
    replace(runtime, id);
    down(runtime, Key::Enter, Modifiers::default());
    down(runtime, Key::Enter, Modifiers::default());
    take_one(controller)
}

fn terminal_destination(controller: &WorkspaceUiController, runtime: &mut Runtime) {
    runtime.clear_focus();
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    up(runtime, Key::Enter, Modifiers::default());
}

#[test]
fn full_visible_registry_is_discoverable_with_effective_bindings_hidden_intents_absent() {
    let (_, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let registry = command_registry();
    for entry in &registry {
        replace(&mut runtime, entry.id);
        assert_eq!(
            palette_command_supported(entry.command),
            entry.palette_visible
        );
        let text = painted(&runtime);
        if entry.palette_visible {
            assert!(text.contains(entry.id), "{}", entry.id);
            assert!(text.contains(entry.label), "{}", entry.id);
            assert!(!text.contains("Not yet integrated"));
        } else {
            assert!(text.contains("No matching commands"), "{}", entry.id);
        }
    }
    for command in [
        AppCommand::SearchChanged,
        AppCommand::CloseCommandPalette(1),
        AppCommand::DismissNotification(1),
    ] {
        assert!(!palette_command_supported(command));
    }
    replace(&mut runtime, "terminal.paste");
    assert!(painted(&runtime).contains("Ctrl+V"));
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert("terminal.paste".into(), vec![]);
    controller.publish_keybindings(validate_keybindings(&raw).unwrap());
    runtime.update(Instant::now());
    assert!(painted(&runtime).contains("Unbound"));
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn palette_copy_always_copies_empty_once_then_ctrl_c_still_interrupts() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    let request = submit(&controller, &mut runtime, "terminal.copy");
    assert_eq!(request.command, AppCommand::Copy);
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    let outcome = tabs.dispatch_terminal_command(request.command).unwrap();
    assert_eq!(outcome.clipboard_text.as_deref(), Some(""));
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    terminal_destination(&controller, &mut runtime);
    assert!(bytes.lock().unwrap().is_empty());
    down(&mut runtime, Key::Character('c'), ctrl());
    up(&mut runtime, Key::Character('c'), ctrl());
    assert!(controller.drain_actions().is_empty());
    assert_eq!(&*bytes.lock().unwrap(), b"\x03");
}

#[test]
fn primary_scroll_commands_reduce_once_without_pty_bytes_and_report_real_no_change() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    let terminal = tabs.active_terminal().unwrap();
    terminal.lock().unwrap().put_str(&"history\r\n".repeat(40));
    for (id, at_bottom) in [
        ("terminal.page-up", false),
        ("terminal.scroll-to-top", false),
        ("terminal.page-down", false),
        ("terminal.scroll-to-bottom", true),
        ("terminal.scroll-to-bottom", true),
    ] {
        let request = submit(&controller, &mut runtime, id);
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        let outcome = tabs.dispatch_terminal_command(request.command).unwrap();
        let completion = if outcome.redraw {
            CommandCompletion::Applied
        } else {
            CommandCompletion::Unchanged
        };
        controller.report_palette_completion(request.command, Ok(completion));
        assert!(
            controller
                .take_palette_request(request, &tabs, false)
                .is_err()
        );
        terminal_destination(&controller, &mut runtime);
        assert_eq!(
            terminal.lock().unwrap().screen().view_offset() == 0,
            at_bottom
        );
    }
    assert!(
        controller
            .store
            .state()
            .read()
            .notices
            .message()
            .unwrap()
            .contains("No change")
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn alternate_scroll_is_disabled_rechecked_at_submit_and_drain_then_shortcuts_pass_through() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    let terminal = tabs.active_terminal().unwrap();
    open(&controller, &mut runtime);
    terminal.lock().unwrap().process_output(b"\x1b[?1049h");
    for id in [
        "terminal.page-up",
        "terminal.page-down",
        "terminal.scroll-to-top",
        "terminal.scroll-to-bottom",
    ] {
        replace(&mut runtime, id);
        assert!(
            painted(&runtime).contains("alternate screen"),
            "id={id} text={}",
            painted(&runtime)
        );
        tap(&mut runtime, Key::Enter);
        assert!(controller.drain_actions().is_empty());
    }
    terminal.lock().unwrap().process_output(b"\x1b[?1049l");
    replace(&mut runtime, "terminal.page-up");
    // Enabled rendered row becomes unavailable before submission.
    terminal.lock().unwrap().process_output(b"\x1b[?1049h");
    tap(&mut runtime, Key::Enter);
    assert!(controller.drain_actions().is_empty());
    terminal.lock().unwrap().process_output(b"\x1b[?1049l");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&controller);
    terminal.lock().unwrap().process_output(b"\x1b[?1049h");
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap_err()
            .contains("alternate screen")
    );
    terminal_destination(&controller, &mut runtime);
    assert!(bytes.lock().unwrap().is_empty());
    tap(&mut runtime, Key::PageUp);
    assert!(controller.drain_actions().is_empty());
    assert_eq!(&*bytes.lock().unwrap(), b"\x1b[5~");
}

#[test]
fn palette_search_reuses_query_navigates_matches_and_honors_destination_after_teardown() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("needle\r\nneedle\r\nother\r\nlive\r\n");
    let request = submit(&controller, &mut runtime, "terminal.search");
    assert_eq!(request.command, AppCommand::SearchOpen);
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    runtime.clear_focus();
    let outcome = tabs.execute_search_command(request.command, &controller);
    assert_eq!(outcome.completion, Ok(CommandCompletion::Applied));
    let focus = outcome
        .focus_for_source(request.source, &controller)
        .unwrap();
    assert_eq!(focus, controller.search_focus());
    runtime.request_focus(&focus); // same pre-reconciliation order as Host
    runtime.update(Instant::now());
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert_eq!(
        tabs.execute_search_command(AppCommand::SearchNext, &controller)
            .completion,
        Ok(CommandCompletion::Unchanged)
    );
    replace(&mut runtime, "needle");
    assert_eq!(controller.search_query().unwrap().1, "needle");
    for action in controller.drain_actions() {
        assert_eq!(action.command, AppCommand::SearchChanged);
        tabs.execute_search_command(action.command, &controller);
    }
    for _ in 0..100 {
        if !tabs.advance_search().pending {
            break;
        }
    }
    assert_eq!(tabs.active_bridge().unwrap().decoration_ranges().len(), 2);
    for id in [
        "terminal.search-next",
        "terminal.search-previous",
        "terminal.search",
    ] {
        let request = submit(&controller, &mut runtime, id);
        controller
            .take_palette_request(request, &tabs, false)
            .unwrap();
        runtime.clear_focus();
        let outcome = tabs.execute_search_command(request.command, &controller);
        assert_eq!(outcome.completion, Ok(CommandCompletion::Applied));
        assert_eq!(
            outcome.focus_for_source(request.source, &controller),
            Some(controller.search_focus())
        );
        if request.command != AppCommand::SearchOpen {
            assert_eq!(
                outcome.focus_for_source(CommandSource::Direct, &controller),
                None
            );
        }
        runtime.request_focus(
            &outcome
                .focus_for_source(request.source, &controller)
                .unwrap(),
        );
        runtime.update(Instant::now());
        up(&mut runtime, Key::Enter, Modifiers::default());
        assert_eq!(controller.search_query().unwrap().1, "needle");
    }
    let request = submit(&controller, &mut runtime, "terminal.search-close");
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    let outcome = tabs.execute_search_command(request.command, &controller);
    assert_eq!(
        outcome.focus_for_source(request.source, &controller),
        Some(controller.terminal_focus())
    );
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert!(controller.search_query().is_none());
    assert!(tabs.active_bridge().unwrap().decoration_ranges().is_empty());
    assert!(bytes.lock().unwrap().is_empty());
    tap(&mut runtime, Key::Character('x'));
    assert_eq!(&*bytes.lock().unwrap(), b"x");
}

#[test]
fn palette_closed_search_and_changed_search_context_reject_without_leaking() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    for id in [
        "terminal.search-close",
        "terminal.search-next",
        "terminal.search-previous",
    ] {
        replace(&mut runtime, id);
        assert!(painted(&runtime).contains("Search is closed"));
        tap(&mut runtime, Key::Enter);
        assert!(controller.drain_actions().is_empty());
    }
    controller.open_search(TabId(999));
    replace(&mut runtime, "terminal.search-next");
    assert!(painted(&runtime).contains("Search is closed"));
    tap(&mut runtime, Key::Enter);
    assert!(controller.drain_actions().is_empty());
    controller.open_search(tabs.active_id().unwrap());
    replace(&mut runtime, "terminal.search-next");
    tap(&mut runtime, Key::Enter);
    assert_eq!(input(&controller).text(), "terminal.search-next");
    let request = take_one(&controller);
    controller.close_search();
    assert_eq!(
        controller.take_palette_request(request, &tabs, false),
        Err("Search is closed")
    );
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn confirmation_gate_focus_loss_and_removed_target_expire_pending_palette_commands() {
    for reason in ["gate", "focus", "removal", "root"] {
        let (mut tabs, controller, mut runtime, bytes) = fixture();
        let request = submit(&controller, &mut runtime, "terminal.paste");
        let old_editor = input(&controller);
        match reason {
            "focus" => {
                runtime.dispatch(UiEvent::Focus(FocusEvent::Lost));
                assert!(controller.cancel_palette()); // native Host focus-loss hook
            }
            "root" => {
                runtime.set_root(tab_workspace(controller.clone(), false));
                runtime.update(Instant::now());
            }
            "removal" => {
                tabs.execute_command(TabCommand::CloseActive, |_, _| panic!("no creation"))
                    .unwrap();
            }
            _ => {}
        }
        assert!(
            controller
                .take_palette_request(request, &tabs, reason == "gate")
                .is_err()
        );
        assert!(!controller.palette_is_open());
        assert_eq!(old_editor.text(), "");
        assert!(
            controller
                .take_palette_request(request, &tabs, false)
                .is_err()
        );
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn completion_feedback_distinguishes_execution_no_change_confirmation_rejection_and_failure() {
    let (_, controller, _, _) = fixture();
    for (result, expected) in [
        (Ok(CommandCompletion::Applied), "Executed:"),
        (Ok(CommandCompletion::Unchanged), "No change:"),
        (
            Ok(CommandCompletion::PendingConfirmation),
            "Awaiting confirmation:",
        ),
        (Err("Rejected: context changed"), "Rejected:"),
        (Err("Failed: clipboard unavailable"), "Failed:"),
    ] {
        controller.report_palette_completion(AppCommand::Paste, result);
        assert!(
            controller
                .store
                .state()
                .read()
                .notices
                .message()
                .unwrap()
                .contains(expected)
        );
    }
}

#[test]
fn query_clipboard_editing_does_not_copy_or_clear_terminal_selection() {
    use harbor_terminal::{
        RenderTarget, TerminalEvent, TerminalPointerButton, TerminalPointerEvent,
        TerminalPointerPhase, TextMetrics,
    };
    let (tabs, controller, mut runtime, bytes) = fixture();
    let terminal = tabs.active_terminal().unwrap();
    {
        let mut terminal = terminal.lock().unwrap();
        let (viewport, _) = terminal.prepare_render_frame(
            RenderTarget::new((0.0, 0.0), (220, 100), (220, 100)),
            &TextMetrics {
                cell_width: 10.0,
                line_height: 20.0,
                ascent: 16.0,
                underline_position: 16.0,
                underline_thickness: 2.0,
                strikethrough_position: 10.0,
                strikethrough_thickness: 2.0,
            },
            Instant::now(),
        );
        terminal.put_str("selection");
        for phase in [
            TerminalPointerPhase::Down,
            TerminalPointerPhase::Move,
            TerminalPointerPhase::Up,
        ] {
            let col = if phase == TerminalPointerPhase::Down {
                0
            } else {
                2
            };
            let (x, y) = viewport.cell_pos(0, col);
            terminal
                .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                    (x + 1.0, y + 1.0),
                    phase,
                    TerminalPointerButton::Left,
                    80,
                )))
                .unwrap();
        }
        assert_eq!(terminal.selection_text(), "sel");
    }
    open(&controller, &mut runtime);
    replace(&mut runtime, "query");
    down(&mut runtime, Key::Character('a'), ctrl());
    up(&mut runtime, Key::Character('a'), ctrl());
    for key in ['c', 'x', 'v'] {
        let effect = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character(key),
            modifiers: ctrl(),
        }));
        let Some(ClipboardEffect::Editor(request)) = effect.clipboard else {
            panic!("editor clipboard")
        };
        runtime.complete_clipboard(
            request.id,
            Ok(if key == 'v' {
                "query".into()
            } else {
                String::new()
            }),
        );
        up(&mut runtime, Key::Character(key), ctrl());
        assert!(controller.drain_actions().is_empty());
        assert_eq!(terminal.lock().unwrap().selection_text(), "sel");
    }
    replace(&mut runtime, "terminal.copy");
    down(&mut runtime, Key::Enter, Modifiers::default());
    let request = take_one(&controller);
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    let outcome = tabs.dispatch_terminal_command(request.command).unwrap();
    assert_eq!(outcome.clipboard_text.as_deref(), Some("sel"));
    assert!(!terminal.lock().unwrap().has_non_empty_selection());
    terminal_destination(&controller, &mut runtime);
    assert!(bytes.lock().unwrap().is_empty());
}

#[test]
fn pending_request_expires_on_root_replacement_even_after_reopen_without_an_intermediate_frame() {
    let (tabs, controller, mut runtime, bytes) = fixture();
    open(&controller, &mut runtime);
    let old = input(&controller);
    controller.cancel_palette();
    runtime.request_focus(&controller.open_palette());
    replace(&mut runtime, "terminal.copy");
    down(&mut runtime, Key::Enter, Modifiers::default());
    let request = take_one(&controller);
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    assert!(
        controller
            .take_palette_request(request, &tabs, false)
            .is_err()
    );
    assert_eq!(old.text(), "");
    assert!(bytes.lock().unwrap().is_empty());
}
