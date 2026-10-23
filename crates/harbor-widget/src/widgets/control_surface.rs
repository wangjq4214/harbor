//! Shared control chrome. Interaction stays in InteractiveRegion; content measures itself.
use super::interactive_region::InteractionState;
use crate::layout::{BoxConstraints, Point, Rect, Size};
use crate::scene::primitive::Primitive;
use crate::theme::ControlStyle;
use crate::view::PaintPhase;

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

    pub fn layout(&self, constraints: BoxConstraints, children: &[Size]) -> (Size, Vec<Point>) {
        let child = children.first().copied().unwrap_or(Size::ZERO);
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
        (size, vec![origin; children.len()])
    }

    pub fn paint(&self, phase: PaintPhase, rect: Rect, state: InteractionState) -> Vec<Primitive> {
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
        let mut primitives = vec![Primitive::Quad {
            rect,
            color: colors.background,
            corner_radius: self.style.radius,
        }];
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
