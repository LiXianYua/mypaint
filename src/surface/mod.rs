pub mod fixed;
pub mod operations;
pub mod tile;

use crate::render::DabParams;
use crate::util::rect::Rectangles;
use std::path::Path;

/// Abstract surface trait for the brush engine.
/// Replaces the C vtable struct MyPaintSurface.
pub trait Surface {
    /// Draw a dab onto the surface. Returns true if pixels were modified.
    fn draw_dab(&mut self, params: &DabParams) -> bool;

    /// Get color at a position. Returns (r, g, b, a) in [0, 1].
    fn get_color(&mut self, x: f32, y: f32, radius: f32, paint: f32) -> (f32, f32, f32, f32);

    /// Begin an atomic rendering section.
    fn begin_atomic(&mut self);

    /// End an atomic rendering section, returning affected rectangles.
    fn end_atomic(&mut self) -> Rectangles;

    /// Save a region to PNG.
    fn save_png(&mut self, path: &Path, x: i32, y: i32, width: i32, height: i32);
}
