//! Spec 0037: exercise actual mounted slots, layout, scenes and routed input.
use harbor_widget::{
    Button, Children, Column, ComponentExt, ConstrainedBox, FocusHandle, IconButton, IntoChildView,
    LucideIcon, Theme, ThemeProvider, WithChildren,
    fiber::FiberId,
    input::event::{
        Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
    },
    layout::{Point, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, Primitive},
    signal::Signal,
    view,
    view::{BuildCx, Component},
    widgets::{interactive_region::CONTENT_SLOT_KEY, sized_box::SizedBox, text_label::TextLabel},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

fn update(rt: &mut Runtime) {
    rt.update(Instant::now());
}
fn runtime(tree: impl IntoChildView + 'static, width: u32) -> Runtime {
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(width, 160, 1.0));
    rt.set_root(tree);
    update(&mut rt);
    rt
}
fn primitives(rt: &Runtime) -> Vec<Primitive> {
    let delta = rt.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .map(|item| item.primitive.clone())
        .collect()
}
fn texts(rt: &Runtime) -> Vec<String> {
    primitives(rt)
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}
fn pointer(rt: &mut Runtime, point: Point, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        point,
        phase,
        PointerButton::Left,
        0,
    )));
}
fn key(rt: &mut Runtime, key: Key, shift: bool) {
    rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ..Modifiers::default()
        },
    }));
}
fn ids(rt: &Runtime) -> Vec<FiberId> {
    let mut pending = vec![rt.root_id().unwrap()];
    let mut all = Vec::new();
    while let Some(id) = pending.pop() {
        pending.extend(rt.arena().get(id).unwrap().children());
        all.push(id);
    }
    all
}
fn slot(rt: &Runtime) -> FiberId {
    ids(rt)
        .into_iter()
        .find(|id| {
            rt.arena()
                .get(*id)
                .unwrap()
                .key()
                .is_some_and(|k| *k == harbor_widget::view::Key::new(CONTENT_SLOT_KEY))
        })
        .unwrap()
}

#[test]
fn constructors_setters_and_macro_blocks_select_one_replaceable_slot() {
    for staged in [
        Button::text("old"),
        Button::icon(LucideIcon::Plus, "old"),
        Button::new().content(|cx, _| TextLabel::new("old").build(cx)),
    ] {
        let rt = runtime(
            staged
                .clone()
                .child(TextLabel::new("a"))
                .child(TextLabel::new("b")),
            240,
        );
        assert_eq!(texts(&rt), ["b"]);
        assert!(
            !primitives(&rt)
                .iter()
                .any(|p| matches!(p, Primitive::Icon { .. }))
        );
        let rt = runtime(view! { staged.clone() => { TextLabel::new("b"); } }, 240);
        assert_eq!(texts(&rt), ["b"]);
        let rt = runtime(view! { staged => {} }, 240);
        assert!(texts(&rt).is_empty());
        assert!(
            !primitives(&rt)
                .iter()
                .any(|p| matches!(p, Primitive::Icon { .. }))
        );
        assert!(
            rt.arena()
                .get(rt.root_id().unwrap())
                .unwrap()
                .children()
                .is_empty()
        );
    }
    assert!(texts(&runtime(Button::new(), 240)).is_empty());
    assert_eq!(
        texts(&runtime(
            Button::new()
                .child(TextLabel::new("old"))
                .content(|cx, _| TextLabel::new("new").build(cx)),
            240
        )),
        ["new"]
    );
    let rt = runtime(Button::icon(LucideIcon::Plus, "New"), 240);
    assert_eq!(texts(&rt), ["New"]);
    assert!(
        primitives(&rt)
            .iter()
            .any(|p| matches!(p, Primitive::Icon { .. }))
    );
    assert!(texts(&runtime(IconButton::new(LucideIcon::Plus, "New"), 240)).is_empty());
}

#[test]
fn dynamic_overcount_is_a_construction_error_or_macro_site_panic() {
    let children = || Children::from_iter([TextLabel::new("a"), TextLabel::new("b")]);
    let error = Button::text("old").with_children(children()).err().unwrap();
    assert_eq!(error.widget, "Button");
    assert_eq!(error.received, 2);
    assert!(
        std::panic::catch_unwind(|| {
            view! { Button::new() => { for item in ["a", "b"] { TextLabel::new(item); } } }
        })
        .is_err()
    );
}

#[test]
fn macro_and_builder_have_identical_geometry_scenes_and_activation() {
    let a = runtime(Button::icon(LucideIcon::Plus, "New"), 240);
    let b = runtime(view! { Button::icon(LucideIcon::Plus, "New"); }, 240);
    assert_eq!(primitives(&a), primitives(&b));
    let a = runtime(
        Button::text("discard").child(TextLabel::new("Custom").color(Color::RED)),
        240,
    );
    let b = runtime(
        view! { Button::text("discard") => { TextLabel::new("Custom").color(Color::RED); } },
        240,
    );
    assert_eq!(primitives(&a), primitives(&b));
    let geometry = |rt: &Runtime| {
        ids(rt)
            .iter()
            .map(|id| rt.arena().get(*id).unwrap().layout_rect())
            .collect::<Vec<_>>()
    };
    assert_eq!(geometry(&a), geometry(&b));
    for macro_path in [false, true] {
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let button = Button::text("Click").on_click(move |_| observed.set(observed.get() + 1));
        let tree = if macro_path {
            view! { button; }
        } else {
            button.into_child_view()
        };
        let mut rt = runtime(tree, 240);
        pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Down);
        pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Up);
        assert!(rt.focus_first_focusable());
        key(&mut rt, Key::Enter, false);
        assert_eq!(calls.get(), 2);
    }
}

#[test]
fn fallback_text_ellipsizes_with_icon_budget_and_recovers_on_resize() {
    for icon in [false, true] {
        let text = "WW中文e\u{301}长标题iiii";
        let button = if icon {
            Button::icon(LucideIcon::Plus, text)
        } else {
            Button::text(text)
        };
        let mut rt = runtime(button, 100);
        let narrow = texts(&rt);
        assert_eq!(narrow.len(), 1);
        assert!(narrow[0].ends_with('…'));
        if icon {
            let p = primitives(&rt);
            let icon_rect = p
                .iter()
                .find_map(|p| match p {
                    Primitive::Icon { rect, .. } => Some(*rect),
                    _ => None,
                })
                .unwrap();
            let origin = p
                .iter()
                .find_map(|p| match p {
                    Primitive::Text { origin, .. } => Some(*origin),
                    _ => None,
                })
                .unwrap();
            assert!(origin.x >= icon_rect.max.x + Theme::default().spacing);
        }
        rt.set_viewport(Viewport::new(500, 160, 1.0));
        update(&mut rt);
        assert_eq!(texts(&rt), [text]);
        for width in [0, 1, 12, 32, 38] {
            rt.set_viewport(Viewport::new(width, 160, 1.0));
            update(&mut rt);
            let root = rt.arena().get(rt.root_id().unwrap()).unwrap();
            assert!(root.layout_rect().unwrap().size().width <= width as f32);
            assert!(
                ids(&rt)
                    .iter()
                    .all(|id| rt.arena().get(*id).unwrap().layout_error().is_none())
            );
            assert!(texts(&rt).is_empty());
        }
    }
}

#[test]
fn builtins_center_but_custom_colors_and_region_padding_remain_owned_by_callers() {
    let rt = runtime(
        ConstrainedBox::new()
            .min_width(200.0)
            .max_width(200.0)
            .child(Button::text("Hi")),
        240,
    );
    let p = primitives(&rt);
    let origin = p
        .iter()
        .find_map(|p| match p {
            Primitive::Text { origin, .. } => Some(*origin),
            _ => None,
        })
        .unwrap();
    assert_eq!(origin.x, (200.0 - 2.0 * rt.text_metrics().cell_width) / 2.0);
    let icon_rt = runtime(Button::icon(LucideIcon::Plus, "Hi"), 240);
    let natural = icon_rt
        .arena()
        .get(icon_rt.root_id().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap()
        .size()
        .width;
    assert_eq!(
        natural,
        32.0 + 20.0 + Theme::default().spacing + 2.0 * icon_rt.text_metrics().cell_width
    );
    let rt = runtime(
        Button::text("old")
            .disabled(true)
            .child(TextLabel::new("Custom").color(Color::RED)),
        240,
    );
    assert!(primitives(&rt).iter().any(|p| matches!(
        p,
        Primitive::Text {
            color: Color::RED,
            ..
        }
    )));
    let rt = runtime(
        harbor_widget::InteractiveRegion::new()
            .style(Theme::default().button)
            .child(
                harbor_widget::widgets::padding::Padding::all(3.0)
                    .unwrap()
                    .child(TextLabel::new("x")),
            ),
        240,
    );
    let width = rt
        .arena()
        .get(rt.root_id().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap()
        .size()
        .width;
    assert_eq!(width, rt.text_metrics().cell_width + 6.0 + 32.0);
}

#[test]
fn same_style_and_state_drive_builtin_text_icon_and_surface_and_theme_rebuilds() {
    let mut theme = Theme::default();
    theme.button.normal.foreground = Color::RED;
    theme.button.hovered.foreground = Color::GREEN;
    theme.button.pressed.foreground = Color::BLUE;
    theme.button.selected.foreground = Color::WHITE;
    theme.button.disabled.foreground = Color::BLACK;
    theme.icon_button.normal.foreground = Color::BLACK;
    let theme = Signal::new(theme);
    let flags = Signal::new((false, false));
    let observed = flags.clone();
    let mut rt = runtime(
        ThemeProvider::from_signal(theme.clone()).child(move |cx: &mut BuildCx| {
            cx.track(&observed);
            let (disabled, selected) = *observed.read();
            Button::icon(LucideIcon::Plus, "State")
                .disabled(disabled)
                .selected(selected)
                .build(cx)
        }),
        240,
    );
    let check = |rt: &Runtime, expected| {
        let colors: Vec<_> = primitives(rt)
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { color, .. } | Primitive::Icon { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert_eq!(colors, [expected, expected]);
    };
    check(&rt, Color::RED);
    pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Move);
    update(&mut rt);
    check(&rt, Color::GREEN);
    flags.set((false, true));
    update(&mut rt);
    check(&rt, Color::WHITE);
    pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Down);
    update(&mut rt);
    check(&rt, Color::BLUE);
    flags.set((true, true));
    update(&mut rt);
    check(&rt, Color::BLACK);
    assert!(rt.input().captor(0).is_none());
    flags.set((false, false));
    let mut next = theme.read().clone();
    next.button.hovered.foreground = Color::RED;
    theme.set(next);
    update(&mut rt);
    check(&rt, Color::RED);
    let mut override_style = Theme::default().button;
    override_style.normal.foreground = Color::GREEN;
    let rt = runtime(
        Button::icon(LucideIcon::Plus, "Override").style(override_style),
        240,
    );
    check(&rt, Color::GREEN);
}

#[test]
fn custom_content_preserves_full_surface_capture_nested_first_refusal_and_focus_order() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let parent = calls.clone();
    let child = calls.clone();
    let handle = FocusHandle::new();
    let mut rt = runtime(
        ConstrainedBox::new()
            .min_width(200.0)
            .min_height(80.0)
            .child(
                Button::new()
                    .focus_handle(handle)
                    .on_click(move |_| parent.borrow_mut().push("parent"))
                    .child(
                        Button::new()
                            .child(SizedBox::new(Size::new(20.0, 20.0)).unwrap())
                            .on_click(move |_| child.borrow_mut().push("child")),
                    ),
            ),
        240,
    );
    // Parent padding is interactive even though no content paints there.
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        pointer(&mut rt, Point::new(1.0, 1.0), phase);
    }
    assert_eq!(&*calls.borrow(), &["parent"]);
    let focusables: Vec<_> = ids(&rt)
        .into_iter()
        .filter(|id| rt.arena().get(*id).unwrap().is_focusable())
        .collect();
    assert_eq!(focusables.len(), 2);
    let child_rect = rt
        .arena()
        .get(*focusables.last().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap();
    let point = Point::new(child_rect.min.x + 2.0, child_rect.min.y + 2.0);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        pointer(&mut rt, point, phase);
    }
    assert_eq!(&*calls.borrow(), &["parent", "child"]);
    rt.request_focus(&handle);
    let focused_parent = rt.input().focused().unwrap();
    key(&mut rt, Key::Tab, false);
    assert_ne!(rt.input().focused(), Some(focused_parent));
    key(&mut rt, Key::Tab, true);
    assert_eq!(rt.input().focused(), Some(focused_parent));
    key(&mut rt, Key::Enter, false);
    key(&mut rt, Key::Space, false);
    assert_eq!(&*calls.borrow(), &["parent", "child", "parent", "parent"]);
    for (outside, expected) in [(true, 4), (false, 5)] {
        pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Down);
        assert!(rt.input().captor(0).is_some());
        pointer(&mut rt, Point::new(500.0, 200.0), PointerPhase::Move);
        if !outside {
            pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Move);
        }
        pointer(
            &mut rt,
            if outside {
                Point::new(500.0, 200.0)
            } else {
                Point::new(1.0, 1.0)
            },
            PointerPhase::Up,
        );
        assert_eq!(calls.borrow().len(), expected);
        assert!(rt.input().captor(0).is_none());
    }
    pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Down);
    pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Cancel);
    assert!(rt.input().captor(0).is_none());
    key(&mut rt, Key::Tab, false);
    key(&mut rt, Key::Tab, true);
    update(&mut rt);
    let delta = rt.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    let ring = items
        .iter()
        .find(|item| {
            matches!(item.primitive,
        Primitive::Border { color, .. } if color == Theme::default().button.focus_ring)
        })
        .unwrap();
    assert!(
        items
            .iter()
            .filter(|item| !matches!(item.primitive,
        Primitive::Border { color, .. } if color == Theme::default().button.focus_ring))
            .all(|item| item.paint_order < ring.paint_order),
        "focus ring paints above descendants"
    );
}

#[test]
fn content_has_independent_hooks_theme_and_subscriptions_and_releases_old_captures() {
    for mode_after in [1u8, 2, 3, 4] {
        let mode = Signal::new(0u8);
        let source = Signal::new(1u32);
        let state = Rc::new(RefCell::new(None::<Signal<u32>>));
        let builds = Rc::new(Cell::new(0));
        let tokens = Rc::new(RefCell::new(Vec::new()));
        let observed_mode = mode.clone();
        let observed_source = source.clone();
        let captured_state = state.clone();
        let captured_builds = builds.clone();
        let captured_tokens = tokens.clone();
        let mut theme = Theme::default();
        theme.button.normal.foreground = Color::GREEN;
        let mut rt = runtime(
            ThemeProvider::new(theme).child(move |cx: &mut BuildCx| {
                cx.track(&observed_mode);
                let parent_hook = cx.use_state(|| 77u32);
                assert_eq!(*parent_hook.read(), 77);
                let token = Rc::new(());
                captured_tokens.borrow_mut().push(Rc::downgrade(&token));
                let source = observed_source.clone();
                let state = captured_state.clone();
                let builds = captured_builds.clone();
                let button = Button::new().content(move |cx, _| {
                    let _keep_alive = &token;
                    cx.track(&source);
                    let local = cx.use_state(|| 10u32);
                    *state.borrow_mut() = Some(local.clone());
                    builds.set(builds.get() + 1);
                    TextLabel::new(format!("{}", *source.read() + *local.read()))
                        .color(cx.theme().button.normal.foreground)
                        .build(cx)
                });
                match *observed_mode.read() {
                    0 => view! { Column::new() => { button; } },
                    1 => {
                        view! { Column::new() => { button => { TextLabel::new("replacement"); } } }
                    }
                    2 => view! { Column::new() => { button => {} } },
                    3 => {
                        view! { Column::new() => { button.child(TextLabel::new("replacement")); } }
                    }
                    _ => SizedBox::new(Size::ZERO).unwrap().build(cx),
                }
            }),
            240,
        );
        assert_eq!(texts(&rt), ["11"]);
        let old_slot = slot(&rt);
        state.borrow().as_ref().unwrap().set(20);
        update(&mut rt);
        assert_eq!(texts(&rt), ["21"]);
        pointer(&mut rt, Point::new(1.0, 1.0), PointerPhase::Move);
        update(&mut rt);
        assert_eq!(slot(&rt), old_slot);
        assert_eq!(texts(&rt), ["21"]);
        assert!(primitives(&rt).iter().any(|p| matches!(
            p,
            Primitive::Text {
                color: Color::GREEN,
                ..
            }
        )));
        let old_tokens = tokens.borrow().clone();
        mode.set(mode_after);
        update(&mut rt);
        assert!(!rt.arena().contains(old_slot));
        assert!(old_tokens.iter().all(|t| t.upgrade().is_none()));
        let before = builds.get();
        source.set(99);
        state.borrow().as_ref().unwrap().set(100);
        update(&mut rt);
        assert_eq!(
            builds.get(),
            before,
            "removed content subscriptions must be gone"
        );
    }
}

#[test]
fn keyed_button_reorder_preserves_content_fiber_and_state() {
    let reverse = Signal::new(false);
    let observed = reverse.clone();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let mut rt = runtime(
        move |cx: &mut BuildCx| {
            cx.track(&observed);
            let button = |name| {
                let log = log.clone();
                Button::new()
                    .content(move |cx, _| {
                        let value = cx.use_state(|| name);
                        log.borrow_mut().push((name, *value.read()));
                        TextLabel::new(*value.read()).build(cx)
                    })
                    .keyed(name)
            };
            let order = if *observed.read() {
                ["b", "a"]
            } else {
                ["a", "b"]
            };
            Column::new().children(order.map(button)).build(cx)
        },
        240,
    );
    let children = rt
        .arena()
        .get(rt.root_id().unwrap())
        .unwrap()
        .children()
        .to_vec();
    reverse.set(true);
    update(&mut rt);
    let next = rt.arena().get(rt.root_id().unwrap()).unwrap().children();
    assert_eq!(next, [children[1], children[0]]);
    assert!(seen.borrow().iter().all(|(name, state)| name == state));
}
