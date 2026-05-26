//! RLE-mask-based blending modes.
//! 对应 brushmodes.c。每个函数遍历一个 RLE-encoded mask buffer 并对一个 tile 的 RGBA 数据
//! 应用对应混合模式。
//!
//! Mask 格式（RLE）:
//! - 连续非零 u16：对应像素的 [`Coverage15`] (0..=32768)
//! - 0 后跟一个 skip 计数 [`crate::render::mask::RleSkip`]（已 *4 步长）：跳过 N 个像素
//! - 末尾 0,0 表示终止
//!
//! 像素格式: u16 RGBA premultiplied alpha, 范围 0..=2^15 (32768)。
//! 用 [`Premul15`] 在 type-system 层面区分 premultiplied channel 和其他 u16
//! 类型（mask coverage / RLE skip）。

use crate::render::mask::{Coverage15, Premul15, RleEntry};
use crate::smudge::{rgb_to_spectral, spectral_to_rgb, Spectral};

const SCALE: u32 = Coverage15::SCALE;

// LUMA coefficients matching brushmodes.c:180-182
const LUMA_RED: f32 = 0.2126;
const LUMA_GREEN: f32 = 0.7152;
const LUMA_BLUE: f32 = 0.0722;

#[inline]
fn luma_u16(r: u16, g: u16, b: u16) -> i32 {
    (r as f32 * LUMA_RED + g as f32 * LUMA_GREEN + b as f32 * LUMA_BLUE) as i32
}

/// 遍历 RLE mask，对每个非零像素执行回调 `f(rgba_4channel_pixel, coverage)`。
///
/// callback 收到的 `&mut [Premul15; 4]` 是 tile 切片的 4-channel 窗口。
/// type-system 阻止把 mask coverage / RLE skip 混用为 pixel channel。
///
/// RLE 解码由 [`RleEntry::parse`] 在单一入口完成，杜绝把 skip 槽当
/// coverage 读取（或反之）。
#[inline]
fn iter_rle_mask<F: FnMut(&mut [Premul15; 4], Coverage15)>(
    mask: &[u16],
    rgba: &mut [Premul15],
    mut f: F,
) {
    let mut mi: usize = 0;
    let mut ri: usize = 0;
    loop {
        let (entry, width) = RleEntry::parse(mask, mi);
        mi += width;
        match entry {
            RleEntry::Pixel(cov) => {
                if ri + 4 > rgba.len() {
                    return;
                }
                let px: &mut [Premul15; 4] = (&mut rgba[ri..ri + 4]).try_into().unwrap();
                f(px, cov);
                ri += 4;
            }
            RleEntry::Skip(skip) => {
                ri += skip.as_rgba_offset();
            }
            RleEntry::End => return,
        }
    }
}

// =============================================================================
// 各 blend 模式 — 遍历 RLE mask
// =============================================================================

/// 对应 draw_dab_pixels_BlendMode_Normal。
pub fn blend_dab_normal(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    opacity: Coverage15,
) {
    let color_r = color_r.raw() as u32;
    let color_g = color_g.raw() as u32;
    let color_b = color_b.raw() as u32;
    let opacity = opacity.raw() as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a;
        px[3] = Premul15::from_scaled_u32(opa_a + opa_b * px[3].raw() as u32 / SCALE);
        px[0] = Premul15::from_scaled_u32((opa_a * color_r + opa_b * px[0].raw() as u32) / SCALE);
        px[1] = Premul15::from_scaled_u32((opa_a * color_g + opa_b * px[1].raw() as u32) / SCALE);
        px[2] = Premul15::from_scaled_u32((opa_a * color_b + opa_b * px[2].raw() as u32) / SCALE);
    });
}

/// 对应 draw_dab_pixels_BlendMode_LockAlpha。
pub fn blend_dab_lock_alpha(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    opacity: Coverage15,
) {
    let color_r = color_r.raw() as u32;
    let color_g = color_g.raw() as u32;
    let color_b = color_b.raw() as u32;
    let opacity = opacity.raw() as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a_top = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a_top;
        let opa_a = opa_a_top * px[3].raw() as u32 / SCALE;
        px[0] = Premul15::from_scaled_u32((opa_a * color_r + opa_b * px[0].raw() as u32) / SCALE);
        px[1] = Premul15::from_scaled_u32((opa_a * color_g + opa_b * px[1].raw() as u32) / SCALE);
        px[2] = Premul15::from_scaled_u32((opa_a * color_b + opa_b * px[2].raw() as u32) / SCALE);
    });
}

/// 对应 draw_dab_pixels_BlendMode_Normal_and_Eraser。
pub fn blend_dab_normal_eraser(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    color_a: Premul15,
    opacity: Coverage15,
) {
    let color_r = color_r.raw() as u32;
    let color_g = color_g.raw() as u32;
    let color_b = color_b.raw() as u32;
    let color_a = color_a.raw() as u32;
    let opacity = opacity.raw() as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let opa_a_raw = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a_raw;
        let opa_a = opa_a_raw * color_a / SCALE;
        px[3] = Premul15::from_scaled_u32(opa_a + opa_b * px[3].raw() as u32 / SCALE);
        px[0] = Premul15::from_scaled_u32((opa_a * color_r + opa_b * px[0].raw() as u32) / SCALE);
        px[1] = Premul15::from_scaled_u32((opa_a * color_g + opa_b * px[1].raw() as u32) / SCALE);
        px[2] = Premul15::from_scaled_u32((opa_a * color_b + opa_b * px[2].raw() as u32) / SCALE);
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
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    opacity: Coverage15,
) {
    let color_r_raw = color_r.raw();
    let color_g_raw = color_g.raw();
    let color_b_raw = color_b.raw();
    let opacity = opacity.raw() as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let a = px[3].raw();
        let (mut r, mut g, mut b) = (0u16, 0u16, 0u16);
        if a != 0 {
            r = ((SCALE * px[0].raw() as u32) / a as u32) as u16;
            g = ((SCALE * px[1].raw() as u32) / a as u32) as u16;
            b = ((SCALE * px[2].raw() as u32) / a as u32) as u16;
        }
        set_rgb16_lum_from_rgb16(
            color_r_raw,
            color_g_raw,
            color_b_raw,
            &mut r,
            &mut g,
            &mut b,
        );
        r = ((r as u32 * a as u32) / SCALE) as u16;
        g = ((g as u32 * a as u32) / SCALE) as u16;
        b = ((b as u32 * a as u32) / SCALE) as u16;
        let opa_a = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a;
        px[0] = Premul15::from_scaled_u32((opa_a * r as u32 + opa_b * px[0].raw() as u32) / SCALE);
        px[1] = Premul15::from_scaled_u32((opa_a * g as u32 + opa_b * px[1].raw() as u32) / SCALE);
        px[2] = Premul15::from_scaled_u32((opa_a * b as u32 + opa_b * px[2].raw() as u32) / SCALE);
    });
}

/// 对应 draw_dab_pixels_BlendMode_Posterize。`opacity` 已包含 opaque/mask 调制。
pub fn blend_dab_posterize(
    mask: &[u16],
    rgba: &mut [Premul15],
    opacity: Coverage15,
    posterize_num: u16,
) {
    let pn = posterize_num as f32;
    if pn == 0.0 {
        return;
    }
    let opacity = opacity.raw() as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let r = px[0].raw() as f32 / SCALE as f32;
        let g = px[1].raw() as f32 / SCALE as f32;
        let b = px[2].raw() as f32 / SCALE as f32;
        let post_r = (SCALE as f32 * (r * pn).round() / pn) as u32;
        let post_g = (SCALE as f32 * (g * pn).round() / pn) as u32;
        let post_b = (SCALE as f32 * (b * pn).round() / pn) as u32;
        let opa_a = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a;
        px[0] = Premul15::from_scaled_u32((opa_a * post_r + opa_b * px[0].raw() as u32) / SCALE);
        px[1] = Premul15::from_scaled_u32((opa_a * post_g + opa_b * px[1].raw() as u32) / SCALE);
        px[2] = Premul15::from_scaled_u32((opa_a * post_b + opa_b * px[2].raw() as u32) / SCALE);
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
#[allow(clippy::too_many_arguments)]
pub fn blend_dab_normal_eraser_paint(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    color_a: Premul15,
    opacity: Coverage15,
    spectral_a: &Spectral,
) {
    let color_r = color_r.raw() as u32;
    let color_g = color_g.raw() as u32;
    let color_b = color_b.raw() as u32;
    let color_a_raw = color_a.raw() as u32;
    let opacity = opacity.raw().max(150) as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let px3 = px[3].raw() as u32;
        let opa_a = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a;
        let opa_a2 = opa_a * color_a_raw / SCALE;
        let opa_out = opa_a2 + opa_b * px3 / SCALE;
        let mut rgb = [0u32; 3];
        let spectral_factor = spectral_blend_factor(px3 as f32 / SCALE as f32).clamp(0.0, 1.0);
        let additive_factor = 1.0 - spectral_factor;
        if additive_factor != 0.0 {
            rgb[0] = (opa_a2 * color_r + opa_b * px[0].raw() as u32) / SCALE;
            rgb[1] = (opa_a2 * color_g + opa_b * px[1].raw() as u32) / SCALE;
            rgb[2] = (opa_a2 * color_b + opa_b * px[2].raw() as u32) / SCALE;
        }
        if spectral_factor != 0.0 && px3 != 0 {
            let px3f = px3 as f32;
            let spectral_b = rgb_to_spectral(
                px[0].raw() as f32 / px3f,
                px[1].raw() as f32 / px3f,
                px[2].raw() as f32 / px3f,
            );
            let denom = opa_a as f32 + opa_b as f32 * px3f / SCALE as f32;
            let mut fac_a = if denom > 0.0 {
                opa_a as f32 / denom
            } else {
                0.0
            };
            fac_a *= color_a_raw as f32 / SCALE as f32;
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
        px[3] = Premul15::from_scaled_u32(opa_out);
        px[0] = Premul15::from_scaled_u32(rgb[0]);
        px[1] = Premul15::from_scaled_u32(rgb[1]);
        px[2] = Premul15::from_scaled_u32(rgb[2]);
    });
}

/// 对应 draw_dab_pixels_BlendMode_Normal_Paint（color_a = SCALE 退化版）。
pub fn blend_dab_normal_paint(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    opacity: Coverage15,
    spectral_a: &Spectral,
) {
    blend_dab_normal_eraser_paint(
        mask,
        rgba,
        color_r,
        color_g,
        color_b,
        Premul15::FULL,
        opacity,
        spectral_a,
    );
}

/// 对应 draw_dab_pixels_BlendMode_LockAlpha_Paint。
pub fn blend_dab_lock_alpha_paint(
    mask: &[u16],
    rgba: &mut [Premul15],
    color_r: Premul15,
    color_g: Premul15,
    color_b: Premul15,
    opacity: Coverage15,
    spectral_a: &Spectral,
) {
    let color_r = color_r.raw() as u32;
    let color_g = color_g.raw() as u32;
    let color_b = color_b.raw() as u32;
    let opacity = opacity.raw().max(150) as u32;
    iter_rle_mask(mask, rgba, |px, m| {
        let px3 = px[3].raw() as u32;
        let opa_a_raw = (m.raw() as u32 * opacity) / SCALE;
        let opa_b = SCALE - opa_a_raw;
        let opa_a = opa_a_raw * px3 / SCALE;
        if px3 == 0 {
            px[0] =
                Premul15::from_scaled_u32((opa_a * color_r + opa_b * px[0].raw() as u32) / SCALE);
            px[1] =
                Premul15::from_scaled_u32((opa_a * color_g + opa_b * px[1].raw() as u32) / SCALE);
            px[2] =
                Premul15::from_scaled_u32((opa_a * color_b + opa_b * px[2].raw() as u32) / SCALE);
            return;
        }
        let px3f = px3 as f32;
        let denom = opa_a as f32 + opa_b as f32 * px3f / SCALE as f32;
        let fac_a = if denom > 0.0 {
            opa_a as f32 / denom
        } else {
            0.0
        };
        let fac_b = 1.0 - fac_a;
        let spectral_b = rgb_to_spectral(
            px[0].raw() as f32 / px3f,
            px[1].raw() as f32 / px3f,
            px[2].raw() as f32 / px3f,
        );
        let mut sr_arr = [0.0f32; 10];
        for i in 0..10 {
            sr_arr[i] = spectral_a[i].powf(fac_a) * spectral_b[i].powf(fac_b);
        }
        let (sr, sg, sb) = spectral_to_rgb(&sr_arr);
        px[0] = Premul15::from_scaled_u32((sr * px3f + 0.5) as u32);
        px[1] = Premul15::from_scaled_u32((sg * px3f + 0.5) as u32);
        px[2] = Premul15::from_scaled_u32((sb * px3f + 0.5) as u32);
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
        let mut rgba = vec![Premul15::ZERO; 4];
        let mut count = 0;
        iter_rle_mask(&mask, &mut rgba, |_, _| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn rle_mask_with_skip() {
        // 第一个值，跳过 2 像素（=8 步长），再一个值，终止
        let mask = vec![SCALE as u16, 0, 8, SCALE as u16, 0, 0];
        let mut rgba = vec![Premul15::ZERO; 16]; // 4 pixels
        let mut indices = Vec::new();
        let mut ptr = 0;
        iter_rle_mask(&mask, &mut rgba, |px, _| {
            indices.push(ptr);
            ptr += 1;
            let _ = px;
        });
        assert_eq!(indices.len(), 2);
    }

    #[test]
    fn normal_blend_full_opacity_writes_color() {
        // 单像素，full mask + opacity
        let mask = vec![SCALE as u16, 0, 0];
        let mut rgba = vec![Premul15::ZERO; 4];
        blend_dab_normal(
            &mask,
            &mut rgba,
            Premul15::FULL,
            Premul15::ZERO,
            Premul15::ZERO,
            Coverage15::FULL,
        );
        assert_eq!(rgba[0].raw(), SCALE as u16);
        assert_eq!(rgba[3].raw(), SCALE as u16);
    }

    #[test]
    fn rle_entry_parse_pixel() {
        let buf = [42u16, 0, 0];
        let (e, w) = RleEntry::parse(&buf, 0);
        assert!(matches!(e, RleEntry::Pixel(_)));
        assert_eq!(w, 1);
    }

    #[test]
    fn rle_entry_parse_skip() {
        let buf = [0u16, 8, 42, 0, 0];
        let (e, w) = RleEntry::parse(&buf, 0);
        assert!(matches!(e, RleEntry::Skip(_)));
        assert_eq!(w, 2);
    }

    #[test]
    fn rle_entry_parse_end() {
        let buf = [0u16, 0];
        let (e, w) = RleEntry::parse(&buf, 0);
        assert!(matches!(e, RleEntry::End));
        assert_eq!(w, 0);
    }
}
