//! CPU baseline 渲染 CLI。
//!
//! 加载 .myb preset 和 stroke JSON，渲染并保存为 PNG。
//!
//! 用法：
//!   cargo run --release --example render_baseline -- <preset.myb> <stroke.json> <out.png>
//!
//! stroke JSON schema:
//!   { "name": str, "canvas_w": int, "canvas_h": int,
//!     "samples": [{ "x": f32, "y": f32, "pressure": f32, "dtime": f64 }] }
//!
//! cross.json 内含 2 段独立 stroke：当 sample 的 dtime==0.0 且不是 samples[0] 时，
//! 调 brush.new_stroke() 重置 stroke 状态。

use mypaint::surface::fixed::FixedTiledSurface;
use mypaint::Brush;
use mypaint::Surface;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process;

#[derive(Deserialize)]
struct StrokeSample {
    x: f32,
    y: f32,
    pressure: f32,
    dtime: f64,
}

#[derive(Deserialize)]
struct StrokeData {
    canvas_w: u32,
    canvas_h: u32,
    samples: Vec<StrokeSample>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: render_baseline <preset.myb> <stroke.json> <out.png>");
        process::exit(1);
    }
    let preset_path = &args[1];
    let stroke_path = &args[2];
    let out_path = &args[3];

    // 加载 preset
    let preset_json = fs::read_to_string(preset_path).unwrap_or_else(|e| {
        eprintln!("Failed to read preset {}: {}", preset_path, e);
        process::exit(1);
    });
    let mut brush = Brush::new();
    if let Err(e) = brush.from_string(&preset_json) {
        eprintln!("Failed to load brush from {preset_path}: {e}");
        process::exit(1);
    }

    // 加载 stroke JSON
    let stroke_json = fs::read_to_string(stroke_path).unwrap_or_else(|e| {
        eprintln!("Failed to read stroke {}: {}", stroke_path, e);
        process::exit(1);
    });
    let stroke: StrokeData = serde_json::from_str(&stroke_json).unwrap_or_else(|e| {
        eprintln!("Failed to parse stroke JSON {}: {}", stroke_path, e);
        process::exit(1);
    });

    let w = stroke.canvas_w as usize;
    let h = stroke.canvas_h as usize;

    // 创建画布
    let mut surface = FixedTiledSurface::new(w, h);

    surface.begin_atomic();
    brush.new_stroke();

    for (i, s) in stroke.samples.iter().enumerate() {
        // dtime==0.0 且不是第一个 sample：新的独立 stroke 段（如 cross.json 的第二条对角线）。
        // sentinel 比较：dtime 字段是从 JSON 反序列化的，"dtime": 0 → IEEE 754 +0.0 精确比较安全。
        if i > 0 && s.dtime == 0.0 {
            brush.new_stroke();
        }
        brush.stroke_to(
            &mut *surface,
            s.x,
            s.y,
            s.pressure,
            0.0,
            0.0,
            s.dtime,
            1.0,
            0.0,
            0.0,
            false,
        );
    }

    surface.end_atomic();

    // 保存 PNG
    let out = Path::new(out_path);
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).unwrap_or_else(|e| {
                eprintln!("Failed to create output dir: {}", e);
                process::exit(1);
            });
        }
    }
    surface.save_png(out, 0, 0, stroke.canvas_w as i32, stroke.canvas_h as i32);
    println!("Saved {}", out.display());
}
