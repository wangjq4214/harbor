//! Generic controlled inputs: actual mounted scenes, focus and routed events.
#[path = "support/control_input.rs"]
mod control_input;
use control_input::{key, pointer, update};
use harbor_widget::{
    Button, Checkbox, Column, ControlStyle, IntoChildView, LucideIcon, SegmentedControl, Theme,
    input::event::{Key, PointerPhase},
    layout::{Point, Rect},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, Primitive},
    signal::Signal,
    view::{BuildCx, Component},
};
use std::{cell::RefCell, rc::Rc};

fn runtime(tree: impl IntoChildView + 'static) -> Runtime {
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(360, 400, 1.0));
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

fn checkmarks(rt: &Runtime) -> usize {
    primitives(rt)
        .iter()
        .filter(|p| {
            matches!(
                p,
                Primitive::Icon {
                    icon: LucideIcon::Check,
                    ..
                }
            )
        })
        .count()
}

fn selected_labels(rt: &Runtime) -> Vec<String> {
    primitives(rt)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Text {
                text,
                color: Color::GREEN,
                ..
            } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn focus_ring(rt: &Runtime) -> Option<Rect> {
    primitives(rt).iter().find_map(|p| match p {
        Primitive::Border {
            rect,
            color: Color::RED,
            ..
        } => Some(*rect),
        _ => None,
    })
}

fn style() -> ControlStyle {
    let mut style = Theme::default().button;
    style.normal.foreground = Color::BLUE;
    style.hovered.foreground = Color::BLUE;
    style.selected.foreground = Color::GREEN;
    style.disabled.foreground = Color::WHITE;
    style.focus_ring = Color::RED;
    style
}

fn click(rt: &mut Runtime, position: Point) {
    pointer(rt, position, PointerPhase::Down);
    pointer(rt, position, PointerPhase::Up);
    update(rt);
}

fn choices() -> [(String, String); 3] {
    [
        ("a".into(), "Alpha".into()),
        ("b".into(), "Beta".into()),
        ("c".into(), "Gamma".into()),
    ]
}

#[test]
fn checkbox_artwork_and_change_intent_remain_caller_controlled() {
    let checked = Signal::new(false);
    let value = checked.clone();
    let intents = Rc::new(RefCell::new(Vec::new()));
    let log = intents.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        let log = log.clone();
        Checkbox::new("Include item", *value.read())
            .on_change(move |next| log.borrow_mut().push(next))
            .style(style())
            .unwrap()
            .build(cx)
    });
    assert_eq!(checkmarks(&rt), 0);
    assert!(primitives(&rt).iter().any(|p| matches!(
        p,
        Primitive::Border {
            color: Color::BLUE,
            ..
        }
    )));
    assert!(
        primitives(&rt)
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if &**text == "Include item"))
    );
    click(&mut rt, Point::new(5.0, 5.0));
    key(&mut rt, Key::Space, false);
    key(&mut rt, Key::Enter, false);
    update(&mut rt);
    assert_eq!(*intents.borrow(), [true, true, true]);
    assert!(
        !*checked.read(),
        "callbacks are intent, not a local value mutation"
    );
    assert_eq!(checkmarks(&rt), 0);

    checked.set(true);
    update(&mut rt);
    assert_eq!(checkmarks(&rt), 1);
    assert_eq!(selected_labels(&rt), ["Include item"]);
    key(&mut rt, Key::Space, false);
    update(&mut rt);
    assert_eq!(*intents.borrow(), [true, true, true, false]);
    assert!(*checked.read());
    assert_eq!(checkmarks(&rt), 1);
}

#[test]
fn checkbox_keyboard_focus_is_visible_and_disabling_retires_input() {
    let disabled = Signal::new(false);
    let value = disabled.clone();
    let intents = Rc::new(RefCell::new(Vec::new()));
    let log = intents.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        let log = log.clone();
        Checkbox::new("Check", true)
            .disabled(*value.read())
            .on_change(move |next| log.borrow_mut().push(next))
            .style(style())
            .unwrap()
            .build(cx)
    });
    assert!(focus_ring(&rt).is_none());
    key(&mut rt, Key::Tab, false);
    update(&mut rt);
    assert!(rt.input().focused().is_some());
    assert!(focus_ring(&rt).is_some());
    key(&mut rt, Key::Space, false);
    assert_eq!(*intents.borrow(), [false]);

    // Disable while a pointer is held: stale release cannot activate it.
    pointer(&mut rt, Point::new(5.0, 5.0), PointerPhase::Down);
    assert!(rt.input().captor(0).is_some());
    disabled.set(true);
    update(&mut rt);
    assert!(rt.input().captor(0).is_none());
    assert!(!rt.focus_first_focusable());
    key(&mut rt, Key::Space, false);
    key(&mut rt, Key::Enter, false);
    pointer(&mut rt, Point::new(5.0, 5.0), PointerPhase::Up);
    click(&mut rt, Point::new(5.0, 5.0));
    assert_eq!(*intents.borrow(), [false]);
    assert!(focus_ring(&rt).is_none());
    assert_eq!(
        checkmarks(&rt),
        1,
        "disabled still displays supplied checked value"
    );
    assert!(primitives(&rt).iter().any(|p| matches!(
        p,
        Primitive::Text {
            color: Color::WHITE,
            ..
        }
    )));
}

#[test]
fn segments_emit_pointer_and_tab_space_enter_intent_without_owning_selection() {
    let selected = Signal::new("a".to_string());
    let value = selected.clone();
    let intents = Rc::new(RefCell::new(Vec::new()));
    let log = intents.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        let log = log.clone();
        SegmentedControl::new(choices(), value.read().clone())
            .on_change(move |next| log.borrow_mut().push(next))
            .style(style())
            .unwrap()
            .build(cx)
    });
    assert_eq!(selected_labels(&rt), ["Alpha"]);
    // Equal-width segments share the finite width without overflowing.
    click(&mut rt, Point::new(180.0, 5.0));
    assert_eq!(*intents.borrow(), ["b"]);
    assert_eq!(*selected.read(), "a");
    assert_eq!(selected_labels(&rt), ["Alpha"]);
    assert!(
        focus_ring(&rt).is_none(),
        "pointer focus is not keyboard-visible"
    );

    key(&mut rt, Key::Tab, false);
    update(&mut rt);
    assert!(focus_ring(&rt).unwrap().min.x >= 240.0);
    assert_eq!(*intents.borrow(), ["b"], "focus is not selection");
    key(&mut rt, Key::Space, false);
    key(&mut rt, Key::Tab, true);
    key(&mut rt, Key::Enter, false);
    update(&mut rt);
    assert_eq!(*intents.borrow(), ["b", "c", "b"]);
    assert_eq!(selected_labels(&rt), ["Alpha"]);

    selected.set("b".into());
    update(&mut rt);
    assert_eq!(selected_labels(&rt), ["Beta"]);
    click(&mut rt, Point::new(300.0, 5.0));
    assert_eq!(intents.borrow().last().unwrap(), "c");
    assert_eq!(selected_labels(&rt), ["Beta"]);
}

#[test]
fn disabled_segments_ignore_pointer_keyboard_and_are_skipped_by_normal_tab_order() {
    let disabled = Signal::new(false);
    let value = disabled.clone();
    let intents = Rc::new(RefCell::new(Vec::new()));
    let log = intents.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        let log = log.clone();
        Column::new()
            .children([
                Button::text("Before").into_child_view(),
                SegmentedControl::new(choices(), "a".into())
                    .disabled(*value.read())
                    .on_change(move |next| log.borrow_mut().push(next))
                    .style(style())
                    .unwrap()
                    .into_child_view(),
                Button::text("After").into_child_view(),
            ])
            .build(cx)
    });
    key(&mut rt, Key::Tab, false); // before
    let before = rt.input().focused().unwrap();
    key(&mut rt, Key::Tab, false); // alpha
    update(&mut rt);
    assert!(focus_ring(&rt).is_some());
    disabled.set(true);
    update(&mut rt);
    key(&mut rt, Key::Space, false);
    key(&mut rt, Key::Enter, false);
    click(
        &mut rt,
        Point::new(180.0, Theme::default().button.min_height + 5.0),
    );
    assert!(intents.borrow().is_empty());
    assert!(focus_ring(&rt).is_none());
    assert_eq!(
        checkmarks(&rt),
        1,
        "disabled selection retains a non-color indicator"
    );
    let selected_marker = primitives(&rt)
        .into_iter()
        .find_map(|p| match p {
            Primitive::Icon {
                icon: LucideIcon::Check,
                rect,
                ..
            } => Some(rect),
            _ => None,
        })
        .unwrap();
    assert!(
        selected_marker.max.x < 120.0,
        "only caller-selected Alpha is marked"
    );
    // Focus fallback after disabling may choose either surrounding control.
    // Start at a known neighbor and verify ordinary forward/backward traversal.
    rt.set_focus(before);
    key(&mut rt, Key::Tab, false);
    let after = rt.input().focused().unwrap();
    assert_ne!(before, after);
    assert!(
        rt.arena().get(after).unwrap().layout_rect().unwrap().min.y
            >= 2.0 * Theme::default().button.min_height
    );
    key(&mut rt, Key::Tab, true);
    assert_eq!(rt.input().focused(), Some(before));
}

#[test]
fn checkbox_label_wraps_without_source_loss_across_resize() {
    let source = "A complete Unicode e\u{0301} 👩\u{200d}💻 项目 explanation for this choice";
    let mut rt = runtime(Checkbox::new(source, true));
    let root = rt.root_id().unwrap();
    for width in [100, 360, 70, 800, 100] {
        rt.set_viewport(Viewport::new(width, 1600, 1.0));
        update(&mut rt);
        let mut lines: Vec<_> = primitives(&rt)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Text { text, origin, .. } => Some((origin.y, text.to_string())),
                _ => None,
            })
            .collect();
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(
            lines
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<String>(),
            source
        );
        let rect = rt.arena().get(root).unwrap().layout_rect().unwrap();
        assert!(rt.arena().get(root).unwrap().layout_error().is_none());
        assert!(rect.size().width <= width as f32);
        assert_eq!(checkmarks(&rt), 1);
        if width <= 100 {
            assert!(lines.len() > 1);
            assert!(rect.size().height > Theme::default().button.min_height);
        }
    }
}

#[test]
fn keyed_segments_preserve_choice_focus_when_caller_reorders_choices() {
    let reverse = Signal::new(false);
    let observed = reverse.clone();
    let intents = Rc::new(RefCell::new(Vec::new()));
    let log = intents.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&observed);
        let log = log.clone();
        let mut choices = choices();
        if *observed.read() {
            choices.reverse();
        }
        SegmentedControl::new(choices, "b".into())
            .on_change(move |value| log.borrow_mut().push(value))
            .style(style())
            .unwrap()
            .build(cx)
    });
    click(&mut rt, Point::new(30.0, 5.0)); // alpha
    let alpha = rt.input().focused();
    reverse.set(true);
    update(&mut rt);
    assert_eq!(rt.input().focused(), alpha);
    assert!(
        rt.arena()
            .get(alpha.unwrap())
            .unwrap()
            .layout_rect()
            .unwrap()
            .min
            .x
            >= 240.0
    );
    key(&mut rt, Key::Space, false);
    assert_eq!(*intents.borrow(), ["a", "a"]);
    assert_eq!(selected_labels(&rt), ["Beta"]);
}

#[test]
fn selection_edge_cases_and_complete_style_validation_are_generic() {
    let mut rt = runtime(SegmentedControl::new([], String::new()));
    assert!(!rt.focus_first_focusable());
    let mut rt = runtime(
        SegmentedControl::new(choices(), "unknown".into())
            .style(style())
            .unwrap(),
    );
    assert!(selected_labels(&rt).is_empty());
    assert!(rt.focus_first_focusable());

    let rt = runtime(
        SegmentedControl::new(
            [
                ("a".into(), "First".into()),
                ("a".into(), "Duplicate".into()),
            ],
            "a".into(),
        )
        .style(style())
        .unwrap(),
    );
    assert_eq!(selected_labels(&rt), ["First"]);

    for invalid in [f32::NAN, -1.0, f32::INFINITY] {
        let mut bad = style();
        bad.icon_size = invalid;
        assert!(Checkbox::new("", false).style(bad.clone()).is_err());
        assert!(
            SegmentedControl::new(choices(), "a".into())
                .style(bad)
                .is_err()
        );
    }
    let mut bad = style();
    bad.disabled.foreground.r = f32::NAN;
    assert!(Checkbox::new("", false).style(bad.clone()).is_err());
    assert!(
        SegmentedControl::new(choices(), "a".into())
            .style(bad)
            .is_err()
    );
}

#[test]
fn selected_segment_keeps_a_shape_marker_when_hovered_and_labels_survive_narrow_layout() {
    let selected = Signal::new("ssh".to_string());
    let value = selected.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        SegmentedControl::new(
            [
                ("local".into(), "Local shell".into()),
                ("wsl".into(), "WSL distribution".into()),
                ("ssh".into(), "Direct SSH destination".into()),
            ],
            value.read().clone(),
        )
        .style(style())
        .unwrap()
        .build(cx)
    });
    assert_eq!(
        checkmarks(&rt),
        1,
        "enabled selection cannot rely on color alone"
    );
    pointer(&mut rt, Point::new(300.0, 5.0), PointerPhase::Move);
    update(&mut rt);
    assert_eq!(checkmarks(&rt), 1, "hover feedback cannot erase selection");
    for width in [360, 180, 240, 600] {
        rt.set_viewport(Viewport::new(width, 800, 1.0));
        update(&mut rt);
        for (index, expected) in ["Local shell", "WSL distribution", "Direct SSH destination"]
            .into_iter()
            .enumerate()
        {
            let mut lines: Vec<_> = primitives(&rt)
                .into_iter()
                .filter_map(|p| match p {
                    Primitive::Text { text, origin, .. }
                        if origin.x >= index as f32 * width as f32 / 3.0
                            && origin.x < (index + 1) as f32 * width as f32 / 3.0 =>
                    {
                        Some((origin.y, text.to_string()))
                    }
                    _ => None,
                })
                .collect();
            lines.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(
                lines.into_iter().map(|(_, s)| s).collect::<String>(),
                expected
            );
        }
        assert_eq!(checkmarks(&rt), 1);
    }
    selected.set("local".into());
    update(&mut rt);
    assert_eq!(checkmarks(&rt), 1);
}

#[test]
fn segment_labels_are_centered_and_do_not_move_when_selection_changes() {
    let selected = Signal::new(String::new());
    let value = selected.clone();
    let disabled = Signal::new(false);
    let disabled_value = disabled.clone();
    let mut rt = runtime(move |cx: &mut BuildCx| {
        cx.track(&value);
        cx.track(&disabled_value);
        SegmentedControl::new(choices(), value.read().clone())
            .disabled(*disabled_value.read())
            .style(style())
            .unwrap()
            .build(cx)
    });
    let text_positions = |rt: &Runtime| {
        primitives(rt)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Text { text, origin, .. } => Some((text.to_string(), origin)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for width in [900, 360, 180, 900] {
        rt.set_viewport(Viewport::new(width, 1000, 1.0));
        selected.set(String::new());
        disabled.set(false);
        update(&mut rt);
        let baseline = text_positions(&rt);
        if width >= 360 {
            for (index, (_, label)) in choices().into_iter().enumerate() {
                let origin = baseline.iter().find(|(text, _)| text == &label).unwrap().1;
                let text_width = label.chars().count() as f32 * rt.text_metrics().cell_width;
                let center = (index as f32 + 0.5) * width as f32 / 3.0;
                assert!((origin.x + text_width / 2.0 - center).abs() < 0.01);
            }
        }
        for selection in ["a", "b", "c", "unknown"] {
            selected.set(selection.into());
            update(&mut rt);
            assert_eq!(
                text_positions(&rt),
                baseline,
                "selection must not move text"
            );
            assert_eq!(checkmarks(&rt), usize::from(selection != "unknown"));
            pointer(
                &mut rt,
                Point::new(width as f32 / 2.0, 5.0),
                PointerPhase::Move,
            );
            key(&mut rt, Key::Tab, false);
            update(&mut rt);
            assert_eq!(
                text_positions(&rt),
                baseline,
                "hover/focus must not move text"
            );
            disabled.set(true);
            update(&mut rt);
            assert_eq!(text_positions(&rt), baseline, "disabled must not move text");
            disabled.set(false);
            update(&mut rt);
        }
    }
}

#[test]
fn segmented_material_is_theme_injected_local_and_retained_across_states() {
    use harbor_widget::{
        BackdropMaterial, BackdropRim, BackdropTone, ControlBackdrop, EdgeRefraction, ThemeProvider,
    };
    let material = BackdropMaterial::new(
        4.0,
        -0.1,
        BackdropTone {
            offset: 0.0,
            slope: 1.0,
            quadratic: 0.0,
            saturation: 1.02,
        },
        BackdropRim {
            highlight: 0.3,
            shade: 0.22,
        },
    )
    .unwrap();
    let refraction = EdgeRefraction::new(6.0, 2.0).unwrap();
    for sigma in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(ControlBackdrop::new(sigma, refraction, material).is_err());
    }
    let mut theme = Theme::default();
    theme.button.selected_backdrop =
        Some(ControlBackdrop::new(f32::MAX, refraction, material).unwrap());
    theme.button.selected.background.a = 0.1;
    theme.button.selected.border = Color::TRANSPARENT;
    let themed = Signal::new(theme.clone());
    let selected = Signal::new("a".to_string());
    let value = selected.clone();
    let disabled = Signal::new(false);
    let disabled_value = disabled.clone();
    let mut rt = runtime(ThemeProvider::from_signal(themed.clone()).child(
        move |cx: &mut BuildCx| {
            cx.track(&value);
            cx.track(&disabled_value);
            SegmentedControl::new(choices(), value.read().clone())
                .disabled(*disabled_value.read())
                .build(cx)
        },
    ));
    let material_rects = |rt: &Runtime| {
        primitives(rt)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::BackdropMaterial {
                    rect,
                    sigma,
                    refraction: actual,
                    material: effect,
                    ..
                } => {
                    assert_eq!(sigma, 64.0, "reuse the existing bounded filter");
                    assert_eq!(actual, refraction);
                    assert_eq!(effect, material);
                    Some(rect)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for (index, (value, _)) in choices().into_iter().enumerate() {
        selected.set(value);
        update(&mut rt);
        let rects = material_rects(&rt);
        assert_eq!(rects.len(), 1);
        let rect = rects[0];
        assert_eq!(rect.min.x, index as f32 * 120.0);
        assert_eq!(rect.size().width, 120.0);
        let center = Point::new(rect.min.x + 60.0, 5.0);
        for phase in [PointerPhase::Move, PointerPhase::Down, PointerPhase::Up] {
            pointer(&mut rt, center, phase);
            update(&mut rt);
            assert_eq!(material_rects(&rt), rects);
            assert_eq!(checkmarks(&rt), 1);
        }
        disabled.set(true);
        update(&mut rt);
        assert_eq!(material_rects(&rt), rects);
        disabled.set(false);
        update(&mut rt);
    }
    selected.set("unknown".into());
    update(&mut rt);
    assert!(material_rects(&rt).is_empty());
    selected.set("a".into());
    update(&mut rt);
    assert_eq!(material_rects(&rt).len(), 1);
    theme.button.selected_backdrop = None;
    themed.set(theme);
    update(&mut rt);
    assert!(
        material_rects(&rt).is_empty(),
        "theme changes remove the material"
    );
    assert_eq!(checkmarks(&rt), 1);
}

#[test]
fn segment_gaps_are_validated_and_keep_each_surface_separate_under_resize() {
    for gap in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(
            SegmentedControl::new(choices(), "a".into())
                .gap(gap)
                .is_err()
        );
    }
    let mut rt = runtime(
        SegmentedControl::new(choices(), "a".into())
            .gap(6.0)
            .unwrap(),
    );
    for width in [360, 180, 600] {
        rt.set_viewport(Viewport::new(width, 400, 1.0));
        update(&mut rt);
        let mut surfaces: Vec<_> = primitives(&rt)
            .into_iter()
            .filter_map(|p| match p {
                Primitive::Quad { rect, .. } => Some(rect),
                _ => None,
            })
            .collect();
        surfaces.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        assert_eq!(surfaces.len(), 3);
        assert_eq!(surfaces[0].min.x, 0.0);
        assert_eq!(surfaces[2].max.x, width as f32);
        for pair in surfaces.windows(2) {
            assert_eq!(pair[1].min.x - pair[0].max.x, 6.0);
            assert_eq!(pair[0].size().width, pair[1].size().width);
        }
    }
}
