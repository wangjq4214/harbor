use harbor_widget::{
    Children, ComponentExt, ExternalSurface, IntoChildView, SizedBox, WithChildren,
    input::event_ctx::EventHandled,
    layout::Size,
    scene::primitive::{ExternalDrawFn, ExternalImeFn, ExternalScheduleDemand, ExternalScheduleFn},
    view,
    widgets::external_surface::{ExternalInputFn, ExternalUnmountFn},
};
use std::{cell::Cell, rc::Rc, sync::Arc};

fn main() {
    // External providers remain Arc-backed, UI-local and 'static, not Send/Sync.
    let calls = Rc::new(Cell::new(0));
    let draw_calls = calls.clone();
    let draw: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, _, _| {
        draw_calls.set(draw_calls.get() + 1);
    });
    let schedule_calls = calls.clone();
    let schedule: Arc<ExternalScheduleFn> = Arc::new(move |_, _| {
        schedule_calls.set(schedule_calls.get() + 1);
        ExternalScheduleDemand::empty()
    });
    let ime_calls = calls.clone();
    let ime: Arc<ExternalImeFn> = Arc::new(move |_, _| {
        ime_calls.set(ime_calls.get() + 1);
        None
    });
    let input_calls = calls.clone();
    let input: Arc<ExternalInputFn> = Arc::new(move |_, _| {
        input_calls.set(input_calls.get() + 1);
        EventHandled::Ignored
    });
    let unmount: Arc<ExternalUnmountFn> = Arc::new(move || calls.set(calls.get() + 1));
    let surface = ExternalSurface::new(42)
        .on_draw(draw)
        .schedule_with(schedule)
        .ime_with(ime)
        .on_input(input)
        .on_unmount(unmount)
        .child(SizedBox::new(Size::ZERO).unwrap())
        .child(SizedBox::new(Size::ZERO).unwrap());
    let _module_path: harbor_widget::widgets::external_surface::ExternalSurface = surface.clone();
    let _widgets_path: harbor_widget::widgets::ExternalSurface = surface.clone();
    let _ = surface
        .clone()
        .with_children(Children::new())
        .unwrap()
        .into_child_view();
    let _ = view! { surface.clone().keyed("external") => { SizedBox::new(Size::ZERO); } };
    let _ = view! { surface => {} };
}
