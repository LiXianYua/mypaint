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

    /// 从 raw u16 构造（不做范围检查）。
    ///
    /// **仅供 crate 内部**从 [`MaskBuffer`] 这样已知合法的 RLE buffer
    /// 中读取时使用。外部代码请用 [`Self::new_saturating`]。
    /// debug build 会校验 `v <= SCALE`；release build 信任 caller。
    #[inline]
    pub(crate) fn from_raw(v: u16) -> Self {
        debug_assert!(
            (v as u32) <= Self::SCALE,
            "Coverage15::from_raw: {v} > SCALE ({}) — buffer corruption?",
            Self::SCALE
        );
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
    /// 长度为 0 的 skip。
    pub const ZERO: Self = Self(0);

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

    /// 从已经 `*4` 过的 raw u16 构造（不做检查）。
    ///
    /// **仅供 crate 内部**从 [`MaskBuffer`] 这样已知合法的 RLE buffer
    /// 中读取时使用。外部代码请用 [`Self::from_pixel_count`]。
    #[inline]
    pub(crate) const fn from_raw(v: u16) -> Self {
        Self(v)
    }

    /// 取出内部 u16（已 *4）。
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 用作 RGBA slice 的 u16 偏移量（**已经包含 *4 步长**，不要再乘）。
    #[inline]
    pub const fn as_rgba_offset(self) -> usize {
        self.0 as usize
    }

    /// 还原回逻辑像素数（向下取整 / 4，丢弃饱和时丢失的精度）。
    #[inline]
    pub const fn pixel_count(self) -> usize {
        self.0 as usize / 4
    }

    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

// 编译期常量校验：Coverage15::SCALE 必须能装进 u16 字段。
const _: () = assert!(Coverage15::SCALE <= u16::MAX as u32);

// ============================================================================
// Premul15 — premultiplied 颜色 channel
// ============================================================================

/// 0..=`SCALE` (1<<15 = 32768) 的 premultiplied 颜色 channel 值。
///
/// **Invariant:** `Premul15` 内部 u16 始终在 `0..=SCALE` 范围内。所有构造
/// 函数都饱和到该范围；不存在能产生 OOR 值的 safe API。
///
/// 用 newtype 区分像素 channel 和其他 u16 类型（[`Coverage15`] mask coverage /
/// [`RleSkip`] RLE skip 长度）。type system 阻止把这三类 u16 互相误传。
///
/// 不实现 `From<u16>` / 算术运算符 / `Deref` —— 这会绕过范围检查或隐藏
/// 15-bit fixed-point 乘法语义，丢失 newtype 的核心安全价值。需要做整数
/// 运算请显式 `.raw()` 解包。
///
/// `#[repr(transparent)]` + 无 niche 保证 layout、对齐、bit-pattern
/// validity 都与 u16 等价，编译期 `const _ assert` 校验。因此
/// `&mut [u16] ↔ &mut [Premul15]` 在 layout 层面是 sound 的 cast，
/// [`Self::slice_as_u16_mut`] / [`Self::slice_as_u16`] 提供 safe 方向
/// （Premul15 ⊆ u16）；反方向需要 unsafe 并由 caller 负责 invariant。
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Premul15(u16);

impl Premul15 {
    /// 满 channel 值（1<<15）。共享 [`Coverage15::SCALE`]。
    pub const SCALE: u32 = Coverage15::SCALE;

    /// 全 0（透明黑的某个 channel）。
    pub const ZERO: Self = Self(0);

    /// 满值（不透明白的某个 channel）。
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

    /// 从 0..=1 的 f32 构造（自动 clamp + scale）。
    #[inline]
    pub fn from_unit_f32(v: f32) -> Self {
        Self((v.clamp(0.0, 1.0) * Self::SCALE as f32) as u16)
    }

    /// blend 算法专用：从 15-bit fixed-point 算式（如 `(a*b + c*d) / SCALE`）
    /// 转回 `Premul15`。**始终饱和**到 `0..=SCALE`，保住类型不变量。
    ///
    /// 不加 `debug_assert!(v <= SCALE)`：保住 invariant 是这里的硬要求
    /// （Phase 3 `Vec<Premul15>` 存储依赖之），饱和是廉价的 defense-in-depth；
    /// algorithm bugs 通过单元测试覆盖。
    #[inline]
    pub(crate) fn from_scaled_u32(v: u32) -> Self {
        Self(v.min(Self::SCALE) as u16)
    }

    /// 取出内部 u16，用于 u32 / u16 整数运算。
    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 把 15-bit premultiplied 值压到 8-bit `u8`（sRGB-ish PNG 输出用）。
    ///
    /// `>> 7` 后再 `min(255)` 防 [`FULL`](Self::FULL) (= 32768) 的边界值：
    /// 32768 >> 7 = 256，`256 as u8` 会 wrap 到 0；min(255) 让满值像素
    /// 正确输出为 255。
    #[inline]
    pub const fn to_u8(self) -> u8 {
        let v = (self.0 >> 7) as u32;
        if v > 255 {
            255
        } else {
            v as u8
        }
    }

    /// 把 `&[Premul15]` 重新解释为 `&[u16]`（只读方向无 invariant 风险）。
    ///
    /// 主要给非 RGBA-blend 场景的下游（如以 raw u16 形式 dump tile 内容
    /// 做调试输出）用。trait `tile_request_start` / `tile_snapshot` 已经
    /// 直接暴露 `[Premul15]`，多数 caller 不再需要这个 cast。
    #[inline]
    pub fn slice_as_u16(s: &[Self]) -> &[u16] {
        // SAFETY: Premul15 is #[repr(transparent)] over u16 with no niches.
        // Layout, alignment, and bit-pattern validity of [Premul15] and
        // [u16] are identical.
        unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u16, s.len()) }
    }
}

// size/align must match u16 for the unsafe casts in `slice_as_u16*` and in
// `iter_rle_mask*` (cast to `[Premul15; 4]` view) to be sound.
const _: () = {
    assert!(std::mem::size_of::<Premul15>() == std::mem::size_of::<u16>());
    assert!(std::mem::align_of::<Premul15>() == std::mem::align_of::<u16>());
};

// ============================================================================
// RleEntry — 集中化的 mask buffer decode
// ============================================================================

/// 从 RLE mask buffer 解析出的一个 entry。
///
/// 由 [`RleEntry::parse`] 在单一入口产生，消除"读 buffer 时把 skip 当
/// coverage（或反之）"的可能 — 这是 [`Coverage15`] / [`RleSkip`] newtype
/// 安全保证的关键 chokepoint。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RleEntry {
    /// 一个像素的 coverage。
    Pixel(Coverage15),
    /// 跳过 N 个像素。
    Skip(RleSkip),
    /// 终止符 / buffer 越界。
    End,
}

impl RleEntry {
    /// 解析 `buf[offset]` 处的下一个 entry。返回 `(entry, 在 buf 中占的 u16 数)`。
    ///
    /// 终止符占 0 个 u16（视作 buffer 边界），便于 caller 直接 break。
    #[inline]
    pub fn parse(buf: &[u16], offset: usize) -> (Self, usize) {
        if offset >= buf.len() {
            return (Self::End, 0);
        }
        let v = buf[offset];
        if v != 0 {
            return (Self::Pixel(Coverage15::from_raw(v)), 1);
        }
        // v == 0：检查后续是 skip 长度还是终止
        if offset + 1 >= buf.len() || buf[offset + 1] == 0 {
            return (Self::End, 0);
        }
        (Self::Skip(RleSkip::from_raw(buf[offset + 1])), 2)
    }
}

const _: () = assert!(Premul15::SCALE <= u16::MAX as u32);

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
        assert_eq!(RleSkip::from_pixel_count(7).as_rgba_offset(), 28);
    }

    #[test]
    fn rle_skip_saturates_at_u16_max() {
        // 16383 * 4 = 65532, last value that fits in u16.
        assert_eq!(RleSkip::from_pixel_count(16383).raw(), 65532);
        // 16384 * 4 = 65536 > u16::MAX, should saturate.
        assert_eq!(RleSkip::from_pixel_count(16384).raw(), u16::MAX);
        assert_eq!(RleSkip::from_pixel_count(usize::MAX).raw(), u16::MAX);
    }

    #[test]
    fn rle_skip_pixel_count_roundtrip() {
        let s = RleSkip::from_pixel_count(13);
        assert_eq!(s.pixel_count(), 13);
        assert_eq!(s.as_rgba_offset(), 52);
    }

    #[test]
    fn rle_skip_zero_and_is_zero() {
        assert!(RleSkip::ZERO.is_zero());
        assert_eq!(RleSkip::ZERO.raw(), 0);
        assert!(!RleSkip::from_pixel_count(1).is_zero());
    }

    #[test]
    fn premul15_new_saturating_caps_at_scale() {
        assert_eq!(Premul15::new_saturating(0).raw(), 0);
        assert_eq!(
            Premul15::new_saturating(Premul15::SCALE as u16).raw(),
            Premul15::SCALE as u16
        );
        assert_eq!(
            Premul15::new_saturating(u16::MAX).raw(),
            Premul15::SCALE as u16
        );
    }

    #[test]
    fn premul15_from_unit_f32_boundary() {
        assert_eq!(Premul15::from_unit_f32(0.0).raw(), 0);
        assert_eq!(Premul15::from_unit_f32(1.0), Premul15::FULL);
        // 50% should land at SCALE/2 = 16384 (or 16384-1 due to floor)
        let half = Premul15::from_unit_f32(0.5).raw();
        assert!(
            (16383..=16384).contains(&half),
            "0.5 → {} not in [16383, 16384]",
            half
        );
        // Out of [0,1] clamps to bounds
        assert_eq!(Premul15::from_unit_f32(-1.0).raw(), 0);
        assert_eq!(Premul15::from_unit_f32(2.0), Premul15::FULL);
        // NaN clamps to NaN then `as u16` = 0
        assert_eq!(Premul15::from_unit_f32(f32::NAN).raw(), 0);
    }

    #[test]
    fn premul15_to_u8_handles_full_boundary() {
        // The latent bug from C upstream: 32768 >> 7 = 256, naive `as u8` wraps to 0.
        assert_eq!(Premul15::ZERO.to_u8(), 0);
        assert_eq!(
            Premul15::FULL.to_u8(),
            255,
            "FULL must map to 255, not wrap to 0"
        );
        // Just below SCALE: 32767 >> 7 = 255
        assert_eq!(
            Premul15::new_saturating((Premul15::SCALE - 1) as u16).to_u8(),
            255
        );
        // Half-ish
        assert_eq!(Premul15::new_saturating(16384).to_u8(), 128);
    }

    #[test]
    fn premul15_from_scaled_u32_saturates_in_release() {
        // Within range: passes through
        assert_eq!(Premul15::from_scaled_u32(0).raw(), 0);
        assert_eq!(
            Premul15::from_scaled_u32(Premul15::SCALE).raw(),
            Premul15::SCALE as u16
        );
        // Above SCALE: must saturate (not truncate as u16) — this is the
        // Phase 2 review fix that protects Phase 3's Vec<Premul15> contract.
        assert_eq!(
            Premul15::from_scaled_u32(Premul15::SCALE + 1).raw(),
            Premul15::SCALE as u16
        );
        assert_eq!(
            Premul15::from_scaled_u32(u32::MAX).raw(),
            Premul15::SCALE as u16
        );
    }

    #[test]
    fn rle_entry_parse_unterminated_skip_collapses_to_end() {
        // Skip slot without a value behind it — buffer truncated mid-skip.
        // Should decode as End (the offset+1 guard at parse).
        let buf = [0u16, 8]; // 0 marker, skip-len 8, no terminator after.
        let (e, w) = RleEntry::parse(&buf, 0);
        // The skip itself decodes because offset+1=1 < len=2 and buf[1]=8 != 0.
        // So this becomes Skip, not End.
        assert!(matches!(e, RleEntry::Skip(_)));
        assert_eq!(w, 2);
        // The truly unterminated case: trailing 0 with no follow-up.
        let buf = [42u16, 0]; // pixel, then 0 at the very end.
        let (_, w1) = RleEntry::parse(&buf, 0);
        let (e2, w2) = RleEntry::parse(&buf, w1);
        assert!(matches!(e2, RleEntry::End));
        assert_eq!(w2, 0);
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
