//! Non-terminal callers exercise the reusable presentation/extension surface.
use harbor_widget::{
    Actions, Alignment, Button, Column, ConstrainedBox, FocusScope, Icon, KeyChord, LucideIcon,
    Notification, Portal, ScrollArea, Shortcuts, Store, ThemeProvider,
    input::{
        event::{
            Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
        },
        event_ctx::{EventCtx, EventHandled},
    },
    layout::{BoxConstraints, Point, Rect, Size},
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, Primitive},
    text::TextMetrics,
    view::{BuildCx, Component, PrimitiveLeaf, View},
    widgets::{
        notification::{NotificationPlacement, NotificationStyle},
        sized_box::SizedBox,
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

#[test]
fn generic_controls_and_actions_need_no_application_models() {
    let store = Store::<(), u8>::new(());
    let dispatcher = store.dispatcher();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(320, 200, 1.0));
    runtime.set_root(move |cx: &mut BuildCx| {
        let action = dispatcher.clone();
        harbor_widget::view! { cx;
            ThemeProvider::new(harbor_widget::Theme::default()) => {
                FocusScope::new() => {
                    Actions::handler(move |value: u8| { action.dispatch(value); harbor_widget::ActionOutcome::Consumed }) => {
                        Shortcuts::empty().bind(KeyChord { key: Key::F6, modifiers: Modifiers::default() }, 7u8) => {
                            Column::new() => {
                                Button::new("Test content");
                                Icon::new(LucideIcon::Plus);
                                ConstrainedBox::new().max_height(60.0) => {
                                    ScrollArea::new() => {
                                        SizedBox::new(Size::new(100.0, 400.0));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    runtime.update(Instant::now());
    assert!(runtime.focus_first_focusable());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::F6,
        modifiers: Modifiers::default(),
    }));
    assert_eq!(store.drain_actions(), [7]);
    assert!(store.drain_actions().is_empty());
}

fn pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        1,
    )));
}

#[test]
fn notification_placement_style_scroll_and_dismissal_are_caller_composable() {
    for (horizontal, vertical) in [
        (Alignment::Start, Alignment::End),
        (Alignment::Center, Alignment::Start),
    ] {
        let dismissed = Arc::new(AtomicUsize::new(0));
        let count = dismissed.clone();
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(600, 240, 2.0));
        let message = (0..40)
            .map(|i| format!("line {i:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        runtime.set_root(
            Portal::new().child(
                Notification::new(message, move || {
                    count.fetch_add(1, Ordering::Relaxed);
                })
                .placement(NotificationPlacement {
                    horizontal,
                    vertical,
                    inset: 7.0,
                })
                .style(NotificationStyle {
                    max_width: 180.0,
                    corner_radius: 11.0,
                    foreground: Some(Color::BLUE),
                    background: Some(Color::RED),
                    ..Default::default()
                }),
            ),
        );
        runtime.update(Instant::now());
        assert!(runtime.input().focused().is_none());
        let items = &runtime.pending_delta().unwrap().added;
        let rect = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::Quad {
                    rect,
                    color: Color::RED,
                    corner_radius: 11.0,
                } => Some(rect),
                _ => None,
            })
            .expect("caller-chosen card appearance");
        assert_eq!(rect.size().width, 180.0);
        assert_eq!(
            rect.min.x,
            if horizontal == Alignment::Start {
                7.0
            } else {
                60.0
            }
        );
        assert_eq!(rect.min.y, 7.0);
        assert_eq!(rect.max.y, 113.0);
        pointer(
            &mut runtime,
            Point::new(rect.min.x + 20.0, rect.min.y + 50.0),
            PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
        );
        runtime.update(Instant::now());
        assert!(runtime.pending_delta().unwrap().modified.iter().chain(&runtime.pending_delta().unwrap().added).any(|item| matches!(&item.primitive, Primitive::Text { text, color: Color::BLUE, .. } if text.as_ref() == "line 01")));
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            pointer(
                &mut runtime,
                Point::new(rect.max.x - 10.0, rect.min.y + 10.0),
                phase,
            );
        }
        assert_eq!(dismissed.load(Ordering::Relaxed), 1);
        assert!(runtime.input().focused().is_none());
    }
}

#[test]
fn notification_can_be_inline_without_a_root_escape() {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(400, 300, 1.0));
    runtime.set_root(
        Column::new().child(
            Notification::new("Inline content", || {}).style(NotificationStyle {
                max_width: 140.0,
                background: Some(Color::RED),
                ..Default::default()
            }),
        ),
    );
    runtime.update(Instant::now());
    let rect = runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                color: Color::RED,
                ..
            } => Some(rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(rect.min, Point::ZERO);
    assert_eq!(rect.size().width, 140.0);
    assert!(rect.max.y < 300.0);
}

#[derive(Clone)]
struct TestLeaf(Arc<AtomicUsize>);
impl PrimitiveLeaf for TestLeaf {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::new(80.0, 40.0))
    }
    fn paint_primitives(&self, rect: Rect, _: &TextMetrics) -> Vec<Primitive> {
        vec![Primitive::Quad {
            rect,
            color: Color::RED,
            corner_radius: 0.0,
        }]
    }
    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _: Rect) -> EventHandled {
        if matches!(event, UiEvent::Pointer(pointer) if matches!(pointer.phase, PointerPhase::WheelLine { .. }))
        {
            self.0.fetch_add(1, Ordering::Relaxed);
            ctx.invalidate_paint();
            return EventHandled::Handled;
        }
        EventHandled::Ignored
    }
}
impl Component for TestLeaf {
    fn build(&self, _: &mut BuildCx) -> View {
        View::leaf(self.clone())
    }
}

#[test]
fn application_leaf_uses_public_primitives_and_input_without_focus_or_external_renderer() {
    let events = Arc::new(AtomicUsize::new(0));
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(200, 100, 1.0));
    runtime.set_root(TestLeaf(events.clone()));
    runtime.update(Instant::now());
    assert!(!runtime.has_external_draws());
    assert!(!runtime.focus_first_focusable());
    let effects = runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(10.0, 10.0),
        PointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
        PointerButton::Left,
        1,
    )));
    assert!(effects.request_redraw);
    assert_eq!(events.load(Ordering::Relaxed), 1);
}
