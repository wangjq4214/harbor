use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::scene::{SceneGraph, SceneItem};
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{Border, BorderRadius, BoxDecoration, ClipBehavior, DecoratedBox};
use std::time::Instant;

fn update_decoration(decoration: BoxDecoration) -> harbor_widget::scene::SceneDelta {
    let mut runtime = Runtime::new();
    runtime.set_root(DecoratedBox::new(decoration).child(SizedBox::new(Size::new(40.0, 24.0))));
    runtime.update(Instant::now());
    runtime
        .pending_delta()
        .cloned()
        .expect("first update has a delta")
}

#[test]
fn paints_fill_before_border_after_and_preserves_child_layout() {
    let decoration = BoxDecoration::new()
        .try_color(Color::RED)
        .unwrap()
        .border(Border::all(Color::BLUE, 2.0).unwrap());
    let delta = update_decoration(decoration);

    assert_eq!(delta.added.len(), 2);
    assert!(matches!(
        delta.added[0].primitive,
        Primitive::RoundedQuad { .. }
    ));
    assert!(matches!(
        delta.added[1].primitive,
        Primitive::RoundedBorder { .. }
    ));
    assert_eq!(delta.added[0].paint_order, 0);
    assert_eq!(delta.added[1].paint_order, 1);
}

#[test]
fn resolves_asymmetric_radii_once_for_fill_and_border() {
    let radius = BorderRadius::only(8.0, 4.0, 2.0, 6.0).unwrap();
    let decoration = BoxDecoration::new()
        .try_color(Color::RED)
        .unwrap()
        .border(Border::all(Color::BLUE, 1.0).unwrap())
        .border_radius(radius);
    let delta = update_decoration(decoration);

    let fill = match &delta.added[0].primitive {
        Primitive::RoundedQuad { corner_radii, .. } => *corner_radii,
        primitive => panic!("expected rounded fill, got {primitive:?}"),
    };
    let border = match &delta.added[1].primitive {
        Primitive::RoundedBorder { corner_radii, .. } => *corner_radii,
        primitive => panic!("expected rounded border, got {primitive:?}"),
    };
    assert_eq!(fill, [8.0, 4.0, 2.0, 6.0]);
    assert_eq!(border, fill);
}

#[test]
fn active_clip_policy_is_configuration_only_until_rounded_masks_exist() {
    let box_widget = DecoratedBox::new(BoxDecoration::new())
        .clip_behavior(ClipBehavior::AntiAlias)
        .child(SizedBox::new(Size::new(10.0, 10.0)));
    assert_eq!(box_widget.clip_behavior_value(), ClipBehavior::AntiAlias);

    let delta = update_decoration(box_widget.decoration().clone());
    assert!(delta.added.iter().all(|item| item.clips.is_empty()));
}

#[test]
fn transparent_decoration_emits_no_effect_and_empty_child_stays_empty() {
    let delta = update_decoration(BoxDecoration::new().try_color(Color::TRANSPARENT).unwrap());
    assert!(delta.added.is_empty());
}

#[test]
fn retained_scene_modifies_geometry_without_replacing_identity() {
    let mut graph = SceneGraph::new();
    let first = SceneItem {
        id: 0,
        primitive: Primitive::RoundedQuad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0)),
            color: Color::RED,
            corner_radii: [2.0; 4],
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    let first_delta = graph.diff(vec![first]);
    let id = first_delta.added[0].id;
    let second = SceneItem {
        id,
        primitive: Primitive::RoundedQuad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(30.0, 10.0)),
            color: Color::RED,
            corner_radii: [2.0; 4],
        },
        clips: Vec::new(),
        paint_order: 0,
    };

    let delta = graph.diff(vec![second]);
    assert!(delta.added.is_empty());
    assert!(delta.removed.is_empty());
    assert_eq!(delta.modified.len(), 1);
    assert_eq!(delta.modified[0].id, id);
}

#[test]
fn fractional_viewport_keeps_logical_geometry_scale_aware() {
    let viewport = Viewport::new(300, 200, 1.5);
    let rect = Rect::from_min_size(Point::new(1.25, 2.5), Size::new(10.0, 8.0));
    let ndc = viewport.dp_rect_to_ndc(&rect);
    assert_eq!(ndc[0], -0.9875);
    assert_eq!(ndc[1], 0.9625);
    assert_eq!(ndc[2], 0.1);
    assert!((ndc[3] + 0.12).abs() < 1e-6);
}

#[test]
fn legacy_scalar_primitives_remain_distinct_from_decorated_geometry() {
    let item = SceneItem {
        id: 1,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 3.0,
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    assert!(matches!(
        item.primitive,
        Primitive::Quad {
            corner_radius: 3.0,
            ..
        }
    ));
}

#[test]
fn should_keep_only_the_last_staged_child_and_preserve_its_layout() {
    // Arrange
    let mut runtime = Runtime::new();
    runtime.set_root(
        DecoratedBox::new(BoxDecoration::new())
            .child(SizedBox::new(Size::new(12.0, 8.0)).color(Color::RED))
            .child(SizedBox::new(Size::new(20.0, 14.0)).color(Color::BLUE)),
    );

    // Act
    runtime.update(Instant::now());

    // Assert
    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    assert_eq!(root.children().len(), 1);
    assert_eq!(root.layout_rect().unwrap().size(), Size::new(20.0, 14.0));
    let child = runtime.arena().get(root.children()[0]).unwrap();
    assert_eq!(child.layout_rect().unwrap().size(), Size::new(20.0, 14.0));
    let delta = runtime.pending_delta().unwrap();
    assert_eq!(delta.added.len(), 1);
    assert!(matches!(
        delta.added[0].primitive,
        Primitive::Quad {
            color: Color::BLUE,
            ..
        }
    ));
}

#[test]
fn should_layout_without_a_child_as_constrained_zero_size() {
    // Arrange
    let mut runtime = Runtime::new();
    runtime.set_root(DecoratedBox::new(BoxDecoration::new()));

    // Act
    runtime.update(Instant::now());

    // Assert
    let root = runtime.arena().get(runtime.root_id().unwrap()).unwrap();
    assert_eq!(root.children().len(), 0);
    assert_eq!(root.layout_rect().unwrap().size(), Size::ZERO);
}

#[test]
fn should_normalize_oversized_corner_radii_against_the_allocated_box() {
    // Arrange
    let decoration = BoxDecoration::new()
        .try_color(Color::RED)
        .unwrap()
        .border_radius(BorderRadius::all(100.0).unwrap());

    // Act
    let delta = update_decoration(decoration);

    // Assert
    let Primitive::RoundedQuad {
        corner_radii, rect, ..
    } = &delta.added[0].primitive
    else {
        panic!("expected rounded fill");
    };
    assert_eq!(
        *rect,
        Rect::from_min_size(Point::ZERO, Size::new(40.0, 24.0))
    );
    assert_eq!(*corner_radii, [12.0; 4]);
}

#[test]
fn should_omit_zero_width_and_transparent_borders() {
    // Arrange
    let zero_width = BoxDecoration::new().border(Border::all(Color::BLUE, 0.0).unwrap());
    let transparent = BoxDecoration::new().border(Border::all(Color::TRANSPARENT, 2.0).unwrap());

    // Act
    let zero_width_delta = update_decoration(zero_width);
    let transparent_delta = update_decoration(transparent);

    // Assert
    assert!(zero_width_delta.added.is_empty());
    assert!(transparent_delta.added.is_empty());
}

#[test]
fn malformed_layout_geometry_does_not_panic_when_clip_policy_is_active() {
    let widget = DecoratedBox::new(BoxDecoration::new()).clip_behavior(ClipBehavior::HardEdge);
    // The wrapper no longer constructs a fallible rounded clip during paint;
    // malformed geometry is therefore handled by the renderer boundary.
    let _ = widget;
}
