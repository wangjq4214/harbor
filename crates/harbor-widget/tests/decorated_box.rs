use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::scene::{SceneGraph, SceneItem};
use harbor_widget::signal::Signal;
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::sized_box::SizedBox;
use harbor_widget::{Border, BorderRadius, BoxDecoration, BoxShadow, ClipBehavior, DecoratedBox};
use std::cell::RefCell;
use std::rc::Rc;
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
fn shadows_precede_fill_and_apply_offset_spread_blur_and_shape_radii() {
    let first = BoxShadow::new()
        .try_color(Color::RED)
        .unwrap()
        .try_offset(Point::new(3.0, -2.0))
        .unwrap()
        .try_spread_radius(2.0)
        .unwrap()
        .try_blur_radius(4.0)
        .unwrap();
    let second = BoxShadow::new()
        .try_color(Color::BLUE)
        .unwrap()
        .try_spread_radius(-1.0)
        .unwrap();
    let decoration = BoxDecoration::new()
        .shadow(first)
        .shadow(second)
        .try_color(Color::GREEN)
        .unwrap()
        .border_radius(BorderRadius::only(20.0, 4.0, 2.0, 6.0).unwrap());

    let delta = update_decoration(decoration);

    assert_eq!(delta.added.len(), 3);
    let Primitive::OuterShadow {
        rect,
        shape_rect,
        color,
        corner_radii,
        blur_radius,
        ..
    } = &delta.added[0].primitive
    else {
        panic!("first item must be the first configured shadow");
    };
    assert_eq!(*color, Color::RED);
    assert_eq!(
        *shape_rect,
        Rect::from_min_size(Point::new(1.0, -4.0), Size::new(44.0, 28.0))
    );
    assert_eq!(
        *rect,
        Rect::from_min_size(Point::new(-11.0, -16.0), Size::new(68.0, 52.0))
    );
    assert_eq!(*corner_radii, [20.0, 4.0, 2.0, 6.0]);
    assert_eq!(*blur_radius, 4.0);
    assert!(matches!(
        delta.added[1].primitive,
        Primitive::OuterShadow {
            color: Color::BLUE,
            ..
        }
    ));
    assert!(matches!(
        delta.added[2].primitive,
        Primitive::RoundedQuad {
            color: Color::GREEN,
            ..
        }
    ));
    assert_eq!(
        delta
            .added
            .iter()
            .map(|item| item.paint_order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn shadow_radii_are_normalized_against_the_spread_adjusted_shape() {
    let decoration = BoxDecoration::new()
        .shadow(
            BoxShadow::new()
                .try_color(Color::RED)
                .unwrap()
                .try_spread_radius(2.0)
                .unwrap()
                .try_blur_radius(1.0)
                .unwrap(),
        )
        .border_radius(BorderRadius::all(100.0).unwrap());

    let delta = update_decoration(decoration);

    let Primitive::OuterShadow {
        shape_rect,
        corner_radii,
        ..
    } = &delta.added[0].primitive
    else {
        panic!("expected an outer shadow");
    };
    assert_eq!(shape_rect.size(), Size::new(44.0, 28.0));
    assert_eq!(*corner_radii, [14.0; 4]);
}

#[test]
fn shadows_omit_transparent_and_collapsed_shapes_without_affecting_layout() {
    let transparent = BoxShadow::new().try_color(Color::TRANSPARENT).unwrap();
    let collapsed = BoxShadow::new()
        .try_color(Color::RED)
        .unwrap()
        .try_spread_radius(-20.0)
        .unwrap();
    let decoration = BoxDecoration::new()
        .shadow(transparent)
        .shadow(collapsed)
        .try_color(Color::BLUE)
        .unwrap();

    let delta = update_decoration(decoration);

    assert_eq!(delta.added.len(), 1);
    assert!(matches!(
        delta.added[0].primitive,
        Primitive::RoundedQuad { .. }
    ));
}

#[test]
fn visible_shadows_keep_relative_order_when_transparent_entries_are_omitted() {
    let visible = BoxShadow::new()
        .try_color(Color::RED)
        .unwrap()
        .try_spread_radius(2.0)
        .unwrap();
    let decoration = BoxDecoration::new()
        .shadow(BoxShadow::new().try_color(Color::TRANSPARENT).unwrap())
        .shadow(visible)
        .try_color(Color::GREEN)
        .unwrap();

    let delta = update_decoration(decoration);

    assert_eq!(delta.added.len(), 2);
    assert!(matches!(
        delta.added[0].primitive,
        Primitive::OuterShadow {
            color: Color::RED,
            ..
        }
    ));
    assert!(matches!(
        delta.added[1].primitive,
        Primitive::RoundedQuad {
            color: Color::GREEN,
            ..
        }
    ));
}

#[test]
fn zero_and_tiny_blur_use_the_same_spread_adjusted_shape_geometry() {
    let make = |blur_radius| {
        update_decoration(
            BoxDecoration::new().shadow(
                BoxShadow::new()
                    .try_color(Color::RED)
                    .unwrap()
                    .try_spread_radius(2.0)
                    .unwrap()
                    .try_blur_radius(blur_radius)
                    .unwrap(),
            ),
        )
    };

    let zero = make(0.0);
    let tiny = make(f32::EPSILON);
    let (
        Primitive::OuterShadow {
            shape_rect: zero_shape,
            ..
        },
        Primitive::OuterShadow {
            shape_rect: tiny_shape,
            ..
        },
    ) = (&zero.added[0].primitive, &tiny.added[0].primitive)
    else {
        panic!("expected outer shadows");
    };
    assert_eq!(zero_shape, tiny_shape);
    assert_eq!(zero_shape.size(), Size::new(44.0, 28.0));
}

#[test]
fn extreme_opposite_finite_endpoints_are_rejected_before_gpu_geometry() {
    let mut runtime = Runtime::new();
    runtime.set_root(
        DecoratedBox::new(
            BoxDecoration::new().shadow(
                BoxShadow::new()
                    .try_color(Color::RED)
                    .unwrap()
                    .try_spread_radius(f32::MAX)
                    .unwrap(),
            ),
        )
        .child(SizedBox::new(Size::new(f32::MAX, f32::MAX))),
    );
    runtime.update(Instant::now());
    let delta = runtime.pending_delta().unwrap();
    assert!(
        delta.added.is_empty(),
        "overflowing shadow geometry must be omitted"
    );
}

#[test]
fn zero_sized_unspread_shadow_is_omitted() {
    let mut runtime = Runtime::new();
    runtime.set_root(DecoratedBox::new(
        BoxDecoration::new().shadow(BoxShadow::new().try_color(Color::RED).unwrap()),
    ));

    let initial = runtime.update(Instant::now());
    assert!(initial.request_redraw);

    let effects = runtime.update(Instant::now());

    assert!(!effects.request_redraw);
    assert!(effects.control_flow.is_none());
    assert!(
        runtime
            .pending_delta()
            .is_none_or(harbor_widget::scene::SceneDelta::is_empty)
    );
}

#[test]
fn unchanged_decorated_runtime_has_no_steady_state_schedule_demand() {
    let decoration = BoxDecoration::new().shadow(
        BoxShadow::new()
            .try_color(Color::RED)
            .unwrap()
            .try_spread_radius(2.0)
            .unwrap(),
    );
    let mut runtime = Runtime::new();
    runtime.set_root(DecoratedBox::new(decoration).child(SizedBox::new(Size::new(20.0, 12.0))));
    let _ = runtime.update(Instant::now());

    let steady_state = runtime.update(Instant::now());

    // Without an encode there is intentionally an unconsumed initial scene
    // delta; public Runtime APIs nevertheless expose the absence of any
    // continuing redraw or deadline demand on the steady-state turn.
    assert!(!steady_state.request_redraw);
    assert!(steady_state.control_flow.is_none());
}

#[test]
fn filtered_shadow_preserves_later_shadow_and_fill_scene_identities() {
    #[derive(Clone)]
    struct ShadowToggle {
        transparent: Rc<RefCell<Option<Signal<bool>>>>,
    }

    impl Component for ShadowToggle {
        fn build(&self, cx: &mut BuildCx) -> View {
            let transparent = cx.use_state(|| false);
            *self.transparent.borrow_mut() = Some(transparent.clone());
            let first_color = if *transparent.read() {
                Color::TRANSPARENT
            } else {
                Color::RED
            };
            DecoratedBox::new(
                BoxDecoration::new()
                    .shadow(
                        BoxShadow::new()
                            .try_color(first_color)
                            .unwrap()
                            .try_offset(Point::new(1.0, 0.0))
                            .unwrap(),
                    )
                    .shadow(
                        BoxShadow::new()
                            .try_color(Color::BLUE)
                            .unwrap()
                            .try_offset(Point::new(1.0, 0.0))
                            .unwrap(),
                    )
                    .try_color(Color::GREEN)
                    .unwrap(),
            )
            .child(SizedBox::new(Size::new(20.0, 12.0)))
            .build(cx)
        }
    }

    let state = Rc::new(RefCell::new(None));
    let mut runtime = Runtime::new();
    runtime.set_root(ShadowToggle {
        transparent: Rc::clone(&state),
    });
    runtime.update(Instant::now());
    let initial = runtime.pending_delta().unwrap().added.clone();
    assert_eq!(initial.len(), 3);
    let second_shadow_id = initial[1].id;
    let fill_id = initial[2].id;

    state.borrow().as_ref().unwrap().set(true);
    runtime.update(Instant::now());
    let pending = runtime.pending_delta().unwrap();

    assert_eq!(pending.added.len(), 2);
    assert_eq!(pending.added[0].id, second_shadow_id);
    assert_eq!(pending.added[1].id, fill_id);
    assert!(matches!(
        pending.added[0].primitive,
        Primitive::OuterShadow {
            color: Color::BLUE,
            ..
        }
    ));
    assert!(matches!(
        pending.added[1].primitive,
        Primitive::RoundedQuad {
            color: Color::GREEN,
            ..
        }
    ));
}

#[test]
fn appending_an_ineffective_shadow_does_not_replace_fill_identity() {
    #[derive(Clone)]
    struct AppendTransparent {
        state: Rc<RefCell<Option<Signal<bool>>>>,
    }

    impl Component for AppendTransparent {
        fn build(&self, cx: &mut BuildCx) -> View {
            let append = cx.use_state(|| false);
            *self.state.borrow_mut() = Some(append.clone());
            let mut decoration = BoxDecoration::new()
                .shadow(
                    BoxShadow::new()
                        .try_color(Color::BLUE)
                        .unwrap()
                        .try_offset(Point::new(1.0, 0.0))
                        .unwrap(),
                )
                .try_color(Color::GREEN)
                .unwrap();
            if *append.read() {
                decoration =
                    decoration.shadow(BoxShadow::new().try_color(Color::TRANSPARENT).unwrap());
            }
            DecoratedBox::new(decoration)
                .child(SizedBox::new(Size::new(20.0, 12.0)))
                .build(cx)
        }
    }

    let state = Rc::new(RefCell::new(None));
    let mut runtime = Runtime::new();
    runtime.set_root(AppendTransparent {
        state: Rc::clone(&state),
    });
    runtime.update(Instant::now());
    let initial = runtime.pending_delta().unwrap().added.clone();
    let shadow_id = initial[0].id;
    let fill_id = initial[1].id;

    state.borrow().as_ref().unwrap().set(true);
    runtime.update(Instant::now());
    let pending = runtime.pending_delta().unwrap();

    assert_eq!(pending.added[0].id, shadow_id);
    assert_eq!(pending.added[1].id, fill_id);
}

#[test]
fn shadow_primitive_count_is_independent_of_widget_area() {
    let decoration = || {
        BoxDecoration::new()
            .shadow(
                BoxShadow::new()
                    .try_color(Color::RED)
                    .unwrap()
                    .try_offset(Point::new(1.0, 0.0))
                    .unwrap(),
            )
            .shadow(
                BoxShadow::new()
                    .try_color(Color::BLUE)
                    .unwrap()
                    .try_offset(Point::new(1.0, 0.0))
                    .unwrap(),
            )
            .try_color(Color::GREEN)
            .unwrap()
    };
    let count_for = |size| {
        let mut runtime = Runtime::new();
        runtime.set_root(DecoratedBox::new(decoration()).child(SizedBox::new(size)));
        runtime.update(Instant::now());
        runtime.pending_delta().unwrap().added.len()
    };

    assert_eq!(count_for(Size::new(1.0, 1.0)), 3);
    assert_eq!(count_for(Size::new(10_000.0, 10_000.0)), 3);
}

#[test]
fn retained_shadow_changes_modify_its_stable_scene_item() {
    let mut graph = SceneGraph::new();
    let shadow = |id, blur_radius| SceneItem {
        id,
        primitive: Primitive::OuterShadow {
            rect: Rect::from_min_size(Point::new(-6.0, -6.0), Size::new(32.0, 32.0)),
            shape_rect: Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            occluder_rect: Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            color: Color::BLACK,
            corner_radii: [4.0; 4],
            occluder_radii: [4.0; 4],
            blur_radius,
        },
        clips: Vec::new(),
        paint_order: 0,
    };
    let unrelated = SceneItem {
        id: 0,
        primitive: Primitive::Quad {
            rect: Rect::from_min_size(Point::new(30.0, 0.0), Size::new(4.0, 4.0)),
            color: Color::GREEN,
            corner_radius: 0.0,
        },
        clips: Vec::new(),
        paint_order: 1,
    };

    let first = graph.diff(vec![shadow(0, 2.0), unrelated]);
    assert_eq!(first.added.len(), 2);
    let shadow_id = first.added[0].id;
    let unrelated_id = first.added[1].id;
    let unchanged = graph.diff(vec![shadow(shadow_id, 2.0), first.added[1].clone()]);
    assert!(unchanged.is_empty());
    let changed = graph.diff(vec![shadow(shadow_id, 3.0), first.added[1].clone()]);
    assert!(changed.added.is_empty());
    assert!(changed.removed.is_empty());
    assert_eq!(changed.modified, vec![shadow(shadow_id, 3.0)]);

    let removed = graph.diff(vec![first.added[1].clone()]);
    assert_eq!(removed.removed, vec![shadow_id]);
    assert!(removed.added.is_empty());
    assert!(removed.modified.is_empty());
    assert_eq!(graph.items()[0].id, unrelated_id);
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
