//! Host-owned product decoration for the main terminal root.
//!
//! Appearance lives here so `harbor-terminal` and `TerminalWidgetBridge` stay
//! free of visual-style properties. The 2dp window inset stays a separate
//! `Padding` composition in [`build_main_terminal_root`].

use harbor_widget::layout::Point;
use harbor_widget::scene::primitive::Color;
use harbor_widget::view::Component;
use harbor_widget::widgets::padding::Padding;
use harbor_widget::{BorderRadius, BoxDecoration, BoxShadow, ClipBehavior, DecoratedBox};

/// Product appearance for the main Harbor terminal.
pub(crate) struct TerminalDecorationPreset;

impl TerminalDecorationPreset {
    pub(crate) fn decoration() -> BoxDecoration {
        let shadow = BoxShadow::new()
            .try_color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            })
            .expect("product shadow color is finite")
            .try_offset(Point::new(0.0, 3.0))
            .expect("product shadow offset is finite")
            .try_blur_radius(3.0)
            .expect("product shadow blur is finite and non-negative")
            .try_spread_radius(0.0)
            .expect("product shadow spread is finite");
        BoxDecoration::new()
            .border_radius(
                BorderRadius::all(12.0).expect("product radius is finite and non-negative"),
            )
            .shadow(shadow)
    }

    pub(crate) fn clip_behavior() -> ClipBehavior {
        ClipBehavior::AntiAlias
    }

    pub(crate) fn wrap(child: impl Component + 'static) -> DecoratedBox {
        DecoratedBox::new(Self::decoration())
            .clip_behavior(Self::clip_behavior())
            .child(child)
    }
}

/// Main-window root: a 12dp inset around the product decoration.
///
/// With a compositor backdrop the root paints nothing so the unified window
/// tint (ADR 0026) shows through; without one it paints the opaque fallback
/// so the transparent frame clear never shows through as black.
pub(crate) fn build_main_terminal_root(
    backdrop_available: bool,
    child: impl Component + 'static,
) -> Padding {
    // Product/native colors are sRGB; widget colors feed a linear-light shader.
    let fallback = harbor_config::WindowBackdropStyle::default()
        .fallback
        .map(|channel| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
    let root = Padding::all(12.0);
    let root = if backdrop_available {
        root
    } else {
        root.background(Color {
            r: fallback[0],
            g: fallback[1],
            b: fallback[2],
            a: 1.0,
        })
    };
    root.child(TerminalDecorationPreset::wrap(child))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::layout::{Point, Rect, Size};
    use harbor_widget::renderer::Viewport;
    use harbor_widget::runtime::Runtime;
    use harbor_widget::scene::SceneItem;
    use harbor_widget::scene::primitive::{Color, Primitive};
    use harbor_widget::widgets::custom_paint::CustomPaint;
    use harbor_widget::widgets::padding::Padding;
    use harbor_widget::widgets::sized_box::SizedBox;
    use harbor_widget::{
        BorderRadius, ClipBehavior, ControlFlowEffect, DecoratedBox, RuntimeEffects,
    };
    use std::any::TypeId;
    use std::time::Instant;

    fn mounted_main_root(viewport: Option<Viewport>) -> (Runtime, RuntimeEffects) {
        let mut runtime = Runtime::new();
        if let Some(viewport) = viewport {
            runtime.set_viewport(viewport);
        }
        runtime.set_root(build_main_terminal_root(false, CustomPaint::new(1)));
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

    #[test]
    fn should_expose_product_decoration_values_when_wrapping() {
        // Arrange
        let child = SizedBox::new(Size::new(10.0, 10.0));

        // Act
        let wrapped = TerminalDecorationPreset::wrap(child);

        // Assert — ticket literals, not aliases of factory constants
        assert_eq!(wrapped.clip_behavior_value(), ClipBehavior::AntiAlias);
        let decoration = wrapped.decoration();
        assert_eq!(decoration.color(), None);
        assert_eq!(decoration.border_value(), None);
        let radii = decoration
            .border_radius_value()
            .expect("product radius is present")
            .as_array();
        assert_eq!(radii, [12.0, 12.0, 12.0, 12.0]);
        let shadows = decoration.shadows();
        assert_eq!(shadows.len(), 1);
        let shadow = shadows[0];
        assert_eq!(
            shadow.color(),
            Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.25,
            }
        );
        assert_eq!(shadow.offset(), Point::new(0.0, 3.0));
        assert_eq!(shadow.blur_radius(), 3.0);
        assert_eq!(shadow.spread_radius(), 0.0);
    }

    #[test]
    fn should_apply_twelve_dp_inset_with_opaque_fallback_when_no_backdrop() {
        // Arrange / Act
        let root = build_main_terminal_root(false, CustomPaint::new(1));

        // Assert
        assert_eq!(root.top, 12.0);
        assert_eq!(root.right, 12.0);
        assert_eq!(root.bottom, 12.0);
        assert_eq!(root.left, 12.0);
        assert_opaque_fallback_color(root.background.expect("opaque root background"));
    }

    fn assert_opaque_fallback_color(color: Color) {
        // An sRGB framebuffer encodes linear shader output back to display bytes.
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
    fn should_omit_root_background_when_backdrop_is_available() {
        // Arrange / Act
        let root = build_main_terminal_root(true, CustomPaint::new(1));

        // Assert — the compositor tint shows through instead.
        assert_eq!(root.background, None);
    }

    #[test]
    fn should_emit_no_root_quad_and_keep_inset_when_backdrop_is_available() {
        // Arrange
        let mut runtime = Runtime::new();
        runtime.set_root(build_main_terminal_root(true, CustomPaint::new(1)));
        let _effects = runtime.update(Instant::now());
        let items = painted_items(&runtime);

        // Assert — no root Quad: the compositor tint shows through instead.
        assert!(
            !items
                .iter()
                .any(|item| matches!(item.primitive, Primitive::Quad { .. })),
            "backdrop-aware root must not emit a base Quad"
        );

        // Assert — the 12dp inset is independent of the backdrop fact.
        let (_, _padding_rect, _, decorated_rect, _, external_rect) = fiber_chain(&runtime);
        let expected_child = Rect::from_min_size(Point::new(12.0, 12.0), Size::new(776.0, 576.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);
    }

    #[test]
    fn should_paint_dark_fallback_across_viewport_when_backdrop_is_unavailable() {
        for (width, height) in [(800, 600), (40, 40), (10, 10)] {
            // Arrange — CustomPaint is an in-memory terminal stand-in.
            let viewport = Viewport::new(width, height, 1.0);
            let expected_rect = Rect::from_min_size(Point::new(0.0, 0.0), viewport.logical_size);

            // Act
            let (runtime, _) = mounted_main_root(Some(viewport));
            let items = painted_items(&runtime);

            // Assert — inspect the actual paint output, not Padding configuration.
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
            // Arrange
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(width, height, 1.0));
            runtime.set_root(build_main_terminal_root(true, CustomPaint::new(1)));

            // Act
            runtime.update(Instant::now());
            let items = painted_items(&runtime);

            // Assert — only the base fill disappears; shadow and clipped content remain.
            assert_eq!(items.len(), 2);
            let expected_child = Rect::from_min_size(
                Point::new(12.0, 12.0),
                Size::new(width as f32 - 24.0, height as f32 - 24.0),
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
            assert_eq!(color.a, 0.25);
            assert_eq!(*blur_radius, 3.0);
            assert_eq!(*occluder_rect, expected_child);
            assert!(
                items[0].clips.is_empty(),
                "child clip must not clip the shadow"
            );
            assert!(matches!(
                items[1].primitive,
                Primitive::External { draw: 1, rect } if rect == expected_child
            ));
            let clip = items[1]
                .clips
                .last()
                .expect("terminal keeps its rounded clip");
            assert_eq!(clip.behavior(), ClipBehavior::AntiAlias);
            let radius = if width == 40 { 8.0 } else { 12.0 };
            assert_eq!(clip.radii().as_array(), [radius; 4]);
        }
    }

    #[test]
    fn should_omit_base_fill_when_backdrop_viewport_has_no_content_space() {
        for (width, height) in [(0, 0), (10, 10), (24, 24)] {
            // Arrange
            let mut runtime = Runtime::new();
            runtime.set_viewport(Viewport::new(width, height, 1.0));
            runtime.set_root(build_main_terminal_root(true, CustomPaint::new(1)));

            // Act
            let effects = runtime.update(Instant::now());
            let items = painted_items(&runtime);

            // Assert — even collapsed content must not restore the old white veil.
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
        // Arrange
        let (runtime, _effects) = mounted_main_root(None);

        // Act
        let (
            padding_type,
            padding_rect,
            decorated_type,
            decorated_rect,
            external_type,
            external_rect,
        ) = fiber_chain(&runtime);
        let items = painted_items(&runtime);

        // Assert — fiber types and 12dp inset on the 800×600 fallback
        assert_eq!(padding_type, TypeId::of::<Padding>());
        assert_eq!(decorated_type, TypeId::of::<DecoratedBox>());
        assert_eq!(external_type, TypeId::of::<CustomPaint>());
        assert_eq!(padding_rect.min, Point::new(0.0, 0.0));
        assert_eq!(padding_rect.size(), Size::new(800.0, 600.0));
        let expected_child = Rect::from_min_size(Point::new(12.0, 12.0), Size::new(776.0, 576.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);

        // Assert — base fill quad, then OuterShadow, then External
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

        // Assert — OuterShadow after the base quad, before the External
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
            panic!("shadow item must be OuterShadow");
        };
        assert_eq!(color.a, 0.25);
        assert_eq!(*blur_radius, 3.0);
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
            .expect("CustomPaint emits External");
        assert!(matches!(
            external.primitive,
            Primitive::External {
                draw: 1,
                rect
            } if rect == expected_child
        ));
        assert!(!external.clips.is_empty());
        let innermost = external.clips.last().expect("innermost clip");
        assert_eq!(innermost.behavior(), ClipBehavior::AntiAlias);
        assert_eq!(innermost.radii().as_array(), [12.0, 12.0, 12.0, 12.0]);
    }

    #[test]
    fn should_keep_zero_child_size_without_repeating_redraw_when_viewport_is_zero() {
        // Arrange
        let viewport = Viewport::new(0, 0, 1.0);

        // Act
        let (runtime, effects) = mounted_main_root(Some(viewport));
        let (
            _padding_type,
            padding_rect,
            _decorated_type,
            decorated_rect,
            _external_type,
            external_rect,
        ) = fiber_chain(&runtime);

        // Assert — layout exists; 12dp deflate saturates at zero
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
        // Arrange
        let viewport = Viewport::new(40, 40, 1.0);
        let logical = viewport.logical_size;

        // Act
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

        // Assert
        let expected_child = Rect::from_min_size(
            Point::new(12.0, 12.0),
            Size::new(logical.width - 24.0, logical.height - 24.0),
        );
        assert_eq!(decorated_rect.min, Point::new(12.0, 12.0));
        assert_eq!(decorated_rect, expected_child);
        assert_eq!(external_rect, expected_child);

        let external = items
            .iter()
            .find(|item| matches!(item.primitive, Primitive::External { .. }))
            .expect("CustomPaint emits External");
        let innermost = external
            .clips
            .last()
            .expect("External carries a child clip");
        let expected_radii = BorderRadius::all(12.0)
            .expect("ticket radius is finite")
            .normalize(expected_child.size())
            .expect("child size is finite")
            .as_array();
        assert_eq!(innermost.radii().as_array(), expected_radii);
    }
}
