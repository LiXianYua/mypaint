//! RLE-mask-based blending modes.
//! 对应 brushmodes.c。每个函数遍历一个 RLE-encoded mask buffer 并对一个 tile 的 RGBA 数据
//! 应用对应混合模式。
//!
//! Mask 格式（RLE）:
//! - 连续非零 u16：对应像素的 opacity (0..32768)
//! - 0 后跟一个 skip 计数（× 4，因 RGBA 步长）：跳过 N 个像素
//! - 末尾 0,0 表示终止
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
    (r as f32 * LUMA_RED + g as f32 * LUMA_GREEN + b as f32 * LUMA_BLUE) as i32
}

/// 遍历 RLE mask，对每个非零像素执行回调 `f(rgba_offset, mask_val)`。
#[inline]
fn iter_rle_mask<F: FnMut(&mut [u16], u16)>(mask: &[u16], rgba: &mut [u16], mut f: F) {
    let mut mi: usize = 0;
    let mut ri: usize = 0;
    loop {
        // 处理连续非零段
        while mi < mask.len() && mask[mi] != 0 {
            let m = mask[mi];
            if ri + 4 > rgba.len() {
                return;
            }
            // SAFETY-equivalent: rgba slice 长度足够
            f(&mut rgba[ri..ri + 4], m);
            mi += 1;
            ri += 4;
        }
        // 遇到 0，检查后面是 skip 计数还是终止
        if mi + 1 >= mask.len() || mask[mi + 1] == 0 {
            return; // 终止符
        }
        ri += mask[mi + 1] as usize;
        mi += 2;
    }
}

// =============================================================================
// 各 blend 模式 — 遍历 RLE mask
// =============================================================================

/// 对应 draw_dab_pixels_BlendMode_Normal。
pub fn blend_dab_normal(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    opacity: u16,
) {
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a;
        px[3] = (opa_a + opa_b * px[3] as u32 / SCALE) as u16;
        px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
    });
}

/// 对应 draw_dab_pixels_BlendMode_LockAlpha。
pub fn blend_dab_lock_alpha(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    opacity: u16,
) {
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a_top = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a_top;
        let opa_a = opa_a_top * px[3] as u32 / SCALE;
        px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
    });
}

/// 对应 draw_dab_pixels_BlendMode_Normal_and_Eraser。
pub fn blend_dab_normal_eraser(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    color_a: u16,
    opacity: u16,
) {
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a_raw = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a_raw;
        let opa_a = opa_a_raw * color_a as u32 / SCALE;
        px[3] = (opa_a + opa_b * px[3] as u32 / SCALE) as u16;
        px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
    });
}

/// SetLum + ClipColor — brushmodes.c:199-241。
#[inline]
fn set_rgb16_lum_from_rgb16(
    topr: u16,
    topg: u16,
    topb: u16,
    botr: &mut u16,
    botg: &mut u16,
    botb: &mut u16,
) {
    let botlum = luma_u16(*botr, *botg, *botb);
    let toplum = luma_u16(topr, topg, topb);
    let diff = botlum - toplum;
    let mut r = topr as i32 + diff;
    let mut g = topg as i32 + diff;
    let mut b = topb as i32 + diff;
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

/// 对应 draw_dab_pixels_BlendMode_Color。
pub fn blend_dab_color(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    opacity: u16,
) {
    iter_rle_mask(mask, rgba, |px, m| {
        let a = px[3];
        let (mut r, mut g, mut b) = (0u16, 0u16, 0u16);
        if a != 0 {
            r = ((SCALE * px[0] as u32) / a as u32) as u16;
            g = ((SCALE * px[1] as u32) / a as u32) as u16;
            b = ((SCALE * px[2] as u32) / a as u32) as u16;
        }
        set_rgb16_lum_from_rgb16(color_r, color_g, color_b, &mut r, &mut g, &mut b);
        r = ((r as u32 * a as u32) / SCALE) as u16;
        g = ((g as u32 * a as u32) / SCALE) as u16;
        b = ((b as u32 * a as u32) / SCALE) as u16;
        let opa_a = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a;
        px[0] = ((opa_a * r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
    });
}

/// 对应 draw_dab_pixels_BlendMode_Posterize。`opacity` 已包含 opaque/mask 调制。
pub fn blend_dab_posterize(mask: &[u16], rgba: &mut [u16], opacity: u16, posterize_num: u16) {
    let pn = posterize_num as f32;
    if pn == 0.0 {
        return;
    }
    iter_rle_mask(mask, rgba, |px, m| {
        let r = px[0] as f32 / SCALE as f32;
        let g = px[1] as f32 / SCALE as f32;
        let b = px[2] as f32 / SCALE as f32;
        let post_r = (SCALE as f32 * (r * pn).round() / pn) as u32;
        let post_g = (SCALE as f32 * (g * pn).round() / pn) as u32;
        let post_b = (SCALE as f32 * (b * pn).round() / pn) as u32;
        let opa_a = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a;
        px[0] = ((opa_a * post_r + opa_b * px[0] as u32) / SCALE) as u16;
        px[1] = ((opa_a * post_g + opa_b * px[1] as u32) / SCALE) as u16;
        px[2] = ((opa_a * post_b + opa_b * px[2] as u32) / SCALE) as u16;
    });
}

// =============================================================================
// Spectral (_Paint) variants
// =============================================================================

#[inline]
fn spectral_blend_factor(x: f32) -> f32 {
    const VER_FAC: f32 = 1.65;
    const HOR_FAC: f32 = 8.0;
    const HOR_OFFS: f32 = 3.0;
    let b = x * HOR_FAC - HOR_OFFS;
    0.5 + b / (1.0 + b.abs() * VER_FAC)
}

/// 对应 draw_dab_pixels_BlendMode_Normal_and_Eraser_Paint。
pub fn blend_dab_normal_eraser_paint(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    color_a: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    let opacity = opacity.max(150);
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a;
        let opa_a2 = opa_a * color_a as u32 / SCALE;
        let opa_out = opa_a2 + opa_b * px[3] as u32 / SCALE;
        let mut rgb = [0u32; 3];
        let spectral_factor = spectral_blend_factor(px[3] as f32 / SCALE as f32).clamp(0.0, 1.0);
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
                px[2] as f32 / px[3] as f32,
            );
            let denom = opa_a as f32 + opa_b as f32 * px[3] as f32 / SCALE as f32;
            let mut fac_a = if denom > 0.0 {
                opa_a as f32 / denom
            } else {
                0.0
            };
            fac_a *= color_a as f32 / SCALE as f32;
            let fac_b = 1.0 - fac_a;
            let mut sr_arr = [0.0f32; 10];
            for i in 0..10 {
                sr_arr[i] = spectral_a[i].powf(fac_a) * spectral_b[i].powf(fac_b);
            }
            let (sr, sg, sb) = spectral_to_rgb(&sr_arr);
            rgb[0] =
                (additive_factor * rgb[0] as f32 + spectral_factor * sr * opa_out as f32) as u32;
            rgb[1] =
                (additive_factor * rgb[1] as f32 + spectral_factor * sg * opa_out as f32) as u32;
            rgb[2] =
                (additive_factor * rgb[2] as f32 + spectral_factor * sb * opa_out as f32) as u32;
        }
        px[3] = opa_out as u16;
        px[0] = rgb[0] as u16;
        px[1] = rgb[1] as u16;
        px[2] = rgb[2] as u16;
    });
}

/// 对应 draw_dab_pixels_BlendMode_Normal_Paint（color_a = SCALE 退化版）。
pub fn blend_dab_normal_paint(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    blend_dab_normal_eraser_paint(
        mask,
        rgba,
        color_r,
        color_g,
        color_b,
        SCALE as u16,
        opacity,
        spectral_a,
    );
}

/// 对应 draw_dab_pixels_BlendMode_LockAlpha_Paint。
pub fn blend_dab_lock_alpha_paint(
    mask: &[u16],
    rgba: &mut [u16],
    color_r: u16,
    color_g: u16,
    color_b: u16,
    opacity: u16,
    spectral_a: &[f32; 10],
) {
    let opacity = opacity.max(150);
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a_raw = (m as u32 * opacity as u32) / SCALE;
        let opa_b = SCALE - opa_a_raw;
        let opa_a = opa_a_raw * px[3] as u32 / SCALE;
        if px[3] == 0 {
            px[0] = ((opa_a * color_r as u32 + opa_b * px[0] as u32) / SCALE) as u16;
            px[1] = ((opa_a * color_g as u32 + opa_b * px[1] as u32) / SCALE) as u16;
            px[2] = ((opa_a * color_b as u32 + opa_b * px[2] as u32) / SCALE) as u16;
            return;
        }
        let denom = opa_a as f32 + opa_b as f32 * px[3] as f32 / SCALE as f32;
        let fac_a = if denom > 0.0 {
            opa_a as f32 / denom
        } else {
            0.0
        };
        let fac_b = 1.0 - fac_a;
        let spectral_b = rgb_to_spectral(
            px[0] as f32 / px[3] as f32,
            px[1] as f32 / px[3] as f32,
            px[2] as f32 / px[3] as f32,
        );
        let mut sr_arr = [0.0f32; 10];
        for i in 0..10 {
            sr_arr[i] = spectral_a[i].powf(fac_a) * spectral_b[i].powf(fac_b);
        }
        let (sr, sg, sb) = spectral_to_rgb(&sr_arr);
        px[0] = (sr * px[3] as f32 + 0.5) as u16;
        px[1] = (sg * px[3] as f32 + 0.5) as u16;
        px[2] = (sb * px[3] as f32 + 0.5) as u16;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_pixel_mask(opa: u16) -> Vec<u16> {
        // 单像素 mask：一个 opa 值 + 终止符
        vec![opa, 0, 0]
    }

    #[test]
    fn rle_mask_iterates_single_value() {
        let mask = single_pixel_mask(SCALE as u16);
        let mut rgba = vec![0u16; 4];
        let mut count = 0;
        iter_rle_mask(&mask, &mut rgba, |_, _| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn rle_mask_with_skip() {
        // 第一个值，跳过 2 像素（=8 步长），再一个值，终止
        let mask = vec![SCALE as u16, 0, 8, SCALE as u16, 0, 0];
        let mut rgba = vec![0u16; 16]; // 4 pixels
        let mut indices = Vec::new();
        let mut ptr = 0;
        iter_rle_mask(&mask, &mut rgba, |px, _| {
            indices.push(ptr);
            ptr += 1;
            // 我们不知道 px 的实际偏移，但能数到两次回调
            let _ = px;
        });
        assert_eq!(indices.len(), 2);
    }

    #[test]
    fn normal_blend_full_opacity_writes_color() {
        // 单像素，full mask + opacity
        let mask = vec![SCALE as u16, 0, 0];
        let mut rgba = vec![0u16; 4];
        blend_dab_normal(&mask, &mut rgba, SCALE as u16, 0, 0, SCALE as u16);
        assert_eq!(rgba[0], SCALE as u16);
        assert_eq!(rgba[3], SCALE as u16);
    }
}
