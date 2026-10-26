use harbor_widget::{
    Button, ControlStyle, EditableText, Icon, IconButton, InteractiveRegion, LucideIcon, SizedBox,
    TextEditingController, TextField, Theme, ThemeProvider,
    input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
    layout::{LayoutError, Point, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, Primitive},
    signal::Signal,
    view::{BuildCx, Component},
    widgets::{Separator, text_label::TextLabel},
};
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::Instant,
};

fn error<T, E: std::fmt::Display>(result: Result<T, E>, field: &str, rule: &str) {
    let message = result
        .err()
        .expect("invalid input must be recoverably rejected")
        .to_string();
    assert!(message.contains(field), "{message}");
    assert!(message.contains(rule), "{message}");
}

fn panic_message(action: impl FnOnce()) -> String {
    // A fresh Runtime is discarded after each panic: this is not a recovery test.
    let panic =
        catch_unwind(AssertUnwindSafe(action)).expect_err("invalid dynamic input must panic");
    if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        panic.downcast_ref::<&str>().unwrap().to_string()
    }
}

fn mount(component: impl Component + 'static) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(120, 80, 1.0));
    runtime.set_root(component);
    runtime.update(Instant::now());
    runtime
}

fn scalar(style: &mut ControlStyle, index: usize) -> (&'static str, &mut f32) {
    match index {
        0 => ("min_height", &mut style.min_height),
        1 => ("horizontal_padding", &mut style.horizontal_padding),
        2 => ("icon_size", &mut style.icon_size),
        3 => ("border_width", &mut style.border_width),
        _ => ("radius", &mut style.radius),
    }
}

fn state_color(style: &mut ControlStyle, state: usize, kind: usize) -> &mut Color {
    let colors = match state {
        0 => &mut style.normal,
        1 => &mut style.hovered,
        2 => &mut style.pressed,
        3 => &mut style.selected,
        _ => &mut style.disabled,
    };
    match kind {
        0 => &mut colors.background,
        1 => &mut colors.border,
        _ => &mut colors.foreground,
    }
}

fn channel(color: &mut Color, index: usize) -> &mut f32 {
    match index {
        0 => &mut color.r,
        1 => &mut color.g,
        2 => &mut color.b,
        _ => &mut color.a,
    }
}

#[test]
fn scalar_entries_reject_at_construction_and_accept_zero_and_large_finite_values() {
    for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        error(
            Icon::new(LucideIcon::X).size(value),
            "Icon.size",
            "finite and nonnegative",
        );
        error(
            IconButton::new(LucideIcon::X, "x").icon_size(value),
            "IconButton.icon_size",
            "finite and nonnegative",
        );
        error(
            TextField::new(TextEditingController::new("")).padding(value),
            "TextField.padding",
            "finite and nonnegative",
        );
        for separator in [Separator::horizontal(), Separator::vertical()] {
            error(
                separator.thickness(value),
                "Separator.thickness",
                "finite and nonnegative",
            );
        }
        let theme = Theme {
            spacing: value,
            ..Theme::default()
        };
        error(
            ThemeProvider::new(theme),
            "Theme.spacing",
            "finite and nonnegative",
        );
        for index in 0..5 {
            let mut style = Theme::default().button;
            let (field, slot) = scalar(&mut style, index);
            *slot = value;
            error(
                Button::new().style(style.clone()),
                field,
                "finite and nonnegative",
            );
            error(
                InteractiveRegion::new().style(style.clone()),
                field,
                "finite and nonnegative",
            );
            error(
                harbor_widget::NotificationCard::new("notice", || {})
                    .close_button_style(style.clone()),
                field,
                "finite and nonnegative",
            );
            error(
                IconButton::new(LucideIcon::X, "x").style(style.clone()),
                field,
                "finite and nonnegative",
            );
            for icon_role in [false, true] {
                let mut theme = Theme::default();
                if icon_role {
                    theme.icon_button = style.clone();
                } else {
                    theme.button = style.clone();
                }
                error(
                    ThemeProvider::new(theme),
                    if icon_role {
                        "Theme.icon_button"
                    } else {
                        "Theme.button"
                    },
                    field,
                );
            }
        }
    }
    for value in [0.0, -0.0, f32::MAX] {
        assert!(Icon::new(LucideIcon::X).size(value).is_ok());
        assert!(IconButton::new(LucideIcon::X, "x").icon_size(value).is_ok());
        assert!(
            TextField::new(TextEditingController::new(""))
                .padding(value)
                .is_ok()
        );
        assert!(Separator::horizontal().thickness(value).is_ok());
        let mut theme = Theme {
            spacing: value,
            ..Theme::default()
        };
        for index in 0..5 {
            *scalar(&mut theme.button, index).1 = value;
            *scalar(&mut theme.icon_button, index).1 = value;
        }
        assert!(ThemeProvider::new(theme.clone()).is_ok());
        assert!(Button::new().style(theme.button.clone()).is_ok());
        assert!(
            IconButton::new(LucideIcon::X, "x")
                .style(theme.icon_button)
                .is_ok()
        );
    }
}

#[test]
fn every_rgba_component_in_every_state_and_focus_ring_is_validated_by_consumers() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for state in 0..5 {
            for kind in 0..3 {
                for component in 0..4 {
                    let mut style = Theme::default().button;
                    *channel(state_color(&mut style, state, kind), component) = value;
                    let field = format!(
                        "{}.{}",
                        ["normal", "hovered", "pressed", "selected", "disabled"][state],
                        ["background", "border", "foreground"][kind]
                    );
                    error(Button::new().style(style.clone()), &field, "finite RGBA");
                    error(
                        InteractiveRegion::new().style(style.clone()),
                        &field,
                        "finite RGBA",
                    );
                    error(
                        harbor_widget::NotificationCard::new("notice", || {})
                            .close_button_style(style.clone()),
                        &field,
                        "finite RGBA",
                    );
                    error(
                        IconButton::new(LucideIcon::X, "x").style(style.clone()),
                        &field,
                        "finite RGBA",
                    );
                    for icon_role in [false, true] {
                        let mut theme = Theme::default();
                        if icon_role {
                            theme.icon_button = style.clone();
                        } else {
                            theme.button = style.clone();
                        }
                        error(ThemeProvider::new(theme), &field, "finite RGBA");
                    }
                }
            }
        }
        for component in 0..4 {
            let mut style = Theme::default().button;
            *channel(&mut style.focus_ring, component) = value;
            error(
                Button::new().style(style.clone()),
                "focus_ring",
                "finite RGBA",
            );
            error(
                InteractiveRegion::new().style(style.clone()),
                "focus_ring",
                "finite RGBA",
            );
            error(
                IconButton::new(LucideIcon::X, "x").style(style.clone()),
                "focus_ring",
                "finite RGBA",
            );
            error(
                harbor_widget::NotificationCard::new("notice", || {}).close_button_style(style),
                "focus_ring",
                "finite RGBA",
            );
        }
    }
}

#[test]
fn ordinary_colors_reject_nonfinite_components_without_clamping_finite_colors() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for component in 0..4 {
            let mut color = Color::WHITE;
            *channel(&mut color, component) = value;
            error(
                SizedBox::width(10.0).unwrap().background(color),
                "SizedBox.background",
                "finite RGBA",
            );
            error(
                harbor_widget::Align::new(harbor_widget::Alignment2D::CENTER).background(color),
                "Align.background",
                "finite RGBA",
            );
            error(
                harbor_widget::Padding::all(0.0).unwrap().background(color),
                "Padding.background",
                "finite RGBA",
            );
            error(
                harbor_widget::Row::new().background(color),
                "Row.background",
                "finite RGBA",
            );
            error(
                harbor_widget::Column::new().background(color),
                "Column.background",
                "finite RGBA",
            );
            error(
                harbor_widget::Flex::new(harbor_widget::Axis::Horizontal).background(color),
                "Flex.background",
                "finite RGBA",
            );
            error(
                harbor_widget::widgets::stack::Stack::new().background(color),
                "Stack.background",
                "finite RGBA",
            );
            error(
                TextLabel::new("text").foreground(color),
                "TextLabel.foreground",
                "finite RGBA",
            );
            error(
                Icon::new(LucideIcon::X).foreground(color),
                "Icon.foreground",
                "finite RGBA",
            );
            error(
                EditableText::new(TextEditingController::new("")).foreground(color),
                "EditableText.foreground",
                "finite RGBA",
            );
            error(
                TextField::new(TextEditingController::new("")).foreground(color),
                "TextField.foreground",
                "finite RGBA",
            );
            error(
                Separator::horizontal().color(color),
                "Separator.color",
                "finite RGBA",
            );
        }
    }
    let color = Color {
        r: -2.0,
        g: 3.0,
        b: f32::MAX,
        a: 2.0,
    };
    let mut style = Theme::default().button;
    for state in 0..5 {
        for kind in 0..3 {
            *state_color(&mut style, state, kind) = color;
        }
    }
    style.focus_ring = color;
    assert!(style.validate().is_ok());
    let components: Vec<Box<dyn Fn() -> Runtime>> = vec![
        Box::new(move || mount(SizedBox::width(10.0).unwrap().background(color).unwrap())),
        Box::new(move || {
            mount(
                harbor_widget::Align::new(harbor_widget::Alignment2D::CENTER)
                    .background(color)
                    .unwrap(),
            )
        }),
        Box::new(move || {
            mount(
                harbor_widget::Padding::all(0.0)
                    .unwrap()
                    .background(color)
                    .unwrap(),
            )
        }),
        Box::new(move || mount(harbor_widget::Row::new().background(color).unwrap())),
        Box::new(move || mount(harbor_widget::Column::new().background(color).unwrap())),
        Box::new(move || {
            mount(
                harbor_widget::Flex::new(harbor_widget::Axis::Vertical)
                    .background(color)
                    .unwrap(),
            )
        }),
        Box::new(move || {
            mount(
                harbor_widget::widgets::stack::Stack::new()
                    .background(color)
                    .unwrap(),
            )
        }),
        Box::new(move || mount(TextLabel::new("text").foreground(color).unwrap())),
        Box::new(move || mount(Icon::new(LucideIcon::X).foreground(color).unwrap())),
        Box::new(move || {
            mount(
                EditableText::new(TextEditingController::new("text"))
                    .foreground(color)
                    .unwrap(),
            )
        }),
        Box::new(move || {
            mount(
                TextField::new(TextEditingController::new("text"))
                    .foreground(color)
                    .unwrap(),
            )
        }),
        Box::new(move || mount(Separator::horizontal().color(color).unwrap())),
    ];
    for component in components {
        let runtime = component();
        let delta = runtime.pending_delta().unwrap();
        assert!(delta.added.iter().any(|item| match &item.primitive {
            Primitive::Quad { color: c, .. }
            | Primitive::Icon { color: c, .. }
            | Primitive::Text { color: c, .. } => *c == color,
            _ => false,
        }));
    }
}

#[test]
fn signal_theme_validates_both_roles_spacing_and_inactive_colors_on_initial_and_later_consumption()
{
    let mut invalid = Vec::new();
    invalid.push((
        "Theme.spacing",
        Theme {
            spacing: f32::NAN,
            ..Theme::default()
        },
    ));
    for icon_role in [false, true] {
        for index in 0..5 {
            let mut theme = Theme::default();
            let style = if icon_role {
                &mut theme.icon_button
            } else {
                &mut theme.button
            };
            let (field, slot) = scalar(style, index);
            *slot = f32::INFINITY;
            invalid.push((field, theme));
        }
        for state in 0..5 {
            for kind in 0..3 {
                for component in 0..4 {
                    let mut theme = Theme::default();
                    let style = if icon_role {
                        &mut theme.icon_button
                    } else {
                        &mut theme.button
                    };
                    *channel(state_color(style, state, kind), component) = f32::NAN;
                    invalid.push((
                        ["normal", "hovered", "pressed", "selected", "disabled"][state],
                        theme,
                    ));
                }
            }
        }
        let mut theme = Theme::default();
        let style = if icon_role {
            &mut theme.icon_button
        } else {
            &mut theme.button
        };
        style.focus_ring.a = f32::NEG_INFINITY;
        invalid.push(("focus_ring", theme));
    }
    for (field, theme) in invalid {
        for initial in [true, false] {
            let signal = Signal::new(if initial {
                theme.clone()
            } else {
                Theme::default()
            });
            let mut runtime = Runtime::new();
            let message = panic_message(|| {
                runtime.set_root(
                    ThemeProvider::from_signal(signal.clone()).child(Button::text("child")),
                );
                if !initial {
                    runtime.update(Instant::now());
                    signal.set(theme.clone());
                }
                runtime.update(Instant::now());
            });
            assert!(message.contains(field), "{message}");
            assert!(message.contains("ThemeProvider::from_signal"), "{message}");
        }
    }
}

#[test]
fn foreground_provider_is_deferred_ui_local_and_validated_on_every_consumed_result() {
    for initial in [true, false] {
        for component in 0..4 {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut invalid = Color::WHITE;
                *channel(&mut invalid, component) = bad;
                let color = Signal::new(if initial { invalid } else { Color::RED });
                let source = color.clone();
                let calls = Rc::new(Cell::new(0));
                let count = calls.clone();
                let button = IconButton::new(LucideIcon::X, "x").foreground_with(move |_| {
                    count.set(count.get() + 1);
                    *source.read()
                });
                assert_eq!(calls.get(), 0, "attachment must not evaluate the provider");
                let mut runtime = Runtime::new();
                let message = panic_message(|| {
                    let tracked = color.clone();
                    runtime.set_root(move |cx: &mut BuildCx| {
                        cx.track(&tracked);
                        button.build(cx)
                    });
                    if !initial {
                        runtime.update(Instant::now());
                        color.set(Color::GREEN);
                        runtime.update(Instant::now());
                        assert!(calls.get() >= 2);
                        color.set(invalid);
                    }
                    runtime.update(Instant::now());
                });
                assert!(message.contains("IconButton.foreground_with"), "{message}");
                assert!(message.contains("finite RGBA"), "{message}");
            }
        }
    }
}

#[test]
fn foreground_changes_only_icon_and_preserves_surface_and_interaction_consumption() {
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let color = Color {
        r: -1.0,
        g: 2.0,
        b: 0.5,
        a: 1.5,
    };
    let mut runtime = mount(
        IconButton::new(LucideIcon::X, "x").foreground_with(move |state| {
            count.set(count.get() + 1);
            if state.hovered { color } else { Color::RED }
        }),
    );
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(4.0, 4.0),
        PointerPhase::Move,
        PointerButton::Left,
        0,
    )));
    runtime.update(Instant::now());
    assert!(calls.get() >= 2);
    let delta = runtime.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    assert!(
        items
            .iter()
            .any(|item| matches!(&item.primitive, Primitive::Icon { color: c, .. } if *c == color))
    );
    assert!(items.iter().any(|item| matches!(&item.primitive, Primitive::Quad { color: c, .. } if *c == Theme::default().icon_button.hovered.background)));
}

#[test]
fn valid_large_style_and_field_inputs_fail_only_at_actual_combination_without_committing_geometry()
{
    let mut style = Theme::default().button;
    style.horizontal_padding = f32::MAX;
    let mut runtime = mount(Button::text("overflow").style(style).unwrap());
    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    assert_eq!(root.layout_error(), Some(LayoutError::InvalidGeometry));
    assert_eq!(
        root.layout_rect()
            .map(|rect| rect.size())
            .unwrap_or(Size::ZERO),
        Size::ZERO
    );
    runtime.set_viewport(Viewport::new(800, 600, 1.0));
    runtime.update(Instant::now());
    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    assert_eq!(root.layout_error(), Some(LayoutError::InvalidGeometry));

    let runtime = mount(
        TextField::new(TextEditingController::new("overflow"))
            .padding(f32::MAX)
            .unwrap(),
    );
    assert_eq!(
        runtime
            .arena()
            .get(runtime.root_id().unwrap())
            .unwrap()
            .layout_error(),
        Some(LayoutError::InvalidGeometry)
    );
}

#[test]
fn final_macro_results_construct_or_report_field_rule_and_source_location() {
    let _valid = harbor_widget::view! { Icon::new(LucideIcon::X).size(0.0); };
    let _valid =
        harbor_widget::view! { ThemeProvider::new(Theme::default()) => { Button::text("ok"); } };
    let message = panic_message(|| {
        let _ = harbor_widget::view! { Icon::new(LucideIcon::X).size(f32::NAN); };
    });
    assert!(message.contains("Icon.size"), "{message}");
    assert!(message.contains("finite and nonnegative"), "{message}");
    assert!(message.contains("appearance_validation.rs:"), "{message}");
}

#[test]
fn a_later_style_combination_overflow_preserves_the_previously_committed_geometry() {
    let styles = Signal::new(Theme::default().button);
    let source = styles.clone();
    let mut runtime = mount(move |cx: &mut BuildCx| {
        cx.track(&source);
        InteractiveRegion::new()
            .style(source.read().clone())
            .unwrap()
            .child(SizedBox::new(Size::new(8.0, 6.0)).unwrap())
            .build(cx)
    });
    let id = runtime.root_id().unwrap();
    let committed = runtime.arena().get(id).unwrap().layout_rect();
    assert!(committed.is_some());
    let mut next = styles.read().clone();
    next.horizontal_padding = f32::MAX;
    styles.set(next);
    runtime.update(Instant::now());
    assert_eq!(runtime.root_id(), Some(id));
    let fiber = runtime.arena().get(id).unwrap();
    assert_eq!(fiber.layout_error(), Some(LayoutError::InvalidGeometry));
    assert_eq!(fiber.layout_rect(), committed);
}
