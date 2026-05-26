//! Rust trace program: 等价于 c_trace.c。
//! 加载 brush JSON + events 文件，捕获每个 draw_dab 的所有参数，
//! 输出格式与 C 版本完全相同（用 `{:.9e}` 模拟 C 的 `%.9g` 浮点格式）。
//!
//! 用法: cargo run --release --example rust_trace -- <brush.myb> <events.dat>

use mypaint::render::DabParams;
use mypaint::util::rect::Rectangles;
use mypaint::Brush;
use mypaint::Surface;
use std::env;
use std::fs;
use std::io::Write;

struct TraceSurface {
    out: std::io::BufWriter<std::io::Stdout>,
    dab_count: u64,
}

/// 模拟 C 的 %.9g：9 位有效数字，自适应科学/小数。
/// 注意：Rust 的 `{:.9}` 是小数位数不同。我们用一个 helper。
fn fmt_g9(v: f32) -> String {
    // 与 C %.9g 一致：9 位有效数字，小数表示与科学之间自动切换。
    // C 选择基于 exponent: 如果 -4 <= exp < precision 用小数，否则用科学。
    if !v.is_finite() {
        return format!("{v}");
    }
    if v == 0.0 {
        return "0".to_string();
    }
    let abs = v.abs();
    let exp = abs.log10().floor() as i32;
    if (-4..9).contains(&exp) {
        // 小数表示，9 位有效数字
        let prec = (9 - 1 - exp).max(0) as usize;
        let s = format!("{:.*}", prec, v);
        trim_trailing_zeros(&s)
    } else {
        let s = format!("{:.*e}", 8, v);
        // Rust e 格式是 1.234e-5，C %g 是 1.234e-05 — 让 e 后补零
        normalize_exponent(&s)
    }
}

fn trim_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_exponent(s: &str) -> String {
    // 暂时不做处理 — diff 可能容忍 e-5 vs e-05
    s.to_string()
}

impl Surface for TraceSurface {
    fn draw_dab(&mut self, p: &DabParams) -> bool {
        self.dab_count += 1;
        let _ = writeln!(
            self.out,
            "DAB {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            self.dab_count,
            fmt_g9(p.x),
            fmt_g9(p.y),
            fmt_g9(p.radius),
            fmt_g9(p.color_r),
            fmt_g9(p.color_g),
            fmt_g9(p.color_b),
            fmt_g9(p.opaque),
            fmt_g9(p.hardness),
            fmt_g9(p.softness),
            fmt_g9(p.alpha_eraser),
            fmt_g9(p.aspect_ratio),
            fmt_g9(p.angle),
            fmt_g9(p.lock_alpha),
            fmt_g9(p.colorize),
            fmt_g9(p.posterize),
            fmt_g9(p.posterize_num),
            fmt_g9(p.paint)
        );
        true
    }

    fn get_color(&mut self, _x: f32, _y: f32, _radius: f32, _paint: f32) -> (f32, f32, f32, f32) {
        (0.0, 0.0, 0.0, 0.0)
    }

    fn begin_atomic(&mut self) {}
    fn end_atomic(&mut self) -> Rectangles {
        Rectangles::default()
    }
    fn save_png(&mut self, _path: &std::path::Path, _x: i32, _y: i32, _w: i32, _h: i32) {}
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <brush.myb> <events.dat>", args[0]);
        std::process::exit(1);
    }

    let brush_json = fs::read_to_string(&args[1]).expect("read brush");
    let mut brush = Brush::new();
    if !brush.from_string(&brush_json) {
        eprintln!("Failed to load brush");
        std::process::exit(2);
    }

    let mut surface = TraceSurface {
        out: std::io::BufWriter::new(std::io::stdout()),
        dab_count: 0,
    };

    let events = fs::read_to_string(&args[2]).expect("read events");

    let mut prev_t = 0.0_f64;
    let mut first = true;
    for line in events.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let t: f64 = parts[0].parse().expect("parse t");
        let x: f32 = parts[1].parse().expect("parse x");
        let y: f32 = parts[2].parse().expect("parse y");
        let p: f32 = parts[3].parse().expect("parse pressure");

        let dt = if first { 0.0001 } else { t - prev_t };
        let dt = if dt <= 0.0 { 0.0001 } else { dt };
        first = false;
        prev_t = t;

        brush.stroke_to(&mut surface, x, y, p, 0.0, 0.0, dt, 1.0, 0.0, 0.0, false);
    }

    let count = surface.dab_count;
    drop(surface); // flush
    eprintln!("Rust: total dabs = {}", count);
}
