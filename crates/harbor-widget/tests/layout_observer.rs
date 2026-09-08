use harbor_widget::layout::{Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::custom_paint::CustomPaint;
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{ConstrainedBox, LayoutObserver};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn update(runtime: &mut Runtime) -> bool {
    runtime.update(Instant::now()).request_redraw
}

fn root_rect(runtime: &Runtime) -> Rect {
    runtime
        .arena()
        .get(runtime.root_id().unwrap())
        .unwrap()
        .layout_rect()
        .unwrap()
}

#[test]
fn observer_delivers_distinct_committed_rects_and_callback_work_waits_for_next_update() {
    #[derive(Clone)]
    struct ResizingObserved {
        size: Signal<Size>,
        calls: Rc<RefCell<Vec<Rect>>>,
    }

    impl Component for ResizingObserved {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.size);
            let size = *self.size.read();
            let requested_size = self.size.clone();
            let calls = self.calls.clone();
            LayoutObserver::new(move |rect| {
                calls.borrow_mut().push(rect);
                if rect.size().width == 20.0 {
                    requested_size.set(Size::new(30.0, 10.0));
                }
            })
            .child(SizedBox::new(size))
            .build(cx)
        }
    }

    let size = Signal::new_distinct(Size::new(20.0, 10.0));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new();
    runtime.set_root(ResizingObserved {
        size: size.clone(),
        calls: calls.clone(),
    });

    assert!(update(&mut runtime));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(root_rect(&runtime).size(), Size::new(20.0, 10.0));

    assert!(update(&mut runtime));
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(root_rect(&runtime).size(), Size::new(30.0, 10.0));
    assert!(!update(&mut runtime));
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn observer_ignores_failed_layout_and_notifies_after_recovery() {
    #[derive(Clone)]
    struct FallibleObserved {
        invalid: Signal<bool>,
        width: Signal<f32>,
        calls: Rc<RefCell<Vec<Rect>>>,
    }

    impl Component for FallibleObserved {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.invalid);
            cx.track(&self.width);
            let invalid = *self.invalid.read();
            let width = *self.width.read();
            let child = if invalid {
                ConstrainedBox::new().min_width(20.0).max_width(10.0)
            } else {
                ConstrainedBox::new().min_width(width).max_width(width)
            }
            .min_height(10.0)
            .max_height(10.0)
            .child(SizedBox::new(Size::new(width, 10.0)));
            let calls = self.calls.clone();
            LayoutObserver::new(move |rect| calls.borrow_mut().push(rect))
                .child(child)
                .build(cx)
        }
    }

    let invalid = Signal::new_distinct(false);
    let width = Signal::new_distinct(20.0);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new();
    runtime.set_root(FallibleObserved {
        invalid: invalid.clone(),
        width: width.clone(),
        calls: calls.clone(),
    });
    update(&mut runtime);
    assert_eq!(calls.borrow().len(), 1);

    invalid.set(true);
    assert!(update(&mut runtime));
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(root_rect(&runtime).size(), Size::new(20.0, 10.0));

    invalid.set(false);
    width.set(30.0);
    assert!(update(&mut runtime));
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(root_rect(&runtime).size(), Size::new(30.0, 10.0));
}

#[test]
fn observer_skips_zero_viewport_and_coalesces_restore_to_same_rect() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = calls.clone();
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(0, 0, 1.0));
    runtime.set_root(
        LayoutObserver::new(move |rect| observed.borrow_mut().push(rect))
            .child(CustomPaint::new(7)),
    );
    update(&mut runtime);
    assert!(calls.borrow().is_empty());

    runtime.set_viewport(Viewport::new(100, 50, 1.0));
    update(&mut runtime);
    assert_eq!(calls.borrow().len(), 1);

    runtime.set_viewport(Viewport::new(0, 0, 1.0));
    update(&mut runtime);
    runtime.set_viewport(Viewport::new(100, 50, 1.0));
    update(&mut runtime);
    assert_eq!(calls.borrow().len(), 1);
}

#[test]
fn observer_drops_staged_callback_when_fiber_unmounts_before_delivery() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = calls.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(
        LayoutObserver::new(move |rect| observed.borrow_mut().push(rect))
            .child(SizedBox::new(Size::new(20.0, 10.0))),
    );
    runtime.set_root(SizedBox::new(Size::new(5.0, 5.0)));

    update(&mut runtime);
    assert!(calls.borrow().is_empty());
}

#[test]
fn replacing_callback_without_geometry_change_does_not_notify_again() {
    #[derive(Clone)]
    struct ReplacedCallback {
        generation: Signal<u32>,
        calls: Rc<RefCell<Vec<u32>>>,
    }

    impl Component for ReplacedCallback {
        fn build(&self, cx: &mut BuildCx) -> View {
            cx.track(&self.generation);
            let generation = *self.generation.read();
            let calls = self.calls.clone();
            LayoutObserver::new(move |_| calls.borrow_mut().push(generation))
                .child(SizedBox::new(Size::new(20.0, 10.0)))
                .build(cx)
        }
    }

    let generation = Signal::new_distinct(0);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut runtime = Runtime::new();
    runtime.set_root(ReplacedCallback {
        generation: generation.clone(),
        calls: calls.clone(),
    });
    update(&mut runtime);
    assert_eq!(&*calls.borrow(), &[0]);

    generation.set(1);
    update(&mut runtime);
    assert_eq!(&*calls.borrow(), &[0]);
}
