use super::*;
use harbor_terminal::{TerminalGpuAccess, TerminalRenderPipeline, load_system_fonts};
use harbor_widget::{renderer::FrameTarget, scene::primitive::ExternalDrawGpu};

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rt: &mut Runtime,
    viewport: Viewport,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = viewport.physical_size;
    let target =
        crate::test_gpu::readback::target(device, "mixed pane clipping", width, height, format);
    let view = target.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    rt.prepare_text(queue);
    rt.encode_frame(
        ExternalDrawGpu::new(device, queue, format),
        &mut encoder,
        FrameTarget {
            view: &view,
            clear_color: wgpu::Color::TRANSPARENT,
        },
        viewport,
        false,
    );
    crate::test_gpu::readback::read(
        device,
        queue,
        encoder,
        &target,
        crate::test_gpu::readback::Rows::Tight,
    )
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
#[allow(clippy::arc_with_non_send_sync)]
fn mixed_panes_present_simultaneous_output_clip_and_suspend_hidden_draw_and_blink() {
    let (device, queue) = crate::test_gpu::device();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let fonts = load_system_fonts(&harbor_config::FontSettings::default()).unwrap();
    let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
    let make = |draw| {
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
        let renderer = TerminalRenderPipeline::new(
            TerminalGpuAccess::new(&device, &queue, format),
            (1000, 600),
            load_system_fonts(&harbor_config::FontSettings::default()).unwrap(),
            metrics,
            &terminal.lock().unwrap().read_update(Instant::now()),
        )
        .unwrap();
        let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
            draw,
            terminal.clone(),
            renderer,
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        );
        TerminalTabResources::new(terminal, bridge, "rendered")
    };
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(make(draw))).unwrap();
    let a = tabs.active_binding().unwrap();
    let (b, draw) = tabs.reserve_split_ids(a).unwrap();
    tabs.attach_split(a, b, draw, make(draw), SplitAxis::Horizontal)
        .unwrap();
    let (c, draw) = tabs.reserve_split_ids(b).unwrap();
    tabs.attach_split(b, c, draw, make(draw), SplitAxis::Vertical)
        .unwrap();
    let ui = WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    ui.sync_panes(&tabs, 1000.0);
    let mut rt = Runtime::new();
    rt.init_renderer(&device, format);
    rt.init_text_renderer(&device, &queue, format).unwrap();
    let mut viewport = Viewport::new(1000, 600, 1.0);
    rt.set_viewport(viewport.clone());
    rt.set_root(tab_workspace(ui.clone(), false));
    rt.update(Instant::now());
    let targets = tabs.configuration_targets();
    for (index, (_, terminal, _)) in targets.iter().enumerate() {
        terminal.lock().unwrap().put_bytes(
            format!(
                "\x1b[?25l\x1b[48;2;{}m\x1b[2J",
                ["240;10;10", "10;240;10", "10;10;240"][index]
            )
            .as_bytes(),
        );
    }
    for scale in [1.0, 1.5, 2.0] {
        viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
        rt.set_viewport(viewport.clone());
        rt.update(Instant::now());
        tabs.observe_pane_allocations(
            &ui.pane_allocations(),
            scale,
            viewport.physical_size,
            &metrics,
        );
        settle(&mut tabs);
        rt.update(Instant::now());
        let pixels = render(&device, &queue, &mut rt, viewport.clone());
        let sample = |point: Point| {
            let index = (((point.y * scale) as usize) * viewport.physical_size.0 as usize
                + (point.x * scale) as usize)
                * 4;
            &pixels[index..index + 4]
        };
        for (index, binding) in [a, b, c].iter().enumerate() {
            let rect = allocation(&ui, *binding).rect;
            let color = sample(Point::new(rect.min.x + 30.0, rect.min.y + 30.0));
            assert!(
                color[index] > 150 && color[(index + 1) % 3] < 50,
                "pane {index}: {color:?}"
            );
            let corner = sample(Point::new(rect.min.x + 0.1, rect.min.y + 0.1));
            assert!(
                corner[index] < color[index],
                "rounded ancestor clips terminal corner"
            );
            assert!(
                targets[index].2.activity().0 > 0,
                "every visible bridge draws without focus"
            );
        }
        // Actual rendered bridges publish candidate geometry from each pane,
        // not the outer terminal panel or a sibling's origin.
        for (index, binding) in [a, b, c].iter().enumerate() {
            targets[index].1.lock().unwrap().put_bytes(b"\x1b[2;3H");
            rt.request_focus(&ui.pane_focus(*binding));
            for request in ui.drain_actions() {
                if let crate::command::AppCommand::Pane(command) = request.command {
                    tabs.execute_pane_command(command);
                }
            }
            ui.sync_panes(&tabs, 1000.0);
            rt.update(Instant::now());
            let effect = rt.dispatch(UiEvent::ImePreedit(
                harbor_widget::input::event::ImePreedit::new("x", Some((1, 1))),
            ));
            let position = effect
                .ime
                .and_then(|ime| ime.position)
                .expect("candidate geometry");
            let rect = allocation(&ui, *binding).rect;
            assert!(
                (position.x - (rect.min.x + (16.0 + 3.0 * metrics.cell_width) / scale)).abs() < 1.0
            );
            assert!((position.y - (rect.min.y + (16.0 + metrics.line_height) / scale)).abs() < 1.0);
            for (other, (_, terminal, _)) in targets.iter().enumerate() {
                if other != index {
                    assert!(terminal.lock().unwrap().preedit().is_none());
                }
            }
            rt.dispatch(UiEvent::ImePreedit(
                harbor_widget::input::event::ImePreedit::new("", None),
            ));
        }
        let left = allocation(&ui, a).rect;
        let gap = sample(Point::new(left.max.x + 3.0, left.min.y + 30.0));
        assert!(
            gap[0].abs_diff(gap[1]) < 8 && gap[1].abs_diff(gap[2]) < 8,
            "no terminal paint crosses divider: {gap:?}"
        );
    }
    tabs.maximize_pane(a);
    ui.sync_panes(&tabs, 1000.0);
    rt.update(Instant::now());
    let hidden = [targets[1].2.activity(), targets[2].2.activity()];
    targets[1]
        .1
        .lock()
        .unwrap()
        .put_bytes(b"\x1b[?25hhidden output");
    targets[2].1.lock().unwrap().put_bytes(b"\x1b[?25h");
    for _ in 0..3 {
        rt.update(Instant::now());
        render(&device, &queue, &mut rt, viewport.clone());
    }
    let quiet_before = targets[0].2.activity();
    for _ in 0..120 {
        let effects = rt.update(Instant::now());
        assert!(!effects.request_redraw);
        assert!(matches!(
            effects.control_flow,
            None | Some(harbor_widget::effects::ControlFlowEffect::Wait)
        ));
    }
    assert_eq!(targets[0].2.activity().0, quiet_before.0);
    let output_before = targets[0].2.activity();
    for tick in 0..60 {
        for (index, (session, terminal, _)) in targets.iter().enumerate() {
            terminal
                .lock()
                .unwrap()
                .put_bytes(format!("\rwork-{index}-{tick}\x1b[K").as_bytes());
            assert_eq!(
                tabs.process_output(*session).request_active_invalidation,
                index == 0
            );
        }
        rt.update(Instant::now());
        render(&device, &queue, &mut rt, viewport.clone());
    }
    println!(
        "T0005_WORK quiet_updates=120 quiet_draws=0 output_ticks=60 visible_draws={} hidden_draws=0 hidden_schedule_calls=0",
        targets[0].2.activity().0 - output_before.0
    );
    assert_eq!(targets[0].2.activity().0 - output_before.0, 60);
    assert_eq!([targets[1].2.activity(), targets[2].2.activity()], hidden);
    // Visible cursor is disabled and its projection acknowledged: hidden cursor/output
    // cannot create a deadline or request a redraw.
    let effects = rt.update(Instant::now());
    assert!(!effects.request_redraw);
    assert!(matches!(
        effects.control_flow,
        None | Some(harbor_widget::effects::ControlFlowEffect::Wait)
    ));
    tabs.maximize_pane(a);
    ui.sync_panes(&tabs, 1000.0);
    rt.update(Instant::now());
    tabs.observe_pane_allocations(
        &ui.pane_allocations(),
        viewport.scale_factor,
        viewport.physical_size,
        &metrics,
    );
    settle(&mut tabs);
    render(&device, &queue, &mut rt, viewport);
    assert!(targets[1].2.activity().0 > hidden[0].0);
    assert!(
        targets[1]
            .1
            .lock()
            .unwrap()
            .snapshot()
            .dirty_ranges
            .is_empty()
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
#[allow(clippy::arc_with_non_send_sync)]
fn pending_authentication_late_prompt_is_visible_and_reachable() {
    use harbor_widget::input::event::ImePreedit;
    let (device, queue) = crate::test_gpu::device();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    for scale in [1.0, 1.5, 2.0] {
        let (mut tabs, panes, ui, _) = fixture();
        let (target, draw) = tabs.reserve_split_ids(panes[0]).unwrap();
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(40, 100)));
        terminal.lock().unwrap().use_host_resize_scheduling();
        terminal
            .lock()
            .unwrap()
            .set_clipboard_delivery(harbor_terminal::ClipboardDelivery::Discard);
        let fonts = load_system_fonts(&harbor_config::FontSettings::default()).unwrap();
        let metrics = TextMetrics::from_font_metrics(fonts.font_metrics());
        let renderer = TerminalRenderPipeline::new(
            TerminalGpuAccess::new(&device, &queue, format),
            (1000, 600),
            fonts,
            metrics,
            &terminal.lock().unwrap().read_update(Instant::now()),
        )
        .unwrap();
        let bridge = TerminalWidgetBridge::new_rendered_with_hyperlink_activation(
            draw,
            terminal.clone(),
            renderer,
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        );
        let resources = TerminalTabResources::new(terminal.clone(), bridge.clone(), "unpublished");
        let focus = ui.show_split_authentication(target, bridge);
        let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
        let mut rt = Runtime::new();
        rt.init_renderer(&device, format);
        rt.init_text_renderer(&device, &queue, format).unwrap();
        rt.set_viewport(viewport.clone());
        rt.set_root(tab_workspace(ui.clone(), false));
        rt.request_focus(&focus);
        terminal
            .lock()
            .unwrap()
            .put_bytes(b"\x1b[20;1H\x1b[42mPassword: \x1b[0m");
        rt.update(Instant::now());
        let _ = render(&device, &queue, &mut rt, viewport.clone());
        assert_eq!(
            terminal.lock().unwrap().screen().rows(),
            40,
            "drawing must not resize the unpublished PTY"
        );
        // The same bounded Host admission used by ActiveSession, after mounted layout.
        for _ in 0..8 {
            ui.advance_split_authentication(target, scale, viewport.physical_size, &metrics);
            rt.update(Instant::now());
        }
        assert_eq!(
            terminal.lock().unwrap().screen().rows(),
            ((240.0 * scale - 32.0) / metrics.line_height)
                .floor()
                .max(1.0) as usize,
            "grid must fit the mounted allocation at current DPI"
        );
        let pixels = render(&device, &queue, &mut rt, viewport.clone());
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|p| p[1] > 90
                && p[1] > p[0].saturating_add(40)
                && p[1] > p[2].saturating_add(40)),
            "late Password prompt clipped at DPI {scale}"
        );
        let effects = rt.dispatch(UiEvent::ImePreedit(ImePreedit::new("x", None)));
        let position = effects
            .ime
            .and_then(|ime| ime.position)
            .expect("cursor candidate position");
        assert!(position.y < 276.0 && position.x < 1000.0);
        assert!(
            terminal.lock().unwrap().preedit().is_some(),
            "reserved terminal receives input"
        );
        assert!(!tabs.contains_binding(target));
        terminal
            .lock()
            .unwrap()
            .put_bytes(b"\x1b]52;c;bm90LWF1dGhvcml6ZWQ=\x07");
        assert!(
            terminal
                .lock()
                .unwrap()
                .drain_output_events()
                .iter()
                .all(|event| !matches!(
                    event,
                    harbor_terminal::TerminalOutputEvent::ClipboardWrite(_)
                ))
        );
        drop(resources);
    }
}
