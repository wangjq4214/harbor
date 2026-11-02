use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_construct_text_renderer_with_supported_formats() {
    let (device, _queue) = gpu_device();
    let atlas_texture = create_atlas_texture(&device);
    let atlas_sampler = create_atlas_sampler(&device);
    let (bind_layout, bind_group) = create_bind_group(&device, &atlas_texture, &atlas_sampler);
    for format in [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        eprintln!("construct TextRenderer with {format:?}");
        let _renderer = TextRenderer::new(&device, format, &bind_layout, &bind_group);
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_leave_glyph_pixels_unchanged_when_clip_policy_is_none() {
    // Arrange
    let (device, queue) = gpu_device();
    let none_clip = RoundedClip::new(
        Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
        BorderRadius::all(8.0).unwrap(),
        ClipBehavior::None,
    )
    .unwrap();

    // Act
    let unclipped = render_text_item(&device, &queue, text_item(Vec::new()));
    let none_policy = render_text_item(&device, &queue, text_item(vec![none_clip]));

    // Assert
    assert_eq!(unclipped, none_policy);
    assert_eq!(bgra(&unclipped, 0, 0)[2], 255);
    assert_eq!(bgra(&unclipped, 16, 16)[2], 255);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_clip_glyph_pixels_outside_rounded_corner_when_hard_edge_is_active() {
    // Arrange
    let (device, queue) = gpu_device();
    let clip = RoundedClip::new(
        Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
        BorderRadius::all(8.0).unwrap(),
        ClipBehavior::HardEdge,
    )
    .unwrap();

    // Act
    let pixels = render_text_item(&device, &queue, text_item(vec![clip]));

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert_eq!(bgra(&pixels, 16, 16)[2], 255);
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn should_keep_straight_edge_glyph_pixels_when_clip_is_rounded_not_aabb() {
    // Arrange
    let (device, queue) = gpu_device();
    let clip = RoundedClip::new(
        Rect::from_min_size(Point::ZERO, Size::new(32.0, 32.0)),
        BorderRadius::all(8.0).unwrap(),
        ClipBehavior::HardEdge,
    )
    .unwrap();

    // Act
    let pixels = render_text_item(&device, &queue, text_item(vec![clip]));

    // Assert
    assert_eq!(bgra(&pixels, 0, 0), [0, 0, 0, 0]);
    assert!(
        bgra(&pixels, 16, 0)[2] > 200,
        "straight top edge must remain; an inscribed scissor would drop it: {:?}",
        bgra(&pixels, 16, 0)
    );
}
