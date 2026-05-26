//! RLE-encoded dab mask renderer.
//! 对应 mypaint-tiled-surface.c:render_dab_mask。
//!
//! 一个 tile (64×64) 的 mask buffer 用 run-length encoding：
//! - 连续非零值：每个值是该像素的 opacity ([`Coverage15`], 0..=32768)
//! - 0 后跟一个跳过计数（[`RleSkip`]，已乘 *4 步长）：跳过 N 个像素
//! - 末尾 0,0 表示结束
//!
//! Blend 函数遍历这个 buffer 来高效跳过透明区域。

use crate::render::dab::{calculate_opa, calculate_rr, calculate_rr_antialiased, MaskParams};
use crate::surface::tile::TILE_SIZE;

// ============================================================================
// Mask newtypes — 区分 RLE 编码里的两种 u16 字段（coverage 和 skip）
// ============================================================================

/// 0..=`SCALE` (1<<15 = 32768) 的 mask alpha-coverage（不透明度）值。
///
/// 用 newtype 是为了在 type 层面区分两种 u16：
/// - **coverage**（本类型）— 像素的 mask 不透明度
/// - **skip length**（[`RleSkip`]）— RLE 跳过段的 *4 偏移
///
/// 两者在 RLE buffer 里都是 u16，但语义完全不同。若混用（例如把
/// skip length 当 coverage 传给 blend），会产生静默的图像 corruption。
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Coverage15(u16);

impl Coverage15 {
    /// 满 coverage 值（1<<15）。
    pub const SCALE: u32 = 1 << 15;

    /// 完全透明（=0）。
    pub const ZERO: Self = Self(0);

    /// 完全不透明（=SCALE）。
    pub const FULL: Self = Self(Self::SCALE as u16);

    /// 从 u16 构造，超过 SCALE 时饱和到 [`FULL`](Self::FULL)。
    #[inline]
    pub const fn new_saturating(v: u16) -> Self {
        if (v as u32) > Self::SCALE {
            Self::FULL
        } else {
            Self(v)
        }
    }

    /// 从 raw u16 构造（不做范围检查）。仅供从 [`MaskBuffer`] 这样
    /// 已知合法的 buffer 中读取时使用。
    #[inline]
    pub const fn from_raw(v: u16) -> Self {
        Self(v)
    }

    /// 取出内部 u16，用于 blend 算法的 u32 / u16 整数运算。
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// RLE 编码里的 skip 段长度，**已经包含 `*4` 的 RGBA 步长**。
///
/// 例如 `RleSkip::from_pixel_count(7).raw() == 28` —— 表示跳过 7 个像素
/// 等于在 RGBA tile slice 里前进 28 个 u16。
///
/// 与 [`Coverage15`] 不可互换（这是 newtype 的核心目的）。
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RleSkip(u16);

impl RleSkip {
    /// 从像素数构造（内部乘 4 并饱和到 u16::MAX）。
    #[inline]
    pub const fn from_pixel_count(px: usize) -> Self {
        let n = px.saturating_mul(4);
        if n > u16::MAX as usize {
            Self(u16::MAX)
        } else {
            Self(n as u16)
        }
    }

    /// 从已经 `*4` 过的 raw u16 构造（不做检查）。仅供从 [`MaskBuffer`]
    /// 这样已知合法的 buffer 中读取时使用。
    #[inline]
    pub const fn from_raw(v: u16) -> Self {
        Self(v)
    }

    /// 取出内部 u16（已 *4）。
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 用作 rgba slice 偏移（同 raw）。
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

const SCALE: u32 = Coverage15::SCALE;

/// Mask buffer for one tile. 最长情况：每个像素一个值 + tile 边界跳过。
/// 上游为 `TILE_SIZE*TILE_SIZE + 2*TILE_SIZE` u16。
pub const MASK_BUFFER_LEN: usize = TILE_SIZE * TILE_SIZE + 2 * TILE_SIZE;

/// Reusable mask buffer.
pub struct MaskBuffer {
    buf: Vec<u16>,
    len: usize,
}

impl MaskBuffer {
    pub fn new() -> Self {
        Self {
            buf: vec![0; MASK_BUFFER_LEN],
            len: 0,
        }
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.buf[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl Default for MaskBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// 渲染 dab mask 到 RLE buffer。x/y 是 tile-local 坐标（dab 中心相对于 tile 左上角）。
/// 对应 render_dab_mask in mypaint-tiled-surface.c:376-493。
pub fn render_dab_mask(
    mask_buf: &mut MaskBuffer,
    x: f32,
    y: f32,
    radius: f32,
    hardness: f32,
    softness: f32,
    aspect_ratio: f32,
    angle: f32,
) {
    let hardness = hardness.clamp(0.0, 1.0);
    debug_assert!(hardness != 0.0); // 调用方应已检查
    let aspect_ratio = aspect_ratio.max(1.0);

    let mask_params = MaskParams::from_hardness_softness(hardness, softness);
    let angle_rad = angle.to_radians();
    let cs = angle_rad.cos();
    let sn = angle_rad.sin();
    let one_over_radius2 = 1.0 / (radius * radius);

    // 边界（tile-local）
    let r_fringe = radius + 1.0;
    let x0 = ((x - r_fringe).floor() as i32).max(0) as usize;
    let y0 = ((y - r_fringe).floor() as i32).max(0) as usize;
    let x1 = (((x + r_fringe).floor() as i32).min(TILE_SIZE as i32 - 1)).max(0) as usize;
    let y1 = (((y + r_fringe).floor() as i32).min(TILE_SIZE as i32 - 1)).max(0) as usize;

    // 小半径走 AA 分支
    let use_aa = radius < 3.0;
    let aa_border = 1.0_f32;
    let mut r_aa_start = if radius > aa_border {
        radius - aa_border
    } else {
        0.0
    };
    r_aa_start = r_aa_start * r_aa_start / aspect_ratio;

    // RLE 输出（buf 的写入指针）
    mask_buf.buf.iter_mut().for_each(|v| *v = 0);
    let mut write_idx: usize = 0;
    let mut skip: usize = 0;

    // 行 y < y0 的所有像素都跳过
    skip += y0 * TILE_SIZE;
    for py in y0..=y1 {
        // 行内 x < x0 的也跳过
        skip += x0;
        let mut xp = x0;
        while xp <= x1 {
            let rr = if use_aa {
                calculate_rr_antialiased(
                    xp as i32,
                    py as i32,
                    x,
                    y,
                    aspect_ratio,
                    sn,
                    cs,
                    one_over_radius2,
                    r_aa_start,
                )
            } else {
                calculate_rr(
                    xp as i32,
                    py as i32,
                    x,
                    y,
                    aspect_ratio,
                    sn,
                    cs,
                    one_over_radius2,
                )
            };
            let opa = calculate_opa(rr, &mask_params);
            let cov = Coverage15::new_saturating((opa * SCALE as f32) as u16);
            if cov.is_zero() {
                skip += 1;
            } else {
                if skip > 0 {
                    // 写入 0, RleSkip (RGBA 步长 *4 由 from_pixel_count 处理)
                    mask_buf.buf[write_idx] = 0;
                    write_idx += 1;
                    mask_buf.buf[write_idx] = RleSkip::from_pixel_count(skip).raw();
                    write_idx += 1;
                    skip = 0;
                }
                mask_buf.buf[write_idx] = cov.raw();
                write_idx += 1;
            }
            xp += 1;
        }
        // 行末跳过
        skip += TILE_SIZE - (x1 + 1);
    }
    // 末尾终止符 0, 0
    mask_buf.buf[write_idx] = 0;
    write_idx += 1;
    mask_buf.buf[write_idx] = 0;
    write_idx += 1;
    mask_buf.len = write_idx;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // newtype tests
    // ------------------------------------------------------------------------

    #[test]
    fn coverage15_new_saturating_caps_at_scale() {
        assert_eq!(Coverage15::new_saturating(0).raw(), 0);
        assert_eq!(Coverage15::new_saturating(1).raw(), 1);
        assert_eq!(
            Coverage15::new_saturating(Coverage15::SCALE as u16).raw(),
            Coverage15::SCALE as u16
        );
        assert_eq!(
            Coverage15::new_saturating(u16::MAX).raw(),
            Coverage15::SCALE as u16
        );
    }

    #[test]
    fn coverage15_constants() {
        assert!(Coverage15::ZERO.is_zero());
        assert!(!Coverage15::FULL.is_zero());
        assert_eq!(Coverage15::FULL.raw() as u32, Coverage15::SCALE);
    }

    #[test]
    fn rle_skip_from_pixel_count_multiplies_by_4() {
        assert_eq!(RleSkip::from_pixel_count(0).raw(), 0);
        assert_eq!(RleSkip::from_pixel_count(7).raw(), 28);
        assert_eq!(RleSkip::from_pixel_count(7).as_usize(), 28);
    }

    #[test]
    fn rle_skip_saturates_at_u16_max() {
        // 16384 * 4 = 65536 > u16::MAX, should saturate.
        assert_eq!(RleSkip::from_pixel_count(16384).raw(), u16::MAX);
        assert_eq!(RleSkip::from_pixel_count(usize::MAX).raw(), u16::MAX);
    }

    // ------------------------------------------------------------------------
    // mask render tests
    // ------------------------------------------------------------------------

    #[test]
    fn empty_mask_when_radius_outside_tile() {
        let mut mb = MaskBuffer::new();
        render_dab_mask(&mut mb, -100.0, -100.0, 5.0, 0.8, 0.0, 1.0, 0.0);
        let slice = mb.as_slice();
        // 至少有终止符
        assert!(slice.len() >= 2);
        // 全是 0
        assert!(slice.iter().all(|&v| v == 0));
    }

    #[test]
    fn dab_at_center_has_nonzero() {
        let mut mb = MaskBuffer::new();
        render_dab_mask(&mut mb, 32.0, 32.0, 5.0, 0.8, 0.0, 1.0, 0.0);
        let slice = mb.as_slice();
        // 应该有非零的 opacity 值
        let has_nonzero = slice.iter().any(|&v| v != 0);
        assert!(
            has_nonzero,
            "dab at tile center should produce non-zero mask values"
        );
    }

    #[test]
    fn terminates_with_zero_zero() {
        let mut mb = MaskBuffer::new();
        render_dab_mask(&mut mb, 32.0, 32.0, 5.0, 0.8, 0.0, 1.0, 0.0);
        let slice = mb.as_slice();
        assert!(slice.len() >= 2);
        assert_eq!(slice[slice.len() - 2], 0);
        assert_eq!(slice[slice.len() - 1], 0);
    }
}
