// ── CrossAxisAlignment ────────────────────────────────────────────────────────────────

/// Cross-axis alignment for layout containers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    Center,
    End,
    Stretch,
}

impl CrossAxisAlignment {
    /// Returns the offset for a child of the given size within the available
    /// space. For Stretch, returns 0.0 (stretch is handled by constraints,
    /// not positioning).
    pub fn position(&self, child_size: f32, available: f32) -> f32 {
        match self {
            CrossAxisAlignment::Start => 0.0,
            CrossAxisAlignment::Center => (available - child_size).max(0.0) * 0.5,
            CrossAxisAlignment::End => (available - child_size).max(0.0),
            CrossAxisAlignment::Stretch => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_position() {
        assert_eq!(CrossAxisAlignment::Start.position(50.0, 200.0), 0.0);
    }

    #[test]
    fn center_position() {
        assert_eq!(CrossAxisAlignment::Center.position(50.0, 200.0), 75.0);
    }

    #[test]
    fn center_with_child_larger_than_available() {
        // Child larger than available — offset clamped to 0
        assert_eq!(CrossAxisAlignment::Center.position(300.0, 200.0), 0.0);
    }

    #[test]
    fn end_position() {
        assert_eq!(CrossAxisAlignment::End.position(50.0, 200.0), 150.0);
    }

    #[test]
    fn end_with_child_larger_than_available() {
        assert_eq!(CrossAxisAlignment::End.position(300.0, 200.0), 0.0);
    }

    #[test]
    fn stretch_position() {
        assert_eq!(CrossAxisAlignment::Stretch.position(50.0, 200.0), 0.0);
    }
}
