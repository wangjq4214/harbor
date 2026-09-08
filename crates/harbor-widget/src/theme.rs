//! Semantic desktop theme tokens shared by widget controls.

use crate::scene::primitive::Color;
use crate::widgets::InteractionState;

/// The colors used to draw one control state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlColors {
    pub background: Color,
    pub border: Color,
    pub foreground: Color,
}

/// Control dimensions and state colors.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlStyle {
    pub min_height: f32,
    pub horizontal_padding: f32,
    pub icon_size: f32,
    pub border_width: f32,
    pub radius: f32,
    pub normal: ControlColors,
    pub hovered: ControlColors,
    pub pressed: ControlColors,
    pub selected: ControlColors,
    pub disabled: ControlColors,
    pub focus_ring: Color,
}

/// The resolved visual state category for a control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ControlVisualState {
    #[default]
    Normal,
    Hovered,
    Selected,
    Pressed,
    Disabled,
}

impl From<&InteractionState> for ControlVisualState {
    fn from(state: &InteractionState) -> Self {
        if state.disabled {
            Self::Disabled
        } else if state.pressed {
            Self::Pressed
        } else if state.selected {
            Self::Selected
        } else if state.hovered {
            Self::Hovered
        } else {
            Self::Normal
        }
    }
}

impl ControlStyle {
    /// Resolves colors for a discrete control visual state.
    pub fn resolve_visual_state(&self, state: ControlVisualState) -> ControlColors {
        match state {
            ControlVisualState::Disabled => self.disabled,
            ControlVisualState::Pressed => self.pressed,
            ControlVisualState::Selected => self.selected,
            ControlVisualState::Hovered => self.hovered,
            ControlVisualState::Normal => self.normal,
        }
    }

    /// Resolves the visual state for an interaction state snapshot.
    pub fn resolve_state(&self, state: &InteractionState) -> ControlColors {
        self.resolve_visual_state(ControlVisualState::from(state))
    }

    /// Resolves the stable visual state precedence for a control.
    ///
    /// Prefer [`resolve_state`](Self::resolve_state) or [`resolve_visual_state`](Self::resolve_visual_state)
    /// to avoid positional boolean arguments.
    pub fn resolve(
        &self,
        disabled: bool,
        selected: bool,
        hovered: bool,
        pressed: bool,
    ) -> ControlColors {
        if disabled {
            self.disabled
        } else if pressed {
            self.pressed
        } else if selected {
            self.selected
        } else if hovered {
            self.hovered
        } else {
            self.normal
        }
    }
}

/// Theme values used by generic desktop controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub button: ControlStyle,
    pub icon_button: ControlStyle,
    pub spacing: f32,
}

impl Default for Theme {
    fn default() -> Self {
        let normal = ControlColors {
            background: Color {
                r: 0.25,
                g: 0.25,
                b: 0.25,
                a: 1.0,
            },
            border: Color {
                r: 0.5,
                g: 0.5,
                b: 0.5,
                a: 1.0,
            },
            foreground: Color::WHITE,
        };
        let button = ControlStyle {
            min_height: 32.0,
            horizontal_padding: 16.0,
            icon_size: 20.0,
            border_width: 1.0,
            radius: 4.0,
            normal,
            hovered: ControlColors {
                background: Color {
                    r: 0.35,
                    g: 0.35,
                    b: 0.35,
                    a: 1.0,
                },
                ..normal
            },
            pressed: ControlColors {
                background: Color {
                    r: 0.15,
                    g: 0.15,
                    b: 0.15,
                    a: 1.0,
                },
                ..normal
            },
            selected: ControlColors {
                background: Color {
                    r: 0.28,
                    g: 0.38,
                    b: 0.58,
                    a: 1.0,
                },
                border: Color {
                    r: 0.45,
                    g: 0.65,
                    b: 1.0,
                    a: 1.0,
                },
                ..normal
            },
            disabled: ControlColors {
                background: Color {
                    r: 0.18,
                    g: 0.18,
                    b: 0.18,
                    a: 1.0,
                },
                border: Color {
                    r: 0.3,
                    g: 0.3,
                    b: 0.3,
                    a: 1.0,
                },
                foreground: Color {
                    r: 0.55,
                    g: 0.55,
                    b: 0.55,
                    a: 1.0,
                },
            },
            focus_ring: Color {
                r: 0.4,
                g: 0.6,
                b: 1.0,
                a: 1.0,
            },
        };
        let icon_button = ControlStyle {
            min_height: 28.0,
            horizontal_padding: 6.0,
            icon_size: 18.0,
            radius: 4.0,
            ..button.clone()
        };
        Self {
            button,
            icon_button,
            spacing: 8.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_visual_state_precedence() {
        let theme = Theme::default();
        let style = &theme.button;

        // Normal when idle
        let idle = InteractionState::default();
        assert_eq!(style.resolve_state(&idle), style.normal);
        assert_eq!(ControlVisualState::from(&idle), ControlVisualState::Normal);

        // Hovered
        let hovered = InteractionState { hovered: true, ..idle };
        assert_eq!(style.resolve_state(&hovered), style.hovered);
        assert_eq!(ControlVisualState::from(&hovered), ControlVisualState::Hovered);

        // Selected beats hovered
        let selected = InteractionState { selected: true, hovered: true, ..idle };
        assert_eq!(style.resolve_state(&selected), style.selected);
        assert_eq!(ControlVisualState::from(&selected), ControlVisualState::Selected);

        // Pressed beats selected and hovered
        let pressed = InteractionState { pressed: true, selected: true, hovered: true, ..idle };
        assert_eq!(style.resolve_state(&pressed), style.pressed);
        assert_eq!(ControlVisualState::from(&pressed), ControlVisualState::Pressed);

        // Disabled beats all
        let disabled = InteractionState { disabled: true, pressed: true, selected: true, hovered: true, ..idle };
        assert_eq!(style.resolve_state(&disabled), style.disabled);
        assert_eq!(ControlVisualState::from(&disabled), ControlVisualState::Disabled);
    }
}
