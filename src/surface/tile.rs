//! Tile-based surface implementation.
//! Corresponds to mypaint-tiled-surface.c.

use crate::surface::Surface;
use crate::render::DabParams;
use crate::render::dab::{calculate_rr, dab_mask_value_scaled};
use crate::render::blend::{blend_pixel_normal, blend_pixel_eraser, blend_pixel_lock_alpha, blend_pixel_colorize, blend_pixel_posterize};
use crate::util::rect::{Rect, Rectangles};
use crate::symmetry::SymmetryData;
use std::path::Path;

/// Tile size in pixels. Default is 64 (from mypaint-config.h).
pub const TILE_SIZE: usize = 64;
const SCALE: u32 = 1 << 15;

/// A tile request. Corresponds to MyPaintTileRequest.
pub struct TileRequest {
    pub tx: i32,
    pub ty: i32,
    pub readonly: bool,
    pub buffer: Option<Vec<u16>>,
    pub thread_id: i32,
    pub mipmap_level: i32,
}

impl TileRequest {
    pub fn init(level: i32, tx: i32, ty: i32, readonly: bool) -> Self {
        Self {
            tx, ty, readonly,
            buffer: None,
            thread_id: 0,
            mipmap_level: level,
        }
    }
}

/// Tile-based surface.
pub struct TiledSurface {
    pub symmetry_data: SymmetryData,
    pixel_buffer: Vec<u16>,
    width: usize,
    height: usize,
    bboxes: Vec<Rect>,
    current_bbox: Option<Rect>,
}

impl TiledSurface {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            symmetry_data: SymmetryData::default(),
            pixel_buffer: vec![0; width * height * 4],
            width,
            height,
            bboxes: Vec::new(),
            current_bbox: None,
        }
    }

    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }

    #[inline]
    fn pixel_index(&self, x: usize, y: usize) -> usize {
        (y * self.width + x) * 4
    }

    fn clamp_coords(&self, x: i32, y: i32) -> (usize, usize) {
        (
            x.clamp(0, self.width as i32 - 1) as usize,
            y.clamp(0, self.height as i32 - 1) as usize,
        )
    }

    /// Render a single dab at a given position with symmetry transforms.
    fn render_dab_at(&mut self, params: &DabParams) {
        let radius = params.radius;
        let iradius = (radius as i32).max(1) + 1;
        let cx = params.x.round() as i32;
        let cy = params.y.round() as i32;

        let num_points = self.symmetry_data.num_symmetry_points();

        for sym_idx in 0..num_points {
            let (sx, sy) = self.symmetry_data.transform_point(sym_idx, params.x, params.y);
            let scx = sx.round() as i32;
            let scy = sy.round() as i32;

            // Compute bounding box for this dab
            let x0 = (scx - iradius).max(0) as usize;
            let y0 = (scy - iradius).max(0) as usize;
            let x1 = (scx + iradius).min(self.width as i32 - 1) as usize;
            let y1 = (scy + iradius).min(self.height as i32 - 1) as usize;

            if x0 >= x1 || y0 >= y1 { continue; }

            // Expand atomic bbox
            if let Some(ref mut bbox) = self.current_bbox {
                bbox.expand_to_include_point(x0 as i32, y0 as i32);
                bbox.expand_to_include_point(x1 as i32, y1 as i32);
            }

            let mask_hardness = params.hardness;
            let mask_softness = params.softness;
            let opacity = (params.opaque * SCALE as f32) as u16;

            let color_r = (params.color_r * SCALE as f32) as u16;
            let color_g = (params.color_g * SCALE as f32) as u16;
            let color_b = (params.color_b * SCALE as f32) as u16;

            let aspect_ratio = params.aspect_ratio;
            let angle = params.angle;

            for py in y0..=y1 {
                for px in x0..=x1 {
                    let dx = (px as i32 - scx) as f32;
                    let dy = (py as i32 - scy) as f32;
                    let rr = calculate_rr(dx, dy, aspect_ratio, angle);

                    if rr > radius * radius { continue; }

                    let mask_val = dab_mask_value_scaled(rr, radius, mask_hardness, mask_softness);
                    if mask_val == 0 { continue; }

                    let idx = self.pixel_index(px, py);
                    let pixel = &mut self.pixel_buffer[idx..idx + 4];

                    // Apply blend mode based on params
                    if params.posterize > 0.0 {
                        blend_pixel_posterize(
                            pixel.try_into().unwrap(),
                            mask_val,
                            (params.posterize * SCALE as f32) as u16,
                            (params.posterize_num * SCALE as f32) as u16,
                        );
                    }

                    if params.lock_alpha > 0.5 {
                        blend_pixel_lock_alpha(
                            pixel.try_into().unwrap(),
                            mask_val,
                            color_r, color_g, color_b,
                            opacity,
                        );
                    } else if params.colorize > 0.5 {
                        blend_pixel_colorize(
                            pixel.try_into().unwrap(),
                            mask_val,
                            color_r, color_g, color_b,
                            opacity,
                        );
                    } else if params.alpha_eraser < 1.0 {
                        blend_pixel_eraser(
                            pixel.try_into().unwrap(),
                            mask_val,
                            color_r, color_g, color_b,
                            (params.alpha_eraser * SCALE as f32) as u16,
                            opacity,
                        );
                    } else {
                        blend_pixel_normal(
                            pixel.try_into().unwrap(),
                            mask_val,
                            color_r, color_g, color_b,
                            opacity,
                        );
                    }
                }
            }
        }
    }
}

impl Surface for TiledSurface {
    fn draw_dab(&mut self, params: &DabParams) -> bool {
        self.symmetry_data.update();
        self.render_dab_at(params);
        true
    }

    fn get_color(&mut self, x: f32, y: f32, radius: f32, _paint: f32) -> (f32, f32, f32, f32) {
        // Sample color at position — average over radius
        let iradius = radius as i32;
        let cx = x.round() as i32;
        let cy = y.round() as i32;
        let mut sum_r = 0.0;
        let mut sum_g = 0.0;
        let mut sum_b = 0.0;
        let mut sum_a = 0.0;
        let mut count = 0;

        for dy in -iradius..=iradius {
            for dx in -iradius..=iradius {
                if dx * dx + dy * dy > iradius * iradius { continue; }
                let px = (cx + dx).clamp(0, self.width as i32 - 1) as usize;
                let py = (cy + dy).clamp(0, self.height as i32 - 1) as usize;
                let idx = self.pixel_index(px, py);
                sum_r += self.pixel_buffer[idx] as f32 / SCALE as f32;
                sum_g += self.pixel_buffer[idx + 1] as f32 / SCALE as f32;
                sum_b += self.pixel_buffer[idx + 2] as f32 / SCALE as f32;
                sum_a += self.pixel_buffer[idx + 3] as f32 / SCALE as f32;
                count += 1;
            }
        }

        if count == 0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let inv = 1.0 / count as f32;
        (sum_r * inv, sum_g * inv, sum_b * inv, sum_a * inv)
    }

    fn begin_atomic(&mut self) {
        self.current_bbox = Some(Rect::new(0, 0, 0, 0));
    }

    fn end_atomic(&mut self) -> Rectangles {
        if let Some(bbox) = self.current_bbox.take() {
            if bbox.width > 0 && bbox.height > 0 {
                self.bboxes.push(bbox);
            }
        }
        let rects = self.bboxes.drain(..).collect();
        Rectangles { rects }
    }

    fn save_png(&mut self, path: &Path, x: i32, y: i32, width: i32, height: i32) {
        let x = x.max(0) as usize;
        let y = y.max(0) as usize;
        let w = width.min(self.width as i32 - x as i32) as usize;
        let h = height.min(self.height as i32 - y as i32) as usize;

        // Convert u16 RGBA to u8 RGBA for PNG
        let mut png_data = vec![0u8; w * h * 4];
        for py in 0..h {
            for px in 0..w {
                let src_idx = self.pixel_index(x + px, y + py);
                let dst_idx = (py * w + px) * 4;
                png_data[dst_idx] = (self.pixel_buffer[src_idx] >> 7) as u8;
                png_data[dst_idx + 1] = (self.pixel_buffer[src_idx + 1] >> 7) as u8;
                png_data[dst_idx + 2] = (self.pixel_buffer[src_idx + 2] >> 7) as u8;
                png_data[dst_idx + 3] = (self.pixel_buffer[src_idx + 3] >> 7) as u8;
            }
        }

        let mut encoder = png::Encoder::new(
            std::fs::File::create(path).expect("create PNG file"),
            w as u32,
            h as u32,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write PNG header");
        writer.write_image_data(&png_data).expect("write PNG data");
    }
}
