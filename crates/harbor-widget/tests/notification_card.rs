use harbor_widget::{
    BuildCx, Component, NotificationCard, Portal, PositionAlignment, Theme, View,
    input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
    layout::{LayoutError, Point, Rect, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, Primitive},
    signal::Signal,
    widgets::notification_card::{NotificationCardPlacement, NotificationCardStyle},
};
use std::{cell::Cell, rc::Rc, time::Instant};

fn style_scalar(style: &mut NotificationCardStyle, index: usize) -> (&'static str, &mut f32) {
    match index {
        0 => ("max_width", &mut style.max_width),
        1 => ("corner_radius", &mut style.corner_radius),
        2 => ("horizontal_padding", &mut style.horizontal_padding),
        3 => ("header_height", &mut style.header_height),
        4 => ("bottom_padding", &mut style.bottom_padding),
        _ => ("close_width", &mut style.close_width),
    }
}

fn error<T, E: std::fmt::Display>(result: Result<T, E>, field: &str, rule: &str) {
    let message = result.err().expect("must reject raw input").to_string();
    assert!(message.contains(field), "{message}");
    assert!(message.contains(rule), "{message}");
}

fn placement(inset: f32) -> NotificationCardPlacement {
    NotificationCardPlacement {
        horizontal: PositionAlignment::End,
        vertical: PositionAlignment::Start,
        inset,
    }
}

#[test]
fn every_style_scalar_and_placement_inset_is_checked_at_each_consumption() {
    for index in 0..6 {
        for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut style = NotificationCardStyle::default();
            style.validate().unwrap();
            let (field, slot) = style_scalar(&mut style, index);
            *slot = value; // A previously validated public struct is not trusted.
            error(style.validate(), field, "finite");
            error(
                NotificationCard::new("", || {}).style(style),
                field,
                "finite",
            );
        }
        for value in [0.0, -0.0, 1.0, f32::MAX] {
            let mut style = NotificationCardStyle::default();
            *style_scalar(&mut style, index).1 = value;
            let result = NotificationCard::new("", || {}).style(style);
            if index == 0 && value == 0.0 {
                error(result, "max_width", "strictly positive");
            } else {
                assert!(result.is_ok());
            }
        }
    }
    for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut input = placement(0.0);
        input.validate().unwrap();
        input.inset = value;
        error(input.validate(), "inset", "finite and nonnegative");
        error(
            NotificationCard::new("", || {}).placement(input),
            "inset",
            "finite and nonnegative",
        );
    }
    for value in [0.0, -0.0, 12.0, f32::MAX] {
        assert!(
            NotificationCard::new("", || {})
                .placement(placement(value))
                .is_ok()
        );
    }
}

#[test]
fn every_optional_and_detail_rgba_channel_is_finite_only() {
    for index in 0..4 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut color = Color::WHITE;
            *match index {
                0 => &mut color.r,
                1 => &mut color.g,
                2 => &mut color.b,
                _ => &mut color.a,
            } = value;
            for foreground in [true, false] {
                let mut style = NotificationCardStyle::default();
                let field = if foreground {
                    style.foreground = Some(color);
                    "foreground"
                } else {
                    style.background = Some(color);
                    "background"
                };
                error(style.validate(), field, "finite");
                error(
                    NotificationCard::new("", || {}).style(style),
                    field,
                    "finite",
                );
            }
            error(
                NotificationCard::new("", || {}).detail_foreground(color),
                "detail_foreground",
                "finite",
            );
        }
    }
    let color = Color {
        r: -2.0,
        g: 3.0,
        b: f32::MAX,
        a: 2.0,
    };
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(600, 300, 1.0));
    runtime.set_root(
        NotificationCard::new("detail", || {})
            .title("title")
            .detail_foreground(color)
            .unwrap()
            .style(NotificationCardStyle {
                foreground: Some(color),
                background: Some(color),
                ..Default::default()
            })
            .unwrap(),
    );
    runtime.update(Instant::now());
    let items = &runtime.pending_delta().unwrap().added;
    assert!(items.iter().any(
        |item| matches!(item.primitive, Primitive::Quad { color: actual, .. } if actual == color)
    ));
    for text in ["title", "detail"] {
        assert!(items.iter().any(|item| matches!(&item.primitive, Primitive::Text { text: actual, color: actual_color, .. } if actual.as_ref() == text && *actual_color == color)));
    }
    // Complete inactive close-state validation belongs to the shared validator.
    let mut close_style = Theme::default().icon_button;
    close_style.disabled.border.a = f32::NAN;
    error(
        NotificationCard::new("", || {}).close_button_style(close_style),
        "disabled.border",
        "finite",
    );
}

fn pointer(runtime: &mut Runtime, at: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        at,
        phase,
        PointerButton::Left,
        1,
    )));
}

fn card_rect(runtime: &Runtime) -> Rect {
    runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .chain(&runtime.pending_delta().unwrap().modified)
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                color: Color::RED,
                ..
            } => Some(rect),
            _ => None,
        })
        .expect("card scene rectangle")
}

fn offset(alignment: PositionAlignment, free: f32) -> f32 {
    match alignment {
        PositionAlignment::Start => 0.0,
        PositionAlignment::Center => free * 0.5,
        PositionAlignment::End => free,
    }
}

#[test]
fn placement_close_hit_and_resize_cover_all_positioning_alignments() {
    let alignments = [
        PositionAlignment::Start,
        PositionAlignment::Center,
        PositionAlignment::End,
    ];
    for horizontal in alignments {
        for vertical in alignments {
            for close_alignment in alignments {
                for inset in [0.0, 17.0] {
                    for max_width in [180.0, 420.0] {
                        let dismissed = Rc::new(Cell::new(0));
                        let count = dismissed.clone();
                        let mut runtime = Runtime::new();
                        runtime.set_viewport(Viewport::new(600, 260, 1.0));
                        runtime.set_root(
                            Portal::new().child(
                                NotificationCard::new("line 00\nline 01\nline 02", move || {
                                    count.set(count.get() + 1)
                                })
                                .close_alignment(close_alignment)
                                .style(NotificationCardStyle {
                                    max_width,
                                    background: Some(Color::RED),
                                    ..Default::default()
                                })
                                .unwrap()
                                .placement(NotificationCardPlacement {
                                    horizontal,
                                    vertical,
                                    inset,
                                })
                                .unwrap(),
                            ),
                        );
                        let mut identity = None;
                        for (width, height) in [(600, 260), (300, 180)] {
                            runtime.set_viewport(Viewport::new(width, height, 1.0));
                            runtime.update(Instant::now());
                            let root = runtime.root_id().unwrap();
                            assert_eq!(
                                *identity.get_or_insert(root),
                                root,
                                "resize retains identity"
                            );
                            let rect = card_rect(&runtime);
                            let available =
                                Size::new(width as f32 - 2.0 * inset, height as f32 - 2.0 * inset);
                            assert_eq!(
                                rect.size(),
                                Size::new(max_width.min(available.width), 104.0)
                            );
                            assert_eq!(
                                rect.min,
                                Point::new(
                                    inset + offset(horizontal, available.width - rect.size().width),
                                    inset + offset(vertical, available.height - rect.size().height)
                                )
                            );
                            let close_center = Point::new(
                                rect.max.x - 18.0,
                                rect.min.y
                                    + offset(close_alignment, rect.size().height - 32.0)
                                    + 16.0,
                            );
                            let delta = runtime.pending_delta().unwrap();
                            let icon = delta
                                .added
                                .iter()
                                .chain(&delta.modified)
                                .find_map(|item| match item.primitive {
                                    Primitive::Icon {
                                        icon: harbor_widget::LucideIcon::X,
                                        rect,
                                        ..
                                    } => Some(rect),
                                    _ => None,
                                })
                                .expect("painted close icon");
                            assert_eq!(
                                Point::new(
                                    (icon.min.x + icon.max.x) / 2.0,
                                    (icon.min.y + icon.max.y) / 2.0
                                ),
                                close_center
                            );
                            // The right column outside the close control is also card body.
                            let outside_y = if close_alignment == PositionAlignment::Start {
                                rect.max.y - 1.0
                            } else {
                                rect.min.y + 1.0
                            };
                            for phase in [PointerPhase::Down, PointerPhase::Up] {
                                pointer(&mut runtime, Point::new(close_center.x, outside_y), phase);
                            }
                            assert_eq!(dismissed.get(), 0);
                            // Clicking the card body must not dismiss it.
                            for phase in [PointerPhase::Down, PointerPhase::Up] {
                                pointer(
                                    &mut runtime,
                                    Point::new(rect.min.x + 1.0, close_center.y),
                                    phase,
                                );
                            }
                            assert_eq!(dismissed.get(), 0);
                            if width == 300 {
                                for phase in [PointerPhase::Down, PointerPhase::Up] {
                                    pointer(&mut runtime, close_center, phase);
                                }
                                assert_eq!(dismissed.get(), 1);
                            }
                            assert!(runtime.input().focused().is_none());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn default_dimensions_close_center_and_large_valid_insets_are_preserved() {
    let style = NotificationCardStyle::default();
    assert_eq!(
        [
            style.max_width,
            style.corner_radius,
            style.horizontal_padding,
            style.header_height,
            style.bottom_padding,
            style.close_width
        ],
        [520.0, 6.0, 12.0, 32.0, 12.0, 36.0]
    );
    let dismissed = Rc::new(Cell::new(0));
    let count = dismissed.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(600, 300, 1.0));
    runtime.set_root(
        NotificationCard::new("one\ntwo\nthree", move || count.set(count.get() + 1))
            .style(NotificationCardStyle {
                background: Some(Color::RED),
                ..Default::default()
            })
            .unwrap(),
    );
    runtime.update(Instant::now());
    let rect = card_rect(&runtime);
    assert_eq!(rect.size(), Size::new(520.0, 104.0));
    let center = Point::new(rect.max.x - 18.0, rect.min.y + rect.size().height / 2.0);
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        pointer(&mut runtime, center, phase);
    }
    assert_eq!(dismissed.get(), 1);
    runtime.set_root(
        NotificationCard::new("inset", || {})
            .placement(placement(f32::MAX))
            .unwrap(),
    );
    runtime.update(Instant::now());
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .layout_error(),
        None
    );
}

#[test]
fn legal_combinations_overflow_only_during_layout_without_replacing_committed_geometry() {
    for overflow in [
        NotificationCardStyle {
            horizontal_padding: f32::MAX,
            ..Default::default()
        },
        NotificationCardStyle {
            close_width: f32::MAX,
            horizontal_padding: f32::MAX,
            ..Default::default()
        },
        NotificationCardStyle {
            header_height: f32::MAX,
            bottom_padding: f32::MAX,
            ..Default::default()
        },
    ] {
        let input = Signal::new(NotificationCardStyle::default());
        let style = input.clone();
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(120, 80, 1.0));
        runtime.set_root(move |cx: &mut BuildCx| {
            cx.track(&style);
            NotificationCard::new("wrapped detail", || {})
                .style(style.read().clone())
                .unwrap()
                .build(cx)
        });
        runtime.update(Instant::now());
        let root = runtime.root_id().unwrap();
        let previous = runtime.arena().get(root).unwrap().layout_rect();
        assert!(
            NotificationCard::new("", || {})
                .style(overflow.clone())
                .is_ok(),
            "raw fields are legal"
        );
        input.set(overflow);
        runtime.update(Instant::now());
        let fiber = runtime.arena().get(root).unwrap();
        assert_eq!(fiber.layout_error(), Some(LayoutError::InvalidGeometry));
        assert_eq!(fiber.layout_rect(), previous);
        input.set(NotificationCardStyle::default());
        runtime.update(Instant::now());
        assert_eq!(runtime.arena().get(root).unwrap().layout_error(), None);
    }
}

#[test]
fn scroll_and_pressed_close_survive_stable_rebuild_without_gaining_focus_or_timeout() {
    let rebuild = Signal::new(0);
    let input = rebuild.clone();
    let dismissed = Rc::new(Cell::new(0));
    let count = dismissed.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(240, 124, 1.0));
    runtime.set_root(move |cx: &mut BuildCx| -> View {
        cx.track(&input);
        let _ = *input.read();
        let count = count.clone();
        NotificationCard::new(
            (0..30)
                .map(|i| format!("line {i:02}"))
                .collect::<Vec<_>>()
                .join("\n"),
            move || count.set(count.get() + 1),
        )
        .style(NotificationCardStyle {
            background: Some(Color::RED),
            ..Default::default()
        })
        .unwrap()
        .build(cx)
    });
    runtime.update(Instant::now());
    let root = runtime.root_id().unwrap();
    let rect = card_rect(&runtime);
    pointer(
        &mut runtime,
        Point::new(10.0, 70.0),
        PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
    );
    let close = Point::new(rect.max.x - 18.0, rect.size().height / 2.0);
    pointer(&mut runtime, close, PointerPhase::Down);
    rebuild.set(1);
    runtime.update(Instant::now() + std::time::Duration::from_secs(3600));
    assert_eq!(runtime.root_id(), Some(root));
    let delta = runtime.pending_delta().unwrap();
    assert!(delta.added.iter().chain(&delta.modified).any(|item| matches!(&item.primitive, Primitive::Text { text, origin, .. } if text.as_ref() == "line 01" && origin.y == 32.0)));
    pointer(&mut runtime, close, PointerPhase::Up);
    assert_eq!(dismissed.get(), 1);
    assert!(runtime.input().focused().is_none());
    runtime.set_root(harbor_widget::TextLabel::new("replacement"));
    rebuild.set(2);
    runtime.update(Instant::now());
    assert!(!runtime.arena().contains(root));
    assert_eq!(dismissed.get(), 1, "unmount is not a dismissal");
}

#[test]
fn invalid_final_card_configuration_uses_the_existing_macro_error_boundary() {
    fn panic_message(action: impl FnOnce()) -> String {
        let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action))
            .expect_err("invalid final node panics at construction");
        value
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| value.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap()
    }
    let message = panic_message(|| {
        let _ = harbor_widget::view! { NotificationCard::new("", || {}).style(NotificationCardStyle {
            max_width: 0.0, ..Default::default()
        }); };
    });
    assert!(
        message.contains(file!())
            && message.contains("max_width must be finite and strictly positive"),
        "{message}"
    );
    let message = panic_message(|| {
        let _ =
            harbor_widget::view! { NotificationCard::new("", || {}).placement(placement(-1.0)); };
    });
    assert!(
        message.contains(file!()) && message.contains("inset must be finite and nonnegative"),
        "{message}"
    );
    let message = panic_message(|| {
        let _ = harbor_widget::view! { NotificationCard::new("", || {}).detail_foreground(Color { a: f32::NAN, ..Color::WHITE }); };
    });
    assert!(
        message.contains(file!())
            && message.contains("detail_foreground")
            && message.contains("finite"),
        "{message}"
    );
}

#[test]
fn inline_parent_constraints_and_unbounded_scroll_measurement_keep_their_authority() {
    for minimum in [0.0, 580.0] {
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(600, 300, 1.0));
        runtime.set_root(
            harbor_widget::ScrollView::new().child(
                harbor_widget::ConstrainedBox::new()
                    .min_width(minimum)
                    .child(
                        NotificationCard::new("inline", || {})
                            .style(NotificationCardStyle {
                                background: Some(Color::RED),
                                ..Default::default()
                            })
                            .unwrap(),
                    ),
            ),
        );
        runtime.update(Instant::now());
        let rect = card_rect(&runtime);
        assert_eq!(rect.size(), Size::new(520.0_f32.max(minimum), 64.0));
        assert_eq!(
            runtime
                .arena()
                .get(runtime.root_id().unwrap())
                .unwrap()
                .layout_error(),
            None
        );
    }
}
