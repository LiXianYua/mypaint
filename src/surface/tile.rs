//! Tile-based surface implementation.
//! 对应 mypaint-tiled-surface.c。
//!
//! 架构：
//! 1. 画布按 64×64 tile 分块（`TILE_SIZE`）
//! 2. 每个 tile 是 `[u16; TILE_SIZE*TILE_SIZE*4]` 的 premultiplied RGBA 缓冲区
//! 3. `TileBackend` trait 由用户实现，提供 tile 存储后端（如 fixed buffer / 无限稀疏存储 / GPU 等）
//! 4. `draw_dab` 把 op 推入每个触及 tile 的队列，`end_atomic` 批量处理

use crate::surface::Surface;
use crate::surface::operations::{OperationQueue, OpDrawDab, TileIndex};
use crate::render::DabParams;
use crate::render::dab::{calculate_rr, calculate_rr_antialiased, calculate_opa, MaskParams};
use crate::render::mask::{render_dab_mask, MaskBuffer};
use crate::render::blend::{
    blend_dab_normal, blend_dab_normal_eraser,
    blend_dab_lock_alpha, blend_dab_color, blend_dab_posterize,
    blend_dab_normal_paint, blend_dab_normal_eraser_paint, blend_dab_lock_alpha_paint,
};
use crate::util::rect::{Rect, Rectangles};
use crate::symmetry::SymmetryData;
use crate::smudge::rgb_to_spectral;
use std::path::Path;

/// Tile size in pixels (`MYPAINT_TILE_SIZE`).
pub const TILE_SIZE: usize = 64;
/// Pixels per tile.
pub const TILE_PIXELS: usize = TILE_SIZE * TILE_SIZE;
/// u16 channels per tile (RGBA × TILE_PIXELS).
pub const TILE_BUFFER_LEN: usize = TILE_PIXELS * 4;

const SCALE: u32 = 1 << 15;

/// A tile request. Backend fills `buffer_idx` to point into its storage; or
/// supplies a fresh buffer slice via `tile_request_start`.
/// 对应 MyPaintTileRequest in mypaint-tiled-surface.h。
#[derive(Debug)]
pub struct TileRequest {
    pub tx: i32,
    pub ty: i32,
    pub readonly: bool,
    pub mipmap_level: i32,
    pub thread_id: i32,
}

impl TileRequest {
    pub fn init(level: i32, tx: i32, ty: i32, readonly: bool) -> Self {
        Self { tx, ty, readonly, mipmap_level: level, thread_id: -1 }
    }
}

/// User-implementable trait providing tile storage.
/// 对应 C 的 `tile_request_start` + `tile_request_end` vfunc pair。
///
/// `tile_buffer_mut(req)` 返回该 tile 的可变缓冲区（如果不存在则可能分配或返回 null tile）。
/// `commit(req)` 提交修改（默认为 noop，对于内存后端通常不需要）。
pub trait TileBackend {
    /// 请求 tile 缓冲区。返回的 slice 长度应为 TILE_BUFFER_LEN。
    /// 越界 tile 应返回一个 null/scratch tile（写入会被丢弃）。
    fn tile_request_start<'a>(&'a mut self, req: &TileRequest) -> &'a mut [u16];

    /// 提交（可选）。对于持有 raw buffer 的后端通常是 noop。
    fn tile_request_end(&mut self, req: &TileRequest) {
        let _ = req;
    }

    /// 输出整张画布到 PNG（可选）。
    fn save_png(&mut self, _path: &Path, _x: i32, _y: i32, _width: i32, _height: i32) {
        // 默认实现：不支持
    }

    /// 取一个 readonly tile 的快照（用于 get_color）。
    /// 返回 None 表示该 tile 不存在/越界（视为透明）。
    fn tile_snapshot(&mut self, tx: i32, ty: i32) -> Option<Vec<u16>>;
}

/// Tile-based surface（真正按 tile 渲染，对应 MyPaintTiledSurface）。
pub struct TiledSurface {
    backend: Box<dyn TileBackend>,
    pub symmetry_data: SymmetryData,
    operation_queue: OperationQueue,
    bboxes: Vec<Rect>,
    num_bboxes_dirtied: usize,
}

impl TiledSurface {
    /// 创建一个带自定义 backend 的 tiled surface。
    pub fn with_backend(backend: Box<dyn TileBackend>) -> Self {
        Self {
            backend,
            symmetry_data: SymmetryData::default(),
            operation_queue: OperationQueue::new(),
            bboxes: vec![Rect::default(); 32],
            num_bboxes_dirtied: 0,
        }
    }

    /// 计算 tile 索引（floor 除法，处理负数）。
    #[inline]
    fn pixel_to_tile(p: f32) -> i32 {
        (p.floor() as i32).div_euclid(TILE_SIZE as i32)
    }

    /// 把 op 推入触及的所有 tile 的队列，更新 bbox。
    /// 对应 mypaint-tiled-surface.c:draw_dab_internal。
    fn enqueue_dab(&mut self, op: OpDrawDab, bbox_index: usize) -> bool {
        let r_fringe = op.radius + 1.0;
        let tx1 = Self::pixel_to_tile(op.x - r_fringe);
        let tx2 = Self::pixel_to_tile(op.x + r_fringe);
        let ty1 = Self::pixel_to_tile(op.y - r_fringe);
        let ty2 = Self::pixel_to_tile(op.y + r_fringe);

        for ty in ty1..=ty2 {
            for tx in tx1..=tx2 {
                self.operation_queue.add(TileIndex { x: tx, y: ty }, op);
            }
        }

        // 更新 bbox
        if bbox_index < self.bboxes.len() {
            let bb_x = (op.x - r_fringe).floor() as i32;
            let bb_y = (op.y - r_fringe).floor() as i32;
            let bb_x_max = (op.x + r_fringe).floor() as i32;
            let bb_y_max = (op.y + r_fringe).floor() as i32;
            self.bboxes[bbox_index].expand_to_include_point(bb_x, bb_y);
            self.bboxes[bbox_index].expand_to_include_point(bb_x_max, bb_y_max);
            if bbox_index >= self.num_bboxes_dirtied {
                self.num_bboxes_dirtied = bbox_index + 1;
            }
        }
        true
    }

    /// 处理单个 tile 的所有排队 op。
    /// 对应 mypaint-tiled-surface.c:process_tile。
    fn process_tile(&mut self, tx: i32, ty: i32) {
        let tile_index = TileIndex { x: tx, y: ty };
        let mut ops = self.operation_queue.pop_all(tile_index);
        if ops.is_empty() { return; }

        let req = TileRequest::init(0, tx, ty, false);
        let mut mask_buf = MaskBuffer::new();
        let buffer = self.backend.tile_request_start(&req);
        if buffer.len() < TILE_BUFFER_LEN {
            self.backend.tile_request_end(&req);
            return;
        }

        for op in ops.drain(..) {
            process_op(buffer, &mut mask_buf, tx, ty, &op);
        }

        self.backend.tile_request_end(&req);
    }
}

/// 对单个 tile 应用一个 dab op，先渲染 mask，再按 blend mode 逐通道混合。
/// 对应 mypaint-tiled-surface.c:process_op。
fn process_op(
    rgba: &mut [u16],
    mask_buf: &mut MaskBuffer,
    tx: i32, ty: i32,
    op: &OpDrawDab,
) {
    // mask 计算（tile-local 坐标）
    let local_x = op.x - (tx * TILE_SIZE as i32) as f32;
    let local_y = op.y - (ty * TILE_SIZE as i32) as f32;
    render_dab_mask(mask_buf, local_x, local_y, op.radius, op.hardness, op.softness,
                    op.aspect_ratio, op.angle);
    let mask = mask_buf.as_slice();

    let one_scale = SCALE as f32;

    // Spectral 表示（仅当 paint > 0 时需要）
    let spectral_a: [f32; 10] = if op.paint > 0.0 {
        rgb_to_spectral(op.color_r as f32 / one_scale, op.color_g as f32 / one_scale, op.color_b as f32 / one_scale)
    } else {
        [0.0; 10]
    };

    // Normal (non-paint) pass
    if op.paint < 1.0 {
        if op.normal > 0.0 {
            let opacity = (op.normal * op.opaque * (1.0 - op.paint) * one_scale) as u16;
            if op.color_a >= 1.0 {
                blend_dab_normal(mask, rgba, op.color_r, op.color_g, op.color_b, opacity);
            } else {
                blend_dab_normal_eraser(mask, rgba, op.color_r, op.color_g, op.color_b,
                    (op.color_a * one_scale) as u16, opacity);
            }
        }
        if op.lock_alpha > 0.0 && op.color_a != 0.0 {
            let opacity = (op.lock_alpha * op.opaque * (1.0 - op.colorize) *
                (1.0 - op.posterize) * (1.0 - op.paint) * one_scale) as u16;
            blend_dab_lock_alpha(mask, rgba, op.color_r, op.color_g, op.color_b, opacity);
        }
    }

    // Paint (spectral) pass
    if op.paint > 0.0 {
        if op.normal > 0.0 {
            let opacity = (op.normal * op.opaque * op.paint * one_scale) as u16;
            if op.color_a >= 1.0 {
                blend_dab_normal_paint(mask, rgba, op.color_r, op.color_g, op.color_b, opacity, &spectral_a);
            } else {
                blend_dab_normal_eraser_paint(mask, rgba, op.color_r, op.color_g, op.color_b,
                    (op.color_a * one_scale) as u16, opacity, &spectral_a);
            }
        }
        if op.lock_alpha > 0.0 && op.color_a != 0.0 {
            let opacity = (op.lock_alpha * op.opaque * (1.0 - op.colorize) *
                (1.0 - op.posterize) * op.paint * one_scale) as u16;
            blend_dab_lock_alpha_paint(mask, rgba, op.color_r, op.color_g, op.color_b, opacity, &spectral_a);
        }
    }

    if op.colorize > 0.0 {
        let opacity = (op.colorize * op.opaque * one_scale) as u16;
        blend_dab_color(mask, rgba, op.color_r, op.color_g, op.color_b, opacity);
    }
    if op.posterize > 0.0 {
        let opacity = (op.posterize * op.opaque * one_scale) as u16;
        blend_dab_posterize(mask, rgba, opacity, op.posterize_num);
    }
}

impl Surface for TiledSurface {
    fn draw_dab(&mut self, params: &DabParams) -> bool {
        // 早返：与 C draw_dab_internal 检查一致
        let radius = params.radius.max(0.0);
        let hardness = params.hardness.clamp(0.0, 1.0);
        let softness = params.softness.clamp(0.0, 1.0);
        let opaque = params.opaque.clamp(0.0, 1.0);
        if radius < 0.1 { return false; }
        if hardness == 0.0 { return false; }
        if softness == 1.0 { return false; }
        if opaque == 0.0 { return false; }

        // 预计算 op
        let lock_alpha = params.lock_alpha.clamp(0.0, 1.0);
        let colorize = params.colorize.clamp(0.0, 1.0);
        let posterize = params.posterize.clamp(0.0, 1.0);
        // posterize_num: 0.01..1.28 → ROUND(*100) → 1..128
        let posterize_num = ((params.posterize_num.clamp(0.01, 1.28) * 100.0)
            .round() as u16).clamp(1, 128);
        let paint = params.paint.clamp(0.0, 1.0);

        let mut normal = 1.0_f32;
        normal *= 1.0 - lock_alpha;
        normal *= 1.0 - colorize;
        normal *= 1.0 - posterize;

        let aspect_ratio = params.aspect_ratio.max(1.0);

        let op = OpDrawDab {
            x: params.x, y: params.y, radius,
            color_r: (params.color_r.clamp(0.0, 1.0) * SCALE as f32) as u16,
            color_g: (params.color_g.clamp(0.0, 1.0) * SCALE as f32) as u16,
            color_b: (params.color_b.clamp(0.0, 1.0) * SCALE as f32) as u16,
            color_a: params.alpha_eraser.clamp(0.0, 1.0),
            opaque, hardness, softness,
            aspect_ratio, angle: params.angle,
            lock_alpha, colorize, posterize, posterize_num,
            paint, normal,
        };

        // 主 dab + symmetry pass
        // self.symmetry_data.update() 已在 begin_atomic 中调用
        let num_points = self.symmetry_data.num_symmetry_points();
        let mut modified = false;
        for sym_idx in 0..num_points {
            let (sx, sy) = self.symmetry_data.transform_point(sym_idx, op.x, op.y);
            let mut op_sym = op;
            op_sym.x = sx;
            op_sym.y = sy;
            // bbox index per dab (capped at bboxes.len()-1)
            let bb_idx = sym_idx.min(self.bboxes.len() - 1);
            modified |= self.enqueue_dab(op_sym, bb_idx);
        }
        modified
    }

    fn get_color(&mut self, x: f32, y: f32, radius: f32, paint: f32) -> (f32, f32, f32, f32) {
        if radius < 0.1 { return (0.0, 0.0, 0.0, 0.0); }

        // 与 draw_dab 一样按 tile 切分，遍历每个 tile 用 RLE mask 加权采样
        let r_fringe = radius + 1.0;
        let tx1 = Self::pixel_to_tile(x - r_fringe);
        let tx2 = Self::pixel_to_tile(x + r_fringe);
        let ty1 = Self::pixel_to_tile(y - r_fringe);
        let ty2 = Self::pixel_to_tile(y + r_fringe);

        let mask_params = MaskParams::from_hardness_softness(0.5, 0.5);
        let one_over_radius2 = 1.0 / (radius * radius);

        let (mut sum_w, mut sr, mut sg, mut sb, mut sa) = (0.0f32, 0.0, 0.0, 0.0, 0.0);
        for ty in ty1..=ty2 {
            for tx in tx1..=tx2 {
                let Some(tile) = self.backend.tile_snapshot(tx, ty) else { continue };
                let local_x = x - (tx * TILE_SIZE as i32) as f32;
                let local_y = y - (ty * TILE_SIZE as i32) as f32;
                let _ = paint; // 简化版仍用线性，paint > 0 留 TODO
                accumulate_tile_color(&tile, local_x, local_y,
                    aspect(1.0), 0.0, 1.0, one_over_radius2, &mask_params,
                    &mut sum_w, &mut sr, &mut sg, &mut sb, &mut sa);
            }
        }
        if sum_w == 0.0 { return (0.0, 0.0, 0.0, 0.0); }
        (sr / sum_w, sg / sum_w, sb / sum_w, sa / sum_w)
    }

    /// 对应 mypaint_tiled_surface_begin_atomic：更新 symmetry 矩阵，准备 bbox。
    fn begin_atomic(&mut self) {
        self.symmetry_data.update();
        self.num_bboxes_dirtied = 0;
        for b in self.bboxes.iter_mut() {
            *b = Rect::default();
        }
    }

    /// 对应 mypaint_tiled_surface_end_atomic：处理所有 dirty tile，返回 ROI。
    fn end_atomic(&mut self) -> Rectangles {
        // 遍历所有 dirty tile（在 enqueue 时累积）
        let dirty: Vec<TileIndex> = self.operation_queue.dirty_tiles().collect();
        for ti in dirty {
            self.process_tile(ti.x, ti.y);
        }
        self.operation_queue.clear_dirty_tiles();

        let mut out = Vec::with_capacity(self.num_bboxes_dirtied);
        for i in 0..self.num_bboxes_dirtied {
            let b = self.bboxes[i];
            if b.width > 0 && b.height > 0 {
                out.push(b);
            }
        }
        self.num_bboxes_dirtied = 0;
        Rectangles { rects: out }
    }

    fn save_png(&mut self, path: &Path, x: i32, y: i32, width: i32, height: i32) {
        self.backend.save_png(path, x, y, width, height);
    }
}

/// 一个 tile snapshot 的加权采样累加。供 get_color 使用。
fn accumulate_tile_color(
    tile: &[u16], local_x: f32, local_y: f32,
    aspect_ratio: f32, sn: f32, cs: f32, one_over_radius2: f32,
    mask_params: &MaskParams,
    sum_w: &mut f32, sum_r: &mut f32, sum_g: &mut f32, sum_b: &mut f32, sum_a: &mut f32,
) {
    for py in 0..TILE_SIZE {
        for px in 0..TILE_SIZE {
            let rr = calculate_rr(px as i32, py as i32, local_x, local_y,
                aspect_ratio, sn, cs, one_over_radius2);
            let opa = calculate_opa(rr, mask_params);
            if opa <= 0.0 { continue; }
            let idx = (py * TILE_SIZE + px) * 4;
            *sum_w += opa;
            *sum_r += opa * tile[idx]     as f32 / SCALE as f32;
            *sum_g += opa * tile[idx + 1] as f32 / SCALE as f32;
            *sum_b += opa * tile[idx + 2] as f32 / SCALE as f32;
            *sum_a += opa * tile[idx + 3] as f32 / SCALE as f32;
        }
    }
}

#[inline]
fn aspect(v: f32) -> f32 { v.max(1.0) }
