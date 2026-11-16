use harbor_widget::{
    ExternalSurface, SplitAxis, SplitController, SplitPane,
    input::{
        event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
        event_ctx::EventHandled,
    },
    layout::Point,
    renderer::Viewport,
    runtime::Runtime,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

fn pointer(rt: &mut Runtime, x: f32, y: f32, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        Point::new(x, y),
        phase,
        PointerButton::Left,
        12,
    )));
    rt.update(Instant::now());
}
#[test]
fn split_divider_capture_cancel_and_unmount_never_deliver_drag_to_children() {
    for axis in [SplitAxis::Horizontal, SplitAxis::Vertical] {
        let ratio = SplitController::new(0.5);
        let calls = Arc::new(AtomicUsize::new(0));
        let leaf = |id| {
            let calls = calls.clone();
            ExternalSurface::new(id).on_input(Arc::new(move |_, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                EventHandled::Handled
            }))
        };
        let mut rt = Runtime::new();
        rt.set_viewport(Viewport::new(400, 400, 1.0));
        rt.set_root(SplitPane::new(axis, ratio.clone(), leaf(1), leaf(2)));
        rt.update(Instant::now());
        pointer(&mut rt, 200.0, 200.0, PointerPhase::Down);
        pointer(&mut rt, 350.0, 350.0, PointerPhase::Move);
        assert!(ratio.ratio() > 0.8);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        pointer(&mut rt, 350.0, 350.0, PointerPhase::Cancel);
        let end = ratio.ratio();
        pointer(&mut rt, 40.0, 40.0, PointerPhase::Move);
        assert_eq!(ratio.ratio(), end);
        assert!(calls.load(Ordering::Relaxed) > 0);
        ratio.set_ratio(0.5);
        rt.update(Instant::now());
        pointer(&mut rt, 200.0, 200.0, PointerPhase::Down);
        rt.set_root(SplitPane::new(axis, ratio.clone(), leaf(3), leaf(4)));
        rt.update(Instant::now());
        pointer(&mut rt, 60.0, 60.0, PointerPhase::Move);
        assert_eq!(
            ratio.ratio(),
            0.5,
            "unmount cancels retained controller gesture"
        );
    }
}
#[test]
fn ratio_normalization_and_compression_do_not_rewrite_intent() {
    let ratio = SplitController::new(f32::NAN);
    assert_eq!(ratio.ratio(), 0.5);
    ratio.set_ratio(-1.0);
    assert_eq!(ratio.ratio(), 0.05);
    ratio.set_ratio(f32::INFINITY);
    assert_eq!(ratio.ratio(), 0.05);
    ratio.set_ratio(2.0);
    assert_eq!(ratio.ratio(), 0.95);
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(20, 20, 1.0));
    rt.set_root(SplitPane::new(
        SplitAxis::Horizontal,
        ratio.clone(),
        ExternalSurface::new(1),
        ExternalSurface::new(2),
    ));
    rt.update(Instant::now());
    let items = &rt.pending_delta().unwrap().added;
    let rects: Vec<_> = items
        .iter()
        .filter_map(|item| match item.primitive {
            harbor_widget::scene::primitive::Primitive::External { rect, .. } => Some(rect),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 2);
    assert_eq!(rects[0].size().width, 7.0);
    assert_eq!(rects[1].size().width, 7.0);
    assert_eq!(ratio.ratio(), 0.95);
    rt.set_viewport(Viewport::new(400, 20, 1.0));
    rt.update(Instant::now());
    assert_eq!(ratio.ratio(), 0.95);
}
