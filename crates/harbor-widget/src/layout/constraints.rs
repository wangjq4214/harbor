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

    /// Returns true if min == max (a single valid size).
    pub fn is_tight(&self) -> bool {
        self.min == self.max
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
    fn zero_size_constraints() {
        let c = BoxConstraints::tight(Size::ZERO);
        assert!(c.is_tight());
        assert_eq!(c.constrain(Size::new(100.0, 50.0)), Size::ZERO);
    }
}
