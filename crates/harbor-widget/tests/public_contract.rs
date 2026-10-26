//! T0006: one public declaration combines the independently migrated contracts.
use harbor_widget::{
    ActionHandler, ActionOutcome, Axis, Button, Component, CrossAxisAlignment, ExternalSurface,
    Flex, Focus, FocusHandle, IntoChildView, NotificationCard, PositionAlignment, ShortcutMap,
    SizedBox, Stack, TextLabel, Theme, ThemeProvider, View,
    layout::Size,
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{Color, ExternalScheduleDemand, Primitive},
    signal::Signal,
    view,
    widgets::notification_card::{NotificationCardPlacement, NotificationCardStyle},
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};

fn declaration(
    macro_path: bool,
    source: Signal<u32>,
    builds: Rc<Cell<u32>>,
    unmounts: Rc<Cell<u32>>,
) -> View {
    let theme = Theme::default();
    // Intermediate Results are handled explicitly; only the last is a macro node.
    let button = Button::text("replaced")
        .style(theme.button.clone())
        .unwrap()
        .child_with(move |cx, _| {
            builds.set(builds.get() + 1);
            cx.track(&source);
            let retained = cx.use_state(|| 7_u32);
            TextLabel::new(format!("{}:{}", *retained.read(), *source.read()))
                .foreground(cx.theme().button.normal.foreground)
                .unwrap()
                .build(cx)
        })
        .style(theme.button.clone());
    let card = NotificationCard::new("combined card", || {})
        .detail_foreground(Color::WHITE)
        .unwrap()
        .close_button_style(theme.icon_button.clone())
        .unwrap()
        .style(NotificationCardStyle::default())
        .unwrap()
        .close_alignment(PositionAlignment::Start)
        .placement(NotificationCardPlacement {
            horizontal: PositionAlignment::End,
            vertical: PositionAlignment::Start,
            inset: 4.0,
        });
    #[expect(
        clippy::arc_with_non_send_sync,
        reason = "public external providers intentionally accept UI-local captures"
    )]
    let external = ExternalSurface::new(19)
        .on_draw(Arc::new(|_, _, _, _, _| {}))
        .schedule_with(Arc::new(|_, _| ExternalScheduleDemand::empty()))
        .ime_with(Arc::new(|_, _| None))
        .on_unmount(Arc::new(move || unmounts.set(unmounts.get() + 1)));
    let focus = Focus::new()
        .focus_handle(FocusHandle::new())
        .tab_order(2)
        .focusable(true);
    if macro_path {
        view! {
            ThemeProvider::new(theme) => {
                ActionHandler::new(|_: u8| ActionOutcome::Consumed) => {
                    ShortcutMap::<u8>::new() => {
                        focus => {
                            Stack::new().background(Color::TRANSPARENT) => {
                                Flex::new(Axis::Horizontal)
                                    .cross_axis_alignment(CrossAxisAlignment::Stretch) => {
                                    button;
                                    SizedBox::width(8.0);
                                }
                                card;
                                external => { SizedBox::new(Size::ZERO); }
                            }
                        }
                    }
                }
            }
        }
    } else {
        ThemeProvider::new(theme)
            .unwrap()
            .child(
                ActionHandler::new(|_: u8| ActionOutcome::Consumed).child(
                    ShortcutMap::<u8>::new().child(
                        focus.child(
                            Stack::new()
                                .background(Color::TRANSPARENT)
                                .unwrap()
                                .push_child(
                                    Flex::new(Axis::Horizontal)
                                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                        .push_child(button.unwrap())
                                        .push_child(SizedBox::width(8.0).unwrap()),
                                )
                                .push_child(card.unwrap())
                                .push_child(external.child(SizedBox::new(Size::ZERO).unwrap())),
                        ),
                    ),
                ),
            )
            .into_child_view()
    }
}

fn scene(runtime: &Runtime) -> Vec<Primitive> {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .map(|item| item.primitive.clone())
        .collect()
}

#[test]
fn mixed_result_nodes_and_content_hooks_mount_equally_and_teardown_on_both_public_paths() {
    let mut initial_scenes = Vec::new();
    for macro_path in [false, true] {
        let source = Signal::new(1);
        let builds = Rc::new(Cell::new(0));
        let unmounts = Rc::new(Cell::new(0));
        let tree = declaration(macro_path, source.clone(), builds.clone(), unmounts.clone());
        assert_eq!(builds.get(), 0, "declarations must not evaluate content");
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(800, 300, 1.0));
        runtime.set_root(tree);
        runtime.update(Instant::now());
        let root = runtime.root_id().unwrap();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            let fiber = runtime.arena().get(id).unwrap();
            assert_eq!(fiber.layout_error(), None, "mixed declaration must lay out");
            pending.extend(fiber.children());
        }
        let initial = scene(&runtime);
        assert!(
            initial
                .iter()
                .any(|p| matches!(p, Primitive::Text { text, .. } if text.as_ref() == "7:1"))
        );
        assert!(
            initial
                .iter()
                .any(|p| matches!(p, Primitive::External { .. }))
        );
        assert!(runtime.has_external_draws());
        initial_scenes.push(initial);
        source.set(2);
        runtime.update(Instant::now());
        assert_eq!(runtime.root_id(), Some(root));
        assert!(
            scene(&runtime)
                .iter()
                .any(|p| matches!(p, Primitive::Text { text, .. } if text.as_ref() == "7:2"))
        );
        assert!(builds.get() >= 2);
        assert_eq!(
            unmounts.get(),
            0,
            "content rebuild must not unmount its sibling"
        );
        runtime.set_root(SizedBox::new(Size::ZERO).unwrap());
        runtime.update(Instant::now());
        assert_eq!(unmounts.get(), 1);
        assert!(!runtime.has_external_draws());
        let count = builds.get();
        source.set(3);
        assert!(!runtime.update(Instant::now()).request_redraw);
        assert_eq!(builds.get(), count, "content subscription must be released");
    }
    assert_eq!(initial_scenes[0], initial_scenes[1]);
}
