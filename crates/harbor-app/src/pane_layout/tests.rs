use super::*;
use crate::{
    tab_manager::{TabManager, TerminalTabResources},
    workspace::{WorkspaceUiController, tab_workspace},
};
use harbor_terminal::{Terminal, TerminalSize, TextMetrics};
use harbor_widget::{
    input::event::{PointerButton, PointerEvent, PointerPhase, UiEvent},
    layout::{Point, Size},
    renderer::Viewport,
    runtime::Runtime,
};
use std::{sync::atomic::AtomicBool, time::Instant};

#[allow(clippy::arc_with_non_send_sync)]
fn resources(draw: ExternalDrawId) -> TerminalTabResources {
    let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
    let bridge =
        TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(terminal, bridge, "fixture")
}
fn split(tabs: &mut TabManager, source: PaneBinding, axis: SplitAxis) -> PaneBinding {
    let (target, draw) = tabs.reserve_split_ids(source).unwrap();
    tabs.attach_split(source, target, draw, resources(draw), axis)
        .unwrap();
    target
}
fn fixture() -> (TabManager, [PaneBinding; 3], WorkspaceUiController, Runtime) {
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(resources(draw))).unwrap();
    let a = tabs.active_binding().unwrap();
    let b = split(&mut tabs, a, SplitAxis::Horizontal);
    let c = split(&mut tabs, b, SplitAxis::Vertical);
    let PaneLayout::Split { ratio, second, .. } = tabs.active_layout().unwrap() else {
        panic!()
    };
    ratio.set_ratio(0.6);
    let PaneLayout::Split { ratio, .. } = *second else {
        panic!()
    };
    ratio.set_ratio(0.4);
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    ui.sync_panes(&tabs, 1000.0);
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(1000, 600, 1.0));
    rt.set_root(tab_workspace(ui.clone(), false));
    rt.update(Instant::now());
    (tabs, [a, b, c], ui, rt)
}
fn grid(tabs: &TabManager, binding: PaneBinding) -> TerminalSize {
    let terminal = tabs.session_terminal(binding.session).unwrap();
    let terminal = terminal.lock().unwrap();
    TerminalSize {
        rows: terminal.screen().rows(),
        cols: terminal.screen().cols(),
    }
}
fn observe(tabs: &mut TabManager, ui: &WorkspaceUiController, viewport: &Viewport) {
    tabs.observe_pane_allocations(
        &ui.pane_allocations(),
        viewport.scale_factor,
        viewport.physical_size,
        &TextMetrics {
            cell_width: 8.0,
            line_height: 16.0,
            ..harbor_widget::runtime::DEFAULT_TEXT_METRICS
        },
    );
}
fn settle(tabs: &mut TabManager) {
    for _ in 0..20 {
        let batch = tabs.advance_resizes();
        assert!(batch.attempted <= 2);
        if !batch.ready_remaining {
            return;
        }
    }
    panic!("ready work did not converge");
}
fn allocation(ui: &WorkspaceUiController, binding: PaneBinding) -> PaneAllocation {
    ui.pane_allocations()
        .into_iter()
        .find(|allocation| allocation.binding == binding)
        .unwrap()
}
fn pointer(rt: &mut Runtime, position: Point, phase: PointerPhase) {
    rt.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        71,
    )));
    rt.update(Instant::now());
}

#[test]
fn mounted_mixed_layout_has_independent_bindings_grids_dpi_and_minimize_retention() {
    let (mut tabs, [a, b, c], ui, mut rt) = fixture();
    let left = allocation(&ui, a).rect;
    let top = allocation(&ui, b).rect;
    let bottom = allocation(&ui, c).rect;
    assert_eq!(left.min, Point::new(204.0, 4.0));
    assert!((left.size().width - 471.6).abs() < 0.01);
    assert!((top.min.x - left.max.x - 6.0).abs() < 0.01);
    assert_eq!(top.min.x, bottom.min.x);
    assert!((bottom.min.y - top.max.y - 6.0).abs() < 0.01);
    assert_eq!(left.max.y, 596.0);
    assert_eq!(bottom.max.y, 596.0);
    assert_eq!(ui.pane_allocations().len(), 3);
    assert_eq!(tabs.active_binding(), Some(a));
    observe(&mut tabs, &ui, &Viewport::new(1000, 600, 1.0));
    assert_eq!(
        grid(&tabs, a),
        TerminalSize { rows: 4, cols: 20 },
        "allocation does not admit work"
    );
    settle(&mut tabs);
    let before = [grid(&tabs, a), grid(&tabs, b), grid(&tabs, c)];
    assert_ne!(before[0].cols, before[1].cols);
    assert_ne!(before[1].rows, before[2].rows);
    observe(&mut tabs, &ui, &Viewport::new(1000, 600, 1.0));
    assert_eq!(
        tabs.advance_resizes().attempted,
        0,
        "unchanged targets are not transactions"
    );
    observe(&mut tabs, &ui, &Viewport::new(0, 0, 1.0));
    observe(&mut tabs, &ui, &Viewport::new(1000, 600, f32::NAN));
    assert_eq!(tabs.advance_resizes().attempted, 0);
    assert_eq!([grid(&tabs, a), grid(&tabs, b), grid(&tabs, c)], before);
    rt.set_viewport(Viewport::new(1500, 900, 1.5));
    rt.update(Instant::now());
    observe(&mut tabs, &ui, &Viewport::new(1500, 900, 1.5));
    settle(&mut tabs);
    assert!(grid(&tabs, a).cols > before[0].cols);
    assert!(grid(&tabs, b).rows > before[1].rows);
    let mut invalid = allocation(&ui, a);
    invalid.rect = Rect::from_min_size(Point::ZERO, Size::ZERO);
    tabs.observe_pane_allocations(
        &[invalid],
        1.0,
        (1500, 900),
        &harbor_widget::runtime::DEFAULT_TEXT_METRICS,
    );
    assert_eq!(tabs.advance_resizes().attempted, 0);
    invalid.rect = Rect::from_min_size(Point::ZERO, Size::new(1.0, 1.0));
    tabs.observe_pane_allocations(
        &[invalid],
        1.0,
        (1500, 900),
        &harbor_widget::runtime::DEFAULT_TEXT_METRICS,
    );
    settle(&mut tabs);
    assert_eq!(grid(&tabs, a), TerminalSize { rows: 1, cols: 2 });
    invalid.rect = Rect::from_min_size(Point::ZERO, Size::new(1000.0, 600.0));
    let tiny_metrics = TextMetrics {
        cell_width: 0.0001,
        line_height: 0.0001,
        ..harbor_widget::runtime::DEFAULT_TEXT_METRICS
    };
    tabs.observe_pane_allocations(&[invalid], 1.0, (1500, 900), &tiny_metrics);
    assert_eq!(
        tabs.advance_resizes().attempted,
        0,
        "unsupported signed-16-bit PTY grids are rejected before admission"
    );
}

#[test]
fn mounted_drag_is_captured_without_terminal_input_and_admission_stays_bounded() {
    let (mut tabs, [a, b, c], ui, mut rt) = fixture();
    let before = allocation(&ui, a).rect;
    let press = Point::new(before.max.x + 3.0, before.min.y + 20.0);
    pointer(&mut rt, press, PointerPhase::Down);
    for x in [800.0, 300.0, 950.0, 650.0] {
        pointer(&mut rt, Point::new(x, press.y), PointerPhase::Move);
        observe(&mut tabs, &ui, &Viewport::new(1000, 600, 1.0));
        assert_eq!(grid(&tabs, a), TerminalSize { rows: 4, cols: 20 });
    }
    pointer(&mut rt, Point::new(650.0, press.y), PointerPhase::Up);
    let after = allocation(&ui, a).rect;
    assert_ne!(after, before);
    assert!((after.max.x - 647.0).abs() < 0.01);
    assert_eq!(tabs.active_binding(), Some(a));
    assert!(rt.drain_external_input().is_empty());
    settle(&mut tabs);
    assert_ne!(grid(&tabs, a).cols, grid(&tabs, b).cols);
    assert_ne!(grid(&tabs, b).rows, grid(&tabs, c).rows);
    pointer(&mut rt, Point::new(400.0, press.y), PointerPhase::Move);
    assert_eq!(allocation(&ui, a).rect, after, "release ends drag");
}

#[test]
fn maximize_restore_inactive_tabs_and_collapse_preserve_sessions_and_ratios() {
    let (mut tabs, [a, b, c], ui, mut rt) = fixture();
    observe(&mut tabs, &ui, &Viewport::new(1000, 600, 1.0));
    settle(&mut tabs);
    let retained = [grid(&tabs, b), grid(&tabs, c)];
    let b_terminal = tabs.session_terminal(b.session).unwrap();
    b_terminal.lock().unwrap().put_bytes(b"retained sibling");
    tabs.maximize_pane(a);
    ui.sync_panes(&tabs, 1000.0);
    rt.update(Instant::now());
    assert_eq!(ui.pane_allocations().len(), 1);
    rt.set_viewport(Viewport::new(1600, 1000, 2.0));
    rt.update(Instant::now());
    observe(&mut tabs, &ui, &Viewport::new(1600, 1000, 2.0));
    settle(&mut tabs);
    assert_eq!([grid(&tabs, b), grid(&tabs, c)], retained);
    assert!(!tabs.process_output(b.session).request_active_invalidation);
    tabs.maximize_pane(a);
    ui.sync_panes(&tabs, 800.0);
    rt.update(Instant::now());
    observe(&mut tabs, &ui, &Viewport::new(1600, 1000, 2.0));
    settle(&mut tabs);
    assert_ne!(grid(&tabs, b), retained[0]);
    assert!(Arc::ptr_eq(
        &b_terminal,
        &tabs.session_terminal(b.session).unwrap()
    ));
    let PaneLayout::Split { ratio, .. } = tabs.active_layout().unwrap() else {
        panic!()
    };
    assert_eq!(ratio.ratio(), 0.6);
    let a_grid = grid(&tabs, a);
    tabs.create_tab(|_, draw| Ok(resources(draw))).unwrap();
    let other = tabs.active_binding().unwrap();
    ui.sync_panes(&tabs, 800.0);
    rt.update(Instant::now());
    rt.set_viewport(Viewport::new(1200, 700, 1.0));
    rt.update(Instant::now());
    observe(&mut tabs, &ui, &Viewport::new(1200, 700, 1.0));
    settle(&mut tabs);
    assert_eq!(grid(&tabs, a), a_grid);
    tabs.activate(a.workspace);
    ui.sync_panes(&tabs, 1200.0);
    rt.update(Instant::now());
    observe(&mut tabs, &ui, &Viewport::new(1200, 700, 1.0));
    settle(&mut tabs);
    assert_ne!(grid(&tabs, a), a_grid);
    assert!(!tabs.close_pane(b).close_window);
    ui.sync_panes(&tabs, 1200.0);
    rt.update(Instant::now());
    assert_eq!(
        tabs.active_layout()
            .unwrap()
            .leaves()
            .iter()
            .map(|(binding, _)| *binding)
            .collect::<Vec<_>>(),
        [a, c]
    );
    assert!(tabs.contains_binding(other));
    assert!(b_terminal.lock().unwrap().is_session_closed());
    assert_eq!(ratio.ratio(), 0.6, "ancestor ratio survives collapse");
    assert_eq!(tabs.process_output(b.session), Default::default());
}

#[test]
fn visible_unfocused_output_schedules_while_hidden_cursor_providers_are_unmounted() {
    let (mut tabs, [a, b, c], ui, mut rt) = fixture();
    let targets = tabs.configuration_targets();
    rt.request_focus(&ui.terminal_focus());
    rt.update(Instant::now());
    for binding in [a, b, c] {
        tabs.session_terminal(binding.session)
            .unwrap()
            .lock()
            .unwrap()
            .put_bytes(b"simultaneous output");
        assert!(
            tabs.process_output(binding.session)
                .request_active_invalidation
        );
    }
    let before: Vec<_> = targets
        .iter()
        .map(|(_, _, bridge)| bridge.activity().1)
        .collect();
    rt.update(Instant::now());
    for (index, (_, _, bridge)) in targets.iter().enumerate() {
        assert!(bridge.activity().1 > before[index]);
    }
    tabs.maximize_pane(a);
    ui.sync_panes(&tabs, 1000.0);
    rt.update(Instant::now());
    let before: Vec<_> = targets
        .iter()
        .map(|(_, _, bridge)| bridge.activity().1)
        .collect();
    for _ in 0..3 {
        rt.update(Instant::now());
    }
    assert!(targets[0].2.activity().1 > before[0]);
    assert_eq!(targets[1].2.activity().1, before[1]);
    assert_eq!(targets[2].2.activity().1, before[2]);
    rt.set_root(harbor_widget::SizedBox::new(Size::ZERO).unwrap());
    rt.update(Instant::now());
    let before = targets[0].2.activity();
    let effects = rt.update(Instant::now());
    assert_eq!(targets[0].2.activity(), before);
    assert!(!effects.request_redraw);
}

#[test]
fn stale_source_registration_and_reused_engine_attachment_never_replace_survivors() {
    let (mut tabs, [a, b, _], _, _) = fixture();
    let (target, draw) = tabs.reserve_split_ids(a).unwrap();
    let wrong = resources(draw.checked_add(100).unwrap());
    let rejected = wrong.terminal().clone();
    assert!(
        tabs.attach_split(a, target, draw, wrong, SplitAxis::Horizontal)
            .is_err()
    );
    assert!(rejected.lock().unwrap().is_session_closed());
    assert!(!tabs.contains_binding(target));
    let (target, draw) = tabs.reserve_split_ids(b).unwrap();
    tabs.close_pane(b);
    assert!(
        tabs.attach_split(b, target, draw, resources(draw), SplitAxis::Horizontal)
            .is_err()
    );
    let survivor = tabs.session_terminal(a.session).unwrap();
    let (target, draw) = tabs.reserve_split_ids(a).unwrap();
    let bridge =
        TerminalWidgetBridge::new(draw, survivor.clone(), Arc::new(AtomicBool::new(false)));
    assert!(
        tabs.attach_split(
            a,
            target,
            draw,
            TerminalTabResources::new(survivor.clone(), bridge, "bad"),
            SplitAxis::Vertical
        )
        .is_err()
    );
    assert!(!survivor.lock().unwrap().is_session_closed());
    assert!(tabs.contains_binding(a));
    let (target, draw) = tabs.reserve_split_ids(a).unwrap();
    let old_bridge = tabs.active_bridge().unwrap();
    let malformed = TerminalTabResources::new(
        resources(draw).terminal().clone(),
        old_bridge.clone(),
        "bad bridge",
    );
    assert!(
        tabs.attach_split(a, target, draw, malformed, SplitAxis::Vertical)
            .is_err()
    );
    let before = old_bridge.activity().1;
    let ui = WorkspaceUiController::new(tabs.snapshots(), old_bridge.clone(), 1000.0);
    ui.sync_panes(&tabs, 1000.0);
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(1000, 600, 1.0));
    rt.set_root(tab_workspace(ui, false));
    rt.update(Instant::now());
    assert!(
        old_bridge.activity().1 > before,
        "malformed attachment cannot revoke a survivor registration"
    );
    assert!(!survivor.lock().unwrap().is_session_closed());
}

#[test]
fn stale_observer_cannot_publish_after_visibility_or_registration_replacement() {
    let (tabs, [a, _, _], _, _) = fixture();
    let layout = tabs.active_layout().unwrap();
    let allocations = PaneAllocations::default();
    allocations.publish(Some(&layout));
    let draw = layout.leaves()[0].1;
    let old = allocations.observer(a, draw);
    allocations.publish(None);
    allocations.publish(Some(&layout));
    let mut rt = Runtime::new();
    rt.set_viewport(Viewport::new(100, 100, 1.0));
    rt.set_root(old.child(harbor_widget::ExternalSurface::new(draw)));
    rt.update(Instant::now());
    assert!(allocations.latest().is_empty());
    let wrong = allocations.observer(a, draw.checked_add(1).unwrap());
    rt.set_root(wrong.child(harbor_widget::ExternalSurface::new(draw)));
    rt.update(Instant::now());
    assert!(allocations.latest().is_empty());
    rt.set_root(
        allocations
            .observer(a, draw)
            .child(harbor_widget::ExternalSurface::new(draw)),
    );
    rt.update(Instant::now());
    assert_eq!(allocations.latest().len(), 1);
}
#[test]
fn attachment_uses_original_workspace_after_switch_and_fair_pane_batches_converge() {
    let (mut tabs, [a, _, _], ui, mut rt) = fixture();
    let (target, draw) = tabs.reserve_split_ids(a).unwrap();
    tabs.create_tab(|_, draw| Ok(resources(draw))).unwrap();
    let other = tabs.active_binding().unwrap();
    tabs.attach_split(a, target, draw, resources(draw), SplitAxis::Vertical)
        .unwrap();
    assert_eq!(tabs.active_binding(), Some(other));
    assert!(tabs.contains_binding(target));
    assert_eq!(tabs.active_layout().unwrap().leaves().len(), 1);
    tabs.activate(a.workspace);
    for _ in 0..4 {
        split(&mut tabs, a, SplitAxis::Horizontal);
    }
    ui.sync_panes(&tabs, 1000.0);
    rt.update(Instant::now());
    assert_eq!(ui.pane_allocations().len(), 8);
    let leaves = tabs.visible_bindings();
    for offset in 0..12 {
        let mut allocations = ui.pane_allocations();
        for allocation in &mut allocations {
            allocation.rect.max.y += offset as f32 * 20.0;
        }
        tabs.observe_pane_allocations(
            &allocations,
            1.0,
            (1000, 1000),
            &harbor_widget::runtime::DEFAULT_TEXT_METRICS,
        );
        let batch = tabs.advance_resizes();
        assert!(batch.attempted <= 2);
    }
    for binding in &leaves {
        assert_ne!(
            grid(&tabs, *binding),
            TerminalSize { rows: 4, cols: 20 },
            "background pane made progress while active kept changing"
        );
    }
    settle(&mut tabs);
    assert_eq!(
        tabs.advance_resizes().attempted,
        0,
        "no extra byte or activation required"
    );
}

#[cfg(test)]
mod gpu;
