use super::*;

#[test]
fn search_editor_publishes_initial_no_result_instead_of_an_immediate_busy_hint() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    controller.publish_search_status("1 / 3");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("needle".into())));
    runtime.update(Instant::now());
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<SearchUiState>()
            .unwrap()
            .status,
        "No Result"
    );
    assert_eq!(controller.search_query().unwrap().1, "needle");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: ctrl(),
    }));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Backspace,
        modifiers: Modifiers::default(),
    }));
    runtime.update(Instant::now());
    assert_eq!(
        controller
            .store
            .state()
            .read()
            .popups
            .get::<SearchUiState>()
            .unwrap()
            .status,
        "No Result"
    );
    assert_eq!(controller.search_query().unwrap().1, "");
}

#[test]
fn search_status_changes_do_not_resize_input_or_shift_result_and_buttons() {
    for (width, height, scale) in [
        (1000, 500, 1.0),
        (1200, 800, 1.5),
        (1200, 800, 2.0),
        (320, 240, 1.0),
    ] {
        let controller = controller(
            width as f64 / scale as f64,
            vec![snapshot(1, true, false)],
            1,
        );
        controller.open_search(TabId(1));
        let mut runtime = mount(controller.clone(), width, height);
        // Native font metrics can be wider than the headless 10px default.
        runtime.set_text_metrics(harbor_widget::text::TextMetrics {
            cell_width: 12.0,
            ..harbor_widget::runtime::DEFAULT_TEXT_METRICS
        });
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        let focused = runtime.input().focused();
        let allocation = controller.latest_terminal_allocation();
        let geometry = |runtime: &Runtime| {
            let panel = search_panel_rect(runtime);
            let rect_for = |widget_type| {
                fiber_ids(runtime)
                    .into_iter()
                    .find_map(|id| {
                        let fiber = runtime.arena().get(id).unwrap();
                        let rect = fiber.layout_rect()?;
                        (fiber.widget_type() == widget_type
                            && panel.contains(rect.min)
                            && rect.size().height < panel.size().height)
                            .then_some(rect)
                    })
                    .expect("search field/status layout")
            };
            let input = rect_for(TypeId::of::<DecoratedBox>());
            let result = rect_for(TypeId::of::<harbor_widget::widgets::text_label::TextLabel>());
            // No GPU encode consumes the initial delta, so coalescing retains the icons
            // even if a status change leaves their primitives unchanged.
            let delta = runtime.pending_delta().unwrap();
            let mut buttons: Vec<_> = delta
                .added
                .iter()
                .chain(&delta.modified)
                .filter_map(|item| match item.primitive {
                    Primitive::Icon { icon, rect, .. }
                        if panel.contains(rect.min)
                            && matches!(
                                icon,
                                LucideIcon::ChevronUp | LucideIcon::ChevronDown | LucideIcon::X
                            ) =>
                    {
                        Some(rect)
                    }
                    _ => None,
                })
                .collect();
            buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
            assert_eq!(buttons.len(), 3);
            (input, result, buttons)
        };
        let initial = geometry(&runtime);
        assert_eq!(initial.1.size().width, 120.0, "compact fixed result slot");
        assert_eq!(
            initial.0.size().width,
            search_panel_rect(&runtime).size().width - 260.0,
            "space released by the result slot belongs to the input"
        );
        for _ in 0..3 {
            for status in [
                "Searching…",
                "— / 9",
                "1 / 9",
                "9 / 10",
                "1024 / 1024",
                "partial/limited · — / 1024",
                "Searching… · 1 / 1024",
                "No Result",
            ] {
                controller.publish_search_status(status);
                runtime.update(Instant::now());
                if matches!(status, "No Result" | "Searching…") {
                    let delta = runtime.pending_delta().unwrap();
                    assert!(delta.added.iter().chain(&delta.modified).any(|item| {
                        matches!(&item.primitive, Primitive::Text { text, .. } if text.as_ref() == status)
                    }), "common status must not be ellipsized: {status}");
                }
                assert_eq!(
                    geometry(&runtime),
                    initial,
                    "status={status}, viewport={width}x{height}@{scale}"
                );
                assert_eq!(runtime.input().focused(), focused);
                assert_eq!(controller.latest_terminal_allocation(), allocation);
            }
        }
    }
}

#[test]
fn search_overlay_preserves_terminal_geometry_and_uses_equal_icon_controls() {
    use harbor_widget::layout::Size;
    for (width, height, scale) in [(1000, 500, 1.0), (1200, 800, 2.0), (320, 240, 1.0)] {
        let controller = controller(
            width as f64 / scale as f64,
            vec![snapshot(1, true, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), width, height);
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        let terminal_focus = runtime.input().focused();
        let allocation = controller.latest_terminal_allocation();
        controller.open_search(TabId(1));
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        assert_eq!(controller.latest_terminal_allocation(), allocation);
        let panel = search_panel_rect(&runtime);
        assert_eq!(panel.min.y, SEARCH_PANEL_INSET);
        assert_eq!(panel.max.x, width as f32 / scale - SEARCH_PANEL_INSET);
        assert_eq!(
            panel.size().width,
            440.0_f32.min(width as f32 / scale - 24.0)
        );
        assert_ne!(runtime.input().focused(), terminal_focus);

        let delta = runtime.pending_delta().unwrap();
        let mut icons: Vec<_> = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match item.primitive {
                Primitive::Icon { icon, rect, .. }
                    if panel.contains(rect.min)
                        && matches!(
                            icon,
                            LucideIcon::ChevronUp | LucideIcon::ChevronDown | LucideIcon::X
                        ) =>
                {
                    Some((icon, rect))
                }
                _ => None,
            })
            .collect();
        icons.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
        assert_eq!(
            icons.iter().map(|(icon, _)| *icon).collect::<Vec<_>>(),
            [
                LucideIcon::ChevronUp,
                LucideIcon::ChevronDown,
                LucideIcon::X
            ]
        );
        assert!(
            icons
                .iter()
                .all(|(_, rect)| rect.size() == Size::new(18.0, 18.0))
        );
        let mut buttons: Vec<_> = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match item.primitive {
                Primitive::Quad { rect, .. }
                    if panel.contains(rect.min) && rect.size() == Size::new(32.0, 32.0) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect();
        buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[1].min.x - buttons[0].max.x, 4.0);
        assert_eq!(buttons[2].min.x - buttons[1].max.x, 4.0);
        assert!(!delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(&item.primitive, Primitive::Text { text, .. }
                if ["Previous", "Next", "Close"].contains(&text.as_ref()))
        }));
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        assert!(items.iter().any(|item| matches!(&item.primitive,
            Primitive::BackdropMaterial { rect, sigma: 8.0, refraction, .. }
                if *rect == panel && refraction.width() == 12.0 && refraction.displacement() == 10.0)));
        let surface = items
            .iter()
            .find_map(|item| match &item.primitive {
                Primitive::PaintedQuad {
                    rect,
                    paint: harbor_widget::Paint::LinearGradient(gradient),
                    corner_radii,
                } if *rect == panel => {
                    assert_eq!(
                        *corner_radii, [12.0; 4],
                        "material does not reshape the panel"
                    );
                    Some(gradient)
                }
                _ => None,
            })
            .expect("panel uses generic gradient fill");
        assert!(
            surface
                .stops()
                .iter()
                .all(|stop| stop.color().a >= 0.04 && stop.color().a <= 0.12),
            "surface preserves backdrop colors rather than covering them with gray"
        );
        assert!(
            surface.stops().iter().all(|stop| {
                let color = stop.color();
                color.r == 0.0 && color.g == 0.0 && color.b == 0.0
            }),
            "faint black veil supports light text without adding a colored tint"
        );
        assert!(
            !items.iter().any(|item| matches!(&item.primitive,
            Primitive::PaintedBorder { rect, .. } if *rect == panel)),
            "shader rim replaces the old gradient stroke rather than doubling it"
        );
        assert!(!items.iter().any(|item| matches!(&item.primitive,
            Primitive::OuterShadow { occluder_rect, .. } if *occluder_rect == panel)));
        let (status_origin, _) = items
            .iter()
            .find_map(|item| match &item.primitive {
                Primitive::Text { text, origin, .. } if text.as_ref() == "No Result" => {
                    Some((*origin, text))
                }
                _ => None,
            })
            .expect("default result appears on the same row");
        assert!(status_origin.x < icons[0].1.min.x);
        assert!((status_origin.y - (panel.min.y + panel.size().height / 2.0)).abs() < 20.0);
        assert!(items.iter().any(|item| matches!(&item.primitive,
            Primitive::RoundedQuad { rect, color, corner_radii }
                if panel.contains(rect.min) && rect.size().width < panel.size().width
                && color.r == 0.0 && color.a > 0.0 && color.a < 0.1 && *corner_radii == [7.0; 4])));
        assert!(
            items
                .iter()
                .filter(|item| matches!(&item.primitive,
            Primitive::Quad { rect, .. } if buttons.contains(rect)))
                .all(|item| matches!(&item.primitive,
                Primitive::Quad { color, .. } if color.a == 0.0))
        );

        for ((_, rect), command) in icons.into_iter().zip([
            AppCommand::SearchPrevious,
            AppCommand::SearchNext,
            AppCommand::SearchClose,
        ]) {
            for phase in [PointerPhase::Down, PointerPhase::Up] {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(rect.min.x + 9.0, rect.min.y + 9.0),
                    phase,
                    PointerButton::Left,
                    42,
                )));
            }
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::popup(
                    command,
                    controller.store.state().read().popups.generation().unwrap()
                )]
            );
        }
        // Blank overlay space still permits normal terminal interaction.
        let terminal = allocation.unwrap();
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(
                    terminal.min.x + 8.0,
                    terminal.min.y + terminal.size().height / 2.0,
                ),
                phase,
                PointerButton::Left,
                43,
            )));
        }
        assert!(
            runtime
                .arena()
                .get(terminal_focus.unwrap())
                .unwrap()
                .children()
                .contains(&runtime.input().focused().unwrap()),
            "click reaches the terminal under its focus wrapper"
        );
        controller.close_search();
        runtime.request_focus(&controller.terminal_focus());
        runtime.update(Instant::now());
        assert_eq!(
            runtime.input().focused(),
            terminal_focus,
            "terminal fiber survives open/close"
        );
        assert_eq!(controller.latest_terminal_allocation(), allocation);
    }
}

#[test]
fn search_buttons_show_transient_glass_without_geometry_cursor_or_command_regressions() {
    use harbor_widget::{CursorEffect, CursorShape, layout::Size};
    for (width, height, scale) in [(1000, 500, 1.0), (1200, 800, 1.5), (1200, 800, 2.0)] {
        let controller = controller(
            width as f64 / scale as f64,
            vec![snapshot(1, true, false)],
            1,
        );
        controller.open_search(TabId(1));
        let mut runtime = mount(controller.clone(), width, height);
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        let panel = search_panel_rect(&runtime);
        let allocation = controller.latest_terminal_allocation();
        let delta = runtime.pending_delta().unwrap();
        let mut buttons: Vec<_> = delta
            .added
            .iter()
            .chain(&delta.modified)
            .filter_map(|item| match item.primitive {
                Primitive::Quad { rect, .. }
                    if panel.contains(rect.min) && rect.size() == Size::new(32.0, 32.0) =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect();
        buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        assert_eq!(buttons.len(), 3);
        assert!(search_button_surfaces(&runtime).is_empty());

        // The material wrapper must not replace the original keyboard tab stop or focus ring.
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        }));
        runtime.update(Instant::now());
        let delta = runtime.pending_delta().unwrap();
        assert!(
            delta
                .added
                .iter()
                .chain(&delta.modified)
                .any(|item| matches!(
                    item.primitive, Primitive::Border { rect, width: 1.0, corner_radius: 12.0, .. }
                        if rect == buttons[0]
                ))
        );
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
        }));
        assert_eq!(
            controller.drain_actions()[0].command,
            AppCommand::SearchPrevious
        );
        runtime.request_focus(&controller.search_focus());
        runtime.update(Instant::now());
        let focus = runtime.input().focused();

        for (rect, command) in buttons.into_iter().zip([
            AppCommand::SearchPrevious,
            AppCommand::SearchNext,
            AppCommand::SearchClose,
        ]) {
            // Hover the padding, not the icon: the whole hit target needs feedback.
            let position = Point::new(rect.min.x + 1.0, rect.min.y + 16.0);
            let effects = runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                PointerPhase::Move,
                PointerButton::Left,
                91,
            )));
            assert_eq!(
                effects.cursor,
                Some(CursorEffect::Set(CursorShape::Default))
            );
            runtime.update(Instant::now());
            assert_eq!(search_button_surfaces(&runtime), [(rect, 0.12)]);
            assert_eq!(runtime.input().focused(), focus);
            assert!(
                controller.drain_actions().is_empty(),
                "hover never executes"
            );
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                PointerPhase::Down,
                PointerButton::Left,
                91,
            )));
            runtime.update(Instant::now());
            assert_eq!(search_button_surfaces(&runtime), [(rect, 0.14)]);
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                PointerPhase::Up,
                PointerButton::Left,
                91,
            )));
            runtime.update(Instant::now());
            assert_eq!(search_button_surfaces(&runtime), [(rect, 0.12)]);
            assert_eq!(
                controller.drain_actions(),
                [AppCommandRequest::popup(
                    command,
                    controller.store.state().read().popups.generation().unwrap(),
                )]
            );
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(panel.min.x - 1.0, panel.min.y + 16.0),
                PointerPhase::Move,
                PointerButton::Left,
                91,
            )));
            runtime.update(Instant::now());
            assert!(
                search_button_surfaces(&runtime).is_empty(),
                "leave restores transparency"
            );
            assert_eq!(search_panel_rect(&runtime), panel);
            assert_eq!(controller.latest_terminal_allocation(), allocation);
            assert!(runtime.drain_external_input().is_empty());
            runtime.request_focus(&controller.search_focus());
            runtime.update(Instant::now());
        }
    }
}

#[test]
fn disabled_search_button_has_no_hover_glass_or_activation() {
    let activated = Arc::new(AtomicBool::new(false));
    let callback = activated.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(100, 100, 1.0));
    runtime.set_root(move |cx: &mut BuildCx| {
        let callback = callback.clone();
        search_icon_button(LucideIcon::X, glass_icon_button_style(cx))
            .disabled(true)
            .on_activate(move |_| callback.store(true, std::sync::atomic::Ordering::Relaxed))
            .build(cx)
    });
    runtime.update(Instant::now());
    for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(16.0, 16.0),
            phase,
            PointerButton::Left,
            92,
        )));
        runtime.update(Instant::now());
        let delta = runtime.pending_delta().unwrap();
        assert!(
            !delta
                .added
                .iter()
                .chain(&delta.modified)
                .any(|item| { matches!(item.primitive, Primitive::BackdropMaterial { .. }) })
        );
    }
    assert!(!activated.load(std::sync::atomic::Ordering::Relaxed));
    assert!(runtime.input().focused().is_none());
}

#[test]
fn configuration_notice_stays_below_search_and_returns_when_search_closes() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("Invalid settings".into()),
    );
    let mut runtime = mount(controller.clone(), 1000, 500);
    let notice_top_y = |runtime: &Runtime| {
        let delta = runtime.pending_delta().unwrap();
        delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match &item.primitive {
                Primitive::BackdropMaterial {
                    rect, corner_radii, ..
                } if *corner_radii == [6.0; 4] => Some(rect.min.y),
                _ => None,
            })
            .unwrap()
    };
    let original_y = notice_top_y(&runtime);
    let allocation = controller.latest_terminal_allocation();
    controller.open_search(TabId(1));
    runtime.update(Instant::now());
    let panel = search_panel_rect(&runtime);
    assert!(notice_top_y(&runtime) > panel.max.y);
    assert_eq!(controller.latest_terminal_allocation(), allocation);
    controller.close_search();
    runtime.update(Instant::now());
    assert_eq!(notice_top_y(&runtime), original_y);
    assert!(controller.store.state().read().notices.message().is_some());
}

#[test]
fn current_session_search_entry_routes_typed_commands_without_pty_leakage() {
    use crate::tab_manager::{TabManager, TerminalTabResources};
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| {
        #[allow(clippy::arc_with_non_send_sync)]
        let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            4,
            20,
            std::io::empty(),
            Writer(bytes.clone()),
            || true,
        )));
        engine
            .lock()
            .unwrap()
            .put_str("needle\r\nneedle\r\nother\r\nlive\r\n");
        let bridge =
            TerminalWidgetBridge::new(draw, engine.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(engine, bridge, "test"))
    })
    .unwrap();
    let controller =
        WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    let key = |key, modifiers| UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers });
    runtime.dispatch(key(
        Key::Character('f'),
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        },
    ));
    let actions = controller.drain_actions();
    assert_eq!(
        actions,
        [AppCommandRequest::shortcut(AppCommand::SearchOpen)]
    );
    let focus = tabs
        .dispatch_search_command(actions[0].command, &controller)
        .unwrap();
    runtime.request_focus(&focus); // requests before mount must survive reconciliation
    runtime.update(Instant::now());
    for ch in "needle".chars() {
        runtime.dispatch(key(Key::Character(ch), Modifiers::default()));
    }
    runtime.dispatch(UiEvent::ImePreedit(
        harbor_widget::input::event::ImePreedit::new("界", None),
    ));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    runtime.dispatch(key(Key::Backspace, Modifiers::default()));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    for action in controller.drain_actions() {
        tabs.dispatch_search_command(action.command, &controller);
    }
    for _ in 0..100 {
        if !tabs.advance_search().pending {
            break;
        }
    }
    assert_eq!(tabs.active_bridge().unwrap().decoration_ranges().len(), 2);
    for event in [
        key(Key::Enter, Modifiers::default()),
        key(
            Key::Enter,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ),
        key(Key::F3, Modifiers::default()),
    ] {
        runtime.dispatch(event);
        let actions = controller.drain_actions();
        assert_eq!(actions.len(), 1);
        tabs.dispatch_search_command(actions[0].command, &controller);
    }
    assert!(
        tabs.active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .screen()
            .view_offset()
            > 0
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "all consumed query/IME/navigation input stays out of PTY"
    );
    runtime.dispatch(key(Key::Escape, Modifiers::default()));
    let actions = controller.drain_actions();
    assert_eq!(actions[0].command, AppCommand::SearchClose);
    let focus = tabs
        .dispatch_search_command(actions[0].command, &controller)
        .unwrap();
    runtime.request_focus(&focus);
    runtime.update(Instant::now());
    assert!(tabs.active_bridge().unwrap().decoration_ranges().is_empty());
    runtime.dispatch(key(Key::Character('x'), Modifiers::default()));
    assert_eq!(
        &*bytes.lock().unwrap(),
        b"x",
        "normal pass-through resumes after close"
    );
}

#[test]
fn search_entry_limits_and_query_cancellation_are_visible_and_coalesced() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("a".repeat(128))));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    assert_eq!(controller.search_query().unwrap().1.len(), 128);
    assert_eq!(controller.drain_actions().len(), 1);
    for _ in 0..128 {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Backspace,
            modifiers: Modifiers::default(),
        }));
    }
    assert_eq!(controller.search_query().unwrap().1, "");
    controller.sync_search_source(Some(TabId(2)));
    assert!(controller.search_query().is_none());
    runtime.update(Instant::now());
}

#[test]
fn reusable_search_editor_replaces_at_limit_and_keeps_clipboard_out_of_terminal_commands() {
    use harbor_widget::{TextSelection, effects::ClipboardEffect};
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    controller.open_search(TabId(1));
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.search_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("a".repeat(128))));
    controller.drain_actions();
    let input = controller
        .store
        .state()
        .read()
        .popups
        .get::<SearchUiState>()
        .unwrap()
        .input
        .clone();
    input.set_selection(TextSelection {
        anchor: 125,
        extent: 128,
    });
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("界".into())));
    assert_eq!(
        controller.search_query().unwrap().1,
        format!("{}界", "a".repeat(125))
    );
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::popup(
            AppCommand::SearchChanged,
            controller.store.state().read().popups.generation().unwrap()
        )]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('a'),
        modifiers: ctrl(),
    }));
    let copied = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('c'),
        modifiers: ctrl(),
    }));
    assert!(matches!(copied.clipboard, Some(ClipboardEffect::Editor(_))));
    assert!(
        controller.drain_actions().is_empty(),
        "copy must not enqueue the terminal copy command"
    );
    let paste = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(request)) = paste.clipboard else {
        panic!("expected editor paste");
    };
    runtime.complete_clipboard(request.id, Ok("needle".into()));
    assert_eq!(controller.search_query().unwrap().1, "needle");
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::popup(
            AppCommand::SearchChanged,
            controller.store.state().read().popups.generation().unwrap()
        )]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('z'),
        modifiers: ctrl(),
    }));
    assert_eq!(controller.search_query().unwrap().1.len(), 128);
    assert_eq!(controller.drain_actions().len(), 1);
    assert!(runtime.drain_external_input().is_empty());
    let late = runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('v'),
        modifiers: ctrl(),
    }));
    let Some(ClipboardEffect::Editor(request)) = late.clipboard else {
        panic!("expected editor paste");
    };
    controller.sync_search_source(Some(TabId(2)));
    controller.open_search(TabId(2));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.search_focus());
    runtime.complete_clipboard(request.id, Ok("wrong session".into()));
    assert_eq!(controller.search_query().unwrap().1, "");
}
