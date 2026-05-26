//! Fixed-size tiled surface.
//! 对应 mypaint-fixed-tiled-surface.c。固定大小画布，按 tile 切分存储。

use crate::render::mask::Premul15;
use crate::surface::tile::{TileBackend, TileRequest, TiledSurface, TILE_BUFFER_LEN, TILE_SIZE};
use std::path::Path;

/// 固定大小画布的 TileBackend 实现。
/// 越界 tile 通过 `null_tile` 返回（写入丢弃）。
pub struct FixedTileBackend {
    width: usize,
    height: usize,
    tiles_width: usize,
    tiles_height: usize,
    /// 所有 tile 的连续 premultiplied RGBA 存储：
    /// `tiles_width × tiles_height × TILE_BUFFER_LEN` 个 [`Premul15`]。
    tile_buffer: Vec<Premul15>,
    null_tile: Vec<Premul15>,
}

/// 单个 tile 的可变 buffer 指针 + 元数据。供并行渲染用，调用者要保证
/// 多个 TileSlot 的 buffer ptr 不重叠。
#[doc(hidden)]
pub struct TileSlot {
    pub tx: i32,
    pub ty: i32,
    /// raw ptr 指向 tile 的 u16 buffer（长度 TILE_BUFFER_LEN）。
    /// 不同 TileSlot 的指针保证互不重叠。内部存储是 [`Premul15`]，
    /// 通过 `#[repr(transparent)]` 重新解释为 `*mut u16`，供 process_op
    /// 等接受 `&mut [u16]` 的 API 使用。
    pub buffer: *mut u16,
}

// SAFETY: 调用者保证 tx/ty 不重复 → 不同 TileSlot 指向 disjoint memory，
// 每个 TileSlot 在并行作业期间被唯一一个线程持有。
unsafe impl Send for TileSlot {}
unsafe impl Sync for TileSlot {}

impl FixedTileBackend {
    /// 创建一个固定大小画布的 backend。背景初始为透明黑（[`Premul15::ZERO`]）。
    pub fn new(width: usize, height: usize) -> Self {
        let tiles_width = width.div_ceil(TILE_SIZE);
        let tiles_height = height.div_ceil(TILE_SIZE);
        let tile_buffer = vec![Premul15::ZERO; tiles_width * tiles_height * TILE_BUFFER_LEN];
        let null_tile = vec![Premul15::ZERO; TILE_BUFFER_LEN];
        Self {
            width,
            height,
            tiles_width,
            tiles_height,
            tile_buffer,
            null_tile,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }

    /// 计算 tile (tx, ty) 在 buffer 中的起始偏移（按 [`Premul15`] 个数计）。
    /// 越界返回 None。
    fn tile_offset(&self, tx: i32, ty: i32) -> Option<usize> {
        if tx < 0 || ty < 0 {
            return None;
        }
        let tx = tx as usize;
        let ty = ty as usize;
        if tx >= self.tiles_width || ty >= self.tiles_height {
            return None;
        }
        let row_stride = self.tiles_width * TILE_BUFFER_LEN;
        Some(ty * row_stride + tx * TILE_BUFFER_LEN)
    }

    fn reset_null_tile(&mut self) {
        self.null_tile.iter_mut().for_each(|v| *v = Premul15::ZERO);
    }

    /// 为并行渲染收集多个不重叠 tile 的可变指针。
    ///
    /// # Safety
    /// 调用者保证 `tiles` 中的 (tx, ty) 互不重复。返回的 TileSlot 之间
    /// 内存不重叠，可并行使用。
    #[doc(hidden)]
    pub unsafe fn parallel_tile_slots(&mut self, tiles: &[(i32, i32)]) -> Vec<TileSlot> {
        debug_assert!(
            {
                let mut seen = std::collections::HashSet::new();
                tiles.iter().all(|t| seen.insert(t))
            },
            "parallel_tile_slots requires unique (tx, ty) — duplicates would alias"
        );
        // 用 *mut u16 暴露给 process_op；Premul15 的 #[repr(transparent)]
        // 保证 layout 等价。
        let base_ptr = self.tile_buffer.as_mut_ptr() as *mut u16;
        tiles
            .iter()
            .filter_map(|&(tx, ty)| {
                let off = self.tile_offset(tx, ty)?;
                Some(TileSlot {
                    tx,
                    ty,
                    buffer: base_ptr.add(off),
                })
            })
            .collect()
    }
}

impl TileBackend for FixedTileBackend {
    fn tile_request_start<'a>(&'a mut self, req: &TileRequest) -> &'a mut [u16] {
        // backend 内部用 Vec<Premul15>，trait 仍按 [u16] 接口暴露给 caller
        // （process_op / blend 等接受 &mut [u16] 然后内部 cast 回 Premul15
        // slice）。这里用 safe 方向的 slice cast (Premul15 ⊆ u16)。
        let slice = match self.tile_offset(req.tx, req.ty) {
            Some(off) => &mut self.tile_buffer[off..off + TILE_BUFFER_LEN],
            None => &mut self.null_tile[..],
        };
        Premul15::slice_as_u16_mut(slice)
    }

    fn tile_request_end(&mut self, req: &TileRequest) {
        // 越界 tile 的修改应丢弃 — 清空 null_tile
        if self.tile_offset(req.tx, req.ty).is_none() {
            self.reset_null_tile();
        }
    }

    fn tile_snapshot(&mut self, tx: i32, ty: i32) -> Option<Vec<u16>> {
        let off = self.tile_offset(tx, ty)?;
        // 返回 raw u16 Vec 给外部消费者（FFI / 检查点工具）。
        Some(Premul15::slice_as_u16(&self.tile_buffer[off..off + TILE_BUFFER_LEN]).to_vec())
    }

    fn save_png(&mut self, path: &Path, x: i32, y: i32, width: i32, height: i32) {
        let x = x.max(0) as usize;
        let y = y.max(0) as usize;
        let w = (width as usize).min(self.width.saturating_sub(x));
        let h = (height as usize).min(self.height.saturating_sub(y));
        if w == 0 || h == 0 {
            return;
        }

        // 拷贝到一张线性 u8 RGBA 图
        let mut png_data = vec![0u8; w * h * 4];
        for py in 0..h {
            let cy = y + py;
            let ty = (cy / TILE_SIZE) as i32;
            let in_tile_y = cy % TILE_SIZE;
            for px in 0..w {
                let cx = x + px;
                let tx = (cx / TILE_SIZE) as i32;
                let in_tile_x = cx % TILE_SIZE;
                if let Some(off) = self.tile_offset(tx, ty) {
                    let pix_idx = off + (in_tile_y * TILE_SIZE + in_tile_x) * 4;
                    let dst = (py * w + px) * 4;
                    // 15-bit premul → 8-bit sRGB-ish；to_u8 处理了 SCALE=32768
                    // 的边界（32768 >> 7 = 256，naive `as u8` 会 wrap 到 0）。
                    png_data[dst] = self.tile_buffer[pix_idx].to_u8();
                    png_data[dst + 1] = self.tile_buffer[pix_idx + 1].to_u8();
                    png_data[dst + 2] = self.tile_buffer[pix_idx + 2].to_u8();
                    png_data[dst + 3] = self.tile_buffer[pix_idx + 3].to_u8();
                }
            }
        }

        let file = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut encoder = png::Encoder::new(file, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = match encoder.write_header() {
            Ok(w) => w,
            Err(_) => return,
        };
        let _ = writer.write_image_data(&png_data);
    }
}

/// 高层封装：固定大小的 TiledSurface。等价 mypaint-fixed-tiled-surface.c。
pub struct FixedTiledSurface {
    inner: TiledSurface,
    width: usize,
    height: usize,
}

impl FixedTiledSurface {
    /// 创建一个固定大小的画布。背景初始为透明黑 ([`Premul15::ZERO`])。
    pub fn new(width: usize, height: usize) -> Self {
        let backend = Box::new(FixedTileBackend::new(width, height));
        Self {
            inner: TiledSurface::with_backend(backend),
            width,
            height,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
}

#[cfg(feature = "parallel")]
impl FixedTiledSurface {
    /// 并行版本的 end_atomic：使用 rayon 把 tile 渲染分发到多个线程。
    /// 对应 C 上游 `threadsafe_tile_requests=TRUE` 时的 OpenMP 行为。
    ///
    /// 仅在启用 `parallel` feature 时可用。
    pub fn end_atomic_parallel(&mut self) -> crate::util::rect::Rectangles {
        use crate::surface::operations::TileIndex;
        use crate::surface::tile::{process_op, TILE_BUFFER_LEN};
        use rayon::prelude::*;

        // 收集所有 dirty tiles
        let dirty: Vec<TileIndex> = self.inner.operation_queue.dirty_tiles().collect();
        if dirty.is_empty() {
            // 没有 op，沿用串行逻辑（也只是清空 bbox 状态）
            use crate::surface::Surface as _;
            return self.inner.end_atomic();
        }

        // pop 所有 tile 的 ops（在主线程串行做，避免 HashMap 并发问题）
        let tile_ops: Vec<(TileIndex, Vec<crate::surface::operations::OpDrawDab>)> = dirty
            .iter()
            .map(|&ti| {
                let ops = self.inner.operation_queue.pop_all(ti);
                (ti, ops)
            })
            .filter(|(_, ops)| !ops.is_empty())
            .collect();

        // 对应的 (tx, ty) 列表
        let tile_coords: Vec<(i32, i32)> = tile_ops.iter().map(|(ti, _)| (ti.x, ti.y)).collect();

        // 拿到 backend 的 FixedTileBackend（unsafe downcast 通过类型已知）
        // SAFETY: FixedTiledSurface 总是用 FixedTileBackend 构造（new() 中）
        let backend_ptr = self.inner.backend.as_mut() as *mut dyn crate::surface::tile::TileBackend
            as *mut FixedTileBackend;
        let slots: Vec<TileSlot> = unsafe {
            // 各 (tx,ty) 唯一 → 各 buffer 指针不重叠
            (*backend_ptr).parallel_tile_slots(&tile_coords)
        };

        // 把 (slot, ops) 配对并并行处理
        tile_ops
            .into_par_iter()
            .zip(slots.into_par_iter())
            .for_each(|((tile_idx, ops), slot)| {
                // SAFETY: 每个 slot 的 buffer 指针对应一个唯一 tile，
                // par_iter 保证当前线程独占该 slot
                let buf: &mut [u16] =
                    unsafe { std::slice::from_raw_parts_mut(slot.buffer, TILE_BUFFER_LEN) };
                let mut mask_buf = crate::render::mask::MaskBuffer::new();
                for op in &ops {
                    process_op(buf, &mut mask_buf, tile_idx.x, tile_idx.y, op);
                }
            });

        self.inner.operation_queue.clear_dirty_tiles();

        // 收集 dirty bboxes 输出
        let mut out = Vec::with_capacity(self.inner.num_bboxes_dirtied);
        for i in 0..self.inner.num_bboxes_dirtied {
            let b = self.inner.bboxes[i];
            if b.width > 0 && b.height > 0 {
                out.push(b);
            }
        }
        self.inner.num_bboxes_dirtied = 0;
        crate::util::rect::Rectangles { rects: out }
    }
}

impl std::ops::Deref for FixedTiledSurface {
    type Target = TiledSurface;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for FixedTiledSurface {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
