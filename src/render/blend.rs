//! Per-pixel blending modes.
//! 对应 brushmodes.c。每个函数实现 C 版函数内层循环的一次迭代。
//!
//! 像素格式: u16 RGBA premultiplied alpha, 范围 0..=2^15 (32768)。

use crate::smudge::{rgb_to_spectral, spectral_to_rgb};

const SCALE: u32 = 1 << 15;

// LUMA coefficients matching brushmodes.c:180-182
const LUMA_RED: f32 = 0.2126;
const LUMA_GREEN: f32 = 0.7152;
const LUMA_BLUE: f32 = 0.0722;

#[inline]
fn luma_u16(r: u16, g: u16, b: u16) -> i32 {
    // returns scaled luma (already divided by 1<<15 in the C macro context)
    (r as f32 * LUMA_RED + g as f32 * LUMA_GREEN + b as f32 * LUMA_BLUE) as i32
}

/// 标准 Normal 模式 (premultiplied alpha over)。
/// 对应 draw_dab_pixels_BlendMode_Normal 单个像素的迭代。
#[inline]
pub fn blend_pixel_normal(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16,
    opacity: u16,
) {
    let opa_a = (mask_val as u32 * opacity as u32) / SCALE; // topAlpha
    let opa_b = SCALE - opa_a;                              // bottomAlpha
    px[3] = (opa_a + opa_b * px[3] as u32 / SCALE) as u16;
    px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
    px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
    px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
}

/// Lock Alpha mode：opa_a 被画布 alpha 调制，alpha 不变。
/// 对应 draw_dab_pixels_BlendMode_LockAlpha。
#[inline]
pub fn blend_pixel_lock_alpha(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16,
    opacity: u16,
) {
    let opa_a_top = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a_top;
    let opa_a = opa_a_top * px[3] as u32 / SCALE; // 受底色 alpha 调制
    px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
    px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
    px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
    // px[3] 不修改
}

/// Normal + Eraser mode：opa_a 被 color_a 调制，alpha 也被擦除。
/// 对应 draw_dab_pixels_BlendMode_Normal_and_Eraser。
#[inline]
pub fn blend_pixel_normal_eraser(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16, color_a: u16,
    opacity: u16,
) {
    let opa_a_raw = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a_raw;
    let opa_a = opa_a_raw * color_a as u32 / SCALE;
    px[3] = (opa_a + opa_b * px[3] as u32 / SCALE) as u16;
    px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
    px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
    px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
}

/// Color blend mode：保留底色 luminance，应用顶色 hue/saturation。
/// 对应 draw_dab_pixels_BlendMode_Color。
#[inline]
pub fn blend_pixel_color(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16,
    opacity: u16,
) {
    // De-premultiply bottom
    let a = px[3];
    let (mut r, mut g, mut b) = (0u16, 0u16, 0u16);
    if a != 0 {
        r = ((SCALE * px[0] as u32) / a as u32) as u16;
        g = ((SCALE * px[1] as u32) / a as u32) as u16;
        b = ((SCALE * px[2] as u32) / a as u32) as u16;
    }
    set_rgb16_lum_from_rgb16(color_r, color_g, color_b, &mut r, &mut g, &mut b);
    // Re-premultiply
    r = ((r as u32 * a as u32) / SCALE) as u16;
    g = ((g as u32 * a as u32) / SCALE) as u16;
    b = ((b as u32 * a as u32) / SCALE) as u16;
    // Blend as normal (no alpha change)
    let opa_a = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a;
    px[0] = ((opa_a * r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
    px[1] = ((opa_a * g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
    px[2] = ((opa_a * b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
}

/// SetLum + ClipColor，对应 brushmodes.c:199-241 的 set_rgb16_lum_from_rgb16。
#[inline]
fn set_rgb16_lum_from_rgb16(
    topr: u16, topg: u16, topb: u16,
    botr: &mut u16, botg: &mut u16, botb: &mut u16,
) {
    let botlum = luma_u16(*botr, *botg, *botb);
    let toplum = luma_u16(topr, topg, topb);
    let diff = botlum - toplum;
    let mut r = topr as i32 + diff;
    let mut g = topg as i32 + diff;
    let mut b = topb as i32 + diff;

    // ClipColor
    let lum = (r as f32 * LUMA_RED + g as f32 * LUMA_GREEN + b as f32 * LUMA_BLUE) as i32;
    let cmin = r.min(g).min(b);
    let cmax = r.max(g).max(b);
    let scale = SCALE as i32;
    if cmin < 0 && (lum - cmin) != 0 {
        r = lum + ((r - lum) * lum) / (lum - cmin);
        g = lum + ((g - lum) * lum) / (lum - cmin);
        b = lum + ((b - lum) * lum) / (lum - cmin);
    }
    if cmax > scale && (cmax - lum) != 0 {
        r = lum + ((r - lum) * (scale - lum)) / (cmax - lum);
        g = lum + ((g - lum) * (scale - lum)) / (cmax - lum);
        b = lum + ((b - lum) * (scale - lum)) / (cmax - lum);
    }
    *botr = r.clamp(0, scale) as u16;
    *botg = g.clamp(0, scale) as u16;
    *botb = b.clamp(0, scale) as u16;
}

/// Posterize blend (in-place on canvas). 对应 draw_dab_pixels_BlendMode_Posterize。
/// 注意：alpha 不修改；RGB 先 posterize 再按 mask*opacity 混合回画布。
#[inline]
pub fn blend_pixel_posterize(
    px: &mut [u16; 4], mask_val: u16,
    opacity: u16, posterize_num: u16,
) {
    // Posterize the canvas RGB
    let pn = posterize_num as f32;
    if pn == 0.0 { return; }
    let r = px[0] as f32 / SCALE as f32;
    let g = px[1] as f32 / SCALE as f32;
    let b = px[2] as f32 / SCALE as f32;
    let post_r = (SCALE as f32 * (r * pn).round() / pn) as u32;
    let post_g = (SCALE as f32 * (g * pn).round() / pn) as u32;
    let post_b = (SCALE as f32 * (b * pn).round() / pn) as u32;

    let opa_a = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a;
    px[0] = ((opa_a * post_r + opa_b * px[0] as u32) / SCALE) as u16;
    px[1] = ((opa_a * post_g + opa_b * px[1] as u32) / SCALE) as u16;
    px[2] = ((opa_a * post_b + opa_b * px[2] as u32) / SCALE) as u16;
}

// =============================================================================
// Spectral (_Paint) variants — translated from brushmodes.c:73-130, 333-414, 443-489
// =============================================================================

/// Sigmoid blending factor for spectral compositing.
/// 对应 brushmodes.c:spectral_blend_factor。
#[inline]
fn spectral_blend_factor(x: f32) -> f32 {
    const VER_FAC: f32 = 1.65;
    const HOR_FAC: f32 = 8.0;
    const HOR_OFFS: f32 = 3.0;
    let b = x * HOR_FAC - HOR_OFFS;
    0.5 + b / (1.0 + b.abs() * VER_FAC)
}

/// Spectral Normal + Eraser blend (paint mode > 0)。
/// 对应 draw_dab_pixels_BlendMode_Normal_and_Eraser_Paint 单像素迭代。
#[inline]
pub fn blend_pixel_normal_eraser_paint(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16, color_a: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    let opa_a = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a;
    let opa_a2 = opa_a * color_a as u32 / SCALE;
    let opa_out = opa_a2 + opa_b * px[3] as u32 / SCALE;

    let mut rgb = [0u32; 3];

    let spectral_factor =
        spectral_blend_factor(px[3] as f32 / SCALE as f32).clamp(0.0, 1.0);
    let additive_factor = 1.0 - spectral_factor;

    if additive_factor != 0.0 {
        rgb[0] = (opa_a2 * color_r as u32 + opa_b * px[0] as u32) / SCALE;
        rgb[1] = (opa_a2 * color_g as u32 + opa_b * px[1] as u32) / SCALE;
        rgb[2] = (opa_a2 * color_b as u32 + opa_b * px[2] as u32) / SCALE;
    }

    if spectral_factor != 0.0 && px[3] != 0 {
        let spectral_b = rgb_to_spectral(
            px[0] as f32 / px[3] as f32,
            px[1] as f32 / px[3] as f32,
            px[2] as f32 / px[3] as f32);
        let denom = opa_a as f32 + opa_b as f32 * px[3] as f32 / SCALE as f32;
        let mut fac_a = if denom > 0.0 { opa_a as f32 / denom } else { 0.0 };
        fac_a *= color_a as f32 / SCALE as f32;
        let fac_b = 1.0 - fac_a;

        let mut spectral_result = [0.0f32; 10];
        for i in 0..10 {
            spectral_result[i] = spectral_a[i].powf(fac_a) * spectral_b[i].powf(fac_b);
        }
        let (sr, sg, sb) = spectral_to_rgb(&spectral_result);
        let sr_out = (additive_factor * rgb[0] as f32 + spectral_factor * sr * opa_out as f32) as u32;
        let sg_out = (additive_factor * rgb[1] as f32 + spectral_factor * sg * opa_out as f32) as u32;
        let sb_out = (additive_factor * rgb[2] as f32 + spectral_factor * sb * opa_out as f32) as u32;
        rgb = [sr_out, sg_out, sb_out];
    }

    px[3] = opa_out as u16;
    px[0] = rgb[0] as u16;
    px[1] = rgb[1] as u16;
    px[2] = rgb[2] as u16;
}

/// Spectral Lock Alpha + Paint blend.
/// 对应 draw_dab_pixels_BlendMode_LockAlpha_Paint。
#[inline]
pub fn blend_pixel_lock_alpha_paint(
    px: &mut [u16; 4], mask_val: u16,
    _color_r: u16, _color_g: u16, _color_b: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    // C 版强制 opacity 至少 150 (4.6e-3) 避免低 opacity 取整误差
    let opacity = opacity.max(150);
    let opa_a_raw = (mask_val as u32 * opacity as u32) / SCALE;
    let opa_b = SCALE - opa_a_raw;
    let opa_a = opa_a_raw * px[3] as u32 / SCALE;
    if px[3] == 0 {
        // 与 _Paint normal 版本一致：alpha 为 0 时退化到 additive
        // 但 LockAlpha 不修改 alpha，故只更新 RGB
        px[0] = ((opa_a * _color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * _color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * _color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
        return;
    }
    let denom = opa_a as f32 + opa_b as f32 * px[3] as f32 / SCALE as f32;
    let fac_a = if denom > 0.0 { opa_a as f32 / denom } else { 0.0 };
    let fac_b = 1.0 - fac_a;

    let spectral_b = rgb_to_spectral(
        px[0] as f32 / px[3] as f32,
        px[1] as f32 / px[3] as f32,
        px[2] as f32 / px[3] as f32);
    let mut spectral_result = [0.0f32; 10];
    for i in 0..10 {
        spectral_result[i] = spectral_a[i].powf(fac_a) * spectral_b[i].powf(fac_b);
    }
    let (sr, sg, sb) = spectral_to_rgb(&spectral_result);

    px[0] = (sr * px[3] as f32 + 0.5) as u16;
    px[1] = (sg * px[3] as f32 + 0.5) as u16;
    px[2] = (sb * px[3] as f32 + 0.5) as u16;
}

/// 标准 Normal Paint blend（color_a 视为 SCALE，即不擦除）。
#[inline]
pub fn blend_pixel_normal_paint(
    px: &mut [u16; 4], mask_val: u16,
    color_r: u16, color_g: u16, color_b: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    blend_pixel_normal_eraser_paint(
        px, mask_val, color_r, color_g, color_b, SCALE as u16,
        opacity, spectral_a);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_blend_full_opacity_replaces() {
        let mut px = [0, 0, 0, 0];
        blend_pixel_normal(&mut px, SCALE as u16, SCALE as u16, 0, 0, SCALE as u16);
        assert_eq!(px[0], SCALE as u16);
        assert_eq!(px[3], SCALE as u16);
    }

    #[test]
    fn normal_blend_zero_mask_no_change() {
        let mut px = [100, 200, 300, 32000];
        let orig = px;
        blend_pixel_normal(&mut px, 0, SCALE as u16, 0, 0, SCALE as u16);
        assert_eq!(px, orig);
    }

    #[test]
    fn lock_alpha_preserves_alpha() {
        let mut px = [0, 0, 0, 16384]; // half alpha
        blend_pixel_lock_alpha(&mut px, SCALE as u16, SCALE as u16, 0, 0, SCALE as u16);
        assert_eq!(px[3], 16384);
    }
}
