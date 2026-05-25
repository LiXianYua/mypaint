use crate::render::blend::{blend_pixel_normal, blend_pixel_eraser, blend_pixel_lock_alpha, blend_pixel_colorize, blend_pixel_posterize};

/// Parameters for drawing a single dab.
/// Aggregates the 15 parameters of the C `draw_dab` function.
#[derive(Debug, Clone, Copy)]
pub struct DabParams {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub opaque: f32,
    pub hardness: f32,
    pub softness: f32,
    pub alpha_eraser: f32,
    pub aspect_ratio: f32,
    pub angle: f32,
    pub lock_alpha: f32,
    pub colorize: f32,
    pub posterize: f32,
    pub posterize_num: f32,
    pub paint: f32,
}

/// Calculate the squared distance from center, accounting for elliptical dabs.
/// Corresponds to `calculate_rr` in mypaint-tiled-surface.c.
#[inline]
pub fn calculate_rr(dx: f32, dy: f32, aspect_ratio: f32, angle: f32) -> f32 {
    if aspect_ratio <= 1.0 {
        dx * dx + dy * dy
    } else {
        let angle_rad = angle * std::f32::consts::PI / 180.0;
        let cs = angle_rad.cos();
        let sn = angle_rad.sin();
        let yyr = (dy * cs - dx * sn) * aspect_ratio;
        let xxr = dy * sn + dx * cs;
        yyr * yyr + xxr * xxr
    }
}

/// Compute the dab mask value at a given distance from center.
/// Returns a value in [0, SCALE] (SCALE = 2^15).
#[inline]
pub fn dab_mask_value_scaled(rr: f32, radius: f32, hardness: f32, softness: f32) -> u16 {
    let r = rr.sqrt();
    let norm_r = r / radius;
    if norm_r >= 1.0 {
        return 0;
    }
    let hard_edge = hardness;
    let soft_edge = hardness + (1.0 - hardness) * softness;
    let val = if norm_r <= hard_edge {
        1.0
    } else if norm_r >= soft_edge || soft_edge == hard_edge {
        0.0
    } else {
        1.0 - (norm_r - hard_edge) / (soft_edge - hard_edge)
    };
    (val * 32768.0) as u16
}
