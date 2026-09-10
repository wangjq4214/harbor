use std::{cell::RefCell, rc::Rc, time::Instant};

use harbor_widget::{
    layout::Size,
    runtime::Runtime,
    signal::Signal,
    view::{BuildCx, Component},
    widgets::sized_box::SizedBox,
};

#[test]
fn signal_state_rebuilds_closure_component() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let state_handle = Rc::new(RefCell::new(None::<Signal<u32>>));
    let component_observed = Rc::clone(&observed);
    let component_state_handle = Rc::clone(&state_handle);
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        let state = cx.use_state(|| 0u32);
        component_observed.borrow_mut().push(*state.read());
        *component_state_handle.borrow_mut() = Some(state);
        SizedBox::new(Size::new(1.0, 1.0)).build(cx)
    });
    runtime.update(Instant::now());

    assert_eq!(observed.borrow().as_slice(), &[0]);
    state_handle
        .borrow()
        .as_ref()
        .expect("closure component publishes its signal")
        .set(7);
    let effects = runtime.update(Instant::now());

    assert!(effects.request_redraw);
    assert_eq!(observed.borrow().as_slice(), &[0, 7]);
}
