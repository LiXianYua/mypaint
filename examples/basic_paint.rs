//! 端到端示例：用 charcoal 笔刷画一条曲线并保存为 PNG。
//!
//! 运行: cargo run --release --example basic_paint
//! 输出: basic_paint.png

use mypaint::surface::fixed::FixedTiledSurface;
use mypaint::Brush;
use mypaint::Surface;
use std::fs;
use std::path::Path;

fn main() {
    // 1. 创建画布
    let width = 400;
    let height = 200;
    let mut surface = FixedTiledSurface::new(width, height);

    // 2. 加载 charcoal 笔刷
    let brush_json = fs::read_to_string("tests/brushes/charcoal.myb").expect("read brush JSON");
    let mut brush = Brush::new();
    if !brush.from_string(&brush_json) {
        eprintln!("Failed to load brush");
        std::process::exit(1);
    }

    // 3. 画一条正弦曲线
    surface.begin_atomic();

    // 第一次调用 — reset 路径
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

    // 沿 sin 曲线移动
    let steps = 200;
    let dt = 0.016; // ~60Hz
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

    let roi = surface.end_atomic();
    println!("Dirty rectangles: {}", roi.rects.len());
    for r in &roi.rects {
        println!("  bbox: x={} y={} w={} h={}", r.x, r.y, r.width, r.height);
    }

    // 4. 保存 PNG
    let out = Path::new("basic_paint.png");
    surface.save_png(out, 0, 0, width as i32, height as i32);
    println!("Saved {}", out.display());
}
