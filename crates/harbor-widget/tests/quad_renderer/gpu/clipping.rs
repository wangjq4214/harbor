use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_discard_child_pixels_outside_arc_when_clip_is_hard_edge() {
    // Arrange
    let (device, queue) = gpu_device();
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::ZERO, 32.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 1, 1), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
    assert!(
        bgra(&pixels, 16, 0)[2] > 200,
        "straight top edge must remain covered: {:?}",
        bgra(&pixels, 16, 0)
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_apply_fractional_coverage_when_clip_is_anti_alias() {
    // Arrange
    let (device, queue) = gpu_device();
    let item = clipped_child_quad(ClipBehavior::AntiAlias, Point::ZERO, 32.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    let mut saw_fractional = false;
    for y in 0..8 {
        for x in 0..8 {
            let alpha = bgra(&pixels, x, y)[3];
            if alpha > 0 && alpha < 255 {
                saw_fractional = true;
            }
        }
    }
    assert!(
        saw_fractional,
        "anti-aliased clip must produce fractional coverage along the arc"
    );
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_child_quad_when_viewport_scale_is_one_and_a_half() {
    // Arrange
    let (device, queue) = gpu_device();
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::new(0.25, 0.25), 20.0, 8.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(48, 48, 1.5), 48);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert!(
        bgra(&pixels, 16, 16)[2] > 200,
        "interior at 1.5x must keep child color: {:?}",
        bgra(&pixels, 16, 16)
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_child_quad_when_viewport_scale_is_two() {
    // Arrange
    let (device, queue) = gpu_device();
    let item = clipped_child_quad(ClipBehavior::HardEdge, Point::new(0.5, 0.5), 16.0, 6.0);

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(64, 64, 2.0), 64);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert!(
        bgra(&pixels, 16, 16)[2] > 200,
        "interior at 2x must keep child color: {:?}",
        bgra(&pixels, 16, 16)
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_unclipped_wrapper_pixels_in_cutout_when_child_clip_is_active() {
    // Arrange
    let (device, queue) = gpu_device();
    let fill = SceneItem {
        id: 1,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            color: Color::BLUE,
            corner_radius: 0.0,
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    let mut child = make_quad_item(2, 1, 0.0, 0.0, 32.0, 32.0);
    child.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );

    // Act
    let pixels = render_quad_items(
        &device,
        &queue,
        vec![fill, child],
        Viewport::new(32, 32, 1.0),
        32,
        None,
    );

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [255, 0, 0, 255]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_intersect_nested_clips_when_two_rounded_ancestors_are_active() {
    // Arrange
    let (device, queue) = gpu_device();
    let mut item = make_quad_item(1, 0, 0.0, 0.0, 32.0, 32.0);
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
            BorderRadius::all(8.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );
    item.clips.push(
        RoundedClip::new(
            Rect::from_min_size(Point::new(4.0, 4.0), Size::new(24.0, 24.0)),
            BorderRadius::all(4.0).unwrap(),
            ClipBehavior::HardEdge,
        )
        .unwrap(),
    );

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 4, 4), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_not_leak_outside_outer_bound_when_more_than_two_clips_are_packed() {
    // Arrange
    let (device, queue) = gpu_device();
    let mut item = make_quad_item(1, 0, 0.0, 0.0, 32.0, 32.0);
    for (origin, size, radius) in [
        (Point::ZERO, 32.0, 8.0),
        (Point::new(2.0, 2.0), 28.0, 4.0),
        (Point::new(4.0, 4.0), 24.0, 4.0),
    ] {
        item.clips.push(
            RoundedClip::new(
                Rect::from_min_size(origin, Size::new(size, size)),
                BorderRadius::all(radius).unwrap(),
                ClipBehavior::HardEdge,
            )
            .unwrap(),
        );
    }

    // Act
    let pixels = render_quad_item(&device, &queue, item, Viewport::new(32, 32, 1.0), 32);

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 1, 16), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 0, 255, 255]);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn explicit_padding_decoration_order_changes_gpu_pixels_and_clip_bounds() {
    let (device, queue) = gpu_device();
    let decoration = BoxDecoration::new()
        .try_color(Color::RED)
        .unwrap()
        .border_radius(BorderRadius::all(8.0).unwrap());
    let outside = harbor_widget::view! {
        SizedBox::new(Size::new(48.0, 48.0)) => {
            Padding::all(8.0) => {
                DecoratedBox::new(decoration.clone()).clip_behavior(ClipBehavior::HardEdge) => {
                    SizedBox::new(Size::new(32.0, 32.0)).map(|s| s.background(Color::GREEN).expect("valid color"));
                }
            }
        }
    };
    let inside = harbor_widget::view! {
        SizedBox::new(Size::new(48.0, 48.0)) => {
            DecoratedBox::new(decoration).clip_behavior(ClipBehavior::HardEdge) => {
                Padding::all(8.0) => {
                    SizedBox::new(Size::new(32.0, 32.0)).map(|s| s.background(Color::GREEN).expect("valid color"));
                }
            }
        }
    };
    let viewport = Viewport::new(64, 64, 1.0);
    let a = encode_root_to_pixels(
        &device,
        &queue,
        move |_: &mut harbor_widget::view::BuildCx| outside.clone(),
        viewport.clone(),
        64,
    );
    let b = encode_root_to_pixels(
        &device,
        &queue,
        move |_: &mut harbor_widget::view::BuildCx| inside.clone(),
        viewport,
        64,
    );
    assert_eq!(bgra(&a, 4, 24), [0, 0, 0, 0]);
    assert_eq!(bgra(&b, 4, 24), [0, 0, 255, 255]);
    assert_eq!(bgra(&a, 8, 8), [0, 0, 0, 0]);
    assert_eq!(bgra(&b, 8, 8), [0, 255, 0, 255]);
    assert_eq!(bgra(&a, 24, 24), [0, 255, 0, 255]);
    assert_eq!(bgra(&b, 24, 24), [0, 255, 0, 255]);
    eprintln!(
        "0038 explicit wrapper-order GPU readback: padding/background/rounded-clip differences verified"
    );
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_rounded_mask_when_handler_rewrites_scissor_from_context() {
    let (device, queue) = gpu_device();
    let pipeline = create_solid_pipeline(&device);
    let handler: Arc<ExternalDrawFn<'static>> = Arc::new(move |_, context, _, pass, _| {
        let scissor = context.scissor_rect();
        pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        pass.set_pipeline(&pipeline);
        pass.draw(0..3, 0..1);
    });
    let pixels = encode_root_to_pixels(
        &device,
        &queue,
        DecoratedBox::new(BoxDecoration::new().border_radius(BorderRadius::all(8.0).unwrap()))
            .clip_behavior(ClipBehavior::HardEdge)
            .child(ExternalSurface::new(1).on_draw(handler)),
        Viewport::new(32, 32, 1.0),
        32,
    );
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16), [0, 255, 0, 255]);
}
