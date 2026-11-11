use super::*;

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn palette_selection_pixels_have_visible_rounded_cutouts_and_optical_edges_at_multiple_dpi() {
    let (device, queue) = crate::test_gpu::device();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let (_, controller, mut runtime, _) = fixture();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            controller.open_palette();
            let root = controller.clone();
            runtime.set_root(move |cx: &mut BuildCx| {
                let state = root.store.watch(cx).clone();
                let palette = state.popups.get::<palette::PaletteState>().unwrap();
                let panel = crate::popups::Popup::build(
                    palette,
                    cx,
                    &root,
                    &validate_keybindings(&harbor_config::RawKeybindings::default()).unwrap(),
                    0,
                );
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(panel)
                    .build(cx)
            });
            runtime.update(Instant::now());
            let row = row_surface(&runtime, 0.10).unwrap();
            let selected = render(&device, &queue, &mut runtime, viewport.clone(), format);
            let mut state = controller.store.state().read().clone();
            state
                .popups
                .update::<palette::PaletteState>(|palette| palette.selected = 1);
            controller.store.set_state(state);
            runtime.update(Instant::now());
            assert!(
                row_surface(&runtime, 0.10).unwrap().min.y > row.min.y,
                "reference frame deselects the first row"
            );
            let baseline = render(&device, &queue, &mut runtime, viewport.clone(), format);
            if let Ok(directory) = std::env::var("HARBOR_PALETTE_EVIDENCE_DIR") {
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(
                    std::path::Path::new(&directory)
                        .join(format!("selected-{scale}-{format:?}.bgra")),
                    &selected,
                )
                .unwrap();
            }
            let pixel = |pixels: &[u8], x: f32, y: f32| {
                let offset = (((y * scale) as u32 * viewport.physical_size.0 + (x * scale) as u32)
                    * 4) as usize;
                pixels[offset..offset + 4].to_vec()
            };
            assert_ne!(
                pixel(&selected, row.min.x + 1.0, (row.min.y + row.max.y) / 2.0),
                pixel(&baseline, row.min.x + 1.0, (row.min.y + row.max.y) / 2.0),
                "ancestor clip packing must not slice off the straight optical side"
            );
            for (x, y) in [
                (row.min.x + 2.0, row.min.y + 2.0),
                (row.max.x - 3.0, row.min.y + 2.0),
                (row.min.x + 2.0, row.max.y - 3.0),
                (row.max.x - 3.0, row.max.y - 3.0),
            ] {
                let actual = pixel(&selected, x, y);
                let original = pixel(&baseline, x, y);
                assert!(
                    actual
                        .iter()
                        .zip(&original)
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "round corners must expose the original backdrop at scale {scale}, ({x}, {y}): {actual:?} vs {original:?}"
                );
            }
            let x = (row.min.x + row.max.x) / 2.0;
            assert_ne!(
                pixel(&selected, x, row.min.y + 1.0),
                pixel(&baseline, x, row.min.y + 1.0),
                "optical rim must be visible"
            );
            assert_ne!(
                pixel(&selected, x, row.min.y + 15.0),
                pixel(&baseline, x, row.min.y + 15.0),
                "glass remains distinguishable from the panel"
            );
        }
    }
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn search_button_hover_pixels_are_visible_local_and_revert_at_multiple_dpi() {
    let (device, queue) = crate::test_gpu::device();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let (_, controller, mut runtime, _) = fixture();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            controller.open_search(TabId(1));
            let root = controller.clone();
            runtime.set_root(move |cx: &mut BuildCx| {
                let state = root.store.watch(cx).clone();
                let search = state.popups.get::<SearchUiState>().unwrap();
                let panel = search_bar(cx, &root, search, state.popups.generation().unwrap());
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(panel)
                    .build(cx)
            });
            runtime.update(Instant::now());
            let delta = runtime.pending_delta().unwrap();
            let mut buttons: Vec<_> = delta
                .added
                .iter()
                .filter_map(|item| match item.primitive {
                    Primitive::Quad { rect, .. } if rect.size() == Size::new(32.0, 32.0) => {
                        Some(rect)
                    }
                    _ => None,
                })
                .collect();
            buttons.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
            assert_eq!(buttons.len(), 3);
            let baseline = render(&device, &queue, &mut runtime, viewport.clone(), format);
            for rect in buttons {
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(rect.min.x + 1.0, rect.min.y + 16.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    93,
                )));
                runtime.update(Instant::now());
                let hovered = render(&device, &queue, &mut runtime, viewport.clone(), format);
                let mut changed = 0;
                for (index, (before, after)) in baseline
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(hovered.as_chunks::<4>().0.iter())
                    .enumerate()
                {
                    let x = (index as u32 % viewport.physical_size.0) as f32 / scale;
                    let y = (index as u32 / viewport.physical_size.0) as f32 / scale;
                    // Ignore the one-physical-pixel antialiasing boundary for isolation.
                    let edge = 1.0 / scale;
                    if x < rect.min.x - edge
                        || x > rect.max.x + edge
                        || y < rect.min.y - edge
                        || y > rect.max.y + edge
                    {
                        assert_eq!(
                            before, after,
                            "hover must not change pixels outside the button"
                        );
                    } else if before != after {
                        changed += 1;
                    }
                }
                assert!(
                    changed > (100.0 * scale * scale) as usize,
                    "glass must visibly cover the control, not only change the icon"
                );
                let offset = ((((rect.min.y + 16.0) * scale) as u32 * viewport.physical_size.0
                    + ((rect.min.x + 1.0) * scale) as u32)
                    * 4) as usize;
                assert_ne!(
                    &baseline[offset..offset + 4],
                    &hovered[offset..offset + 4],
                    "the optical side must survive ancestor clipping"
                );
                runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                    Point::new(0.0, 0.0),
                    PointerPhase::Move,
                    PointerButton::Left,
                    93,
                )));
                runtime.update(Instant::now());
                let restored = render(&device, &queue, &mut runtime, viewport.clone(), format);
                assert_eq!(
                    baseline, restored,
                    "leaving restores the original frame exactly"
                );
                assert!(controller.drain_actions().is_empty());
            }
        }
    }
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn notification_material_pixels_are_local_rounded_and_retained_at_multiple_dpi() {
    let (device, queue) = crate::test_gpu::device();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let mut runtime = Runtime::new();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            runtime.set_root(View::leaf(TestBackdrop));
            runtime.update(Instant::now());
            let background = render(&device, &queue, &mut runtime, viewport.clone(), format);
            runtime.set_root(|cx: &mut BuildCx| {
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(crate::notifications::configuration_notification(
                        "Configuration reload",
                        "Invalid settings remain visible".into(),
                        0.0,
                        || {},
                    ))
                    .build(cx)
            });
            runtime.update(Instant::now());
            let panel = runtime
                .pending_delta()
                .unwrap()
                .added
                .iter()
                .find_map(|item| match item.primitive {
                    Primitive::BackdropMaterial { rect, material, .. } => {
                        assert_eq!(
                            Some(material),
                            crate::appearance::GlassAppearance::default().material
                        );
                        Some(rect)
                    }
                    _ => None,
                })
                .expect("the actual notification must consume the shared material");
            let actual = render(&device, &queue, &mut runtime, viewport.clone(), format);
            let mut changed_inside = 0;
            for (index, (before, after)) in background
                .as_chunks::<4>()
                .0
                .iter()
                .zip(actual.as_chunks::<4>().0.iter())
                .enumerate()
            {
                let x = (index as u32 % viewport.physical_size.0) as f32 / scale;
                let y = (index as u32 / viewport.physical_size.0) as f32 / scale;
                let edge = 1.0 / scale;
                if x < panel.min.x - edge
                    || x > panel.max.x + edge
                    || y < panel.min.y - edge
                    || y > panel.max.y + edge
                {
                    assert_eq!(
                        before, after,
                        "notification must not filter outside its bounds"
                    );
                } else if before != after {
                    changed_inside += 1;
                }
            }
            assert!(
                changed_inside > (1000.0 * scale * scale) as usize,
                "the actual notification must visibly filter/fill the sampled background"
            );
            let pixel = |data: &[u8], x: f32, y: f32| {
                let offset = (((y * scale) as u32 * viewport.physical_size.0 + (x * scale) as u32)
                    * 4) as usize;
                data[offset..offset + 4].to_vec()
            };
            assert_eq!(
                pixel(&actual, panel.min.x, panel.min.y),
                pixel(&background, panel.min.x, panel.min.y),
                "rounded corner must expose the original background"
            );
            assert_ne!(
                pixel(&actual, panel.min.x + 20.0, panel.min.y + 1.0),
                pixel(&background, panel.min.x + 20.0, panel.min.y + 1.0),
                "the geometric rim must remain visible"
            );
            assert_eq!(
                actual,
                render(&device, &queue, &mut runtime, viewport.clone(), format),
                "unchanged retained notification must reproduce the same pixels"
            );
        }
    }
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn quick_save_review_pixels_match_palette_glass_without_dimming_the_window() {
    let (device, queue) = crate::test_gpu::device();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let (_, controller, mut runtime, _) = fixture();
            let viewport = Viewport::new((1000.0 * scale) as u32, (600.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            runtime.init_text_renderer(&device, &queue, format).unwrap();
            runtime.set_root(View::leaf(TestBackdrop));
            runtime.update(Instant::now());
            let baseline = render(&device, &queue, &mut runtime, viewport.clone(), format);

            let focus = controller.open_quick_save(crate::QuickSaveDraft {
                source: TabId(1),
                launch: harbor_config::ShellSettings::default(),
                project: Some(harbor_config::ProjectRecipe {
                    environment: harbor_config::ProjectEnvironment::LocalPwsh,
                    directory: "C:/synthetic-project".into(),
                    agent: None,
                }),
            });
            let root = controller.clone();
            runtime.set_root(move |cx: &mut BuildCx| {
                let state = root.store.watch(cx).clone();
                let panel = state.popups.active().unwrap().build(
                    cx,
                    &root,
                    &validate_keybindings(&harbor_config::RawKeybindings::default()).unwrap(),
                    state.popups.generation().unwrap(),
                );
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(panel)
                    .build(cx)
            });
            runtime.request_focus(&focus);
            runtime.update(Instant::now());
            let delta = runtime.pending_delta().unwrap();
            let check = delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match item.primitive {
                    Primitive::Icon {
                        icon: LucideIcon::Check,
                        rect,
                        ..
                    } => Some(rect),
                    _ => None,
                })
                .expect("naming advance checkmark");
            let naming = render(&device, &queue, &mut runtime, viewport.clone(), format);
            capture_quick_save("quick-save-naming", scale, format, &viewport, &naming);
            row_pointer(
                &mut runtime,
                Point::new(
                    (check.min.x + check.max.x) / 2.0,
                    (check.min.y + check.max.y) / 2.0,
                ),
                PointerPhase::Move,
            );
            runtime.update(Instant::now());
            let hovered = render(&device, &queue, &mut runtime, viewport.clone(), format);
            assert_ne!(
                naming, hovered,
                "naming checkmark must reveal search-like hover feedback"
            );
            capture_quick_save(
                "quick-save-naming-hover",
                scale,
                format,
                &viewport,
                &hovered,
            );
            row_pointer(&mut runtime, Point::ZERO, PointerPhase::Move);
            runtime.update(Instant::now());
            replace(&mut runtime, "Palette-like glass");
            tap(&mut runtime, Key::Enter);
            let request = take_one(&controller);
            assert_eq!(request.command, AppCommand::AdvanceQuickSave);
            runtime.request_focus(&controller.navigate_quick_save(request).unwrap());
            runtime.update(Instant::now());
            let delta = runtime.pending_delta().unwrap();
            let panel = delta
                .added
                .iter()
                .chain(&delta.modified)
                .find_map(|item| match item.primitive {
                    Primitive::BackdropMaterial { rect, .. }
                    | Primitive::BackdropBlur { rect, .. } => Some(rect),
                    _ => None,
                })
                .expect("review glass panel");
            let (title_origin, title_scale) = delta
                .added
                .iter()
                .chain(&delta.modified)
                .filter_map(|item| item.primitive.text_run())
                .find_map(|(text, origin, _, scale)| {
                    (text == "Save preset").then_some((origin, scale))
                })
                .expect("review heading");
            let title_height = runtime.text_metrics().line_height * title_scale;
            let review = render(&device, &queue, &mut runtime, viewport.clone(), format);

            capture_quick_save("quick-save", scale, format, &viewport, &review);

            // The reference has the same bounds and palette recipe, but no review content.
            runtime.set_root(move |cx: &mut BuildCx| {
                let content = ConstrainedBox::new()
                    .min_width(panel.size().width)
                    .max_width(panel.size().width)
                    .min_height(panel.size().height)
                    .max_height(panel.size().height)
                    .build(cx);
                let glass = GlassAppearance::command_palette().panel(cx, content);
                let aligned = Align::new(harbor_widget::Alignment2D::new(
                    harbor_widget::PositionAlignment::Center,
                    harbor_widget::PositionAlignment::Start,
                ))
                .insets(harbor_widget::Insets::all(12.0))
                .unwrap()
                .child(glass)
                .build(cx);
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(aligned)
                    .build(cx)
            });
            runtime.update(Instant::now());
            let reference = render(&device, &queue, &mut runtime, viewport.clone(), format);
            let pixel = |pixels: &[u8], x: f32, y: f32| {
                let offset = (((y * scale) as u32 * viewport.physical_size.0 + (x * scale) as u32)
                    * 4) as usize;
                pixels[offset..offset + 4].to_vec()
            };
            assert!(
                (title_origin.y as u32..(title_origin.y + title_height) as u32).any(|y| {
                    (title_origin.x as u32..(title_origin.x + 100.0) as u32).any(|x| {
                        pixel(&review, x as f32, y as f32) != pixel(&reference, x as f32, y as f32)
                    })
                }),
                "real UI glyphs must be rendered, not only empty glass surfaces"
            );
            let middle_y = (panel.min.y + panel.max.y) / 2.0;
            assert_eq!(
                pixel(&review, panel.min.x - 8.0, middle_y),
                pixel(&baseline, panel.min.x - 8.0, middle_y),
                "review must leave the window outside the glass undimmed"
            );
            for (x, y) in [
                (panel.max.x - 8.0, middle_y),
                ((panel.min.x + panel.max.x) / 2.0, panel.min.y + 6.0),
            ] {
                let actual = pixel(&review, x, y);
                let expected = pixel(&reference, x, y);
                assert!(
                    actual
                        .iter()
                        .zip(&expected)
                        .all(|(a, b)| a.abs_diff(*b) <= 1),
                    "review glass must match palette transparency at {scale}, {format:?}, ({x}, {y}): {actual:?} vs {expected:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "GPU pixel acceptance; run explicitly with --ignored --nocapture"]
fn segmented_selection_and_hover_pixels_are_local_glass_with_no_separate_press() {
    use crate::appearance::design_system::{self as design, ControlRole};
    use harbor_widget::{SegmentedControl, Theme, ThemeProvider, signal::Signal};
    let (device, queue) = crate::test_gpu::device();
    for scale in [1.0, 1.5, 2.0] {
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let mut runtime = Runtime::new();
            let viewport = Viewport::new((600.0 * scale) as u32, (160.0 * scale) as u32, scale);
            runtime.set_viewport(viewport.clone());
            runtime.init_renderer(&device, format);
            runtime.init_text_renderer(&device, &queue, format).unwrap();
            let mut theme = design::theme(&Theme::default());
            let injected = Signal::new(theme.clone());
            let control = ThemeProvider::from_signal(injected.clone()).child(|cx: &mut BuildCx| {
                SegmentedControl::new(
                    [
                        ("local".into(), "Local pwsh".into()),
                        ("wsl".into(), "WSL bash".into()),
                        ("ssh".into(), "Direct SSH bash".into()),
                    ],
                    "local".into(),
                )
                .gap(design::LABEL_GAP)
                .unwrap()
                .style(design::control(cx, ControlRole::Selection))
                .unwrap()
                .build(cx)
            });
            runtime.set_root(
                Stack::new()
                    .push_child(View::leaf(TestBackdrop))
                    .push_child(
                        Align::new(harbor_widget::Alignment2D::CENTER).child(
                            ConstrainedBox::new()
                                .min_width(480.0)
                                .max_width(480.0)
                                .child(control),
                        ),
                    ),
            );
            runtime.update(Instant::now());
            let delta = runtime.pending_delta().unwrap();
            let surfaces: Vec<_> = delta
                .added
                .iter()
                .chain(&delta.modified)
                .filter_map(|item| match item.primitive {
                    Primitive::BackdropMaterial { rect, .. } => Some(rect),
                    _ => None,
                })
                .collect();
            assert_eq!(surfaces.len(), 1);
            let selection = surfaces[0];
            let glass = render(&device, &queue, &mut runtime, viewport.clone(), format);
            assert_eq!(
                glass,
                render(&device, &queue, &mut runtime, viewport.clone(), format),
                "static selected glass must not animate"
            );
            let wsl = fiber_ids(&runtime)
                .into_iter()
                .find_map(|id| {
                    let fiber = runtime.arena().get(id).unwrap();
                    fiber
                        .key()
                        .is_some_and(|key| key.text() == "wsl")
                        .then(|| fiber.layout_rect().unwrap())
                })
                .unwrap();
            assert!(selection.max.x < wsl.min.x);
            let center = Point::new((wsl.min.x + wsl.max.x) / 2.0, (wsl.min.y + wsl.max.y) / 2.0);
            row_pointer(&mut runtime, center, PointerPhase::Move);
            let hovered = render(&device, &queue, &mut runtime, viewport.clone(), format);
            capture_quick_save(
                "quick-save-segments-hover",
                scale,
                format,
                &viewport,
                &hovered,
            );
            let width = viewport.physical_size.0 as usize;
            let mut hover_changed = false;
            let mut rim_changed = false;
            for (index, (actual, idle)) in hovered
                .as_chunks::<4>()
                .0
                .iter()
                .zip(glass.as_chunks::<4>().0)
                .enumerate()
            {
                let point = Point::new(
                    (index % width) as f32 / scale,
                    (index / width) as f32 / scale,
                );
                if wsl.contains(point) {
                    if actual != idle {
                        hover_changed = true;
                        rim_changed |= point.y < wsl.min.y + 4.0 || point.y > wsl.max.y - 4.0;
                    }
                } else {
                    assert_eq!(
                        actual, idle,
                        "hover must not alter selection, gaps or other segments"
                    );
                }
            }
            assert!(
                hover_changed && rim_changed,
                "hover glass and its optical edge must be visible"
            );
            row_pointer(&mut runtime, center, PointerPhase::Down);
            let pressed = render(&device, &queue, &mut runtime, viewport.clone(), format);
            assert!(
                hovered == pressed,
                "press must not add a different visual treatment"
            );
            row_pointer(&mut runtime, center, PointerPhase::Up);
            row_pointer(&mut runtime, Point::ZERO, PointerPhase::Move);
            let restored = render(&device, &queue, &mut runtime, viewport.clone(), format);
            assert!(
                restored == glass,
                "leaving hover restores the original pixels"
            );
            // Keep exactly the same tint, labels, focus and geometry; remove only the optics.
            theme.button.selected_backdrop = None;
            injected.set(theme);
            runtime.update(Instant::now());
            let flat = render(&device, &queue, &mut runtime, viewport.clone(), format);
            let width = viewport.physical_size.0 as usize;
            let mut changed_inside = false;
            let mut changed_edge = false;
            for (index, (actual, reference)) in glass
                .as_chunks::<4>()
                .0
                .iter()
                .zip(flat.as_chunks::<4>().0)
                .enumerate()
            {
                let x = (index % width) as f32 / scale;
                let y = (index / width) as f32 / scale;
                if selection.contains(Point::new(x, y)) {
                    if actual != reference {
                        changed_inside = true;
                        changed_edge |= y < selection.min.y + 4.0 || y > selection.max.y - 4.0;
                    }
                } else {
                    assert_eq!(
                        actual, reference,
                        "material must stay inside the selected block"
                    );
                }
            }
            assert!(
                changed_inside,
                "alpha-only paint cannot satisfy material acceptance"
            );
            assert!(
                changed_edge,
                "the selected block needs an actual optical rim"
            );
        }
    }
}

fn capture_quick_save(
    stage: &str,
    scale: f32,
    format: wgpu::TextureFormat,
    viewport: &Viewport,
    pixels: &[u8],
) {
    // Opt-in, synthetic visual evidence; no captured terminal/user content.
    let Some(directory) = std::env::var_os("HARBOR_QUICK_SAVE_CAPTURE_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let (width, height) = viewport.physical_size;
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    for bgra in pixels.as_chunks::<4>().0 {
        ppm.extend_from_slice(&[bgra[2], bgra[1], bgra[0]]);
    }
    std::fs::write(
        directory.join(format!("{stage}-{scale}-{format:?}.ppm")),
        ppm,
    )
    .unwrap();
}
