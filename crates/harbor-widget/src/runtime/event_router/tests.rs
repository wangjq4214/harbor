//! Phase-level evidence complements the public mounted F11/F12 regressions.
use super::*;
use crate::IntoChildView;
use crate::input::event::{PointerButton, PointerEvent};
use crate::layout::{BoxConstraints, Size};
use crate::renderer::Viewport;
use crate::runtime::Runtime;
use crate::text::TextMetrics;
use crate::view::{AnyView, BuildCx, Component, View};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

type Step = (&'static str, EventPhase);

#[derive(Clone)]
struct TraceNode {
    name: &'static str,
    child: Option<View>,
    stop_at: Option<Step>,
    trace: Rc<RefCell<Vec<Step>>>,
}

impl Component for TraceNode {
    fn build(&self, _: &mut BuildCx) -> View {
        View::new(self.clone(), self.child.iter().cloned().collect(), None)
    }
}
impl AnyView for TraceNode {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::new(80.0, 40.0))
    }

    fn handle_event(&self, event: &UiEvent, ctx: &mut EventCtx, _: Rect) -> EventHandled {
        let phase = if ctx.is_capture_phase() {
            EventPhase::Capture
        } else if self.child.is_some() {
            EventPhase::Bubble
        } else {
            EventPhase::Target
        };
        let step = (self.name, phase);
        self.trace.borrow_mut().push(step);
        let ordinal = self.trace.borrow().len() as u64;
        ctx.queue_external_input(ExternalDrawId::new(ordinal), event.clone());
        ctx.write_clipboard(format!("{}-{phase:?}", self.name));
        if self.stop_at == Some(step) {
            EventHandled::Handled
        } else {
            EventHandled::Ignored
        }
    }
}

#[test]
fn every_phase_honors_the_return_and_commits_the_delivered_prefix_once_without_paint() {
    let complete = [
        ("outer", EventPhase::Capture),
        ("inner", EventPhase::Capture),
        ("target", EventPhase::Target),
        ("inner", EventPhase::Bubble),
        ("outer", EventPhase::Bubble),
    ];
    for stop_at in complete.into_iter().map(Some).chain([None]) {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut child = None;
        for name in ["target", "inner", "outer"] {
            child = Some(
                TraceNode {
                    name,
                    child,
                    stop_at,
                    trace: trace.clone(),
                }
                .into_child_view(),
            );
        }
        let mut rt = Runtime::new();
        rt.set_viewport(Viewport::new(100, 100, 1.0));
        rt.set_root(child.unwrap());
        rt.update(Instant::now());
        let event = UiEvent::Pointer(PointerEvent::new(
            Point::new(10.0, 10.0),
            PointerPhase::Move,
            PointerButton::Left,
            7,
        ));
        let effects = rt.dispatch(event.clone());
        let count = stop_at.map_or(complete.len(), |step| {
            complete.iter().position(|s| *s == step).unwrap() + 1
        });
        assert_eq!(&*trace.borrow(), &complete[..count], "stop at {stop_at:?}");
        assert!(
            !effects.request_redraw,
            "neither consumption nor clipboard requests paint"
        );
        let (name, phase) = complete[count - 1];
        assert_eq!(
            effects.clipboard,
            Some(crate::effects::ClipboardEffect::Write(format!(
                "{name}-{phase:?}"
            )))
        );
        let expected: Vec<_> = (1..=count as u64)
            .map(|id| (ExternalDrawId::new(id), event.clone()))
            .collect();
        assert_eq!(rt.drain_external_input(), expected);
        assert!(rt.drain_external_input().is_empty());
    }
}

#[test]
fn missing_handler_or_geometry_is_ignored_without_creating_commands() {
    let arena = FiberArena::new();
    let mut keys = slotmap::SlotMap::<FiberId, ()>::with_key();
    let dead = keys.insert(());
    let event = UiEvent::Focus(FocusEvent::Lost);
    let mut ctx = EventCtx::new();
    assert_eq!(
        EventRouter::invoke_handler(&arena, dead, &event, &mut ctx),
        EventHandled::Ignored
    );
    assert!(ctx.take_commands().is_empty());
}
