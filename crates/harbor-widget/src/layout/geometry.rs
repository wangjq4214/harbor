/// A 2D position in logical pixels, with top-left origin.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Point { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// A 2D extent in logical pixels.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Size {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }

    /// Returns true if either dimension is <= 0.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// An axis-aligned rectangle with min/max representation.
///
/// `min` is the top-left corner, `max` is the bottom-right corner.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

impl Rect {
    pub fn from_min_size(min: Point, size: Size) -> Self {
        Rect {
            min,
            max: Point::new(min.x + size.width, min.y + size.height),
        }
    }

    pub fn size(&self) -> Size {
        Size::new(self.max.x - self.min.x, self.max.y - self.min.y)
    }

    /// Returns true if the point is inside the rect (including left/top edges,
    /// excluding right/bottom edges).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x < self.max.x && p.y >= self.min.y && p.y < self.max.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_zero() {
        assert_eq!(Point::ZERO, Point::new(0.0, 0.0));
    }

    #[test]
    fn size_is_empty() {
        assert!(Size::ZERO.is_empty());
        assert!(Size::new(0.0, 5.0).is_empty());
        assert!(Size::new(5.0, 0.0).is_empty());
        assert!(!Size::new(5.0, 5.0).is_empty());
    }

    #[test]
    fn size_negative_dimension_is_empty() {
        assert!(Size::new(-1.0, 5.0).is_empty());
        assert!(Size::new(5.0, -1.0).is_empty());
    }

    #[test]
    fn rect_from_min_size() {
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 50.0));
        assert_eq!(rect.min, Point::new(10.0, 20.0));
        assert_eq!(rect.max, Point::new(110.0, 70.0));
        assert_eq!(rect.size(), Size::new(100.0, 50.0));
    }

    #[test]
    fn rect_contains_inside() {
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        assert!(rect.contains(Point::new(0.0, 0.0)));
        assert!(rect.contains(Point::new(50.0, 25.0)));
        assert!(rect.contains(Point::new(99.0, 49.0)));
    }

    #[test]
    fn rect_contains_boundary_and_outside() {
        let rect = Rect::from_min_size(Point::ZERO, Size::new(100.0, 50.0));
        // Left/top edges inclusive
        assert!(rect.contains(Point::new(0.0, 0.0)));
        // Right/bottom edges exclusive
        assert!(!rect.contains(Point::new(100.0, 50.0)));
        assert!(!rect.contains(Point::new(100.0, 25.0)));
        assert!(!rect.contains(Point::new(50.0, 50.0)));
        // Outside
        assert!(!rect.contains(Point::new(-1.0, 0.0)));
        assert!(!rect.contains(Point::new(101.0, 0.0)));
    }

    #[test]
    fn rect_zero_size() {
        let rect = Rect::from_min_size(Point::ZERO, Size::ZERO);
        assert_eq!(rect.min, rect.max);
        assert_eq!(rect.size(), Size::ZERO);
        // Zero-size rect contains nothing
        assert!(!rect.contains(Point::ZERO));
    }

    #[test]
    fn point_at_boundary() {
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(20.0, 20.0));
        // Top-left boundary inclusive
        assert!(rect.contains(Point::new(10.0, 10.0)));
        // Bottom-right boundary exclusive
        assert!(!rect.contains(Point::new(30.0, 30.0)));
    }
}
