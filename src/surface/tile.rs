//! Tile-based surface implementation.
//! Corresponds to mypaint-tiled-surface.c.

use crate::surface::Surface;
use crate::render::DabParams;
use crate::render::dab::{calculate_rr, calculate_opa, MaskParams};
use crate::render::blend::{
    blend_pixel_normal, blend_pixel_normal_eraser,
    blend_pixel_lock_alpha, blend_pixel_color, blend_pixel_posterize,
};
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
    /// 对应 mypaint-tiled-surface.c:render_dab_mask + process_op 的合并版本。
    fn render_dab_at(&mut self, params: &DabParams) {
        let radius = params.radius;
        if radius < 0.1 || params.hardness == 0.0 {
            return;
        }

        // Pre-compute mask falloff parameters
        let mask_params = MaskParams::from_hardness_softness(params.hardness, params.softness);
        let aspect_ratio = params.aspect_ratio.max(1.0);
        let angle_rad = params.angle.to_radians();
        let cs = angle_rad.cos();
        let sn = angle_rad.sin();
        let one_over_radius2 = 1.0 / (radius * radius);

        let opacity = (params.opaque.clamp(0.0, 1.0) * SCALE as f32) as u16;
        let color_r = (params.color_r.clamp(0.0, 1.0) * SCALE as f32) as u16;
        let color_g = (params.color_g.clamp(0.0, 1.0) * SCALE as f32) as u16;
        let color_b = (params.color_b.clamp(0.0, 1.0) * SCALE as f32) as u16;
        let color_a = (params.alpha_eraser.clamp(0.0, 1.0) * SCALE as f32) as u16;

        let num_points = self.symmetry_data.num_symmetry_points();

        for sym_idx in 0..num_points {
            let (sx, sy) = self.symmetry_data.transform_point(sym_idx, params.x, params.y);

            // Bounding box (放宽 1 像素，与 C 的 r_fringe = radius+1 对应)
            let r_fringe = radius + 1.0;
            let x0 = ((sx - r_fringe).floor() as i32).max(0) as usize;
            let y0 = ((sy - r_fringe).floor() as i32).max(0) as usize;
            let x1 = ((sx + r_fringe).floor() as i32).min(self.width as i32 - 1).max(0) as usize;
            let y1 = ((sy + r_fringe).floor() as i32).min(self.height as i32 - 1).max(0) as usize;

            if x0 > x1 || y0 > y1 { continue; }

            // Expand atomic bbox
            if let Some(ref mut bbox) = self.current_bbox {
                bbox.expand_to_include_point(x0 as i32, y0 as i32);
                bbox.expand_to_include_point(x1 as i32, y1 as i32);
            }

            for py in y0..=y1 {
                for px in x0..=x1 {
                    let rr = calculate_rr(
                        px as i32, py as i32, sx, sy,
                        aspect_ratio, sn, cs, one_over_radius2);
                    let opa = calculate_opa(rr, &mask_params);
                    let mask_val = (opa * SCALE as f32) as u16;
                    if mask_val == 0 { continue; }

                    let idx = self.pixel_index(px, py);
                    let pixel: &mut [u16; 4] = (&mut self.pixel_buffer[idx..idx + 4])
                        .try_into().unwrap();

                    // Posterize is applied first (independent of blend mode)
                    if params.posterize > 0.0 {
                        blend_pixel_posterize(
                            pixel, mask_val,
                            (params.posterize.clamp(0.0, 1.0) * SCALE as f32) as u16,
                            (params.posterize_num.clamp(0.0, 1.28) * SCALE as f32) as u16,
                        );
                    }

                    // Choose blend mode: priority lock_alpha > colorize > eraser > normal
                    if params.lock_alpha >= 1.0 {
                        blend_pixel_lock_alpha(pixel, mask_val,
                            color_r, color_g, color_b, opacity);
                    } else if params.colorize >= 1.0 {
                        blend_pixel_color(pixel, mask_val,
                            color_r, color_g, color_b, opacity);
                    } else if params.alpha_eraser < (SCALE as f32 - 1.0) / SCALE as f32 {
                        blend_pixel_normal_eraser(pixel, mask_val,
                            color_r, color_g, color_b, color_a, opacity);
                    } else {
                        blend_pixel_normal(pixel, mask_val,
                            color_r, color_g, color_b, opacity);
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

    /// 对应 mypaint-tiled-surface.c:get_color + brushmodes.c:get_color_pixels_accumulate。
    /// - paint < 0：legacy 模式（线性 RGB 加权平均）
    /// - paint = 0：仅加性 RGB
    /// - paint = 1：仅光谱
    /// - 0 < paint < 1：两者混合
    fn get_color(&mut self, x: f32, y: f32, radius: f32, paint: f32) -> (f32, f32, f32, f32) {
        use crate::smudge::{rgb_to_spectral, spectral_to_rgb};
        use crate::render::dab::{MaskParams, calculate_rr, calculate_opa};

        if radius < 0.1 {
            return (0.0, 0.0, 0.0, 0.0);
        }

        // dab-mask 形状：与渲染同样用 hardness=0.5, softness=0.5 的圆形 mask
        let mask_params = MaskParams::from_hardness_softness(0.5, 0.5);
        let one_over_radius2 = 1.0 / (radius * radius);
        let r_fringe = radius + 1.0;
        let x0 = ((x - r_fringe).floor() as i32).max(0) as usize;
        let y0 = ((y - r_fringe).floor() as i32).max(0) as usize;
        let x1 = ((x + r_fringe).floor() as i32).min(self.width as i32 - 1).max(0) as usize;
        let y1 = ((y + r_fringe).floor() as i32).min(self.height as i32 - 1).max(0) as usize;
        if x0 > x1 || y0 > y1 {
            return (0.0, 0.0, 0.0, 0.0);
        }

        // Legacy fallback (paint < 0)：线性 RGB 加权平均，无光谱
        if paint < 0.0 {
            let mut sum_w: f32 = 0.0;
            let (mut sr, mut sg, mut sb, mut sa) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for py in y0..=y1 {
                for px in x0..=x1 {
                    let rr = calculate_rr(px as i32, py as i32, x, y, 1.0, 0.0, 1.0, one_over_radius2);
                    let opa = calculate_opa(rr, &mask_params);
                    if opa <= 0.0 { continue; }
                    let idx = self.pixel_index(px, py);
                    sum_w += opa;
                    sr += opa * self.pixel_buffer[idx] as f32 / SCALE as f32;
                    sg += opa * self.pixel_buffer[idx + 1] as f32 / SCALE as f32;
                    sb += opa * self.pixel_buffer[idx + 2] as f32 / SCALE as f32;
                    sa += opa * self.pixel_buffer[idx + 3] as f32 / SCALE as f32;
                }
            }
            if sum_w == 0.0 { return (0.0, 0.0, 0.0, 0.0); }
            return (sr / sum_w, sg / sum_w, sb / sum_w, sa / sum_w);
        }

        // 标准路径：加性 + 可选光谱
        let mut sum_a: f32 = 0.0;
        let mut avg_spectral = [0.0f32; 10];
        let mut avg_rgb = [0.0f32; 3];
        let mut weight: f32 = 0.0;

        for py in y0..=y1 {
            for px in x0..=x1 {
                let rr = calculate_rr(px as i32, py as i32, x, y, 1.0, 0.0, 1.0, one_over_radius2);
                let opa = calculate_opa(rr, &mask_params);
                if opa <= 0.0 { continue; }
                let idx = self.pixel_index(px, py);
                let pa = self.pixel_buffer[idx + 3] as f32 / SCALE as f32;
                let a = opa * pa;
                let alpha_sums = a + sum_a;
                weight += opa;
                let (fac_a, fac_b) = if alpha_sums > 0.0 {
                    (a / alpha_sums, 1.0 - a / alpha_sums)
                } else {
                    (1.0, 1.0)
                };
                if pa > 0.0 {
                    if paint > 0.0 {
                        let spec = rgb_to_spectral(
                            self.pixel_buffer[idx] as f32 / self.pixel_buffer[idx + 3] as f32,
                            self.pixel_buffer[idx + 1] as f32 / self.pixel_buffer[idx + 3] as f32,
                            self.pixel_buffer[idx + 2] as f32 / self.pixel_buffer[idx + 3] as f32);
                        for i in 0..10 {
                            avg_spectral[i] = spec[i].powf(fac_a) * avg_spectral[i].powf(fac_b);
                        }
                    }
                    if paint < 1.0 {
                        for i in 0..3 {
                            avg_rgb[i] = self.pixel_buffer[idx + i] as f32 * fac_a / self.pixel_buffer[idx + 3] as f32
                                + avg_rgb[i] * fac_b;
                        }
                    }
                }
                sum_a += a;
            }
        }
        if weight == 0.0 { return (0.0, 0.0, 0.0, 0.0); }
        let sum_a_norm = sum_a / weight;

        let (spec_r, spec_g, spec_b) = spectral_to_rgb(&avg_spectral);
        let r = spec_r * paint + (1.0 - paint) * avg_rgb[0];
        let g = spec_g * paint + (1.0 - paint) * avg_rgb[1];
        let b = spec_b * paint + (1.0 - paint) * avg_rgb[2];
        (r, g, b, sum_a_norm)
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
