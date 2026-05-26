//! 像素级对比两张 PNG。
//! 用法: cargo run --release --example png_diff -- a.png b.png
//!
//! 输出: 总像素差异统计（max diff, avg diff, %相同像素）

use png::Decoder;
use std::env;
use std::fs::File;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <a.png> <b.png>", args[0]);
        std::process::exit(1);
    }

    let a = load_rgba(&args[1]);
    let b = load_rgba(&args[2]);

    if a.0 != b.0 || a.1 != b.1 {
        eprintln!("Dimension mismatch: {}x{} vs {}x{}", a.0, a.1, b.0, b.1);
        std::process::exit(2);
    }
    let (w, h, ap) = (a.0, a.1, a.2);
    let bp = b.2;
    assert_eq!(ap.len(), bp.len());

    let n = ap.len() / 4;
    let mut exact = 0_u64;
    let mut total_r = 0_u64;
    let mut total_g = 0_u64;
    let mut total_b = 0_u64;
    let mut total_a = 0_u64;
    let mut max_r = 0_u8;
    let mut max_g = 0_u8;
    let mut max_b = 0_u8;
    let mut max_a = 0_u8;
    let mut max_total = 0_u32;

    for i in 0..n {
        let pa = &ap[i * 4..i * 4 + 4];
        let pb = &bp[i * 4..i * 4 + 4];
        let dr = pa[0].abs_diff(pb[0]);
        let dg = pa[1].abs_diff(pb[1]);
        let db = pa[2].abs_diff(pb[2]);
        let da = pa[3].abs_diff(pb[3]);
        max_r = max_r.max(dr);
        max_g = max_g.max(dg);
        max_b = max_b.max(db);
        max_a = max_a.max(da);
        let total = dr as u32 + dg as u32 + db as u32 + da as u32;
        if total == 0 {
            exact += 1;
        }
        max_total = max_total.max(total);
        total_r += dr as u64;
        total_g += dg as u64;
        total_b += db as u64;
        total_a += da as u64;
    }

    println!("Dimension: {w}x{h} = {n} pixels");
    println!(
        "Exact-match pixels:     {} / {} ({:.2}%)",
        exact,
        n,
        100.0 * exact as f64 / n as f64
    );
    println!("Max channel diff:        R={max_r}, G={max_g}, B={max_b}, A={max_a}");
    println!("Max pixel total diff:    {max_total} (sum of RGBA abs diffs)");
    println!(
        "Avg channel diff:        R={:.2}, G={:.2}, B={:.2}, A={:.2}",
        total_r as f64 / n as f64,
        total_g as f64 / n as f64,
        total_b as f64 / n as f64,
        total_a as f64 / n as f64,
    );
}

fn load_rgba(path: &str) -> (u32, u32, Vec<u8>) {
    let decoder = Decoder::new(File::open(path).expect("open file"));
    let mut reader = decoder.read_info().expect("read info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("read frame");
    let bytes = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            // 补 alpha=255
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for px in bytes.chunks(3) {
                out.extend_from_slice(px);
                out.push(255);
            }
            out
        }
        other => panic!("unsupported color type: {other:?}"),
    };
    (info.width, info.height, rgba)
}
