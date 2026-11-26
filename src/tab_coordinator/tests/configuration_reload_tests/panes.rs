use super::*;
use harbor_app::{pane_layout::PaneLayout, tab_manager::PaneBinding};
use harbor_widget::SplitAxis;

fn resources(
    policy: &TerminalConfiguration,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    draw: harbor_widget::scene::primitive::ExternalDrawId,
) -> TerminalTabResources {
    let (terminal, renderer) = policy
        .prepare_new_terminal(
            TerminalGpuAccess::new(device, queue, wgpu::TextureFormat::Rgba8Unorm),
            load_system_fonts(&policy.font_settings).unwrap(),
            TerminalSize { rows: 7, cols: 27 },
            (640, 320),
            false,
            harbor_terminal::ClipboardDelivery::Discard,
        )
        .unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let terminal = Arc::new(Mutex::new(terminal));
    let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
        draw,
        terminal.clone(),
        renderer,
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_| {}),
    );
    TerminalTabResources::new(terminal, bridge, "pane-fixture")
}
fn grids(f: &Fixture) -> Vec<TerminalSize> {
    f.tabs
        .configuration_targets()
        .iter()
        .map(|(_, terminal, _)| {
            let terminal = terminal.lock().unwrap();
            TerminalSize {
                rows: terminal.screen().rows(),
                cols: terminal.screen().cols(),
            }
        })
        .collect()
}
fn sync(f: &mut Fixture) {
    f.ui.sync_panes(&f.tabs, 640.0);
    f._runtime.update(Instant::now());
    f.tabs
        .observe_pane_allocations(&f.ui.pane_allocations(), 1.0, (640, 320), &f.policy.metrics);
}
fn settle(f: &mut Fixture) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        for (id, _, _) in f.tabs.configuration_targets() {
            f.tabs.process_output(id);
        }
        let batch = f.tabs.advance_resizes();
        assert!(batch.attempted <= 2);
        if f.tabs
            .configuration_targets()
            .iter()
            .all(|(_, terminal, _)| !terminal.lock().unwrap().resize_pending())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "pane resize failed to finish without new child output"
        );
        std::thread::yield_now();
    }
}

mod gpu {
    use super::*;
    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn unequal_visible_and_inactive_hidden_reload_rejects_atomically_and_retains_hidden_grids() {
        let mut f = Fixture::new();
        let source = f.tabs.active_binding().unwrap();
        let (target, draw) = f.tabs.reserve_split_ids(source).unwrap();
        let extra = resources(&f.policy, &f.device, &f.queue, draw);
        f.tabs
            .attach_split(source, target, draw, extra, SplitAxis::Horizontal)
            .unwrap();
        let PaneLayout::Split { ratio, .. } = f.tabs.active_layout().unwrap() else {
            panic!()
        };
        ratio.set_ratio(0.7);
        sync(&mut f);
        settle(&mut f);
        let before = grids(&f);
        assert_ne!(before[1], before[2]);
        let targets = f.tabs.configuration_targets();
        let accepted = f.accepted.clone();
        let candidate = f.candidate();
        assert!(
            f.reload_with(candidate.clone(), |index, stage| {
                if index == 2 && stage == ReloadStage::Gpu {
                    anyhow::bail!("later unequal pane GPU rejection");
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(f.accepted, accepted);
        assert_eq!(grids(&f), before);
        let mut effects = Vec::new();
        let error = f
            .reload_with(candidate.clone(), |index, stage| {
                if let ReloadStage::Pty(size) = stage {
                    effects.push((index, size));
                    if index == 2 || (index == 1 && size == before[1]) {
                        anyhow::bail!("unequal native/compensation rejection");
                    }
                }
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("recovery failed"));
        assert_eq!(f.accepted, accepted);
        assert_eq!(effects.len(), 3);
        assert_eq!(effects[0].0, 1);
        assert_eq!(effects[1].0, 2);
        assert_ne!(effects[0].1, effects[1].1);
        assert_eq!(effects[2], (1, before[1]));
        // Retained per-session intent, not a common size or the faulted model's
        // reconciled size, restores the old accepted generation.
        f.reload(accepted.clone(), None).unwrap();
        assert_eq!(grids(&f), before);
        f.reload(candidate.clone(), None).unwrap();
        let after = grids(&f);
        assert_eq!(
            after[0], before[0],
            "inactive session retains its grid, not focused dimensions"
        );
        assert_ne!(after[1], after[2]);
        assert_ne!(after[1], before[1]);
        for ((id, engine, bridge), (current, current_engine, _)) in
            targets.iter().zip(f.tabs.configuration_targets())
        {
            assert_eq!(*id, current);
            assert!(Arc::ptr_eq(engine, &current_engine));
            assert_eq!(
                *bridge.renderer().unwrap().lock().unwrap().metrics(),
                f.policy.metrics
            );
            assert!(!engine.lock().unwrap().is_session_closed());
        }
        f.tabs.maximize_pane(source);
        sync(&mut f);
        settle(&mut f);
        let hidden = grids(&f)[2];
        let mut candidate = f.accepted.clone();
        candidate.font.size += 2.0;
        f.reload(candidate, None).unwrap();
        assert_eq!(
            grids(&f)[2],
            hidden,
            "maximized sibling receives fonts but retains its valid grid"
        );
        f.tabs.maximize_pane(source);
        // The new projection cannot reuse the full-panel rectangle while waiting for
        // its own registration-qualified allocation callbacks.
        f.ui.sync_panes(&f.tabs, 640.0);
        let accepted = f.accepted.clone();
        let mut candidate = accepted.clone();
        candidate.font.size += 1.0;
        assert!(f.reload(candidate, None).is_err());
        assert_eq!(f.accepted, accepted);
        sync(&mut f);
        settle(&mut f);
        assert_ne!(grids(&f)[2], hidden);
    }
}

#[cfg(windows)]
mod pty {
    use super::*;
    #[test]
    #[ignore = "pty: run the matching explicit test recipe"]
    fn native_unequal_panes_output_hidden_retention_reload_and_quiet_restore() {
        let mut f = Fixture::panes();
        f.attach_native_children();
        let source = f.tabs.active_binding().unwrap();
        let native = f.tabs.configuration_targets();
        let sibling: PaneBinding = f.tabs.binding_for_session(native[1].0).unwrap();
        let PaneLayout::Split { ratio, .. } = f.tabs.active_layout().unwrap() else {
            panic!()
        };
        ratio.set_ratio(0.65);
        sync(&mut f);
        settle(&mut f);
        assert_ne!(grids(&f)[0], grids(&f)[1]);
        // An inactive independent session participates in configuration but has no
        // visible registration or allocation. Native children stay in their workspace.
        let policy = &f.policy;
        let device = &f.device;
        let queue = &f.queue;
        f.tabs
            .create_tab(|_, draw| Ok(resources(policy, device, queue, draw)))
            .unwrap();
        f.tabs.activate(source.workspace);
        sync(&mut f);
        settle(&mut f);
        f.tabs.maximize_pane(source);
        sync(&mut f);
        settle(&mut f);
        let hidden = grids(&f);
        output(&mut f, &native, "RESUMED");
        assert!(
            !f.tabs
                .process_output(sibling.session)
                .request_active_invalidation
        );
        assert_eq!(grids(&f)[1], hidden[1]);
        let accepted = f.accepted.clone();
        let candidate = f.candidate();
        assert!(
            f.reload(candidate.clone(), Some(ReloadStage::Model))
                .is_err()
        );
        assert_eq!(f.accepted, accepted);
        assert_eq!(grids(&f), hidden);
        f.reload(candidate, None).unwrap();
        assert_eq!(grids(&f)[1], hidden[1]);
        assert_eq!(grids(&f)[2], hidden[2]);
        f.tabs.maximize_pane(source);
        sync(&mut f);
        settle(&mut f);
        assert_ne!(grids(&f)[1], hidden[1]);
        for ((id, engine, _), (current, current_engine, _)) in
            native.iter().zip(f.tabs.configuration_targets())
        {
            assert_eq!(*id, current);
            assert!(Arc::ptr_eq(engine, &current_engine));
            assert!(!engine.lock().unwrap().resize_pending());
        }
        output(&mut f, &native, "RETRIED");
    }
    fn output(
        f: &mut Fixture,
        native: &[(SessionId, Arc<Mutex<Terminal>>, TerminalWidgetBridge)],
        prefix: &str,
    ) {
        for (_, terminal, _) in native {
            terminal.lock().unwrap().write_pty(b"continue\r").unwrap();
        }
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        loop {
            for (id, _, _) in native {
                f.tabs.process_output(*id);
            }
            if native.iter().enumerate().all(|(index, (_, terminal, _))| {
                retained_text(&terminal.lock().unwrap()).contains(&format!("{prefix}{index}"))
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "native output did not resume");
            std::thread::yield_now();
        }
    }
}
