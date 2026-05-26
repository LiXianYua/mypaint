//! 与 examples/basic_paint.rs 同样的 stroke，但用 new_c_compat
//! 让初始填充对齐 C 上游（0xFFFF）以做 bit-exact 对照。
//!
//! 运行: cargo run --release --example basic_paint_c_compat
//! 输出: basic_paint_compat.png

use mypaint::surface::fixed::FixedTiledSurface;
use mypaint::Brush;
use mypaint::Surface;
use std::fs;
use std::path::Path;

fn main() {
    let width = 400;
    let height = 200;
    // 关键差异：用 new_c_compat 而不是 new
    let mut surface = FixedTiledSurface::new_c_compat(width, height);

    let brush_json = fs::read_to_string("tests/brushes/charcoal.myb").expect("read brush JSON");
    let mut brush = Brush::new();
    if !brush.from_string(&brush_json) {
        eprintln!("Failed to load brush");
        std::process::exit(1);
    }

    surface.begin_atomic();
    brush.stroke_to(
        &mut *surface,
        20.0,
        100.0,
        0.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    let steps = 200;
    let dt = 0.016;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let x = 20.0 + t * 360.0;
        let y = 100.0 + (t * std::f32::consts::PI * 4.0).sin() * 30.0;
        let pressure = 0.3 + (t * std::f32::consts::PI).sin() * 0.7;
        brush.stroke_to(
            &mut *surface,
            x,
            y,
            pressure,
            0.0,
            0.0,
            dt as f64,
            1.0,
            0.0,
            0.0,
            false,
        );
    }
    surface.end_atomic();

    let out = Path::new("basic_paint_compat.png");
    surface.save_png(out, 0, 0, width as i32, height as i32);
    println!("Saved {}", out.display());
}
