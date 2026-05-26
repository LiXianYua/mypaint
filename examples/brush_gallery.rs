//! Brush gallery: 用 5 种不同的笔刷各画一条曲线，保存为一张 PNG。
//!
//! 运行: cargo run --release --example brush_gallery
//! 输出: brush_gallery.png

use mypaint::surface::fixed::FixedTiledSurface;
use mypaint::Brush;
use mypaint::Surface;
use std::fs;
use std::path::Path;

const W: usize = 800;
const H: usize = 500;

fn draw_curve(brush: &mut Brush, surface: &mut FixedTiledSurface, y_offset: f32, label: &str) {
    eprintln!("Painting: {label}");
    // reset stroke
    brush.stroke_to(
        &mut **surface,
        50.0, y_offset, 0.0, 0.0, 0.0, 0.01, 1.0, 0.0, 0.0, false,
    );

    let steps = 300;
    let dt = 0.016;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let x = 50.0 + t * 700.0;
        let y = y_offset + (t * std::f32::consts::PI * 3.0).sin() * 20.0;
        // pressure: 起笔到收笔有变化
        let pressure = 0.2 + (t * std::f32::consts::PI).sin() * 0.8;
        brush.stroke_to(
            &mut **surface,
            x, y, pressure, 0.0, 0.0, dt as f64, 1.0, 0.0, 0.0, false,
        );
    }
}

fn main() {
    let mut surface = FixedTiledSurface::new(W, H);
    surface.begin_atomic();

    // 5 个测试笔刷，每个画一条线
    let brushes = [
        ("tests/brushes/bulk.myb", 60.0, "bulk"),
        ("tests/brushes/charcoal.myb", 160.0, "charcoal"),
        ("tests/brushes/coarse_bulk_2.myb", 260.0, "coarse_bulk_2"),
        ("tests/brushes/impressionism.myb", 360.0, "impressionism"),
        ("tests/brushes/modelling.myb", 460.0, "modelling"),
    ];

    for (path, y, label) in brushes {
        let json = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("跳过 {path}: {e}");
                continue;
            }
        };
        let mut brush = Brush::new();
        if !brush.from_string(&json) {
            eprintln!("跳过 {path}: brush 加载失败");
            continue;
        }
        draw_curve(&mut brush, &mut surface, y, label);
    }

    let roi = surface.end_atomic();
    println!("Dirty area: {} rect(s)", roi.rects.len());

    let out = Path::new("brush_gallery.png");
    surface.save_png(out, 0, 0, W as i32, H as i32);
    println!("Saved {}", out.display());
}
