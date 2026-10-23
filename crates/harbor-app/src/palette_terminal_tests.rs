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
    let text = painted(&runtime);
    assert!(text.contains("Ctrl\nV"), "{text:?}");
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
    // No-change scrolls are silent: no notice is published for a successful no-op.
    assert_eq!(controller.store.state().read().notices.message(), None);
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
        let (_, row) = palette_geometry(&runtime);
        let text = row_text(
            &runtime,
            row,
            harbor_widget::Theme::default().button.disabled.foreground,
        );
        assert!(text.iter().all(|text| !text.contains("alternate screen")));
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
fn palette_replaces_search_clears_results_and_honors_fresh_search_destination() {
    let (mut tabs, controller, mut runtime, bytes) = fixture();
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("needle\r\nneedle\r\nother\r\nlive\r\n");
    let request = submit(&controller, &mut runtime, "terminal.search");
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    runtime.clear_focus();
    let outcome = tabs.execute_search_command(request.command, &controller);
    let focus = outcome
        .focus_for_source(request.source, &controller)
        .unwrap();
    assert_eq!(focus, controller.search_focus());
    runtime.request_focus(&focus); // Host requests the destination before reconciliation.
    runtime.update(Instant::now());
    up(&mut runtime, Key::Enter, Modifiers::default());
    replace(&mut runtime, "needle");
    let old_editor = controller
        .store
        .state()
        .read()
        .popups
        .get::<SearchUiState>()
        .unwrap()
        .input
        .clone();
    for action in controller.drain_actions() {
        assert!(controller.popup_request_is_current(action));
        assert_eq!(action.command, AppCommand::SearchChanged);
        tabs.execute_search_command(action.command, &controller);
    }
    for _ in 0..100 {
        if !tabs.advance_search().pending {
            break;
        }
    }
    assert_eq!(tabs.active_bridge().unwrap().decoration_ranges().len(), 2);
    assert!(tabs.navigate_search(false));

    open(&controller, &mut runtime);
    assert!(controller.search_query().is_none());
    assert_eq!(old_editor.text(), "");
    assert!(
        tabs.active_bridge().unwrap().decoration_ranges().is_empty(),
        "retirement clears highlights before another Host search tick"
    );
    assert_eq!(tabs.search_status(), "No Result");
    assert!(!tabs.navigate_search(false));
    for id in [
        "terminal.search-next",
        "terminal.search-previous",
        "terminal.search-close",
    ] {
        replace(&mut runtime, id);
        let (_, row) = palette_geometry(&runtime);
        let text = row_text(
            &runtime,
            row,
            harbor_widget::Theme::default().button.disabled.foreground,
        );
        assert!(text.iter().all(|text| !text.contains("Search is closed")));
        tap(&mut runtime, Key::Enter);
        assert!(controller.drain_actions().is_empty());
    }

    replace(&mut runtime, "terminal.search");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&controller);
    controller
        .take_palette_request(request, &tabs, false)
        .unwrap();
    runtime.clear_focus();
    let outcome = tabs.execute_search_command(request.command, &controller);
    assert_eq!(outcome.completion, Ok(CommandCompletion::Applied));
    runtime.request_focus(
        &outcome
            .focus_for_source(request.source, &controller)
            .unwrap(),
    );
    runtime.update(Instant::now());
    assert!(!controller.palette_is_open());
    assert_eq!(controller.search_query().unwrap().1, "");
    assert!(painted(&runtime).contains("No Result"));
    up(&mut runtime, Key::Enter, Modifiers::default());
    assert!(!tabs.navigate_search(false));

    tap(&mut runtime, Key::Escape);
    let request = take_one(&controller);
    assert!(controller.popup_request_is_current(request));
    let outcome = tabs.execute_search_command(request.command, &controller);
    assert_eq!(outcome.focus, Some(controller.terminal_focus()));
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    assert!(!controller.popup_is_open());
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
        let (_, row) = palette_geometry(&runtime);
        let text = row_text(
            &runtime,
            row,
            harbor_widget::Theme::default().button.disabled.foreground,
        );
        assert!(text.iter().all(|text| !text.contains("Search is closed")));
        tap(&mut runtime, Key::Enter);
        assert!(controller.drain_actions().is_empty());
    }
    // Replacing a submitted palette with search expires that request, even if
    // both transitions happen before a frame; it must not tear down the search.
    replace(&mut runtime, "terminal.search");
    tap(&mut runtime, Key::Enter);
    let request = take_one(&controller);
    controller.open_search(tabs.active_id().unwrap());
    assert_eq!(
        controller.take_palette_request(request, &tabs, false),
        Err("Palette is closed")
    );
    assert!(controller.search_query().is_some());
    runtime.request_focus(&controller.search_focus());
    runtime.update(Instant::now());
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
fn completion_feedback_notifies_only_failures_and_retires_stale_command_failure() {
    let (_, controller, _, _) = fixture();
    let message = || controller.store.state().read().notices.message();
    for result in [
        Ok(CommandCompletion::Applied),
        Ok(CommandCompletion::Unchanged),
        Ok(CommandCompletion::PendingConfirmation),
    ] {
        controller.report_palette_completion(AppCommand::Paste, result);
        assert_eq!(message(), None, "{result:?}");
    }
    for (result, expected) in [
        (Err("Rejected: context changed"), "Rejected:"),
        (Err("Failed: clipboard unavailable"), "Failed:"),
        (Err("Cancelled: paste was not confirmed"), "Cancelled:"),
    ] {
        controller.report_palette_completion(AppCommand::Paste, result);
        let notice = message().unwrap();
        assert!(notice.contains(expected), "{notice:?}");
        assert!(notice.contains("Paste"), "{notice:?}");
    }
    // A later success retires the command failure...
    controller.report_palette_completion(AppCommand::Paste, Ok(CommandCompletion::Applied));
    assert_eq!(message(), None);
    // ...but never erases an unrelated resource/reload failure.
    controller.publish_configuration_notice(ConfigurationNoticeSource::Resource, Some("P".into()));
    controller.report_palette_completion(AppCommand::Paste, Err("Failed: clipboard unavailable"));
    controller.report_palette_completion(AppCommand::Paste, Ok(CommandCompletion::Unchanged));
    assert_eq!(message().as_deref(), Some("P"));
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
