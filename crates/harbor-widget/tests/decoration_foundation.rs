use harbor_widget::decoration::ClipBehavior;
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::scene::clip::RoundedClip;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::scene::{SceneGraph, SceneItem};
use harbor_widget::{Border, BorderRadius, BoxDecoration, BoxShadow, DecorationError};

fn quad_item(id: u64, clips: Vec<RoundedClip>) -> SceneItem {
    SceneItem {
        id,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        },
        clips,
        paint_order: 0,
    }
}

#[test]
fn should_describe_invalid_decoration_input_when_validation_fails() {
    // Arrange
    let shadow = BoxShadow::new();

    // Act
    let error = shadow
        .try_offset(Point::new(1.0, f32::INFINITY))
        .unwrap_err();

    // Assert
    assert_eq!(error, DecorationError::NonFinite { field: "offset.y" });
    assert_eq!(error.field(), "offset.y");
    assert_eq!(error.rule(), "finite");
    assert_eq!(error.to_string(), "offset.y must be finite");
}

#[test]
fn should_reject_non_finite_color_when_creating_border() {
    // Arrange
    let color = Color {
        a: f32::NAN,
        ..Color::WHITE
    };

    // Act
    let result = Border::all(color, 1.0);

    // Assert
    assert_eq!(result, Err(DecorationError::NonFinite { field: "color" }));
}

#[test]
fn should_replace_fill_color_when_decoration_color_is_configured_twice() {
    // Arrange
    let decoration = BoxDecoration::new().try_color(Color::RED).unwrap();

    // Act
    let decoration = decoration.try_color(Color::GREEN).unwrap();

    // Assert
    assert_eq!(decoration.color(), Some(Color::GREEN));
}

#[test]
fn should_replace_border_when_decoration_border_is_configured_twice() {
    // Arrange
    let first_border = Border::all(Color::RED, 1.0).unwrap();
    let final_border = Border::all(Color::BLUE, 2.0).unwrap();

    // Act
    let decoration = BoxDecoration::new()
        .border(first_border)
        .border(final_border);

    // Assert
    assert_eq!(decoration.border_value(), Some(final_border));
}

#[test]
fn should_replace_corner_radii_when_decoration_radius_is_configured_twice() {
    // Arrange
    let first_radius = BorderRadius::all(2.0).unwrap();
    let final_radius = BorderRadius::only(1.0, 2.0, 3.0, 4.0).unwrap();

    // Act
    let decoration = BoxDecoration::new()
        .border_radius(first_radius)
        .border_radius(final_radius);

    // Assert
    assert_eq!(decoration.border_radius_value(), Some(final_radius));
}

#[test]
fn should_preserve_shadow_order_when_shadows_are_appended() {
    // Arrange
    let first = BoxShadow::new().try_spread_radius(-3.0).unwrap();
    let second = BoxShadow::new().try_spread_radius(5.0).unwrap();

    // Act
    let decoration = BoxDecoration::new().shadow(first).shadow(second);

    // Assert
    assert_eq!(decoration.shadows(), &[first, second]);
}

#[test]
fn should_return_zero_radii_when_normalizing_into_zero_sized_box() {
    // Arrange
    let radius = BorderRadius::only(2.0, 4.0, 6.0, 8.0).unwrap();

    // Act
    let normalized = radius.normalize(Size::new(0.0, 10.0)).unwrap();

    // Assert
    assert_eq!(normalized.as_array(), [0.0; 4]);
}

#[test]
fn should_reject_non_finite_size_when_normalizing_radius() {
    // Arrange
    let radius = BorderRadius::all(1.0).unwrap();

    // Act
    let result = radius.normalize(Size::new(f32::INFINITY, 10.0));

    // Assert
    assert_eq!(result, Err(DecorationError::NonFinite { field: "width" }));
}

#[test]
fn should_retain_bounds_and_normalized_radii_when_clip_is_constructed() {
    // Arrange
    let rect = Rect::from_min_size(Point::new(3.0, 4.0), Size::new(9.0, 12.0));
    let radius = BorderRadius::only(8.0, 4.0, 2.0, 6.0).unwrap();

    // Act
    let clip = RoundedClip::new(rect, radius, ClipBehavior::HardEdge).unwrap();

    // Assert
    assert_eq!(clip.rect(), rect);
    assert_eq!(clip.radii().as_array(), [6.0, 3.0, 1.5, 4.5]);
    assert_eq!(clip.behavior(), ClipBehavior::HardEdge);
}

#[test]
fn should_reject_non_finite_maximum_when_clip_is_constructed() {
    // Arrange
    let rect = Rect {
        min: Point::ZERO,
        max: Point::new(f32::INFINITY, 10.0),
    };

    // Act
    let result = RoundedClip::new(rect, BorderRadius::default(), ClipBehavior::None);

    // Assert
    assert_eq!(
        result,
        Err(DecorationError::NonFinite {
            field: "rect.max.x"
        })
    );
}

#[test]
fn should_preserve_maximum_radii_when_clip_extents_span_opposite_finite_extremes() {
    // Arrange
    let rect = Rect {
        min: Point::new(-f32::MAX, -f32::MAX),
        max: Point::new(f32::MAX, f32::MAX),
    };
    let radius = BorderRadius::all(f32::MAX).unwrap();

    // Act
    let clip = RoundedClip::new(rect, radius, ClipBehavior::HardEdge).unwrap();

    // Assert
    assert_eq!(clip.radii().as_array(), [f32::MAX; 4]);
}

#[test]
fn should_emit_modified_item_when_stable_item_clips_change() {
    // Arrange
    let mut graph = SceneGraph::new();
    let item_id = graph.diff(vec![quad_item(0, Vec::new())]).added[0].id;
    let clip = RoundedClip::new(
        Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0)),
        BorderRadius::all(2.0).unwrap(),
        ClipBehavior::AntiAlias,
    )
    .unwrap();

    // Act
    let delta = graph.diff(vec![quad_item(item_id, vec![clip.clone()])]);

    // Assert
    assert!(delta.added.is_empty());
    assert!(delta.removed.is_empty());
    assert_eq!(delta.modified, vec![quad_item(item_id, vec![clip])]);
}

#[test]
fn should_not_emit_change_when_stable_item_clips_are_unchanged() {
    // Arrange
    let mut graph = SceneGraph::new();
    let clip = RoundedClip::new(
        Rect::from_min_size(Point::new(1.0, 2.0), Size::new(10.0, 5.0)),
        BorderRadius::all(1.0).unwrap(),
        ClipBehavior::HardEdge,
    )
    .unwrap();
    let item_id = graph.diff(vec![quad_item(0, vec![clip.clone()])]).added[0].id;

    // Act
    let delta = graph.diff(vec![quad_item(item_id, vec![clip])]);

    // Assert
    assert!(delta.is_empty());
}

#[test]
fn should_preserve_nested_clip_order_and_diff_when_it_changes() {
    // Arrange
    let outer = RoundedClip::new(
        Rect::from_min_size(Point::ZERO, Size::new(20.0, 10.0)),
        BorderRadius::all(2.0).unwrap(),
        ClipBehavior::HardEdge,
    )
    .unwrap();
    let inner = RoundedClip::new(
        Rect::from_min_size(Point::new(2.0, 2.0), Size::new(16.0, 6.0)),
        BorderRadius::all(1.0).unwrap(),
        ClipBehavior::AntiAlias,
    )
    .unwrap();
    let mut graph = SceneGraph::new();
    let item_id = graph
        .diff(vec![quad_item(0, vec![outer.clone(), inner.clone()])])
        .added[0]
        .id;

    // Act
    let delta = graph.diff(vec![quad_item(item_id, vec![inner.clone(), outer.clone()])]);

    // Assert
    assert_eq!(delta.modified, vec![quad_item(item_id, vec![inner, outer])]);
}

#[test]
fn should_saturate_negative_finite_clip_coordinates_when_scaling_overflows() {
    // Arrange
    let clip = RoundedClip::new(
        Rect {
            min: Point::new(-f32::MAX, -1.0),
            max: Point::new(-f32::MAX, 1.0),
        },
        BorderRadius::default(),
        ClipBehavior::HardEdge,
    )
    .unwrap();
    let viewport = Viewport::new(1, 1, 2.0);

    // Act
    let physical = viewport.to_physical_clip(&clip);

    // Assert
    assert_eq!(physical.min(), (-f32::MAX, -2.0));
    assert_eq!(physical.max(), (-f32::MAX, 2.0));
}
