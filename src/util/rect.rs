/// A rectangle in integer coordinates.
/// Corresponds to MyPaintRectangle in mypaint-rectangle.c/h.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    /// Expand this rectangle to include the given point.
    /// Corresponds to `mypaint_rectangle_expand_to_include_point`.
    pub fn expand_to_include_point(&mut self, x: i32, y: i32) {
        if x < self.x {
            self.width += self.x - x;
            self.x = x;
        }
        if y < self.y {
            self.height += self.y - y;
            self.y = y;
        }
        if x >= self.x + self.width {
            self.width = x - self.x + 1;
        }
        if y >= self.y + self.height {
            self.height = y - self.y + 1;
        }
    }

    /// Expand this rectangle to include another rectangle.
    /// Corresponds to `mypaint_rectangle_expand_to_include_rect`.
    pub fn expand_to_include_rect(&mut self, other: &Rect) {
        self.expand_to_include_point(other.x, other.y);
        self.expand_to_include_point(other.x + other.width - 1, other.y + other.height - 1);
    }
}

/// A collection of rectangles.
/// Corresponds to MyPaintRectangles.
#[derive(Debug, Clone, Default)]
pub struct Rectangles {
    pub rects: Vec<Rect>,
}
