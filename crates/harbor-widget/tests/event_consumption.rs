//! Mounted public-API regressions for spec 0039 (F11/F12).
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use harbor_widget::effects::ClipboardEffect;
use harbor_widget::input::event::{
    FocusEvent, Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
};
use harbor_widget::input::event_ctx::{EventCtx, EventHandled};
use harbor_widget::layout::{BoxConstraints, Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{ExternalDrawId, Primitive};
use harbor_widget::text::TextMetrics;
use harbor_widget::view::{BuildCx, Component, PrimitiveLeaf, View};
use harbor_widget::widgets::{custom_paint::CustomPaint, row::Row};
use harbor_widget::{Focus, InteractiveRegion, MouseRegion};

type Input = dyn Fn(&UiEvent, &mut EventCtx) -> EventHandled;

#[derive(Clone)]
struct Leaf(Rc<Input>);

impl PrimitiveLeaf for Leaf {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::new(80.0, 40.0))
    }
    fn paint_primitives(&self, _: Rect, _: &TextMetrics) -> Vec<Primitive> {
        Vec::new()
    }
    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _: Rect) -> EventHandled {
        (self.0)(event, ctx)
    }
}
impl Component for Leaf {
    fn build(&self, _: &mut BuildCx) -> View {
        View::leaf(self.clone())
    }
}

fn mount(root: impl harbor_widget::IntoChildView + 'static) -> Runtime {
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(240, 100, 1.0));
    rt.set_root(root);
    rt.update(Instant::now());
    rt
}

fn pointer(x: f32, phase: PointerPhase) -> UiEvent {
    UiEvent::Pointer(PointerEvent::new(
        Point::new(x, 10.0),
        phase,
        PointerButton::Left,
        9,
    ))
}

#[test]
fn primitive_leaf_handled_prevents_parent_activation_and_ignored_preserves_effects() {
    for result in [EventHandled::Handled, EventHandled::Ignored] {
        let activations = Rc::new(Cell::new(0));
        let observed = activations.clone();
        let mut rt = mount(
            InteractiveRegion::new()
                .on_activate(move |_| observed.set(observed.get() + 1))
                .child(Leaf(Rc::new(move |event, ctx| {
                    if matches!(event, UiEvent::Pointer(p) if p.phase == PointerPhase::Down) {
                        ctx.write_clipboard("leaf-down");
                        ctx.queue_external_input(ExternalDrawId::new(101), event.clone());
                        return result;
                    }
                    if matches!(event, UiEvent::Pointer(p) if p.phase == PointerPhase::Up) {
                        return result;
                    }
                    EventHandled::Ignored
                }))),
        );
        let down = rt.dispatch(pointer(30.0, PointerPhase::Down));
        assert_eq!(
            down.clipboard,
            Some(ClipboardEffect::Write("leaf-down".into()))
        );
        assert_eq!(
            rt.drain_external_input(),
            vec![(ExternalDrawId::new(101), pointer(30.0, PointerPhase::Down))]
        );
        rt.dispatch(pointer(30.0, PointerPhase::Up));
        assert_eq!(
            activations.get(),
            usize::from(result == EventHandled::Ignored)
        );
        assert!(rt.input().captor(9).is_none());
        assert!(rt.drain_external_input().is_empty());
    }
}

#[test]
fn custom_paint_adapter_consumes_nested_click_without_duplicate_external_delivery() {
    let activations = Rc::new(Cell::new(0));
    let observed = activations.clone();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let mut rt = mount(InteractiveRegion::new()
        .on_activate(move |_| observed.set(observed.get() + 1))
        .child(CustomPaint::new(102).on_input(Arc::new(move |event, _| {
            if matches!(event, UiEvent::Pointer(p) if matches!(p.phase, PointerPhase::Down | PointerPhase::Up)) {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                EventHandled::Handled
            } else {
                EventHandled::Ignored
            }
        }))));
    rt.dispatch(pointer(30.0, PointerPhase::Down));
    rt.dispatch(pointer(30.0, PointerPhase::Up));
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(activations.get(), 0);
    assert!(rt.input().focused().is_some());
    assert!(rt.drain_external_input().is_empty());
}

#[test]
fn hover_redraw_never_skips_move_clipboard_focus_capture_or_ordered_external_input() {
    for result in [EventHandled::Handled, EventHandled::Ignored] {
        let focus = Rc::new(Cell::new(None));
        let requested = focus.clone();
        let moves = Rc::new(Cell::new(0));
        let counted = moves.clone();
        let root = Focus::new().child(
            MouseRegion::new()
                .on_enter(|ctx| {
                    ctx.invalidate_paint();
                    ctx.write_clipboard("enter");
                    ctx.queue_external_input(
                        ExternalDrawId::new(103),
                        UiEvent::Focus(FocusEvent::Gained),
                    );
                })
                .child(Leaf(Rc::new(move |event, ctx| match event {
                    UiEvent::Pointer(p) if p.phase == PointerPhase::Move => {
                        counted.set(counted.get() + 1);
                        ctx.write_clipboard(format!("move-{}", counted.get()));
                        ctx.request_focus(requested.get().unwrap());
                        ctx.capture_pointer(p.pointer_id);
                        ctx.queue_external_input(ExternalDrawId::new(104), event.clone());
                        result
                    }
                    UiEvent::Pointer(p) if p.phase == PointerPhase::Up => {
                        ctx.release_pointer(p.pointer_id);
                        result
                    }
                    _ => EventHandled::Ignored,
                }))),
        );
        let mut rt = mount(root);
        focus.set(rt.root_id());
        let effects = rt.dispatch(pointer(10.0, PointerPhase::Move));
        assert!(effects.request_redraw);
        assert_eq!(
            effects.clipboard,
            Some(ClipboardEffect::Write("move-1".into()))
        );
        assert_eq!(rt.input().focused(), focus.get());
        assert!(rt.input().captor(9).is_some());
        assert_eq!(moves.get(), 1);
        assert_eq!(
            rt.drain_external_input(),
            vec![
                (ExternalDrawId::new(103), UiEvent::Focus(FocusEvent::Gained)),
                (ExternalDrawId::new(104), pointer(10.0, PointerPhase::Move)),
            ]
        );
        assert!(rt.drain_external_input().is_empty());
        let next = rt.dispatch(pointer(10.0, PointerPhase::Move));
        assert_eq!(
            next.clipboard,
            Some(ClipboardEffect::Write("move-2".into()))
        );
        assert_eq!(
            rt.drain_external_input(),
            vec![(ExternalDrawId::new(104), pointer(10.0, PointerPhase::Move))]
        );
        rt.dispatch(pointer(500.0, PointerPhase::Up));
        assert!(rt.input().captor(9).is_none());
        let idle = rt.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
            key: Key::Character('x'),
            modifiers: Modifiers::default(),
        }));
        assert!(idle.clipboard.is_none());
        assert!(rt.drain_external_input().is_empty());
    }
}

#[test]
fn handled_boundary_notifications_do_not_cancel_other_recipients_or_moves() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let region = |name: &'static str| {
        let enter = trace.clone();
        let exit = trace.clone();
        MouseRegion::new()
            .on_enter(move |ctx| {
                enter.borrow_mut().push((name, "enter"));
                ctx.invalidate_paint();
            })
            .on_exit(move |ctx| {
                exit.borrow_mut().push((name, "leave"));
                ctx.invalidate_paint();
            })
    };
    let moves = Rc::new(Cell::new(0));
    let leaf = || {
        let count = moves.clone();
        Leaf(Rc::new(move |event, _| {
            if matches!(event, UiEvent::Pointer(p) if p.phase == PointerPhase::Move) {
                count.set(count.get() + 1);
                EventHandled::Handled
            } else {
                EventHandled::Ignored
            }
        }))
    };
    let mut rt = mount(
        region("outer").child(
            Row::new()
                .push_child(region("a").child(leaf()))
                .push_child(region("b").child(leaf())),
        ),
    );
    assert!(
        rt.dispatch(pointer(10.0, PointerPhase::Move))
            .request_redraw
    );
    assert!(
        rt.dispatch(pointer(90.0, PointerPhase::Move))
            .request_redraw
    );
    assert!(
        rt.dispatch(pointer(500.0, PointerPhase::Move))
            .request_redraw
    );
    assert_eq!(moves.get(), 2);
    assert_eq!(
        *trace.borrow(),
        vec![
            ("outer", "enter"),
            ("a", "enter"),
            ("a", "leave"),
            ("b", "enter"),
            ("b", "leave"),
            ("outer", "leave")
        ]
    );
}

#[test]
fn leave_redraw_commits_the_new_targets_move_in_the_same_dispatch() {
    let first = MouseRegion::new()
        .on_exit(|ctx| {
            ctx.invalidate_paint();
            ctx.write_clipboard("leave");
            ctx.queue_external_input(ExternalDrawId::new(105), UiEvent::Focus(FocusEvent::Lost));
        })
        .child(Leaf(Rc::new(|_, _| EventHandled::Ignored)));
    let second = Leaf(Rc::new(|event, ctx| {
        if matches!(event, UiEvent::Pointer(p) if p.phase == PointerPhase::Move) {
            ctx.write_clipboard("new-target");
            ctx.queue_external_input(ExternalDrawId::new(106), event.clone());
            EventHandled::Handled
        } else {
            EventHandled::Ignored
        }
    }));
    let mut rt = mount(Row::new().push_child(first).push_child(second));
    rt.dispatch(pointer(10.0, PointerPhase::Move));
    let effects = rt.dispatch(pointer(90.0, PointerPhase::Move));
    assert!(effects.request_redraw);
    assert_eq!(
        effects.clipboard,
        Some(ClipboardEffect::Write("new-target".into()))
    );
    assert_eq!(
        rt.drain_external_input(),
        vec![
            (ExternalDrawId::new(105), UiEvent::Focus(FocusEvent::Lost)),
            (ExternalDrawId::new(106), pointer(90.0, PointerPhase::Move)),
        ]
    );
    assert!(rt.drain_external_input().is_empty());
    assert!(
        rt.dispatch(pointer(500.0, PointerPhase::Move))
            .clipboard
            .is_none()
    );
}

#[test]
fn handled_focus_loss_does_not_cancel_the_new_owners_focus_gain() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let focused = |name| {
        let trace = trace.clone();
        Leaf(Rc::new(move |event, ctx| {
            if let UiEvent::Focus(focus) = event {
                trace.borrow_mut().push((name, *focus));
                ctx.invalidate_paint();
                EventHandled::Handled
            } else {
                EventHandled::Ignored
            }
        }))
    };
    let a = harbor_widget::FocusHandle::new();
    let b = harbor_widget::FocusHandle::new();
    let mut rt = mount(
        Row::new()
            .push_child(Focus::new().child(focused("a")).focus_handle(a))
            .push_child(Focus::new().child(focused("b")).focus_handle(b)),
    );
    rt.request_focus(&a);
    rt.take_pending_effects();
    trace.borrow_mut().clear();
    assert!(rt.request_focus(&b).request_redraw);
    assert_eq!(
        *trace.borrow(),
        vec![("a", FocusEvent::Lost), ("b", FocusEvent::GainedVisible)]
    );
}

#[test]
fn consumed_activation_keeps_host_owned_actions_fifo_and_drain_once() {
    let store = harbor_widget::Store::new(0usize);
    let send = store.dispatcher();
    let mut rt = mount(harbor_widget::Button::text("send").on_click(move |ctx| {
        send.dispatch("first");
        ctx.write_clipboard("activation");
        send.dispatch("second");
    }));
    rt.dispatch(pointer(30.0, PointerPhase::Down));
    let effects = rt.dispatch(pointer(30.0, PointerPhase::Up));
    assert_eq!(
        effects.clipboard,
        Some(ClipboardEffect::Write("activation".into()))
    );
    assert_eq!(*store.state().read(), 0);
    assert_eq!(store.drain_actions(), vec!["first", "second"]);
    assert!(store.drain_actions().is_empty());
}
