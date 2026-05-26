//! RLE-encoded dab mask renderer.
//! 对应 mypaint-tiled-surface.c:render_dab_mask。
//!
//! 一个 tile (64×64) 的 mask buffer 用 run-length encoding：
//! - 连续非零值：每个值是该像素的 opacity (0..32768)
//! - 0 后跟一个跳过计数：表示 N 个像素直接跳过
//! - 末尾 0,0 表示结束
//!
//! Blend 函数遍历这个 buffer 来高效跳过透明区域。

use crate::render::dab::{calculate_opa, calculate_rr, calculate_rr_antialiased, MaskParams};
use crate::surface::tile::TILE_SIZE;

const SCALE: u32 = 1 << 15;

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
            let opa_u = (opa * SCALE as f32) as u16;
            if opa_u == 0 {
                skip += 1;
            } else {
                if skip > 0 {
                    // 写入 0, skip*4 (C 的 mask 索引步长是 4 因为是 rgba)
                    mask_buf.buf[write_idx] = 0;
                    write_idx += 1;
                    mask_buf.buf[write_idx] = (skip * 4).min(u16::MAX as usize) as u16;
                    write_idx += 1;
                    skip = 0;
                }
                mask_buf.buf[write_idx] = opa_u;
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

    #[test]
    fn empty_mask_when_radius_outside_tile() {
        let mut mb = MaskBuffer::new();
        render_dab_mask(&mut mb, -100.0, -100.0, 5.0, 0.8, 0.0, 1.0, 0.0);
        let slice = mb.as_slice();
        // 至少有终止符
        assert!(slice.len() >= 2);
        // 全是 0
        assert!(slice.iter().all(|&v| v == 0 || v != 0 && false));
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
