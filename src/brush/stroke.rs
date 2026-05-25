//! stroke_to core algorithm — TODO: full implementation (Task 9)
//! Corresponds to mypaint-brush.c:708-1547.

use crate::brush::Brush;
use crate::surface::Surface;
use crate::render::DabParams;

impl Brush {
    /// Main stroke entry point.
    /// Corresponds to `mypaint_brush_stroke_to` (L1300-1547).
    pub fn stroke_to(&mut self, surface: &mut dyn Surface,
        x: f32, y: f32, pressure: f32,
        xtilt: f32, ytilt: f32,
        dtime: f64, viewzoom: f32, viewrotation: f32,
        barrel_rotation: f32, linear: bool) -> bool
    {
        todo!("stroke_to: Task 9")
    }
}
