/// 2D affine transform (3x3 matrix, stored as 6 elements).
/// Corresponds to MyPaintTransform in mypaint-matrix.c/h.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub xx: f32,
    pub xy: f32,
    pub yx: f32,
    pub yy: f32,
    pub x0: f32,
    pub y0: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            xx: 1.0,
            xy: 0.0,
            yx: 0.0,
            yy: 1.0,
            x0: 0.0,
            y0: 0.0,
        }
    }

    pub fn multiply(&self, other: &Transform) -> Transform {
        Transform {
            xx: self.xx * other.xx + self.xy * other.yx,
            xy: self.xx * other.xy + self.xy * other.yy,
            yx: self.yx * other.xx + self.yy * other.yx,
            yy: self.yx * other.xy + self.yy * other.yy,
            x0: self.x0 * other.xx + self.y0 * other.yx + other.x0,
            y0: self.x0 * other.xy + self.y0 * other.yy + other.y0,
        }
    }

    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.xx * x + self.xy * y + self.x0,
            self.yx * x + self.yy * y + self.y0,
        )
    }
}
