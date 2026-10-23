use super::*;
use crate::{
    Button, Column, EditableText, FocusHandle, TextEditingController,
    input::event::{ImePreedit, Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent},
    scene::primitive::{ExternalScheduleDemand, Primitive},
    signal::Signal,
    view::Component,
    widgets::custom_paint::CustomPaint,
};
use std::{cell::Cell, rc::Rc, time::Duration};

const INTERVAL: Duration = Duration::from_millis(500);

fn mount(root: impl Component + 'static) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_root(root);
    assert!(runtime.focus_first_focusable());
    runtime
}

fn caret_visible(runtime: &Runtime) -> bool {
    runtime.scene_graph.items().iter().any(|item| {
        matches!(&item.primitive, Primitive::Quad { rect, .. }
            if rect.size().width == 1.0 && rect.size().height == runtime.text_metrics.line_height)
    })
}

fn assert_deadline(effects: &RuntimeEffects, deadline: Instant) {
    assert_eq!(
        effects.control_flow,
        Some(ControlFlowEffect::WaitUntil(deadline))
    );
}

#[test]
fn idle_caret_blinks_without_rebuilding_or_notifying_text_changes() {
    let builds = Rc::new(Cell::new(0));
    let changes = Rc::new(Cell::new(0));
    let observed_builds = builds.clone();
    let observed_changes = changes.clone();
    let controller = TextEditingController::new("abc");
    let mut runtime = mount(move |cx: &mut BuildCx| {
        observed_builds.set(observed_builds.get() + 1);
        let changes = observed_changes.clone();
        EditableText::new(controller.clone())
            .on_changed(move |_| changes.set(changes.get() + 1))
            .build(cx)
    });
    let start = Instant::now();
    assert_deadline(&runtime.update(start), start + INTERVAL);
    assert!(caret_visible(&runtime));
    let settled_builds = builds.get();

    let idle = runtime.update(start + INTERVAL - Duration::from_millis(1));
    assert!(!idle.request_redraw);
    assert_deadline(&idle, start + INTERVAL);
    assert!(caret_visible(&runtime));

    let hidden = runtime.update(start + INTERVAL);
    assert!(hidden.request_redraw);
    assert_deadline(&hidden, start + INTERVAL * 2);
    assert!(!caret_visible(&runtime));

    let visible = runtime.update(start + INTERVAL * 2);
    assert!(visible.request_redraw);
    assert_deadline(&visible, start + INTERVAL * 3);
    assert!(caret_visible(&runtime));
    assert_eq!(builds.get(), settled_builds);
    assert_eq!(changes.get(), 0);
}

#[test]
fn typing_navigation_and_pointer_placement_restart_visible_phase() {
    let events = [
        UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('x'),
            modifiers: Modifiers::default(),
        }),
        UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::ArrowLeft,
            modifiers: Modifiers::default(),
        }),
        UiEvent::Pointer(PointerEvent::new(
            Point::new(0.0, 0.0),
            PointerPhase::Down,
            PointerButton::Left,
            1,
        )),
    ];
    for event in events {
        let mut runtime = mount(EditableText::new(TextEditingController::new("abc")));
        let start = Instant::now();
        runtime.update(start);
        runtime.update(start + INTERVAL);
        assert!(!caret_visible(&runtime));

        runtime.dispatch(event);
        let edited = start + INTERVAL + Duration::from_millis(10);
        assert_deadline(&runtime.update(edited), edited + INTERVAL);
        assert!(caret_visible(&runtime));
        runtime.update(edited + INTERVAL);
        assert!(!caret_visible(&runtime));
    }
}

#[test]
fn key_release_does_not_postpone_blink() {
    let mut runtime = mount(EditableText::new(TextEditingController::new("abc")));
    let start = Instant::now();
    runtime.update(start);
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp {
        key: Key::ArrowLeft,
        modifiers: Modifiers::default(),
    }));
    assert_deadline(
        &runtime.update(start + Duration::from_millis(100)),
        start + INTERVAL,
    );
    runtime.update(start + INTERVAL);
    assert!(!caret_visible(&runtime));
}

#[test]
fn unrelated_rebuild_preserves_phase_but_controller_edit_resets_it() {
    let controller = TextEditingController::new("abc");
    let observed = controller.clone();
    let revision = Signal::new(0);
    let observed_revision = revision.clone();
    let mut runtime = mount(move |cx: &mut BuildCx| {
        cx.track(&observed_revision);
        EditableText::new(observed.clone()).build(cx)
    });
    let start = Instant::now();
    runtime.update(start);
    runtime.update(start + INTERVAL);
    assert!(!caret_visible(&runtime));

    revision.set(1);
    let rebuilt = start + INTERVAL + Duration::from_millis(10);
    assert_deadline(&runtime.update(rebuilt), start + INTERVAL * 2);
    assert!(!caret_visible(&runtime));

    controller.set_text("programmatic");
    let edited = rebuilt + Duration::from_millis(10);
    assert_deadline(&runtime.update(edited), edited + INTERVAL);
    assert!(caret_visible(&runtime));
}

#[test]
fn focus_loss_disable_and_unmount_remove_caret_deadline() {
    let enabled = Signal::new(true);
    let observed_enabled = enabled.clone();
    let editor_focus = FocusHandle::new();
    let controller = TextEditingController::new("abc");
    let mut runtime = mount(move |cx: &mut BuildCx| {
        cx.track(&observed_enabled);
        Column::new()
            .push_child(
                EditableText::new(controller.clone())
                    .enabled(*observed_enabled.read())
                    .focus_handle(editor_focus),
            )
            .push_child(Button::new("other"))
            .build(cx)
    });
    let start = Instant::now();
    runtime.update(start);
    runtime.update(start + INTERVAL);
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
    }));
    let lost = runtime.update(start + INTERVAL);
    assert_eq!(lost.control_flow, None);
    assert!(!caret_visible(&runtime));

    runtime.request_focus(&editor_focus);
    let regained = start + INTERVAL * 2;
    assert_deadline(&runtime.update(regained), regained + INTERVAL);
    assert!(caret_visible(&runtime));

    enabled.set(false);
    assert_eq!(runtime.update(regained).control_flow, None);
    assert!(!caret_visible(&runtime));

    enabled.set(true);
    runtime.update(regained);
    runtime.request_focus(&editor_focus);
    assert_deadline(&runtime.update(regained), regained + INTERVAL);
    runtime.clear_root();
    assert_eq!(runtime.update(regained + INTERVAL).control_flow, None);
    assert!(!caret_visible(&runtime));
}

#[test]
fn ime_hidden_caret_has_no_blink_deadline_and_visible_caret_resumes() {
    let mut runtime = mount(EditableText::new(TextEditingController::new("abc")));
    let start = Instant::now();
    runtime.update(start);
    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("draft", None)));
    assert_eq!(runtime.update(start).control_flow, None);
    assert!(!caret_visible(&runtime));

    runtime.dispatch(UiEvent::ImePreedit(ImePreedit::new("draft", Some((2, 2)))));
    assert_deadline(&runtime.update(start), start + INTERVAL);
    assert!(caret_visible(&runtime));
    runtime.update(start + INTERVAL);
    assert!(!caret_visible(&runtime));
}

#[test]
fn caret_deadline_arbitrates_with_external_schedule() {
    for delay in [Duration::from_millis(100), Duration::from_secs(5)] {
        let start = Instant::now();
        let external_deadline = start + delay;
        let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| ExternalScheduleDemand {
            deadline: Some(external_deadline),
            ..ExternalScheduleDemand::empty()
        });
        let mut runtime = mount(
            Column::new()
                .push_child(EditableText::new(TextEditingController::new("abc")))
                .push_child(CustomPaint::new(1).schedule(schedule)),
        );
        assert_deadline(
            &runtime.update(start),
            external_deadline.min(start + INTERVAL),
        );
    }
}
