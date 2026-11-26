//! Application visual roles; reusable widgets still own interaction and rendering.
//! First consumer: Quick Save. These are not widget defaults or terminal theme colors.
use harbor_widget::{
    ControlColors, ControlStyle, Theme, scene::primitive::Color, view::BuildCx,
    widgets::text_label::TextLabel,
};

pub(crate) const SECTION_GAP: f32 = 24.0;
pub(crate) const FIELD_GAP: f32 = 12.0;
pub(crate) const LABEL_GAP: f32 = 6.0;
pub(crate) const PANEL_PADDING: f32 = 24.0;
pub(crate) const INPUT_PADDING: f32 = 8.0;
pub(crate) const CONTROL_HEIGHT: f32 = 36.0;
pub(crate) const BUTTON_HEIGHT: f32 = 32.0;
pub(crate) const BUTTON_PILL_RADIUS: f32 = BUTTON_HEIGHT / 2.0;
pub(crate) const BUTTON_HORIZONTAL_PADDING: f32 = 16.0;
pub(crate) const DIVIDER: Color = neutral(0.12);
pub(crate) const FOREGROUND: Color = neutral(0.96);
pub(crate) const SECONDARY: Color = neutral(0.72);
pub(crate) const WARNING: Color = Color {
    r: 1.0,
    g: 0.76,
    b: 0.53,
    a: 1.0,
};

const fn neutral(a: f32) -> Color {
    Color { a, ..Color::WHITE }
}
const fn colors(background: Color, foreground: Color, border: Color) -> ControlColors {
    ControlColors {
        background,
        foreground,
        border,
    }
}

/// Glacier-blue application recipe, injected through widget ThemeProvider.
/// Only the first consumer's subtree adopts it; generic/terminal themes stay unchanged.
pub(crate) fn theme(inherited: &Theme) -> Theme {
    let mut theme = inherited.clone();
    let blue = Color {
        r: 0.08,
        g: 0.55,
        b: 1.0,
        a: 1.0,
    };
    // Reuse the native row-sized liquid-glass recipe, not an alpha-only imitation.
    let selection = super::GlassAppearance::command_palette_row(
        blue,
        harbor_widget::InteractionState {
            selected: true,
            ..Default::default()
        },
    )
    .expect("selected glass recipe");
    let harbor_widget::Paint::Solid(tint) = selection.panel_fill else {
        unreachable!("row glass uses a solid tint over its material");
    };
    let glass_backdrop = harbor_widget::ControlBackdrop::new(
        selection.blur_sigma,
        selection.refraction,
        selection.material.expect("native liquid-glass material"),
    )
    .expect("valid shared glass recipe");
    // Tinted glacier-blue liquid glass for primary action buttons; the shader's
    // BackdropRim supplies the specular glass edge instead of a flat 1px border.
    theme.button.normal = colors(Color { a: 0.52, ..blue }, Color::WHITE, Color::TRANSPARENT);
    theme.button.hovered = colors(
        Color {
            r: 0.16,
            g: 0.64,
            b: 1.0,
            a: 0.66,
        },
        Color::WHITE,
        Color::TRANSPARENT,
    );
    theme.button.pressed = colors(
        Color {
            r: 0.04,
            g: 0.42,
            b: 0.90,
            a: 0.44,
        },
        Color::WHITE,
        Color::TRANSPARENT,
    );
    theme.button.normal_backdrop = Some(glass_backdrop);
    theme.button.selected = colors(tint, Color::WHITE, Color::TRANSPARENT);
    theme.button.selected_backdrop = Some(glass_backdrop);
    theme.button.disabled = colors(neutral(0.04), neutral(0.50), Color::TRANSPARENT);
    theme.button.focus_ring = neutral(0.42);
    theme
}

#[derive(Clone, Copy)]
pub(crate) enum TextRole {
    Title,
    Section,
    Label,
    Supporting,
    Warning,
}

pub(crate) fn text(value: impl Into<String>, role: TextRole) -> TextLabel {
    let (scale, color) = match role {
        TextRole::Title => (1.6, FOREGROUND),
        TextRole::Section => (1.2, FOREGROUND),
        TextRole::Label => (0.95, FOREGROUND),
        TextRole::Supporting => (0.9, SECONDARY),
        TextRole::Warning => (0.9, WARNING),
    };
    TextLabel::new(value)
        .wrap()
        .scale(scale)
        .unwrap()
        .foreground(color)
        .unwrap()
}

#[derive(Clone, Copy)]
pub(crate) enum ControlRole {
    Primary,
    Secondary,
    Quiet,
    Selection,
    Checkbox,
}

/// Product styling only. Focus/activation/disabled precedence remain widget-owned.
pub(crate) fn control(cx: &BuildCx, role: ControlRole) -> ControlStyle {
    let mut style = cx.theme().button.clone();
    style.min_height = CONTROL_HEIGHT;
    style.horizontal_padding = 12.0;
    style.icon_size = 16.0;
    style.radius = 7.0;
    style.border_width = 1.0;
    if !matches!(role, ControlRole::Primary | ControlRole::Secondary) {
        style.normal_backdrop = None;
    }
    if !matches!(role, ControlRole::Selection) {
        style.selected_backdrop = None;
        style.hovered_backdrop = None;
    }
    if matches!(role, ControlRole::Primary) {
        style.min_height = BUTTON_HEIGHT;
        style.horizontal_padding = BUTTON_HORIZONTAL_PADDING;
        style.radius = BUTTON_PILL_RADIUS;
        style.border_width = 0.0;
        return style;
    }
    style.normal = colors(neutral(0.045), FOREGROUND, neutral(0.12));
    style.hovered = colors(neutral(0.10), FOREGROUND, neutral(0.20));
    style.pressed = colors(neutral(0.16), FOREGROUND, neutral(0.24));
    // Accent selection, focus and disabled colors are inherited, not repainted locally.
    match role {
        ControlRole::Primary => unreachable!(),
        ControlRole::Secondary => {
            // Clear neutral liquid-glass pill in idle, hover, and pressed states.
            style.min_height = BUTTON_HEIGHT;
            style.horizontal_padding = BUTTON_HORIZONTAL_PADDING;
            style.radius = BUTTON_PILL_RADIUS;
            style.border_width = 0.0;
            style.normal = colors(neutral(0.08), FOREGROUND, Color::TRANSPARENT);
            style.hovered = colors(neutral(0.15), Color::WHITE, Color::TRANSPARENT);
            style.pressed = colors(neutral(0.05), FOREGROUND, Color::TRANSPARENT);
        }
        ControlRole::Selection => {
            style.normal.background = Color::TRANSPARENT;
            style.normal.border = Color::TRANSPARENT;
            style.horizontal_padding = 8.0;
            style.border_width = 0.0;
            style.hovered = colors(neutral(0.06), FOREGROUND, Color::TRANSPARENT);
            style.hovered_backdrop = style.selected_backdrop;
            style.pressed_feedback = false;
        }
        ControlRole::Quiet | ControlRole::Checkbox => {
            style.border_width = 0.0;
            style.normal.background = Color::TRANSPARENT;
            style.normal.foreground = SECONDARY;
            style.disabled.background = Color::TRANSPARENT;
            if matches!(role, ControlRole::Checkbox) {
                // The checkmark conveys selection, not a second primary-button surface.
                style.min_height = 28.0;
                style.horizontal_padding = 4.0;
                style.normal.foreground = FOREGROUND;
                style.selected = style.normal;
            }
        }
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_widget::{
        Button, Component, Theme, ThemeProvider, runtime::Runtime, signal::Signal,
    };
    use std::{cell::RefCell, rc::Rc, time::Instant};

    #[test]
    fn control_accents_follow_the_injected_widget_theme() {
        let theme = Signal::new(Theme::default());
        let observed = Rc::new(RefCell::new(Vec::new()));
        let capture = observed.clone();
        let mut runtime = Runtime::new();
        runtime.set_root(ThemeProvider::from_signal(theme.clone()).child(
            move |cx: &mut BuildCx| {
                let styles: Vec<_> = [
                    ControlRole::Primary,
                    ControlRole::Secondary,
                    ControlRole::Selection,
                    ControlRole::Quiet,
                    ControlRole::Checkbox,
                ]
                .into_iter()
                .map(|role| control(cx, role))
                .collect();
                let button = Button::text("Theme probe")
                    .style(styles[0].clone())
                    .unwrap();
                capture.replace(styles);
                button.build(cx)
            },
        ));
        for color in [Color::RED, Color::GREEN] {
            let mut injected = Theme::default();
            injected.button.normal.background = color;
            injected.button.hovered.background = Color { a: 0.9, ..color };
            injected.button.pressed.background = Color { a: 0.8, ..color };
            injected.button.selected.background = Color { a: 0.4, ..color };
            injected.button.selected.border = color;
            injected.button.focus_ring = color;
            injected.button.selected_backdrop = if color == Color::RED {
                super::theme(&injected).button.selected_backdrop
            } else {
                None
            };
            theme.set(injected.clone());
            runtime.update(Instant::now());
            let styles = observed.borrow();
            assert_eq!(styles[0].normal, injected.button.normal);
            assert_eq!(styles[0].hovered, injected.button.hovered);
            assert_eq!(styles[0].pressed, injected.button.pressed);
            assert_eq!(styles[2].selected, injected.button.selected);
            assert_eq!(
                styles[2].selected_backdrop,
                injected.button.selected_backdrop
            );
            assert!(
                styles
                    .iter()
                    .enumerate()
                    .all(|(index, style)| { index == 2 || style.selected_backdrop.is_none() })
            );
            assert!(styles.iter().all(|style| style.focus_ring == color));
        }
    }

    #[test]
    fn glacier_theme_preserves_other_roles_and_uses_liquid_glass_pill_buttons() {
        let inherited = Theme::default();
        let themed = theme(&inherited);
        themed.validate().unwrap();
        assert_eq!(inherited, Theme::default());
        assert_eq!(themed.icon_button, inherited.icon_button);
        assert_eq!(themed.spacing, inherited.spacing);
        // Primary button uses translucent glacier-blue tinted glass over a native backdrop.
        assert!((0.0..1.0).contains(&themed.button.normal.background.a));
        assert!(themed.button.normal.background.a > 0.0);
        assert_eq!(themed.button.normal.border, Color::TRANSPARENT);
        assert!(themed.button.normal_backdrop.is_some());
        // Focus ring is a softened neutral glass rim (no blue tint on neutral buttons).
        assert_eq!(themed.button.focus_ring.r, themed.button.focus_ring.g);
        assert_eq!(themed.button.focus_ring.g, themed.button.focus_ring.b);
        assert!(themed.button.focus_ring.a > 0.0 && themed.button.focus_ring.a < 1.0);
        let selection = themed.button.selected;
        assert!(selection.background.b > selection.background.g);
        assert!(selection.background.g > selection.background.r);
        assert!((0.0..1.0).contains(&selection.background.a));
        assert!(selection.background.a > 0.0);
        assert_eq!(selection.border, Color::TRANSPARENT);
        assert!(themed.button.selected_backdrop.is_some());
        assert_eq!(selection.foreground, Color::WHITE);

        let observed = Rc::new(RefCell::new(Vec::new()));
        let capture = observed.clone();
        let mut runtime = Runtime::new();
        runtime.set_root(
            ThemeProvider::new(themed)
                .unwrap()
                .child(move |cx: &mut BuildCx| {
                    let styles: Vec<_> = [
                        ControlRole::Primary,
                        ControlRole::Secondary,
                        ControlRole::Selection,
                        ControlRole::Quiet,
                        ControlRole::Checkbox,
                    ]
                    .into_iter()
                    .map(|role| control(cx, role))
                    .collect();
                    capture.replace(styles);
                    Button::text("Probe").build(cx)
                }),
        );
        runtime.update(Instant::now());
        let styles = observed.borrow();
        // Primary and Secondary are borderless liquid-glass pill buttons.
        for style in [&styles[0], &styles[1]] {
            assert_eq!(style.min_height, BUTTON_HEIGHT);
            assert_eq!(style.radius, BUTTON_PILL_RADIUS);
            assert_eq!(style.horizontal_padding, BUTTON_HORIZONTAL_PADDING);
            assert_eq!(style.border_width, 0.0);
            assert!(style.normal_backdrop.is_some());
            assert!(style.selected_backdrop.is_none());
            assert!(style.hovered_backdrop.is_none());
        }
        // Selection, Quiet, and Checkbox do not acquire idle normal_backdrop.
        for style in [&styles[2], &styles[3], &styles[4]] {
            assert!(style.normal_backdrop.is_none());
        }
    }
}
