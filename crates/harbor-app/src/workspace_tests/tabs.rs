use super::*;

#[test]
fn breakpoint_is_finite_and_inclusive() {
    assert_eq!(
        RailPresentation::for_logical_width(899.999),
        RailPresentation::Compact
    );
    assert_eq!(
        RailPresentation::for_logical_width(900.0),
        RailPresentation::Expanded
    );
    assert_eq!(
        RailPresentation::for_logical_width(f64::NAN),
        RailPresentation::Compact
    );
    assert_eq!(
        RailPresentation::for_logical_width(f64::INFINITY),
        RailPresentation::Compact
    );
    assert_eq!(
        RailPresentation::for_logical_width(-1.0),
        RailPresentation::Compact
    );
}

#[test]
fn declarative_workspace_binds_all_tab_shortcuts() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    let mut shortcuts = vec![
        (Key::Character('t'), ctrl(), TabCommand::New),
        (Key::Character('w'), ctrl(), TabCommand::CloseActive),
        (Key::Tab, ctrl(), TabCommand::Next),
        (
            Key::Tab,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::default()
            },
            TabCommand::Previous,
        ),
    ];
    shortcuts.extend((1..=9).map(|index| {
        (
            Key::Character(char::from_digit(index as u32, 10).unwrap()),
            ctrl(),
            TabCommand::Numeric(TabIndex::from_valid_u8(index as u8)),
        )
    }));

    for (key, modifiers, command) in shortcuts {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::Tab(command))]
        );
    }
}

#[test]
fn terminal_shortcuts_enqueue_only_when_context_consumes() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    let ctrl_shift = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('C'),
        modifiers: ctrl_shift,
    }));
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::shortcut(AppCommand::Copy)]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
        key: Key::Character('C'),
        modifiers: ctrl_shift,
    }));

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('c'),
        modifiers: ctrl(),
    }));
    assert!(controller.drain_actions().is_empty());

    for (key, command) in [
        (Key::Character('v'), AppCommand::Paste),
        (Key::PageUp, AppCommand::PageUp),
        (Key::Home, AppCommand::ScrollToTop),
    ] {
        let modifiers = if key == Key::Character('v') {
            ctrl()
        } else {
            Modifiers::default()
        };
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(command)]
        );
    }
}

#[test]
fn workspace_publishes_the_final_terminal_panel_allocation() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    let external_rect = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::External { rect, draw } if draw == 1 => Some(rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(controller.latest_terminal_allocation(), Some(external_rect));

    runtime.update(Instant::now());
    assert_eq!(controller.latest_terminal_allocation(), Some(external_rect));
}

#[test]
fn presentation_changes_only_when_crossing_the_breakpoint() {
    let controller = controller(899.0, vec![snapshot(1, true, false)], 1);
    assert!(!controller.tabs().update_presentation(100.0));
    assert!(controller.tabs().update_presentation(900.0));
    assert!(!controller.tabs().update_presentation(1200.0));
    assert!(controller.tabs().update_presentation(f64::NAN));
}

#[test]
fn workspace_allocates_fixed_rail_and_finite_terminal_geometry() {
    for (logical_width, expected_rail) in [(899.0, 56.0), (900.0, 200.0)] {
        let controller = controller(
            logical_width,
            vec![snapshot(1, true, false), snapshot(2, false, true)],
            1,
        );
        let runtime = mount(controller, logical_width as u32, 320);
        let rail = fiber_ids(&runtime)
            .into_iter()
            .find_map(|id| {
                let fiber = runtime.arena().get(id).unwrap();
                (fiber.widget_type() == TypeId::of::<harbor_widget::SizedBox>())
                    .then(|| fiber.layout_rect().unwrap())
            })
            .expect("bounded rail");
        assert_eq!(rail.size().width, expected_rail);
        let terminal = runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::External { rect, draw } if draw == 1 => Some(rect),
                _ => None,
            })
            .expect("active terminal primitive");
        let first_tab = runtime
            .pending_delta()
            .unwrap()
            .added
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::Quad { rect, color, .. }
                | Primitive::RoundedQuad { rect, color, .. }
                    if color.a > 0.0 && rect.min.x >= rail.min.x && rect.max.x <= rail.max.x =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .expect("selected first tab background");
        assert_eq!(
            first_tab.min.y, terminal.min.y,
            "first tab and terminal panel must share the same top edge"
        );
        assert!(
            [
                terminal.min.x,
                terminal.min.y,
                terminal.max.x,
                terminal.max.y
            ]
            .into_iter()
            .all(f32::is_finite)
        );
        assert!(terminal.size().width >= 0.0);
        assert!(terminal.size().height >= 0.0);
        if expected_rail == COMPACT_RAIL_WIDTH {
            let cell_width = runtime.text_metrics().cell_width;
            for item in &runtime.pending_delta().unwrap().added {
                if let Primitive::Text { text, origin, .. } = &item.primitive {
                    let text_width = text.chars().count() as f32 * cell_width;
                    assert!(origin.x >= rail.min.x);
                    assert!(origin.x + text_width <= rail.max.x);
                }
            }
        }
    }

    let narrow = mount(controller(0.0, vec![snapshot(1, true, false)], 1), 0, 0);
    for id in fiber_ids(&narrow) {
        let rect = narrow.arena().get(id).unwrap().layout_rect().unwrap();
        assert!(
            [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
                .into_iter()
                .all(f32::is_finite)
        );
        assert!(rect.size().width >= 0.0 && rect.size().height >= 0.0);
    }
}

#[test]
fn shortcuts_and_pointer_controls_enqueue_distinct_focus_policies() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);

    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('t'),
        modifiers: ctrl(),
    }));
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::shortcut(AppCommand::Tab(
            TabCommand::New
        ))]
    );

    let row = tab_rows(&runtime)[0];
    let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
    let position = Point::new(rect.min.x + 8.0, rect.min.y + rect.size().height / 2.0);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            position,
            phase,
            PointerButton::Left,
            1,
        )));
    }
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::rail(TabCommand::Activate(TabId(1)))]
    );
}

#[test]
fn compact_rail_has_no_close_control_or_close_focus_stop() {
    let controller = controller(
        899.0,
        vec![snapshot(1, true, false), snapshot(2, false, false)],
        1,
    );
    let mut runtime = mount(controller.clone(), 899, 320);
    let rows = tab_rows(&runtime);
    assert_eq!(rows.len(), 2);
    for row in &rows {
        let children = runtime.arena().get(*row).unwrap().children();
        assert_eq!(children.len(), 1, "compact row contains only its label");
        assert_eq!(
            runtime.arena().get(children[0]).unwrap().widget_type(),
            TypeId::of::<harbor_widget::Expanded>()
        );
        assert!(!close_is_visible(&runtime, *row));
    }

    let rect = runtime.arena().get(rows[1]).unwrap().layout_rect().unwrap();
    let position = Point::new(rect.max.x - 4.0, rect.min.y + 8.0);
    move_pointer(&mut runtime, position);
    assert!(!close_is_visible(&runtime, rows[1]));
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            position,
            phase,
            PointerButton::Left,
            7,
        )));
        runtime.update(Instant::now());
    }
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::rail(TabCommand::Activate(TabId(2)))]
    );

    runtime.request_focus(&controller.tabs().tab_focus(TabId(2)).unwrap());
    runtime.update(Instant::now());
    assert!(!close_is_visible(&runtime, rows[1]));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    runtime.update(Instant::now());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::rail(TabCommand::New)]
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('w'),
        modifiers: ctrl(),
    }));
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::shortcut(AppCommand::Tab(
            TabCommand::CloseActive
        ))]
    );
}

#[test]
fn resizing_removes_and_restores_close_controls() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    for width in [1000, 899, 900, 899] {
        runtime.set_viewport(Viewport::new(width, 320, 1.0));
        controller.tabs().sync(
            vec![snapshot(1, true, false)],
            Some(bridge(1)),
            f64::from(width),
        );
        runtime.update(Instant::now());
        let row = tab_rows(&runtime)[0];
        let compact = width < 900;
        assert_eq!(
            runtime.arena().get(row).unwrap().children().len(),
            if compact { 1 } else { 3 }
        );
        assert_eq!(close_is_visible(&runtime, row), !compact);
    }
}

#[test]
fn close_affordance_follows_selection_hover_and_both_keyboard_stops() {
    let controller = controller(
        1000.0,
        vec![snapshot(1, true, false), snapshot(2, false, false)],
        1,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    let rows = tab_rows(&runtime);
    assert!(close_is_visible(&runtime, rows[0]));
    assert!(!close_is_visible(&runtime, rows[1]));
    let rect = runtime.arena().get(rows[1]).unwrap().layout_rect().unwrap();
    move_pointer(&mut runtime, Point::new(rect.min.x + 8.0, rect.min.y + 8.0));
    assert!(close_is_visible(&runtime, rows[1]));
    move_pointer(&mut runtime, Point::new(500.0, 200.0));
    assert!(!close_is_visible(&runtime, rows[1]));

    runtime.request_focus(&controller.tabs().tab_focus(TabId(2)).unwrap());
    runtime.update(Instant::now());
    assert!(close_is_visible(&runtime, rows[1]));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    runtime.update(Instant::now());
    assert!(close_is_visible(&runtime, rows[1]));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(
        controller.drain_actions(),
        [AppCommandRequest::close_rail(TabId(2))]
    );
    runtime.request_focus(&controller.terminal_focus());
    runtime.update(Instant::now());
    assert!(!close_is_visible(&runtime, rows[1]));
}

#[test]
fn nested_close_click_never_activates_the_tab_and_survives_pressed_rebuilds() {
    for width in [900, 1000] {
        let controller = controller(
            f64::from(width),
            vec![snapshot(1, true, false), snapshot(2, false, false)],
            1,
        );
        let mut runtime = mount(controller.clone(), width, 320);
        let row = tab_rows(&runtime)[1];
        let close = *runtime.arena().get(row).unwrap().children().last().unwrap();
        let rect = runtime.arena().get(close).unwrap().layout_rect().unwrap();
        let position = Point::new(
            rect.min.x + rect.size().width / 2.0,
            rect.min.y + rect.size().height / 2.0,
        );
        move_pointer(&mut runtime, position);
        assert!(close_is_visible(&runtime, row));
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                position,
                phase,
                PointerButton::Left,
                7,
            )));
            runtime.update(Instant::now());
        }
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::close_rail(TabId(2))]
        );
        assert!(runtime.input().captor(7).is_none());
    }
}

#[test]
fn new_terminal_follows_tabs_and_remains_reachable_at_the_end_of_scroll() {
    for count in [2, 20] {
        let controller = controller(
            1000.0,
            (1..=count).map(|id| snapshot(id, id == 1, false)).collect(),
            1,
        );
        let mut runtime = mount(controller.clone(), 1000, 320);
        if count == 20 {
            controller
                .tabs
                .scroll
                .jump_to(controller.tabs.scroll.metrics().max_scroll_extent());
            runtime.update(Instant::now());
        }
        let last = *tab_rows(&runtime).last().unwrap();
        let last_rect = runtime.arena().get(last).unwrap().layout_rect().unwrap();
        let delta = runtime.pending_delta().unwrap();
        let origin = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match &item.primitive {
                Primitive::Text { text, origin, .. } if text.as_ref() == "New terminal" => {
                    Some(*origin)
                }
                _ => None,
            })
            .expect("new terminal label");
        assert!(origin.y > last_rect.max.y);
        assert!(origin.y < 300.0, "new control must be in the viewport");
        if count == 2 {
            assert!(
                origin.y < 120.0,
                "new control must not be pinned to the bottom"
            );
        }
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(origin.x + 4.0, origin.y + 4.0),
                phase,
                PointerButton::Left,
                7,
            )));
        }
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::rail(TabCommand::New)]
        );
    }
}

#[test]
fn long_unicode_titles_are_ellipsized_clipped_and_never_resize_close_controls() {
    let mut long = snapshot(1, true, false);
    long.title = "项目 / a very long terminal title that must not overlap the close button".into();
    let controller = controller(1000.0, vec![long.clone()], 1);
    let mut runtime = mount(controller.clone(), 1000, 320);
    let row = tab_rows(&runtime)[0];
    let close = *runtime.arena().get(row).unwrap().children().last().unwrap();
    let close_rect = runtime.arena().get(close).unwrap().layout_rect().unwrap();
    let title = runtime.pending_delta().unwrap().added.iter().find(|item| {
            matches!(&item.primitive, Primitive::Text { text, .. } if text.starts_with("项目"))
        }).expect("truncated title");
    assert!(matches!(&title.primitive, Primitive::Text { text, .. } if text.ends_with('…')));
    assert!(
        title
            .clips
            .iter()
            .any(|clip| clip.rect().max.x <= close_rect.min.x)
    );
    assert_eq!(controller.tabs.state.read().snapshots[0].title, long.title);
    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
    runtime.update(Instant::now());
    assert_eq!(
        runtime.arena().get(close).unwrap().layout_rect().unwrap(),
        close_rect
    );
}

#[test]
fn many_tabs_scroll_and_offset_clamps_after_content_shrinks() {
    let snapshots = (1..=20).map(|id| snapshot(id, id == 1, false)).collect();
    let controller = controller(1000.0, snapshots, 1);
    let mut runtime = mount(controller.clone(), 1000, 120);
    assert!(controller.tabs.scroll.metrics().max_scroll_extent() > 0.0);

    for _ in 0..24 {
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Tab,
            modifiers: Modifiers::default(),
        }));
        runtime.update(Instant::now());
    }
    assert!(controller.tabs.scroll.metrics().offset() > 0.0);

    controller
        .tabs()
        .sync(vec![snapshot(1, true, false)], Some(bridge(1)), 1000.0);
    runtime.update(Instant::now());
    assert_eq!(controller.tabs.scroll.metrics().max_scroll_extent(), 0.0);
    assert_eq!(controller.tabs.scroll.metrics().offset(), 0.0);
}

#[test]
fn signal_updates_preserve_surviving_keyed_tab_fibers() {
    let controller = controller(
        1000.0,
        vec![
            snapshot(1, true, false),
            snapshot(2, false, false),
            snapshot(3, false, false),
        ],
        1,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    let before = tab_rows(&runtime);
    assert_eq!(before.len(), 3);

    controller.tabs().sync(
        vec![
            snapshot(1, true, false),
            snapshot(3, false, true),
            snapshot(4, false, false),
        ],
        Some(bridge(1)),
        1000.0,
    );
    runtime.update(Instant::now());
    let after = tab_rows(&runtime);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[2]);
    assert_ne!(after[2], before[1]);
}

#[test]
fn signal_switch_replaces_the_only_mounted_terminal_bridge() {
    let controller = controller(
        1000.0,
        vec![snapshot(1, true, false), snapshot(2, false, false)],
        1,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    assert!(runtime.has_external_draws());

    controller.tabs().sync(
        vec![snapshot(1, false, false), snapshot(2, true, false)],
        Some(bridge(2)),
        1000.0,
    );
    runtime.update(Instant::now());
    let delta = runtime.pending_delta().expect("bridge switch scene delta");
    let mounted: Vec<_> = delta
        .added
        .iter()
        .filter_map(|item| match item.primitive {
            Primitive::External { draw, .. } => Some(draw),
            _ => None,
        })
        .collect();
    assert_eq!(mounted, [2]);
    assert!(runtime.has_external_draws());
}

#[test]
fn rail_icon_artwork_replaces_glyphs_in_expanded_and_compact_modes() {
    use harbor_widget::LucideIcon;
    for width in [899, 1000] {
        let controller = controller(
            f64::from(width),
            vec![snapshot(1, true, false), snapshot(2, false, true)],
            1,
        );
        let runtime = mount(controller, width, 320);
        let items = &runtime.pending_delta().unwrap().added;
        let icons: Vec<_> = items
            .iter()
            .filter_map(|item| match &item.primitive {
                Primitive::Icon { icon, .. } => Some(*icon),
                _ => None,
            })
            .collect();
        assert!(icons.contains(&LucideIcon::Plus));
        assert!(icons.contains(&LucideIcon::Dot));
        assert_eq!(icons.contains(&LucideIcon::Terminal), width >= 900);
        assert_eq!(icons.contains(&LucideIcon::X), width >= 900);
        let labels: Vec<_> = items
            .iter()
            .filter_map(|item| match &item.primitive {
                Primitive::Text { text, .. } => Some(text.as_ref()),
                _ => None,
            })
            .collect();
        assert!(
            !labels
                .iter()
                .any(|text| [">_", "•", "+", "×", "+  New terminal"].contains(text))
        );
        if width < 900 {
            assert!(labels.contains(&"1") && labels.contains(&"2"));
        } else {
            assert!(labels.contains(&"Terminal 1") && labels.contains(&"Terminal 2"));
            assert!(labels.contains(&"New terminal"));
        }
        for item in items {
            if let Primitive::Icon { rect, color, .. } = &item.primitive {
                assert!(rect.is_valid());
                assert!(color.is_finite());
            }
        }
    }
}

#[test]
fn compact_abbreviations_keep_numbers_even_when_unread() {
    assert_eq!(abbreviation(&snapshot(123, false, true)), "3");
    assert_eq!(abbreviation(&snapshot(123, false, false)), "3");
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn stable_external_surface_rebuild_preserves_input_ime_and_unmount_cleans_up() {
    use harbor_widget::{ExternalSurface, ImeEffect, SizedBox, input::event::ImePreedit};

    let bytes = Arc::new(Mutex::new(Vec::new()));
    let terminal = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let weak = Arc::downgrade(&terminal);
    let bridge =
        TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false)));
    let controller =
        WorkspaceUiController::new(vec![snapshot(1, true, false)], bridge.clone(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 320);
    let surface = fiber_ids(&runtime)
        .into_iter()
        .find(|id| {
            runtime.arena().get(*id).unwrap().widget_type() == TypeId::of::<ExternalSurface>()
        })
        .expect("the real workspace must mount ExternalSurface");
    let rect = runtime.arena().get(surface).unwrap().layout_rect().unwrap();
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(rect.min.x + 8.0, rect.min.y + 8.0),
            phase,
            PointerButton::Left,
            7,
        )));
    }
    assert_eq!(runtime.input().focused(), Some(surface));
    let focused = runtime.update(Instant::now());
    assert_eq!(focused.ime, Some(ImeEffect::set_allowed(true)));
    assert!(matches!(
        focused.control_flow,
        Some(harbor_widget::ControlFlowEffect::WaitUntil(_))
    ));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(&*bytes.lock().unwrap(), b"	", "Tab stays terminal input");
    assert_eq!(runtime.input().focused(), Some(surface));
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("draft", Some((0, 5)))));
    assert_eq!(terminal.lock().unwrap().preedit().unwrap().text, "draft");

    let mut renamed = snapshot(1, true, false);
    renamed.title = "updated while composing".into();
    controller
        .tabs()
        .sync(vec![renamed], Some(bridge.clone()), 1000.0);
    runtime.update(Instant::now());
    assert_eq!(runtime.input().focused(), Some(surface));
    assert!(runtime.has_external_draws());
    assert_eq!(
        terminal.lock().unwrap().preedit().unwrap().text,
        "draft",
        "a stable rebuild must not run the terminal unmount callback"
    );
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime("你好".into())));
    assert_eq!(&*bytes.lock().unwrap(), "	你好".as_bytes());
    assert!(terminal.lock().unwrap().preedit().is_none());
    assert!(
        runtime.drain_external_input().is_empty(),
        "in-tree input is not queued twice"
    );
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("next", None)));

    runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO).unwrap());
    let effects = runtime.update(Instant::now());
    assert!(terminal.lock().unwrap().preedit().is_none());
    assert!(!runtime.has_external_draws());
    assert_eq!(effects.ime, Some(ImeEffect::set_allowed(false)));
    assert_eq!(
        effects.control_flow, None,
        "removed schedule contributes no deadline"
    );
    assert_eq!(runtime.input().focused(), None);
    controller.tabs().sync(Vec::new(), None, 1000.0);
    drop(bridge);
    drop(terminal);
    assert!(
        weak.upgrade().is_none(),
        "registrations must not keep the terminal alive"
    );
}

#[test]
fn clearing_workspace_releases_the_last_terminal_bridge() {
    use harbor_widget::widgets::sized_box::SizedBox;

    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
    let weak = Arc::downgrade(&terminal);
    let controller = WorkspaceUiController::new(
        vec![snapshot(1, true, false)],
        TerminalWidgetBridge::new(1, Arc::clone(&terminal), Arc::new(AtomicBool::new(false))),
        1000.0,
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    drop(terminal);

    controller.tabs().sync(Vec::new(), None, 1000.0);
    runtime.set_root(SizedBox::new(harbor_widget::layout::Size::ZERO).unwrap());
    runtime.update(Instant::now());

    assert!(weak.upgrade().is_none());
    assert!(!runtime.has_external_draws());
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn native_escape_reaches_terminal_writer_in_primary_and_alternate_modes() {
    use harbor_widget::winit::WinitAdapter;
    use winit::{
        event::ElementState,
        keyboard::{Key as NativeKey, KeyLocation, NamedKey},
    };
    for alternate in [false, true] {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
            4,
            20,
            std::io::empty(),
            Writer(bytes.clone()),
            || true,
        )));
        if alternate {
            engine.lock().unwrap().put_bytes(b"\x1b[?1049h");
        }
        let test_bridge =
            TerminalWidgetBridge::new(1, engine.clone(), Arc::new(AtomicBool::new(false)));
        let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
        let mut runtime = mount(controller.clone(), 1000, 320);
        controller
            .tabs()
            .sync(vec![snapshot(1, true, false)], Some(test_bridge), 1000.0);
        runtime.update(Instant::now());
        runtime.request_focus(&controller.terminal_focus());
        let mut adapter = WinitAdapter::new();
        let key = NativeKey::Named(NamedKey::Escape);
        adapter.handle_keyboard_input(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
        );
        adapter.handle_keyboard_input_with_repeat(
            &mut runtime,
            &key,
            ElementState::Pressed,
            KeyLocation::Standard,
            true,
        );
        adapter.handle_keyboard_input(
            &mut runtime,
            &key,
            ElementState::Released,
            KeyLocation::Standard,
        );
        assert!(controller.drain_actions().is_empty());
        assert_eq!(*bytes.lock().unwrap(), b"\x1b\x1b", "alternate={alternate}");
        assert_eq!(engine.lock().unwrap().is_alt_screen(), alternate);
    }
}

#[test]
fn tab_index_domain_type_validates_range() {
    assert_eq!(TabIndex::new(0), None);
    assert_eq!(TabIndex::new(10), None);
    assert_eq!(TabIndex::new(255), None);

    let first = TabIndex::new(1).unwrap();
    assert_eq!(first.get(), 1);
    assert_eq!(first.to_zero_based(), 0);

    let ninth = TabIndex::new(9).unwrap();
    assert_eq!(ninth.get(), 9);
    assert_eq!(ninth.to_zero_based(), 8);
}

#[test]
fn unchanged_global_publications_do_not_rebuild_the_workspace() {
    use std::{cell::Cell, rc::Rc};

    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let bindings =
        crate::command::validate_keybindings(&harbor_config::RawKeybindings::default()).unwrap();
    controller.publish_keybindings(bindings.clone());
    controller.open_search(TabId(1));
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    let root = tab_workspace(controller.clone(), false);
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(1000, 320, 1.0));
    runtime.set_root(move |cx: &mut harbor_widget::view::BuildCx| {
        observed.set(observed.get() + 1);
        root.build(cx)
    });
    runtime.update(Instant::now());
    runtime.request_focus(&controller.search_focus());
    runtime.update(Instant::now());
    let focused = runtime.input().focused();
    let allocation = controller.latest_terminal_allocation();
    let version = controller.store.state().version();
    let built = builds.get();
    let revision = controller.store.state().read().notices.revision;
    let deadline = controller.store.state().read().notices.deadline();

    controller.publish_keybindings(bindings);
    controller.publish_search_status("No Result");
    controller.publish_configuration_notice(ConfigurationNoticeSource::Observation, None);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    controller.dismiss_notification(revision + 1);
    assert_eq!(
        controller.advance_notifications(Instant::now()),
        (false, deadline)
    );
    runtime.update(Instant::now());

    assert_eq!(controller.store.state().version(), version);
    assert_eq!(
        builds.get(),
        built,
        "no-op publications must not dirty the root"
    );
    assert_eq!(runtime.input().focused(), focused);
    assert_eq!(controller.latest_terminal_allocation(), allocation);
    assert!(controller.drain_actions().is_empty());

    controller.publish_search_status("1 / 2");
    assert_eq!(controller.store.state().version(), version + 1);
    runtime.update(Instant::now());
    assert_eq!(builds.get(), built + 1);

    controller.dismiss_notification(revision);
    let dismissed_version = controller.store.state().version();
    controller.dismiss_notification(revision);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert_eq!(controller.store.state().version(), dismissed_version);
    assert!(controller.store.state().read().notices.message().is_none());

    // Recovery changes deduplication history even when no notice is visible.
    controller.publish_configuration_notice(ConfigurationNoticeSource::Reload, None);
    assert_eq!(controller.store.state().version(), dismissed_version + 1);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert!(controller.store.state().read().notices.message().is_some());
}

#[test]
fn global_publication_survives_tab_mutations_responsive_changes_and_root_replacement() {
    let controller = controller(1000.0, vec![snapshot(1, true, false)], 1);
    let mut raw = harbor_config::RawKeybindings::default();
    raw.entries.insert(
        "app.reload-configuration".into(),
        vec![harbor_config::RawKeybinding {
            modifiers: vec![],
            key: "q".into(),
        }],
    );
    controller.publish_keybindings(crate::command::validate_keybindings(&raw).unwrap());
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    let mut runtime = mount(controller.clone(), 1000, 320);
    let revision = controller.store.state().read().notices.revision;
    controller.dismiss_notification(revision);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Observation,
        Some("watch unavailable".into()),
    );
    let global_version = controller.store.state().version();
    for (snapshots, active, width) in [
        (
            vec![snapshot(1, false, false), snapshot(2, true, false)],
            2,
            899.0,
        ),
        (vec![snapshot(2, true, false)], 2, 1000.0),
        (
            vec![snapshot(2, false, true), snapshot(3, true, false)],
            3,
            500.0,
        ),
    ] {
        controller
            .tabs()
            .sync(snapshots, Some(bridge(active)), width);
        controller.tabs().update_presentation(width + 1.0);
        runtime.update(Instant::now());
        assert_eq!(
            controller.store.state().version(),
            global_version,
            "tabs cannot republish global state"
        );
        assert_eq!(
            controller.store.state().read().notices.message().as_deref(),
            Some("watch unavailable")
        );
        runtime.request_focus(&controller.terminal_focus());
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('q'),
            modifiers: Modifiers::default(),
        }));
        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('q'),
            modifiers: Modifiers::default(),
        }));
        assert_eq!(
            controller.drain_actions(),
            [AppCommandRequest::shortcut(AppCommand::ReloadConfiguration)]
        );
        assert!(controller.drain_actions().is_empty());
    }
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("watch unavailable")
    );
    controller.publish_configuration_notice(ConfigurationNoticeSource::Observation, None);
    assert!(controller.store.state().read().notices.message().is_none());
    controller.publish_configuration_notice(ConfigurationNoticeSource::Reload, None);
    controller.publish_configuration_notice(
        ConfigurationNoticeSource::Reload,
        Some("invalid TOML".into()),
    );
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("invalid TOML")
    );
    runtime.set_root(
        harbor_widget::widgets::sized_box::SizedBox::new(harbor_widget::layout::Size::ZERO)
            .unwrap(),
    );
    runtime.update(Instant::now());
    runtime.set_root(tab_workspace(controller.clone(), false));
    runtime.update(Instant::now());
    runtime.request_focus(&controller.terminal_focus());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Character('q'),
        modifiers: Modifiers::default(),
    }));
    assert_eq!(controller.drain_actions().len(), 1);
    assert_eq!(controller.tabs.state.read().snapshots.len(), 2);
    assert_eq!(
        controller.store.state().read().notices.message().as_deref(),
        Some("invalid TOML")
    );
}
