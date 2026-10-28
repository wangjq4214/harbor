use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn generic_glass_composes_gradient_fill_stroke_live_blur_and_sharp_foreground() {
    let (device, queue) = gpu_device();
    let paint = test_gradient(
        Point::ZERO,
        Point::new(0.0, 1.0),
        &[
            (
                0.0,
                Color {
                    a: 0.12,
                    ..Color::WHITE
                },
            ),
            (
                1.0,
                Color {
                    a: 0.04,
                    ..Color::WHITE
                },
            ),
        ],
    );
    let radius = BorderRadius::all(6.0).unwrap();
    let decoration = BoxDecoration::new()
        .try_paint(paint)
        .unwrap()
        .border(Border::with_paint(paint, 1.0).unwrap())
        .border_radius(radius);
    let glass = || {
        Padding::all(4.0).unwrap().child(
            harbor_widget::BackdropFilter::new(2.0)
                .unwrap()
                .border_radius(radius)
                .child(
                    DecoratedBox::new(decoration.clone())
                        .clip_behavior(ClipBehavior::AntiAlias)
                        .child(SizedBox::new(Size::new(24.0, 24.0)).unwrap()),
                ),
        )
    };
    let root = |background| {
        Stack::new()
            .push_child(
                SizedBox::new(Size::new(32.0, 32.0))
                    .unwrap()
                    .background(Color::GREEN)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 16.0,
                })
                .unwrap()
                .child(
                    SizedBox::new(Size::new(16.0, 32.0))
                        .unwrap()
                        .background(background)
                        .expect("valid color"),
                ),
            )
            .push_child(glass())
            .push_child(
                Padding::all(13.0).unwrap().child(
                    SizedBox::new(Size::new(6.0, 6.0))
                        .unwrap()
                        .background(Color::RED)
                        .expect("valid color"),
                ),
            )
    };
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let viewport = Viewport::new((32.0 * scale) as u32, (32.0 * scale) as u32, scale);
        runtime.set_viewport(viewport.clone());
        runtime.set_root(root(Color::BLUE));
        runtime.update(Instant::now());
        let first = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        let at =
            |pixels: &[u8], x: f32, y: f32| bgra(pixels, (x * scale) as u32, (y * scale) as u32);
        let mixed = at(&first, 15.0, 10.0);
        assert!(
            mixed[0] > 20 && mixed[1] > 80,
            "live background colors survive glass: {mixed:?}"
        );
        assert_eq!(
            at(&first, 16.0, 16.0),
            [0, 0, 255, 255],
            "foreground remains sharp"
        );
        assert_eq!(
            at(&first, 4.0, 4.0),
            [0, 255, 0, 255],
            "round corner unchanged"
        );
        let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        assert_eq!(first, retained, "unchanged glass does not accumulate alpha");
        runtime.set_root(root(Color::RED));
        runtime.update(Instant::now());
        let changed = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let updated = at(&changed, 15.0, 10.0);
        assert!(
            updated[0] < mixed[0] && updated[2] > mixed[2],
            "new background appears through glass"
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_refraction_moves_edges_and_corners_but_not_center_outside_or_foreground() {
    use harbor_widget::{BackdropFilter, EdgeRefraction, GradientStop, LinearGradient};
    let (device, queue) = gpu_device();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let side = (32.0 * scale) as u32;
        let viewport = Viewport::new(side, side, scale);
        runtime.set_viewport(viewport.clone());
        for sigma in [0.0, 2.0, 8.0] {
            for alpha in [0.5, 1.0] {
                let root = |displacement, flipped| {
                    let start = Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: alpha,
                    };
                    let end = Color {
                        a: alpha,
                        ..Color::WHITE
                    };
                    let (start, end) = if flipped { (end, start) } else { (start, end) };
                    let ramp = LinearGradient::new(
                        Point::ZERO,
                        Point::new(1.0, 1.0),
                        &[GradientStop::new(0.0, start), GradientStop::new(1.0, end)],
                    )
                    .unwrap();
                    Stack::new()
                        .push_child(
                            DecoratedBox::new(BoxDecoration::new().try_paint(ramp.into()).unwrap())
                                .child(SizedBox::new(Size::new(32.0, 32.0)).unwrap()),
                        )
                        .push_child(
                            Padding::all(4.0).unwrap().child(
                                BackdropFilter::new(sigma)
                                    .unwrap()
                                    .edge_refraction(
                                        EdgeRefraction::new(5.0, displacement).unwrap(),
                                    )
                                    .border_radius(BorderRadius::all(6.0).unwrap())
                                    .child(SizedBox::new(Size::new(24.0, 24.0)).unwrap()),
                            ),
                        )
                        // A sharp foreground marker lies directly over the edge band.
                        .push_child(
                            Padding::new(harbor_widget::Insets {
                                top: 4.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: 14.0,
                            })
                            .unwrap()
                            .child(
                                SizedBox::new(Size::new(4.0, 2.0))
                                    .unwrap()
                                    .background(Color::RED)
                                    .expect("valid color"),
                            ),
                        )
                };
                runtime.set_root(root(0.0, false));
                runtime.update(Instant::now());
                let baseline = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                runtime.set_root(root(3.0, false));
                runtime.update(Instant::now());
                let refracted =
                    encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                let at = |pixels: &[u8], x: f32, y: f32| {
                    bgra(pixels, (x * scale) as u32, (y * scale) as u32)
                };
                for (x, y, direction) in [
                    (5.0, 16.0, 1),
                    (26.0, 16.0, -1),
                    (12.0, 5.0, 1),
                    (16.0, 26.0, -1),
                    (7.0, 7.0, 1),
                ] {
                    let before = at(&baseline, x, y);
                    let after = at(&refracted, x, y);
                    assert!(
                        (i32::from(after[0]) - i32::from(before[0])) * direction > 1,
                        "edge/corner displacement at ({x},{y}), scale={scale}, sigma={sigma}, alpha={alpha}: {before:?} -> {after:?}"
                    );
                    // Resampling can move the backdrop's antialiased outer alpha
                    // ramp; it must not add opacity or break premultiplication.
                    assert!(f32::from(after[3]) <= (alpha * 255.0).round() + 1.0);
                    assert!(after[..3].iter().all(|channel| *channel <= after[3]));
                }
                for (x, y) in [(16.0, 16.0), (2.0, 16.0), (4.0, 4.0), (15.0, 5.0)] {
                    let before = at(&baseline, x, y);
                    let after = at(&refracted, x, y);
                    assert!(
                        before
                            .iter()
                            .zip(after)
                            .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= 1),
                        "center/outside/corner-cutout/foreground unchanged ({x},{y}): {before:?} -> {after:?}"
                    );
                }
                let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                assert_eq!(
                    refracted, retained,
                    "retained frames do not compound distortion"
                );
                runtime.set_root(root(3.0, true));
                runtime.update(Instant::now());
                let live = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
                assert_ne!(
                    at(&live, 7.0, 7.0),
                    at(&refracted, 7.0, 7.0),
                    "live background updates"
                );
                assert_eq!(
                    at(&live, 15.0, 5.0),
                    at(&refracted, 15.0, 5.0),
                    "foreground remains sharp"
                );
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_material_tone_is_applied_in_srgb_for_both_attachment_formats() {
    use harbor_widget::{BackdropFilter, BackdropMaterial};
    let (device, queue) = gpu_device();
    let srgb = |v: f32| {
        if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    for format in [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, format);
        let viewport = Viewport::new(32, 32, 1.0);
        runtime.set_viewport(viewport.clone());
        let gray = Color {
            r: 0.25,
            g: 0.25,
            b: 0.25,
            a: 1.0,
        };
        let material = BackdropMaterial::new(
            0.0,
            0.0,
            BackdropTone {
                offset: 0.1,
                slope: 0.5,
                quadratic: 0.0,
                saturation: 1.0,
            },
            BackdropRim {
                highlight: 0.0,
                shade: 0.0,
            },
        )
        .unwrap();
        runtime.set_root(material_test_scene(
            gray,
            gray,
            BackdropFilter::new(0.0).unwrap().material(Some(material)),
        ));
        runtime.update(Instant::now());
        let pixels = encode_filter_frame_format(&device, &queue, &mut runtime, viewport, format);
        let input = if format.is_srgb() { srgb(0.25) } else { 0.25 };
        let expected = ((0.1 + 0.5 * input) * 255.0).round() as i32;
        assert!(
            (i32::from(bgra(&pixels, 16, 16)[0]) - expected).abs() <= 2,
            "{format:?}"
        );
        assert_eq!(bgra(&pixels, 16, 16)[3], 255);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_large_sigma_softens_a_thin_line_without_a_sparse_sampling_comb() {
    let (device, queue) = gpu_device();
    for scale in [1.0, 1.5, 2.0] {
        let side = (32.0 * scale) as u32;
        let viewport = Viewport::new(side, side, scale);
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        let stripe = || {
            Padding::new(harbor_widget::Insets {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 16.0,
            })
            .unwrap()
            .child(
                SizedBox::new(Size::new(1.0, 32.0))
                    .unwrap()
                    .background(Color::WHITE)
                    .expect("valid color"),
            )
        };
        runtime.set_viewport(viewport.clone());
        runtime.set_root(Stack::new().push_child(stripe()));
        runtime.update(Instant::now());
        let source_pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
        // Account for the source shape's DPI-dependent edge coverage rather
        // than assuming a 1dp shape covers an integer number of physical pixels.
        let source_energy: u32 = (0..side)
            .map(|x| u32::from(bgra(&source_pixels, x, side / 2)[3]))
            .sum();
        let expected_peak =
            source_energy as f32 / ((2.0 * std::f32::consts::PI).sqrt() * 8.0 * scale);
        runtime.set_root(
            Stack::new().push_child(stripe()).push_child(
                harbor_widget::BackdropFilter::new(8.0)
                    .unwrap()
                    .child(SizedBox::new(Size::new(32.0, 32.0)).unwrap()),
            ),
        );
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let center = (16.5 * scale) as u32;
        let profile: Vec<_> = (0..side).map(|x| bgra(&pixels, x, side / 2)[3]).collect();
        let peak = *profile.iter().max().unwrap();
        assert!(
            (f32::from(peak) - expected_peak).abs() <= 2.0,
            "sigma=8 should spread a thin line, not repeat sparse taps at {scale}x: expected peak={expected_peak}, {profile:?}"
        );
        for x in 2..center {
            assert!(
                profile[x as usize] <= profile[x as usize + 1] + 1,
                "left Gaussian profile must be smooth: {profile:?}"
            );
        }
        for x in center + 1..side - 2 {
            assert!(
                profile[x as usize + 1] <= profile[x as usize] + 1,
                "right Gaussian profile must be smooth: {profile:?}"
            );
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_blurs_external_pixels_once_per_frame_and_keeps_foreground_sharp() {
    let (device, queue) = gpu_device();
    let material = harbor_widget::BackdropMaterial::new(
        5.0,
        0.25,
        BackdropTone {
            offset: 0.0,
            slope: 1.0,
            quadratic: 0.0,
            saturation: 1.0,
        },
        BackdropRim {
            highlight: 0.0,
            shade: 0.0,
        },
    )
    .unwrap();
    for (displacement, material) in [(0.0, None), (1.0, None), (1.0, Some(material))] {
        let green = create_solid_pipeline_rgb(&device, 0.0, 1.0, 0.0);
        let blue = create_solid_pipeline_rgb(&device, 0.0, 0.0, 1.0);
        let red = create_solid_pipeline_rgb(&device, 1.0, 0.0, 0.0);
        for scale in [1.0, 1.5, 2.0] {
            let calls = Arc::new(AtomicU64::new(0));
            let changed = Arc::new(AtomicBool::new(false));
            let observed = calls.clone();
            let flip = changed.clone();
            let (green, blue, red) = (green.clone(), blue.clone(), red.clone());
            let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, _, _, pass, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                let size = (32.0 * scale) as u32;
                pass.set_scissor_rect(0, 0, size / 2, size);
                pass.set_pipeline(&green);
                pass.draw(0..3, 0..1);
                pass.set_scissor_rect(size / 2, 0, size / 2, size);
                pass.set_pipeline(if flip.load(Ordering::SeqCst) {
                    &red
                } else {
                    &blue
                });
                pass.draw(0..3, 0..1);
            });
            let side = (32.0 * scale) as u32;
            let viewport = Viewport::new(side, side, scale);
            let mut runtime = Runtime::new();
            runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
            runtime.set_root(
                Stack::new()
                    .push_child(ExternalSurface::new(1).on_draw(handler))
                    .push_child(
                        Padding::all(4.0).unwrap().child(
                            harbor_widget::BackdropFilter::new(2.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                                )
                                .material(material)
                                .border_radius(BorderRadius::all(6.0).unwrap())
                                .child(SizedBox::new(Size::new(24.0, 24.0)).unwrap()),
                        ),
                    )
                    .push_child(
                        Padding::all(13.0).unwrap().child(
                            SizedBox::new(Size::new(6.0, 6.0))
                                .unwrap()
                                .background(Color::RED)
                                .expect("valid color"),
                        ),
                    ),
            );
            runtime.set_viewport(viewport.clone());
            runtime.update(Instant::now());
            let first = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
            let at = |pixels: &[u8], x: f32, y: f32| {
                bgra(pixels, (x * scale) as u32, (y * scale) as u32)
            };
            let mixed = at(&first, 15.0, 10.0);
            assert!(
                mixed[0] > 20 && mixed[1] > 80,
                "real background blur at {scale}x: {mixed:?}"
            );
            assert_eq!(
                at(&first, 2.0, 10.0),
                [0, 255, 0, 255],
                "outside filter unchanged"
            );
            assert_eq!(
                at(&first, 4.0, 4.0),
                [0, 255, 0, 255],
                "rounded corner unchanged"
            );
            assert_eq!(
                at(&first, 16.0, 16.0),
                [0, 0, 255, 255],
                "foreground stays sharp"
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "no replay for background capture"
            );
            assert!(runtime.pending_delta().is_none());
            let retained = encode_filter_frame(&device, &queue, &mut runtime, viewport.clone());
            assert_eq!(
                first, retained,
                "unchanged scene survives retained encoding"
            );
            changed.store(true, Ordering::SeqCst);
            let live = encode_filter_frame(&device, &queue, &mut runtime, viewport);
            let updated = at(&live, 15.0, 10.0);
            assert_eq!(updated[0], 0);
            assert!(updated[2] > 20, "live external output appears through blur");
            assert_eq!(calls.load(Ordering::SeqCst), 3);
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_generic_composition_preserves_alpha_clips_and_resize() {
    let (device, queue) = gpu_device();
    let material = harbor_widget::BackdropMaterial::new(
        5.0,
        0.25,
        BackdropTone {
            offset: 0.0,
            slope: 1.0,
            quadratic: 0.0,
            saturation: 1.0,
        },
        BackdropRim {
            highlight: 0.0,
            shade: 0.0,
        },
    )
    .unwrap();
    for (displacement, material) in [(0.0, None), (1.0, None), (1.0, Some(material))] {
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        let translucent = Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 0.5,
        };
        runtime.set_root(
            Stack::new()
                .push_child(
                    SizedBox::new(Size::new(32.0, 32.0))
                        .unwrap()
                        .background(translucent)
                        .expect("valid color"),
                )
                .push_child(
                    Padding::all(4.0).unwrap().child(
                        DecoratedBox::new(
                            BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()),
                        )
                        .clip_behavior(ClipBehavior::AntiAlias)
                        .child(
                            harbor_widget::BackdropFilter::new(3.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                                )
                                .material(material)
                                .child(SizedBox::new(Size::new(24.0, 24.0)).unwrap()),
                        ),
                    ),
                ),
        );
        for (side, scale) in [(32, 1.0), (64, 2.0), (48, 1.5), (32, 1.0)] {
            let viewport = Viewport::new(side, side, scale);
            runtime.set_viewport(viewport.clone());
            runtime.update(Instant::now());
            let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
            for (x, y) in [(16.0, 16.0), (4.0, 4.0), (2.0, 16.0)] {
                let pixel = bgra(&pixels, (x * scale) as u32, (y * scale) as u32);
                assert!(
                    (i32::from(pixel[3]) - 128).abs() <= 1,
                    "alpha not blended twice: {pixel:?}"
                );
                assert_eq!(pixel[0], 0);
                assert_eq!(pixel[2], 0);
            }
        }
        // Removing the filter returns to the direct target path, without stale blur.
        runtime.set_root(
            SizedBox::new(Size::new(32.0, 32.0))
                .unwrap()
                .background(Color::BLUE)
                .expect("valid color"),
        );
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, Viewport::new(32, 32, 1.0));
        assert_eq!(bgra(&pixels, 16, 16), [255, 0, 0, 255]);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_multiple_regions_use_distinct_uniforms_and_respect_ancestor_clip() {
    let (device, queue) = gpu_device();
    for displacement in [0.0, 1.0] {
        let mut runtime = Runtime::new();
        runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
        runtime.set_root(
            Stack::new()
                .push_child(
                    SizedBox::new(Size::new(32.0, 32.0))
                        .unwrap()
                        .background(Color::GREEN)
                        .expect("valid color"),
                )
                .push_child(
                    Padding::new(harbor_widget::Insets {
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 16.0,
                    })
                    .unwrap()
                    .child(
                        SizedBox::new(Size::new(16.0, 32.0))
                            .unwrap()
                            .background(Color::BLUE)
                            .expect("valid color"),
                    ),
                )
                .push_child(
                    Padding::new(harbor_widget::Insets {
                        top: 4.0,
                        right: 18.0,
                        bottom: 4.0,
                        left: 4.0,
                    })
                    .unwrap()
                    .child(
                        harbor_widget::BackdropFilter::new(3.0)
                            .unwrap()
                            .edge_refraction(
                                harbor_widget::EdgeRefraction::new(4.0, displacement).unwrap(),
                            )
                            .child(SizedBox::new(Size::new(10.0, 24.0)).unwrap()),
                    ),
                )
                .push_child(
                    Padding::new(harbor_widget::Insets {
                        top: 4.0,
                        right: 4.0,
                        bottom: 4.0,
                        left: 18.0,
                    })
                    .unwrap()
                    .child(
                        DecoratedBox::new(
                            BoxDecoration::new().border_radius(BorderRadius::all(4.0).unwrap()),
                        )
                        .clip_behavior(ClipBehavior::HardEdge)
                        .child(
                            harbor_widget::BackdropFilter::new(2.0)
                                .unwrap()
                                .edge_refraction(
                                    harbor_widget::EdgeRefraction::new(2.0, displacement * 0.5)
                                        .unwrap(),
                                )
                                .child(SizedBox::new(Size::new(10.0, 24.0)).unwrap()),
                        ),
                    ),
                ),
        );
        let viewport = Viewport::new(32, 32, 1.0);
        runtime.set_viewport(viewport.clone());
        runtime.update(Instant::now());
        let pixels = encode_filter_frame(&device, &queue, &mut runtime, viewport);
        let left = bgra(&pixels, 13, 16);
        let right = bgra(&pixels, 18, 16);
        assert!(
            left[0] > 10 && left[1] > 100,
            "first filter not replaced by second params: {left:?}"
        );
        assert!(
            right[0] > 100 && right[1] > 5,
            "second filter samples independently: {right:?}"
        );
        assert_eq!(bgra(&pixels, 15, 16), [0, 255, 0, 255], "gap remains sharp");
        assert_eq!(bgra(&pixels, 17, 16), [255, 0, 0, 255], "gap remains sharp");
        assert_eq!(
            bgra(&pixels, 18, 4),
            [255, 0, 0, 255],
            "rounded ancestor is authoritative"
        );
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_material_nonlinear_profile_preserves_center_foreground_and_live_updates() {
    use harbor_widget::{BackdropFilter, BackdropMaterial, EdgeRefraction};
    let (device, queue) = gpu_device();
    let material = BackdropMaterial::new(
        0.0,
        0.0,
        BackdropTone {
            offset: 0.0,
            slope: 1.0,
            quadratic: 0.0,
            saturation: 1.0,
        },
        BackdropRim {
            highlight: 0.0,
            shade: 0.0,
        },
    )
    .unwrap();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let viewport = Viewport::new((32.0 * scale) as u32, (32.0 * scale) as u32, scale);
        runtime.set_viewport(viewport.clone());
        for alpha in [0.5, 1.0] {
            let start = Color {
                a: alpha,
                ..Color::BLACK
            };
            let end = Color {
                a: alpha,
                ..Color::WHITE
            };
            let filter = |strength, material| {
                BackdropFilter::new(0.0)
                    .unwrap()
                    .edge_refraction(EdgeRefraction::new(5.0, strength).unwrap())
                    .material(material)
            };
            let mut render = |f, a, b| {
                runtime.set_root(material_test_scene(a, b, f));
                runtime.update(Instant::now());
                encode_filter_frame(&device, &queue, &mut runtime, viewport.clone())
            };
            let base = render(filter(0.0, None), start, end);
            let old = render(filter(3.0, None), start, end);
            let new = render(filter(3.0, Some(material)), start, end);
            let at =
                |data: &[u8], x: f32, y: f32| bgra(data, (x * scale) as u32, (y * scale) as u32);
            let b = i32::from(at(&base, 5.0, 16.0)[0]);
            let o = i32::from(at(&old, 5.0, 16.0)[0]);
            let n = i32::from(at(&new, 5.0, 16.0)[0]);
            assert!(
                b < n && n < o,
                "nonlinear lens differs from old smoothstep: {b} < {n} < {o}"
            );
            for (x, y) in [(16.0, 16.0), (2.0, 16.0), (4.0, 4.0), (15.0, 5.0)] {
                assert_eq!(
                    at(&base, x, y),
                    at(&new, x, y),
                    "center/outside/cutout/foreground"
                );
            }
            assert_eq!(at(&new, 5.0, 16.0)[3], at(&base, 5.0, 16.0)[3]);
            let retained = render(filter(3.0, Some(material)), start, end);
            assert_eq!(
                new, retained,
                "material does not compound across retained frames"
            );
            let live = render(filter(3.0, Some(material)), end, start);
            assert_ne!(at(&live, 5.0, 16.0), at(&new, 5.0, 16.0));
            assert_eq!(at(&live, 15.0, 5.0), at(&new, 15.0, 5.0));
            let disabled = render(filter(3.0, None), start, end);
            assert_eq!(old, disabled, "removing material restores legacy output");
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_material_rim_responds_to_normals_and_tone_preserves_transparent_alpha() {
    use harbor_widget::{BackdropFilter, BackdropMaterial};
    let (device, queue) = gpu_device();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    for scale in [1.0, 1.5, 2.0] {
        let viewport = Viewport::new((32.0 * scale) as u32, (32.0 * scale) as u32, scale);
        runtime.set_viewport(viewport.clone());
        for alpha in [0.0, 0.5, 1.0] {
            let gray = Color {
                r: 0.4,
                g: 0.4,
                b: 0.4,
                a: alpha,
            };
            let mut render = |tone, rim| {
                let m = BackdropMaterial::new(0.0, 0.0, tone, rim).unwrap();
                runtime.set_root(material_test_scene(
                    gray,
                    gray,
                    BackdropFilter::new(0.0).unwrap().material(Some(m)),
                ));
                runtime.update(Instant::now());
                encode_filter_frame(&device, &queue, &mut runtime, viewport.clone())
            };
            let identity = BackdropTone {
                offset: 0.0,
                slope: 1.0,
                quadratic: 0.0,
                saturation: 1.0,
            };
            let base = render(
                identity,
                BackdropRim {
                    highlight: 0.0,
                    shade: 0.0,
                },
            );
            let toned = render(
                BackdropTone {
                    offset: 0.1,
                    slope: 0.8,
                    quadratic: -0.1,
                    saturation: 1.05,
                },
                BackdropRim {
                    highlight: 0.0,
                    shade: 0.0,
                },
            );
            let rim = render(
                identity,
                BackdropRim {
                    highlight: 0.7,
                    shade: 0.6,
                },
            );
            let at =
                |data: &[u8], x: f32, y: f32| bgra(data, (x * scale) as u32, (y * scale) as u32);
            assert_eq!(at(&toned, 16.0, 16.0)[3], at(&base, 16.0, 16.0)[3]);
            let expected = ((0.1 + 0.8 * 0.4 - 0.1 * 0.4 * 0.4) * alpha * 255.0_f32).round() as i32;
            assert!((i32::from(at(&toned, 16.0, 16.0)[0]) - expected).abs() <= 2);
            assert_eq!(at(&rim, 16.0, 16.0), at(&base, 16.0, 16.0));
            assert_eq!(at(&rim, 2.0, 16.0), at(&base, 2.0, 16.0));
            assert_eq!(
                at(&rim, 15.0, 5.0),
                [0, 0, 255, 255],
                "foreground stays sharp over rim"
            );
            if alpha > 0.0 {
                assert!(
                    at(&rim, 12.0, 4.0)[0] > at(&base, 12.0, 4.0)[0],
                    "top highlight"
                );
                assert!(
                    at(&rim, 4.0, 16.0)[0] < at(&base, 4.0, 16.0)[0],
                    "side shade scale={scale} alpha={alpha}: {:?} -> {:?}",
                    at(&base, 4.0, 16.0),
                    at(&rim, 4.0, 16.0)
                );
            }
            for pixel in rim.as_chunks::<4>().0 {
                assert!(
                    pixel[..3].iter().all(|c| *c <= pixel[3]),
                    "premultiplied rim"
                );
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn backdrop_material_broad_frost_blurs_both_axes_and_releases_when_disabled() {
    use harbor_widget::{BackdropFilter, BackdropMaterial};
    let (device, queue) = gpu_device();
    let mut runtime = Runtime::new();
    runtime.init_renderer(&device, wgpu::TextureFormat::Bgra8Unorm);
    let viewport = Viewport::new(32, 32, 1.0);
    runtime.set_viewport(viewport.clone());
    let mut render = |mix| {
        let material = BackdropMaterial::new(
            5.0,
            mix,
            BackdropTone {
                offset: 0.0,
                slope: 1.0,
                quadratic: 0.0,
                saturation: 1.0,
            },
            BackdropRim {
                highlight: 0.0,
                shade: 0.0,
            },
        )
        .unwrap();
        let root = Stack::new()
            .push_child(
                SizedBox::new(Size::new(32.0, 32.0))
                    .unwrap()
                    .background(Color::BLACK)
                    .expect("valid color"),
            )
            .push_child(
                Padding::new(harbor_widget::Insets {
                    top: 15.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 15.0,
                })
                .unwrap()
                .child(
                    SizedBox::new(Size::new(2.0, 2.0))
                        .unwrap()
                        .background(Color::WHITE)
                        .expect("valid color"),
                ),
            )
            .push_child(
                BackdropFilter::new(0.5)
                    .unwrap()
                    .material(Some(material))
                    .child(SizedBox::new(Size::new(32.0, 32.0)).unwrap()),
            );
        runtime.set_root(root);
        runtime.update(Instant::now());
        encode_filter_frame(&device, &queue, &mut runtime, viewport.clone())
    };
    let base = render(0.0);
    let light = render(1.0);
    let dark = render(-1.0);
    assert!(
        bgra(&light, 20, 16)[0] > bgra(&base, 20, 16)[0],
        "wide horizontal support"
    );
    assert!(
        bgra(&light, 16, 20)[0] > bgra(&base, 16, 20)[0],
        "wide vertical support"
    );
    assert!(
        bgra(&dark, 16, 16)[0] < bgra(&base, 16, 16)[0],
        "dark frost mixes a broader field"
    );
    assert_eq!(
        base,
        render(0.0),
        "turning frost off doesn't retain stale broad pixels"
    );
}
