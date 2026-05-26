//! Dab geometry: shape (calculate_rr) + opacity falloff (calculate_opa).
//! 对应 mypaint-tiled-surface.c:237-373。

/// Parameters for drawing a single dab.
/// 汇总 C `draw_dab` 的 15 个参数。
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

/// Pre-computed mask falloff parameters.
/// 对应 mypaint-tiled-surface.c:408-411。
#[derive(Debug, Clone, Copy)]
pub struct MaskParams {
    pub hardness: f32,
    pub segment1_offset: f32,
    pub segment1_slope: f32,
    pub segment2_offset: f32,
    pub segment2_slope: f32,
}

impl MaskParams {
    pub fn from_hardness_softness(hardness: f32, softness: f32) -> Self {
        let hardness = hardness.clamp(0.0, 1.0);
        let one_minus_softness = 1.0 - softness;
        // segment1 covers rr in [0, hardness]
        // segment2 covers rr in [hardness, 1.0]
        let seg1_off = one_minus_softness;
        let seg1_slope = -(1.0 / hardness - 1.0) * one_minus_softness;
        let seg2_off = hardness / (1.0 - hardness) * one_minus_softness;
        let seg2_slope = -hardness / (1.0 - hardness) * one_minus_softness;
        Self {
            hardness,
            segment1_offset: seg1_off,
            segment1_slope: seg1_slope,
            segment2_offset: seg2_off,
            segment2_slope: seg2_slope,
        }
    }
}

/// rr = squared normalized distance from center (with aspect ratio + rotation).
/// 对应 mypaint-tiled-surface.c:237-249 的 calculate_rr。
///
/// 参数：
/// - `(xp, yp)`: 像素整数坐标
/// - `(x, y)`: dab 中心
/// - `aspect_ratio`: 椭圆长宽比 (≥1.0)
/// - `(sn, cs)`: angle 的 sin/cos（弧度）
/// - `one_over_radius2`: 1/(radius²)
#[inline]
pub fn calculate_rr(
    xp: i32, yp: i32, x: f32, y: f32, aspect_ratio: f32,
    sn: f32, cs: f32, one_over_radius2: f32,
) -> f32 {
    let yy = (yp as f32) + 0.5 - y;
    let xx = (xp as f32) + 0.5 - x;
    let yyr = (yy * cs - xx * sn) * aspect_ratio;
    let xxr = yy * sn + xx * cs;
    (yyr * yyr + xxr * xxr) * one_over_radius2
}

/// 计算一个像素点的不透明度（在 0..1）。
/// 对应 mypaint-tiled-surface.c:357-373 的 calculate_opa。
#[inline]
pub fn calculate_opa(rr: f32, params: &MaskParams) -> f32 {
    if rr > 1.0 {
        return 0.0;
    }
    let (offset, slope) = if rr <= params.hardness {
        (params.segment1_offset, params.segment1_slope)
    } else {
        (params.segment2_offset, params.segment2_slope)
    };
    offset + rr * slope
}

/// 计算 r²（不除以 radius²）— C 版 calculate_r_sample。
#[inline]
fn calculate_r_sample(x: f32, y: f32, aspect_ratio: f32, sn: f32, cs: f32) -> f32 {
    let yyr = (y * cs - x * sn) * aspect_ratio;
    let xxr = y * sn + x * cs;
    yyr * yyr + xxr * xxr
}

#[inline]
fn sign_point_in_line(px: f32, py: f32, vx: f32, vy: f32) -> f32 {
    (px - vx) * (-vy) - vx * (py - vy)
}

#[inline]
fn closest_point_to_line(lx: f32, ly: f32, px: f32, py: f32) -> (f32, f32) {
    let l2 = lx * lx + ly * ly;
    let ltp_dot = px * lx + py * ly;
    let t = ltp_dot / l2;
    (lx * t, ly * t)
}

/// Antialiased rr for small radii (radius < 3).
/// 对应 mypaint-tiled-surface.c:277-354 calculate_rr_antialiased。
#[inline]
pub fn calculate_rr_antialiased(
    xp: i32, yp: i32, x: f32, y: f32, aspect_ratio: f32,
    sn: f32, cs: f32, one_over_radius2: f32,
    r_aa_start: f32,
) -> f32 {
    let pixel_right = x - xp as f32;
    let pixel_bottom = y - yp as f32;
    let pixel_center_x = pixel_right - 0.5;
    let pixel_center_y = pixel_bottom - 0.5;
    let pixel_left = pixel_right - 1.0;
    let pixel_top = pixel_bottom - 1.0;

    let (nearest_x, nearest_y, r_near, rr_near) = if pixel_left < 0.0 && pixel_right > 0.0
        && pixel_top < 0.0 && pixel_bottom > 0.0
    {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        let (mut nx, mut ny) = closest_point_to_line(cs, sn, pixel_center_x, pixel_center_y);
        nx = nx.clamp(pixel_left, pixel_right);
        ny = ny.clamp(pixel_top, pixel_bottom);
        let r = calculate_r_sample(nx, ny, aspect_ratio, sn, cs);
        (nx, ny, r, r * one_over_radius2)
    };

    if rr_near > 1.0 {
        return rr_near;
    }

    let center_sign = sign_point_in_line(pixel_center_x, pixel_center_y, cs, -sn);
    let rad_area_1 = (1.0 / std::f32::consts::PI).sqrt();

    let (farthest_x, farthest_y) = if center_sign < 0.0 {
        (nearest_x - sn * rad_area_1, nearest_y + cs * rad_area_1)
    } else {
        (nearest_x + sn * rad_area_1, nearest_y - cs * rad_area_1)
    };

    let r_far = calculate_r_sample(farthest_x, farthest_y, aspect_ratio, sn, cs);
    let rr_far = r_far * one_over_radius2;

    if r_far < r_aa_start {
        return (rr_far + rr_near) * 0.5;
    }

    let visibility_near = 1.0 - rr_near;
    let delta = rr_far - rr_near;
    let delta2 = 1.0 + delta;
    1.0 - visibility_near / delta2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_pixel_has_full_opacity() {
        let mp = MaskParams::from_hardness_softness(0.8, 0.0);
        // At center (rr=0), opa = segment1_offset = 1.0
        assert!((calculate_opa(0.0, &mp) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn edge_pixel_has_zero_opacity() {
        let mp = MaskParams::from_hardness_softness(0.8, 0.0);
        // At edge (rr=1), opa = segment2_offset + 1*segment2_slope
        let v = calculate_opa(1.0, &mp);
        assert!(v.abs() < 1e-6, "expected 0, got {v}");
    }

    #[test]
    fn outside_dab_is_zero() {
        let mp = MaskParams::from_hardness_softness(0.5, 0.0);
        assert_eq!(calculate_opa(1.5, &mp), 0.0);
    }

    #[test]
    fn rr_center_zero() {
        // pixel at exactly center of dab → rr ≈ 0
        let rr = calculate_rr(10, 10, 10.5, 10.5, 1.0, 0.0, 1.0, 1.0 / 25.0);
        assert!(rr.abs() < 1e-6);
    }
}
