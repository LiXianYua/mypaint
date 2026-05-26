//! Fixed-size tiled surface.
//! 对应 mypaint-fixed-tiled-surface.c。固定大小画布，按 tile 切分存储。

use crate::surface::tile::{TileBackend, TileRequest, TiledSurface, TILE_BUFFER_LEN, TILE_SIZE};
use std::path::Path;

/// 固定大小画布的 TileBackend 实现。
/// 越界 tile 通过 `null_tile` 返回（写入丢弃）。
pub struct FixedTileBackend {
    width: usize,
    height: usize,
    tiles_width: usize,
    tiles_height: usize,
    /// 所有 tile 的连续存储：tiles_width × tiles_height × TILE_BUFFER_LEN
    tile_buffer: Vec<u16>,
    null_tile: Vec<u16>,
}

impl FixedTileBackend {
    pub fn new(width: usize, height: usize) -> Self {
        let tiles_width = (width + TILE_SIZE - 1) / TILE_SIZE;
        let tiles_height = (height + TILE_SIZE - 1) / TILE_SIZE;
        let tile_buffer = vec![0u16; tiles_width * tiles_height * TILE_BUFFER_LEN];
        let null_tile = vec![0u16; TILE_BUFFER_LEN];
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

    /// 计算 tile (tx, ty) 在 buffer 中的起始 u16 偏移。
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
        self.null_tile.iter_mut().for_each(|v| *v = 0);
    }
}

impl TileBackend for FixedTileBackend {
    fn tile_request_start<'a>(&'a mut self, req: &TileRequest) -> &'a mut [u16] {
        match self.tile_offset(req.tx, req.ty) {
            Some(off) => &mut self.tile_buffer[off..off + TILE_BUFFER_LEN],
            None => &mut self.null_tile[..],
        }
    }

    fn tile_request_end(&mut self, req: &TileRequest) {
        // 越界 tile 的修改应丢弃 — 清空 null_tile
        if self.tile_offset(req.tx, req.ty).is_none() {
            self.reset_null_tile();
        }
    }

    fn tile_snapshot(&mut self, tx: i32, ty: i32) -> Option<Vec<u16>> {
        let off = self.tile_offset(tx, ty)?;
        Some(self.tile_buffer[off..off + TILE_BUFFER_LEN].to_vec())
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
                    png_data[dst] = (self.tile_buffer[pix_idx] >> 7) as u8;
                    png_data[dst + 1] = (self.tile_buffer[pix_idx + 1] >> 7) as u8;
                    png_data[dst + 2] = (self.tile_buffer[pix_idx + 2] >> 7) as u8;
                    png_data[dst + 3] = (self.tile_buffer[pix_idx + 3] >> 7) as u8;
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
