use super::*;
use harbor_widget::layout::{Point, Rect, Size};
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::scene::SceneItem;
use harbor_widget::scene::primitive::{Color, Primitive};
use harbor_widget::widgets::external_surface::ExternalSurface;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::{BorderRadius, ClipBehavior, ControlFlowEffect, DecoratedBox, RuntimeEffects};
use std::any::TypeId;
use std::time::Instant;

#[derive(Clone)]
struct TestMainTerminalRoot<C: Clone> {
    root: Padding,
    child: C,
}

impl<C: Component + Clone + 'static> Component for TestMainTerminalRoot<C> {
    fn build(&self, _cx: &mut BuildCx) -> View {
        harbor_widget::view! { self.root.clone() => {
            TerminalDecorationPreset::container() => {
                self.child.clone();
            }
        }
        }
    }
}

fn test_main_terminal_root(
    backdrop_available: bool,
    child: impl Component + Clone + 'static,
) -> impl Component {
    TestMainTerminalRoot {
        root: crate::workspace::root_padding(
            backdrop_available,
            harbor_config::WindowBackdropStyle::default().fallback,
        ),
        child,
    }
}

fn mounted_main_root(viewport: Option<Viewport>) -> (Runtime, RuntimeEffects) {
    let mut runtime = Runtime::new();
    if let Some(viewport) = viewport {
        runtime.set_viewport(viewport);
    }
    runtime.set_root(test_main_terminal_root(false, ExternalSurface::new(1)));
    let effects = runtime.update(Instant::now());
    (runtime, effects)
}

fn fiber_chain(runtime: &Runtime) -> (TypeId, Rect, TypeId, Rect, TypeId, Rect) {
    let padding_id = runtime.root_id().expect("root fiber");
    let padding = runtime.arena().get(padding_id).expect("padding fiber");
    let padding_type = padding.widget_type();
    let padding_rect = padding.layout_rect().expect("padding layout");
    let decorated_id = padding.children()[0];
    let decorated = runtime.arena().get(decorated_id).expect("decorated fiber");
    let decorated_type = decorated.widget_type();
    let decorated_rect = decorated.layout_rect().expect("decorated layout");
    let external_id = decorated.children()[0];
    let external = runtime.arena().get(external_id).expect("external fiber");
    (
        padding_type,
        padding_rect,
        decorated_type,
        decorated_rect,
        external.widget_type(),
        external.layout_rect().expect("external layout"),
    )
}

fn painted_items(runtime: &Runtime) -> Vec<SceneItem> {
    let mut items = runtime
        .pending_delta()
        .cloned()
        .expect("mounted root produces a scene delta")
        .added;
    items.sort_by_key(|item| item.paint_order);
    items
}

fn assert_opaque_fallback_color(color: Color) {
    for channel in [color.r, color.g, color.b] {
        let srgb = if channel <= 0.0031308 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        assert_eq!((srgb * 255.0).round() as u8, 0x1E);
    }
    assert_eq!(color.a, 1.0);
}

#[test]
fn should_expose_product_decoration_values() {
    let wrapped = TerminalDecorationPreset::container();

    assert_eq!(wrapped.get_clip_behavior(), ClipBehavior::AntiAlias);
    let decoration = wrapped.get_decoration();
    assert_eq!(decoration.color(), None);
    assert_eq!(decoration.border_value(), None);
    let radii = decoration
        .border_radius_value()
        .expect("product radius is present")
        .as_array();
    assert_eq!(radii, [8.0, 8.0, 8.0, 8.0]);
    let shadows = decoration.shadows();
    assert_eq!(shadows.len(), 1);
    let shadow = shadows[0];
    assert_eq!(shadow.color().a, 0.10);
    assert_eq!(shadow.offset(), Point::new(0.0, 1.0));
    assert_eq!(shadow.blur_radius(), 1.0);
    assert_eq!(shadow.spread_radius(), 0.0);
}

#[test]
fn should_apply_four_dp_inset_with_opaque_fallback_when_no_backdrop() {
    let root = crate::workspace::root_padding(
        false,
        harbor_config::WindowBackdropStyle::default().fallback,
    );
    assert_eq!(root.get_insets().top, 4.0);
    assert_eq!(root.get_insets().right, 4.0);
    assert_eq!(root.get_insets().bottom, 4.0);
    assert_eq!(root.get_insets().left, 4.0);
    let mut runtime = Runtime::new();
    runtime.set_root(root);
    runtime.update(Instant::now());
    let items = painted_items(&runtime);
    let [item] = items.as_slice() else {
        panic!("one opaque root background quad");
    };
    let Primitive::Quad { color, .. } = item.primitive else {
        panic!("opaque root background quad");
    };
    assert_opaque_fallback_color(color);
}

#[test]
fn should_omit_root_background_when_backdrop_is_available() {
    let root = crate::workspace::root_padding(
        true,
        harbor_config::WindowBackdropStyle::default().fallback,
    );
    let mut runtime = Runtime::new();
    runtime.set_root(root);
    runtime.update(Instant::now());
    assert!(
        runtime
            .pending_delta()
            .is_none_or(|delta| delta.added.is_empty())
    );
}

#[test]
fn should_emit_no_root_quad_and_keep_inset_when_backdrop_is_available() {
    let mut runtime = Runtime::new();
    runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
    let _effects = runtime.update(Instant::now());
    let items = painted_items(&runtime);

    assert!(
        !items
            .iter()
            .any(|item| matches!(item.primitive, Primitive::Quad { .. })),
        "backdrop-aware root must not emit a base Quad"
    );

    let (_, _padding_rect, _, decorated_rect, _, external_rect) = fiber_chain(&runtime);
    let expected_child = Rect::from_min_size(Point::new(4.0, 4.0), Size::new(792.0, 592.0));
    assert_eq!(decorated_rect, expected_child);
    assert_eq!(external_rect, expected_child);
}

#[test]
fn should_paint_dark_fallback_across_viewport_when_backdrop_is_unavailable() {
    for (width, height) in [(800, 600), (40, 40), (10, 10)] {
        let viewport = Viewport::new(width, height, 1.0);
        let expected_rect = Rect::from_min_size(Point::new(0.0, 0.0), viewport.logical_size);
        let (runtime, _) = mounted_main_root(Some(viewport));
        let items = painted_items(&runtime);
        let first = items.first().expect("opaque root emits its fallback");
        let Primitive::Quad { rect, color, .. } = &first.primitive else {
            panic!("opaque fallback must paint before the shadow and terminal");
        };
        assert_eq!(*rect, expected_rect);
        assert_opaque_fallback_color(*color);
        assert!(
            first.clips.is_empty(),
            "fallback must fill the rounded cutouts"
        );
    }
}

#[test]
fn should_preserve_decoration_paint_when_backdrop_is_available() {
    for (width, height) in [(800, 600), (40, 40)] {
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(width, height, 1.0));
        runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
        runtime.update(Instant::now());
        let items = painted_items(&runtime);

        assert_eq!(items.len(), 2);
        let expected_child = Rect::from_min_size(
            Point::new(4.0, 4.0),
            Size::new(width as f32 - 8.0, height as f32 - 8.0),
        );
        let Primitive::OuterShadow {
            color,
            blur_radius,
            occluder_rect,
            ..
        } = &items[0].primitive
        else {
            panic!("shadow must paint before the terminal");
        };
        assert_eq!(color.a, 0.10);
        assert_eq!(*blur_radius, 1.0);
        assert_eq!(*occluder_rect, expected_child);
        assert!(matches!(
            items[1].primitive,
            Primitive::External { draw, rect } if draw == 1 && rect == expected_child
        ));
        let clip = items[1]
            .clips
            .last()
            .expect("terminal keeps its rounded clip");
        assert_eq!(clip.behavior(), ClipBehavior::AntiAlias);
        let radius = 8.0;
        assert_eq!(clip.radii().as_array(), [radius; 4]);
    }
}

#[test]
fn should_omit_base_fill_when_backdrop_viewport_has_no_content_space() {
    for (width, height) in [(0, 0), (4, 4), (8, 8)] {
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(width, height, 1.0));
        runtime.set_root(test_main_terminal_root(true, ExternalSurface::new(1)));
        let effects = runtime.update(Instant::now());
        let items = painted_items(&runtime);

        assert!(
            !items
                .iter()
                .any(|item| matches!(item.primitive, Primitive::Quad { .. }))
        );
        for item in &items {
            if let Primitive::External { rect, .. } = item.primitive {
                assert_eq!(rect.size(), Size::ZERO);
            }
        }
        assert!(!matches!(
            effects.control_flow,
            Some(ControlFlowEffect::WaitUntil(_))
        ));
    }
}

#[test]
fn should_layout_and_paint_preset_tree_when_runtime_uses_default_viewport() {
    let (runtime, _effects) = mounted_main_root(None);
    let (padding_type, padding_rect, decorated_type, decorated_rect, external_type, external_rect) =
        fiber_chain(&runtime);
    let items = painted_items(&runtime);

    // The root Fiber belongs to the mounted description component (ADR 0079);
    // the concrete Padding materializes onto it without an extra layer.
    assert_eq!(
        padding_type,
        TypeId::of::<TestMainTerminalRoot<ExternalSurface>>()
    );
    assert_eq!(decorated_type, TypeId::of::<DecoratedBox>());
    assert_eq!(external_type, TypeId::of::<ExternalSurface>());
    assert_eq!(padding_rect.min, Point::new(0.0, 0.0));
    assert_eq!(padding_rect.size(), Size::new(800.0, 600.0));
    let expected_child = Rect::from_min_size(Point::new(4.0, 4.0), Size::new(792.0, 592.0));
    assert_eq!(decorated_rect, expected_child);
    assert_eq!(external_rect, expected_child);

    let mut primitives = items.iter().map(|item| &item.primitive);
    let first = primitives
        .find(|primitive| {
            matches!(
                primitive,
                Primitive::Quad { .. }
                    | Primitive::OuterShadow { .. }
                    | Primitive::External { .. }
                    | Primitive::RoundedQuad { .. }
                    | Primitive::RoundedBorder { .. }
            )
        })
        .expect("decoration or external primitive");
    let Primitive::Quad { color, .. } = first else {
        panic!("first matching primitive must be the opaque base Quad")
    };
    assert_eq!(color.a, 1.0);
    let shadow_item = items
        .iter()
        .find(|item| matches!(item.primitive, Primitive::OuterShadow { .. }))
        .expect("decoration emits OuterShadow");
    let Primitive::OuterShadow {
        color,
        blur_radius,
        occluder_rect,
        ..
    } = &shadow_item.primitive
    else {
        unreachable!();
    };
    assert_eq!(color.a, 0.10);
    assert_eq!(*blur_radius, 1.0);
    assert_eq!(*occluder_rect, expected_child);
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.primitive, Primitive::RoundedQuad { .. })),
        "no-fill decoration must not emit RoundedQuad"
    );
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.primitive, Primitive::RoundedBorder { .. })),
        "no-border decoration must not emit RoundedBorder"
    );

    let external = items
        .iter()
        .find(|item| matches!(item.primitive, Primitive::External { .. }))
        .expect("ExternalSurface emits External");
    assert!(matches!(
        external.primitive,
        Primitive::External {
            draw,
            rect
        } if draw == 1 && rect == expected_child
    ));
    assert!(!external.clips.is_empty());
    let innermost = external.clips.last().expect("innermost clip");
    assert_eq!(innermost.behavior(), ClipBehavior::AntiAlias);
    assert_eq!(innermost.radii().as_array(), [8.0, 8.0, 8.0, 8.0]);
}

#[test]
fn should_keep_zero_child_size_without_repeating_redraw_when_viewport_is_zero() {
    let viewport = Viewport::new(0, 0, 1.0);
    let (runtime, effects) = mounted_main_root(Some(viewport));
    let (
        _padding_type,
        padding_rect,
        _decorated_type,
        decorated_rect,
        _external_type,
        external_rect,
    ) = fiber_chain(&runtime);

    assert_eq!(padding_rect.min, Point::new(0.0, 0.0));
    assert_eq!(decorated_rect.size(), Size::ZERO);
    assert_eq!(external_rect.size(), Size::ZERO);
    assert!(!matches!(
        effects.control_flow,
        Some(ControlFlowEffect::WaitUntil(_))
    ));
}

#[test]
fn should_normalize_clip_radii_to_child_size_when_viewport_is_smaller_than_radius() {
    let viewport = Viewport::new(12, 12, 1.0);
    let logical = viewport.logical_size;
    let (runtime, _effects) = mounted_main_root(Some(viewport));
    let (
        _padding_type,
        _padding_rect,
        _decorated_type,
        decorated_rect,
        _external_type,
        external_rect,
    ) = fiber_chain(&runtime);
    let items = painted_items(&runtime);

    let expected_child = Rect::from_min_size(
        Point::new(4.0, 4.0),
        Size::new(logical.width - 8.0, logical.height - 8.0),
    );
    assert_eq!(decorated_rect.min, Point::new(4.0, 4.0));
    assert_eq!(decorated_rect, expected_child);
    assert_eq!(external_rect, expected_child);

    let external = items
        .iter()
        .find(|item| matches!(item.primitive, Primitive::External { .. }))
        .expect("ExternalSurface emits External");
    let innermost = external
        .clips
        .last()
        .expect("External carries a child clip");
    let expected_radii = BorderRadius::all(8.0)
        .expect("ticket radius is finite")
        .normalize(expected_child.size())
        .expect("child size is finite")
        .as_array();
    assert_eq!(innermost.radii().as_array(), expected_radii);
}
