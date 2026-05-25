//! Fixed-size tiled surface. For testing and simple use cases.
//! Corresponds to mypaint-fixed-tiled-surface.c.

use crate::surface::tile::TiledSurface;

/// A fixed-size canvas backed by a TiledSurface.
pub struct FixedSurface {
    tiled: TiledSurface,
}

impl FixedSurface {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            tiled: TiledSurface::new(width, height),
        }
    }

    pub fn width(&self) -> usize { self.tiled.width() }
    pub fn height(&self) -> usize { self.tiled.height() }

    pub fn get_pixel(&self, x: usize, y: usize) -> Option<(u16, u16, u16, u16)> {
        if x >= self.tiled.width() || y >= self.tiled.height() {
            return None;
        }
        // Access via TiledSurface's pixel buffer — not public yet
        // For now, use get_color
        None
    }
}

impl std::ops::Deref for FixedSurface {
    type Target = TiledSurface;
    fn deref(&self) -> &Self::Target { &self.tiled }
}

impl std::ops::DerefMut for FixedSurface {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.tiled }
}
