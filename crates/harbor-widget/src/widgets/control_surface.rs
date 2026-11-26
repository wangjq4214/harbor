//! Shared control chrome. Interaction stays in InteractiveRegion; content measures itself.
use super::interactive_region::InteractionState;
use crate::layout::{BoxConstraints, Insets, LayoutError, Point, Rect, Size};
use crate::scene::primitive::Primitive;
use crate::theme::ControlStyle;
use crate::view::{AnyView, PaintPhase};

#[derive(Clone)]
pub(super) struct ControlSurface {
    pub style: ControlStyle,
    pub centered: bool,
}

impl ControlSurface {
    pub fn child_constraints(&self, constraints: BoxConstraints) -> BoxConstraints {
        let content = constraints.deflate(Size::new(self.style.horizontal_padding * 2.0, 0.0));
        if self.centered {
            BoxConstraints::loose(content.max)
        } else {
            content
        }
    }

    pub fn layout(
        &self,
        constraints: BoxConstraints,
        children: &[Size],
    ) -> Result<(Size, Vec<Point>), LayoutError> {
        let child = children.first().copied().unwrap_or(Size::ZERO);
        // Individually valid scalars can overflow only when layout combines them.
        // Check before the parent clamp can hide the unrepresentable natural size.
        Insets {
            left: self.style.horizontal_padding,
            right: self.style.horizontal_padding,
            ..Insets::default()
        }
        .checked_inflate(child)?;
        let size = constraints.constrain(Size::new(
            child.width + self.style.horizontal_padding * 2.0,
            child.height.max(self.style.min_height),
        ));
        let x = if self.centered {
            ((size.width - child.width) / 2.0).max(0.0)
        } else {
            self.style.horizontal_padding.min(size.width)
        };
        let origin = Point::new(x, ((size.height - child.height) / 2.0).max(0.0));
        Ok((size, vec![origin; children.len()]))
    }

    pub fn paint(
        &self,
        phase: PaintPhase,
        rect: Rect,
        state: InteractionState,
        metrics: &crate::text::TextMetrics,
    ) -> Vec<Primitive> {
        if phase == PaintPhase::AfterChildren {
            return if state.focused && state.focus_visible {
                vec![Primitive::Border {
                    rect,
                    width: self.style.border_width + 1.0,
                    color: self.style.focus_ring,
                    corner_radius: self.style.radius,
                }]
            } else {
                Vec::new()
            };
        }
        let colors = self.style.resolve_state(&state);
        let backdrop = if state.selected {
            self.style.selected_backdrop
        } else if state.disabled {
            None
        } else if state.hovered {
            self.style.hovered_backdrop.or(self.style.normal_backdrop)
        } else {
            self.style.normal_backdrop
        };
        let mut primitives = backdrop.map_or_else(Vec::new, |backdrop| {
            backdrop
                .filter(self.style.radius)
                .paint_primitives(rect, metrics)
        });
        primitives.push(Primitive::Quad {
            rect,
            color: colors.background,
            corner_radius: self.style.radius,
        });
        // Zero border width means no outline, not the renderer's fill mode.
        if self.style.border_width > 0.0 && colors.border.a > 0.0 {
            primitives.push(Primitive::Border {
                rect,
                width: self.style.border_width,
                color: colors.border,
                corner_radius: self.style.radius,
            });
        }
        primitives
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_backdrop_is_opt_in_local_and_subordinate_to_selection_and_disabled() {
        use crate::{BackdropMaterial, BackdropRim, BackdropTone, ControlBackdrop, EdgeRefraction};
        let material = BackdropMaterial::new(
            4.0,
            -0.1,
            BackdropTone {
                offset: 0.0,
                slope: 1.0,
                quadratic: 0.0,
                saturation: 1.02,
            },
            BackdropRim {
                highlight: 0.3,
                shade: 0.22,
            },
        )
        .unwrap();
        let mut style = crate::Theme::default().button;
        let hover = ControlBackdrop::new(2.0, EdgeRefraction::default(), material).unwrap();
        let selected = ControlBackdrop::new(4.0, EdgeRefraction::default(), material).unwrap();
        style.hovered_backdrop = Some(hover);
        style.selected_backdrop = Some(selected);
        let surface = ControlSurface {
            style,
            centered: true,
        };
        let rect = Rect::from_min_size(Point::new(20.0, 30.0), Size::new(120.0, 36.0));
        let runtime = crate::runtime::Runtime::new();
        let metrics = runtime.text_metrics();
        for flags in 0..16 {
            let state = InteractionState {
                disabled: flags & 1 != 0,
                selected: flags & 2 != 0,
                hovered: flags & 4 != 0,
                pressed: flags & 8 != 0,
                ..Default::default()
            };
            let primitives = surface.paint(PaintPhase::BeforeChildren, rect, state, metrics);
            let effects: Vec<_> = primitives
                .iter()
                .filter_map(|primitive| match primitive {
                    Primitive::BackdropMaterial {
                        rect: actual,
                        sigma,
                        ..
                    } => {
                        assert_eq!(*actual, rect, "optics stay inside the individual control");
                        Some(*sigma)
                    }
                    _ => None,
                })
                .collect();
            let expected = if state.selected {
                vec![4.0]
            } else if state.hovered && !state.disabled {
                vec![2.0]
            } else {
                vec![]
            };
            assert_eq!(effects, expected);
        }
        let flat = ControlSurface {
            style: crate::Theme::default().button,
            centered: true,
        };
        let hovered = InteractionState {
            hovered: true,
            ..Default::default()
        };
        assert!(
            !flat
                .paint(PaintPhase::BeforeChildren, rect, hovered, metrics)
                .iter()
                .any(|p| matches!(p, Primitive::BackdropMaterial { .. })),
            "generic defaults stay flat"
        );
    }

    #[test]
    fn normal_backdrop_is_opt_in_and_respects_state_precedence() {
        use crate::{BackdropMaterial, BackdropRim, BackdropTone, ControlBackdrop, EdgeRefraction};
        let material = BackdropMaterial::new(
            4.0,
            -0.1,
            BackdropTone {
                offset: 0.0,
                slope: 1.0,
                quadratic: 0.0,
                saturation: 1.02,
            },
            BackdropRim {
                highlight: 0.3,
                shade: 0.22,
            },
        )
        .unwrap();
        let mut style = crate::Theme::default().button;
        assert!(
            style.normal_backdrop.is_none(),
            "generic defaults stay flat"
        );
        let normal = ControlBackdrop::new(1.5, EdgeRefraction::default(), material).unwrap();
        let hover = ControlBackdrop::new(2.5, EdgeRefraction::default(), material).unwrap();
        let selected = ControlBackdrop::new(4.0, EdgeRefraction::default(), material).unwrap();
        style.normal_backdrop = Some(normal);
        style.hovered_backdrop = Some(hover);
        style.selected_backdrop = Some(selected);
        let surface = ControlSurface {
            style: style.clone(),
            centered: true,
        };
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 32.0));
        let runtime = crate::runtime::Runtime::new();
        let metrics = runtime.text_metrics();
        for flags in 0..16 {
            let state = InteractionState {
                disabled: flags & 1 != 0,
                selected: flags & 2 != 0,
                hovered: flags & 4 != 0,
                pressed: flags & 8 != 0,
                ..Default::default()
            };
            let sigmas: Vec<_> = surface
                .paint(PaintPhase::BeforeChildren, rect, state, metrics)
                .into_iter()
                .filter_map(|p| match p {
                    Primitive::BackdropMaterial { sigma, .. } => Some(sigma),
                    _ => None,
                })
                .collect();
            let expected = if state.selected {
                vec![4.0]
            } else if state.disabled {
                vec![]
            } else if state.hovered {
                vec![2.5]
            } else {
                vec![1.5]
            };
            assert_eq!(sigmas, expected);
        }

        // Without hovered_backdrop, hovering falls back to normal_backdrop.
        style.hovered_backdrop = None;
        let fallback_surface = ControlSurface {
            style,
            centered: true,
        };
        let hovered_sigmas: Vec<_> = fallback_surface
            .paint(
                PaintPhase::BeforeChildren,
                rect,
                InteractionState {
                    hovered: true,
                    ..Default::default()
                },
                metrics,
            )
            .into_iter()
            .filter_map(|p| match p {
                Primitive::BackdropMaterial { sigma, .. } => Some(sigma),
                _ => None,
            })
            .collect();
        assert_eq!(hovered_sigmas, vec![1.5]);
    }

    #[test]
    fn valid_scalars_overflow_only_when_combined_even_under_parent_clamps_or_unbounded_maxima() {
        let mut style = crate::Theme::default().button;
        style.horizontal_padding = f32::MAX;
        assert!(style.validate().is_ok());
        let surface = ControlSurface {
            style,
            centered: true,
        };
        for constraints in [
            BoxConstraints::tight(Size::new(1.0, 1.0)),
            BoxConstraints::loose(Size::new(100.0, 50.0)),
            BoxConstraints::loose(Size::new(800.0, 600.0)),
            BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
        ] {
            assert_eq!(
                surface.layout(constraints, &[Size::new(10.0, 5.0)]),
                Err(LayoutError::InvalidGeometry)
            );
        }

        let mut style = crate::Theme::default().button;
        style.horizontal_padding = 4.0;
        style.min_height = f32::MAX;
        let surface = ControlSurface {
            style,
            centered: false,
        };
        let (size, _) = surface
            .layout(
                BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)),
                &[Size::new(10.0, 5.0)],
            )
            .unwrap();
        assert_eq!(size, Size::new(18.0, f32::MAX));
        let (size, _) = surface
            .layout(
                BoxConstraints::tight(Size::new(8.0, 6.0)),
                &[Size::new(0.0, 0.0)],
            )
            .unwrap();
        assert_eq!(size, Size::new(8.0, 6.0));
    }
}
