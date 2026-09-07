use super::Size;

/// Parent-imposed min/max size bounds for layout.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BoxConstraints {
    pub min: Size,
    pub max: Size,
}

impl BoxConstraints {
    /// Creates tight constraints where min == max == size.
    pub fn tight(size: Size) -> Self {
        BoxConstraints {
            min: size,
            max: size,
        }
    }

    /// Creates loose constraints with min = ZERO and the given max.
    pub fn loose(max: Size) -> Self {
        BoxConstraints {
            min: Size::ZERO,
            max,
        }
    }

    /// Clamps the given size to fit within these constraints.
    ///
    /// When min > max in an axis, the min constraint wins (no panic).
    pub fn constrain(&self, size: Size) -> Size {
        fn clamp_axis(val: f32, min: f32, max: f32) -> f32 {
            if min > max {
                return min;
            }
            val.clamp(min, max)
        }
        Size::new(
            clamp_axis(size.width, self.min.width, self.max.width),
            clamp_axis(size.height, self.min.height, self.max.height),
        )
    }

    /// Fills bounded axes, using finite natural content (at least the minimum)
    /// on unbounded axes. Callers supply validated constraints and natural size.
    pub(crate) fn fill_bounded(&self, natural: Size) -> Size {
        let axis = |min: f32, max: f32, natural: f32| {
            if max.is_finite() {
                max
            } else {
                natural.max(min)
            }
        };
        Size::new(
            axis(self.min.width, self.max.width, natural.width),
            axis(self.min.height, self.max.height, natural.height),
        )
    }

    /// Validates layout bounds. Only maximum bounds may be positive infinity.
    pub fn validate(&self) -> Result<(), super::LayoutError> {
        let valid_axis =
            |min: f32, max: f32| min.is_finite() && min >= 0.0 && !max.is_nan() && max >= min;
        if valid_axis(self.min.width, self.max.width)
            && valid_axis(self.min.height, self.max.height)
        {
            Ok(())
        } else {
            Err(super::LayoutError::InvalidConstraints)
        }
    }

    /// Enforces these requested bounds within the authoritative parent interval.
    ///
    /// Unlike [`Self::constrain`], a local minimum never overrides the parent's
    /// maximum. Both intervals are validated before clamping; contradictory
    /// local bounds are errors rather than silently repaired.
    pub fn enforce(&self, parent: Self) -> Result<Self, super::LayoutError> {
        self.validate()?;
        parent.validate()?;
        Ok(Self {
            min: parent.constrain(self.min),
            max: parent.constrain(self.max),
        })
    }

    /// Returns constraints with `insets` removed from each axis, saturating at zero.
    pub fn deflate(&self, insets: Size) -> Self {
        let deflate_axis = |value: f32, inset: f32| (value - inset).max(0.0);
        BoxConstraints {
            min: Size::new(
                deflate_axis(self.min.width, insets.width),
                deflate_axis(self.min.height, insets.height),
            ),
            max: Size::new(
                deflate_axis(self.max.width, insets.width),
                deflate_axis(self.max.height, insets.height),
            ),
        }
    }

    /// Returns true if min == max (a single valid size).
    pub fn is_tight(&self) -> bool {
        self.min == self.max
    }
}

#[cfg(test)]
mod enforcement_tests {
    use super::*;
    use crate::layout::LayoutError;

    #[test]
    fn fill_bounded_uses_natural_extent_and_minimum_only_on_unbounded_axes() {
        let bounds = BoxConstraints {
            min: Size::new(5.0, 10.0),
            max: Size::new(100.0, f32::INFINITY),
        };
        assert_eq!(
            bounds.fill_bounded(Size::new(200.0, 7.0)),
            Size::new(100.0, 10.0)
        );
        assert_eq!(
            bounds.fill_bounded(Size::new(0.0, 20.0)),
            Size::new(100.0, 20.0)
        );
        let bounds = BoxConstraints {
            min: Size::new(10.0, 5.0),
            max: Size::new(f32::INFINITY, 100.0),
        };
        assert_eq!(
            bounds.fill_bounded(Size::new(20.0, 200.0)),
            Size::new(20.0, 100.0)
        );
    }

    #[test]
    fn parent_bounds_win_on_both_sides_without_changing_legacy_constrain() {
        let parent = BoxConstraints {
            min: Size::new(20.0, 30.0),
            max: Size::new(100.0, 90.0),
        };
        assert_eq!(
            BoxConstraints::tight(Size::new(200.0, 5.0))
                .enforce(parent)
                .unwrap(),
            BoxConstraints::tight(Size::new(100.0, 30.0))
        );
        let local = BoxConstraints {
            min: Size::new(200.0, 0.0),
            max: Size::new(100.0, 90.0),
        };
        assert_eq!(local.constrain(Size::ZERO).width, 200.0);
        assert_eq!(local.enforce(parent), Err(LayoutError::InvalidConstraints));
    }

    #[test]
    fn validation_rejects_invalid_bounds_but_allows_unbounded_maxima() {
        for value in [f32::NAN, -1.0, f32::NEG_INFINITY, f32::INFINITY] {
            let bounds = BoxConstraints {
                min: Size::new(value, 0.0),
                max: Size::new(f32::INFINITY, 10.0),
            };
            assert_eq!(bounds.validate(), Err(LayoutError::InvalidConstraints));
        }
        for value in [f32::NAN, -1.0, f32::NEG_INFINITY] {
            let bounds = BoxConstraints::loose(Size::new(10.0, value));
            assert_eq!(bounds.validate(), Err(LayoutError::InvalidConstraints));
        }
        assert_eq!(
            BoxConstraints::loose(Size::new(f32::INFINITY, f32::INFINITY)).validate(),
            Ok(())
        );
        let parent = BoxConstraints::tight(Size::new(0.0, 0.0));
        assert_eq!(
            BoxConstraints::tight(Size::new(200.0, 200.0)).enforce(parent),
            Ok(parent)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tight_constraints() {
        let c = BoxConstraints::tight(Size::new(100.0, 50.0));
        assert_eq!(c.min, Size::new(100.0, 50.0));
        assert_eq!(c.max, Size::new(100.0, 50.0));
        assert!(c.is_tight());
    }

    #[test]
    fn loose_constraints() {
        let c = BoxConstraints::loose(Size::new(200.0, 100.0));
        assert_eq!(c.min, Size::ZERO);
        assert_eq!(c.max, Size::new(200.0, 100.0));
        assert!(!c.is_tight());
    }

    #[test]
    fn constrain_within_bounds() {
        let c = BoxConstraints {
            min: Size::new(50.0, 25.0),
            max: Size::new(200.0, 100.0),
        };
        assert_eq!(c.constrain(Size::new(100.0, 50.0)), Size::new(100.0, 50.0));
    }

    #[test]
    fn constrain_clamp_to_min() {
        let c = BoxConstraints {
            min: Size::new(50.0, 25.0),
            max: Size::new(200.0, 100.0),
        };
        assert_eq!(c.constrain(Size::new(10.0, 10.0)), Size::new(50.0, 25.0));
    }

    #[test]
    fn constrain_clamp_to_max() {
        let c = BoxConstraints {
            min: Size::new(50.0, 25.0),
            max: Size::new(200.0, 100.0),
        };
        assert_eq!(
            c.constrain(Size::new(300.0, 200.0)),
            Size::new(200.0, 100.0)
        );
    }

    #[test]
    fn constrain_min_greater_than_max() {
        // min > max in one axis: clamp uses min (stronger constraint wins)
        let c = BoxConstraints {
            min: Size::new(200.0, 25.0),
            max: Size::new(100.0, 100.0),
        };
        let constrained = c.constrain(Size::new(50.0, 50.0));
        assert_eq!(constrained.width, 200.0); // clamped to min
        assert_eq!(constrained.height, 50.0); // within [25, 100]
    }

    #[test]
    fn deflate_reduces_each_bound() {
        let constraints = BoxConstraints {
            min: Size::new(50.0, 25.0),
            max: Size::new(100.0, 80.0),
        };

        assert_eq!(
            constraints.deflate(Size::new(32.0, 32.0)),
            BoxConstraints {
                min: Size::new(18.0, 0.0),
                max: Size::new(68.0, 48.0),
            }
        );
    }

    #[test]
    fn should_deflate_loose_constraints_when_insets_are_asymmetric() {
        // Arrange
        let constraints = BoxConstraints::loose(Size::new(100.0, 80.0));

        // Act
        let deflated = constraints.deflate(Size::new(32.0, 12.0));

        // Assert
        assert_eq!(deflated, BoxConstraints::loose(Size::new(68.0, 68.0)));
    }

    #[test]
    fn should_preserve_tight_constraints_when_insets_fit() {
        // Arrange
        let constraints = BoxConstraints::tight(Size::new(100.0, 80.0));

        // Act
        let deflated = constraints.deflate(Size::new(32.0, 32.0));

        // Assert
        assert_eq!(deflated, BoxConstraints::tight(Size::new(68.0, 48.0)));
        assert!(deflated.is_tight());
    }

    #[test]
    fn should_saturate_exhausted_bounds_when_insets_exceed_tight_constraints() {
        // Arrange
        let constraints = BoxConstraints::tight(Size::new(30.0, 20.0));

        // Act
        let deflated = constraints.deflate(Size::new(32.0, 32.0));

        // Assert
        assert_eq!(deflated, BoxConstraints::tight(Size::ZERO));
    }

    #[test]
    fn zero_size_constraints() {
        let c = BoxConstraints::tight(Size::ZERO);
        assert!(c.is_tight());
        assert_eq!(c.constrain(Size::new(100.0, 50.0)), Size::ZERO);
    }
}
