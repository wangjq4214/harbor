use harbor_widget::input::event::{
    Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::{sized_box::SizedBox, text_label::TextLabel};
use harbor_widget::{
    Button, Column, FocusHandle, IconButton, InteractiveRegion, LucideIcon, MouseRegion,
    NotificationCard,
};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

fn update(runtime: &mut Runtime) {
    runtime.update(Instant::now());
}
fn pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        0,
    )));
}
fn key(runtime: &mut Runtime, key: Key, shift: bool) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
    }));
}
fn icon(runtime: &Runtime) -> (Rect, Color) {
    runtime
        .pending_delta()
        .unwrap()
        .added
        .iter()
        .chain(&runtime.pending_delta().unwrap().modified)
        .find_map(|item| match item.primitive {
            Primitive::Icon { rect, color, .. } => Some((rect, color)),
            _ => None,
        })
        .expect("painted icon")
}
fn center(rect: Rect) -> Point {
    Point::new(
        (rect.min.x + rect.max.x) / 2.0,
        (rect.min.y + rect.max.y) / 2.0,
    )
}

#[test]
fn buttons_capture_signals_and_update_rendered_text_through_all_activation_paths() {
    for is_icon in [false, true] {
        let count = Signal::new(0u32);
        let observed = count.clone();
        let focus = FocusHandle::new();
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(240, 160, 1.0));
        runtime.set_root(move |cx: &mut BuildCx| {
            cx.track(&observed);
            let value = *observed.read();
            let captured = observed.clone();
            let control = if is_icon {
                IconButton::new(LucideIcon::Plus, "Increment")
                    .focus_handle(focus)
                    .on_click(move |_| {
                        let next = *captured.read() + 1;
                        captured.set(next);
                    })
                    .build(cx)
            } else {
                Button::text("Increment")
                    .focus_handle(focus)
                    .on_click(move |_| {
                        let next = *captured.read() + 1;
                        captured.set(next);
                    })
                    .build(cx)
            };
            Column::new()
                .push_child(control)
                .push_child(TextLabel::new(format!("Count: {value}")))
                .build(cx)
        });
        update(&mut runtime);
        pointer(&mut runtime, Point::new(8.0, 8.0), PointerPhase::Down);
        pointer(&mut runtime, Point::new(8.0, 8.0), PointerPhase::Up);
        update(&mut runtime);
        runtime.request_focus(&focus);
        for (expected, activation) in [(2, Key::Enter), (3, Key::Space), (4, Key::Space)] {
            key(&mut runtime, activation, false); // repeated KeyDown retains existing repeat behavior
            update(&mut runtime);
            assert_eq!(*count.read(), expected);
            assert!(
                runtime
                    .pending_delta()
                    .unwrap()
                    .added
                    .iter()
                    .chain(&runtime.pending_delta().unwrap().modified)
                    .any(
                        |item| matches!(&item.primitive, Primitive::Text { text, .. }
                    if text.as_ref() == format!("Count: {expected}"))
                    )
            );
        }
    }
}

#[test]
fn button_handles_target_single_existing_stops_across_rebuild_and_disablement() {
    let first = FocusHandle::new();
    let second = FocusHandle::new();
    let disabled = Signal::new(false);
    let observed = disabled.clone();
    let focus_visible = Rc::new(Cell::new(false));
    let visible = focus_visible.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&observed);
        Column::new()
            .push_child(
                Button::text("First")
                    .focus_handle(first)
                    .disabled(*observed.read()),
            )
            .push_child(
                IconButton::new(LucideIcon::Plus, "Second")
                    .focus_handle(second)
                    .foreground_with({
                        let visible = visible.clone();
                        move |state| {
                            visible.set(state.focus_visible);
                            Color::RED
                        }
                    }),
            )
            .build(cx)
    });
    update(&mut runtime);
    key(&mut runtime, Key::Tab, false);
    let a = runtime.input().focused().unwrap();
    key(&mut runtime, Key::Tab, false);
    let b = runtime.input().focused().unwrap();
    assert_ne!(a, b);
    update(&mut runtime);
    assert!(focus_visible.get());
    key(&mut runtime, Key::Tab, false);
    assert_eq!(runtime.input().focused(), Some(a), "exactly two stops");
    key(&mut runtime, Key::Tab, true);
    assert_eq!(runtime.input().focused(), Some(b));
    runtime.request_focus(&first);
    assert_eq!(runtime.input().focused(), Some(a));
    disabled.set(false); // ordinary rebuild preserves focus identity
    update(&mut runtime);
    runtime.request_focus(&second);
    assert_eq!(runtime.input().focused(), Some(b));
    disabled.set(true);
    update(&mut runtime);
    runtime.request_focus(&first);
    assert_ne!(runtime.input().focused(), Some(a));
    key(&mut runtime, Key::Tab, false);
    assert_eq!(runtime.input().focused(), Some(b));
    disabled.set(false);
    update(&mut runtime);
    runtime.request_focus(&first);
    assert_eq!(runtime.input().focused(), Some(a));
}

#[test]
fn local_boundary_callbacks_are_edges_and_color_uses_normal_signal_rebuilds() {
    let edges = Rc::new(RefCell::new(Vec::new()));
    let entered = edges.clone();
    let exited = edges.clone();
    let tint = Signal::new(Color::RED);
    let observed = tint.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&observed);
        let color = observed.clone();
        MouseRegion::new()
            .on_enter({
                let entered = entered.clone();
                move |_| entered.borrow_mut().push("enter")
            })
            .on_exit({
                let exited = exited.clone();
                move |_| exited.borrow_mut().push("exit")
            })
            .child(
                IconButton::new(LucideIcon::Plus, "Tint").foreground_with(move |_| *color.read()),
            )
            .build(cx)
    });
    update(&mut runtime);
    assert_eq!(icon(&runtime).1, Color::RED);
    for _ in 0..2 {
        pointer(&mut runtime, Point::new(8.0, 8.0), PointerPhase::Move);
    }
    for _ in 0..2 {
        pointer(&mut runtime, Point::new(400.0, 400.0), PointerPhase::Move);
    }
    assert_eq!(&*edges.borrow(), &["enter", "exit"]);
    tint.set(Color::BLUE);
    update(&mut runtime);
    assert_eq!(icon(&runtime).1, Color::BLUE);
}

#[test]
fn nested_local_close_does_not_activate_parent_and_disabled_icon_releases_capture() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let parent = calls.clone();
    let child = calls.clone();
    let disabled = Signal::new(false);
    let observed = disabled.clone();
    let handle = FocusHandle::new();
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&observed);
        InteractiveRegion::new()
            .on_activate({
                let parent = parent.clone();
                move |_| parent.borrow_mut().push("parent")
            })
            .child(
                IconButton::new(LucideIcon::X, "Close")
                    .focus_handle(handle)
                    .disabled(*observed.read())
                    .on_click({
                        let child = child.clone();
                        move |_| child.borrow_mut().push("child")
                    }),
            )
            .build(cx)
    });
    update(&mut runtime);
    let position = center(icon(&runtime).0);
    pointer(&mut runtime, position, PointerPhase::Down);
    pointer(&mut runtime, position, PointerPhase::Up);
    assert_eq!(&*calls.borrow(), &["child"]);
    pointer(&mut runtime, position, PointerPhase::Down);
    assert!(runtime.input().captor(0).is_some());
    disabled.set(true);
    update(&mut runtime);
    assert!(runtime.input().captor(0).is_none());
    key(&mut runtime, Key::Enter, false);
    // The now-ineligible child yields focus to its enabled parent. Enter may
    // activate that parent, but must never invoke the disabled child again.
    assert_eq!(&*calls.borrow(), &["child", "parent"]);
}

// Each capture is owned only by the widget callback(s), not by the component or test.
#[derive(Clone, Copy, Debug)]
enum CallbackKind {
    Button,
    IconClick,
    Color,
    Enter,
    Exit,
    Dismiss,
}

#[test]
fn replaced_and_removed_local_callbacks_release_captures_and_use_current_closures() {
    for kind in [
        CallbackKind::Button,
        CallbackKind::IconClick,
        CallbackKind::Color,
        CallbackKind::Enter,
        CallbackKind::Exit,
        CallbackKind::Dismiss,
    ] {
        let version = Signal::new(1u8);
        let observed = version.clone();
        let captures = Rc::new(RefCell::new(Vec::<Weak<Cell<u8>>>::new()));
        let weak_captures = captures.clone();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let log = calls.clone();
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(240, 120, 1.0));
        runtime.set_root(move |cx: &mut BuildCx| {
            cx.track(&observed);
            let v = *observed.read();
            if v == 0 {
                return SizedBox::new(Size::new(240.0, 120.0)).unwrap().build(cx);
            }
            let token = Rc::new(Cell::new(v));
            weak_captures.borrow_mut().push(Rc::downgrade(&token));
            let log = log.clone();
            let callback = move || {
                log.borrow_mut().push(token.get());
            };
            match kind {
                CallbackKind::Button => Button::text("Click")
                    .on_click(move |_| callback())
                    .build(cx),
                CallbackKind::IconClick => IconButton::new(LucideIcon::Plus, "Click")
                    .on_click(move |_| callback())
                    .build(cx),
                CallbackKind::Color => IconButton::new(LucideIcon::Plus, "Tint")
                    .foreground_with(move |_| {
                        callback();
                        Color::RED
                    })
                    .build(cx),
                CallbackKind::Enter => MouseRegion::new()
                    .on_enter(move |_| callback())
                    .child(SizedBox::new(Size::new(80.0, 40.0)).unwrap())
                    .build(cx),
                CallbackKind::Exit => MouseRegion::new()
                    .on_exit(move |_| callback())
                    .child(SizedBox::new(Size::new(80.0, 40.0)).unwrap())
                    .build(cx),
                CallbackKind::Dismiss => NotificationCard::new("Notice", callback).build(cx),
            }
        });
        update(&mut runtime);
        let old = captures.borrow().last().unwrap().clone();
        if matches!(kind, CallbackKind::Color) {
            // Content constructors are released after materializing the icon;
            // the component recreates the tint callback on its next build.
            assert!(old.upgrade().is_none());
        } else {
            assert!(old.upgrade().is_some(), "{kind:?} mounted capture");
        }
        version.set(2);
        calls.borrow_mut().clear();
        update(&mut runtime);
        assert!(
            old.upgrade().is_none(),
            "{kind:?} replaced capture released"
        );
        let position = if matches!(kind, CallbackKind::Dismiss) {
            center(icon(&runtime).0)
        } else {
            Point::new(8.0, 8.0)
        };
        match kind {
            CallbackKind::Color => {} // already invoked through content construction
            CallbackKind::Enter | CallbackKind::Exit => {
                pointer(&mut runtime, position, PointerPhase::Move);
                pointer(&mut runtime, Point::new(500.0, 500.0), PointerPhase::Move);
            }
            _ => {
                pointer(&mut runtime, position, PointerPhase::Down);
                pointer(&mut runtime, position, PointerPhase::Up);
            }
        }
        assert!(!calls.borrow().is_empty(), "{kind:?} invoked");
        assert!(
            calls.borrow().iter().all(|v| *v == 2),
            "{kind:?} current callback only"
        );
        version.set(0);
        update(&mut runtime);
        assert!(
            captures.borrow().iter().all(|w| w.upgrade().is_none()),
            "{kind:?} removal"
        );
        calls.borrow_mut().clear();
        pointer(&mut runtime, position, PointerPhase::Move);
        pointer(&mut runtime, position, PointerPhase::Down);
        pointer(&mut runtime, position, PointerPhase::Up);
        key(&mut runtime, Key::Enter, false);
        assert!(
            calls.borrow().is_empty(),
            "{kind:?} no removed callback delivery"
        );
        // Root teardown is also used by the HMR prepare path, but this
        // headless assertion does not simulate a DLL unload/barrier.
        version.set(3);
        update(&mut runtime);
        runtime.set_root(SizedBox::new(Size::new(240.0, 120.0)).unwrap());
        update(&mut runtime);
        assert!(
            captures.borrow().iter().all(|w| w.upgrade().is_none()),
            "{kind:?} root teardown"
        );
        calls.borrow_mut().clear();
        pointer(&mut runtime, position, PointerPhase::Up);
        key(&mut runtime, Key::Enter, false);
        assert!(calls.borrow().is_empty());
    }
}
