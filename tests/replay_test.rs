use mypaint::render::DabParams;
use mypaint::surface::Surface;
use mypaint::util::rect::Rectangles;
use mypaint::Brush;
use std::fs;

/// Parse an events file (timestamp x y pressure per line).
fn load_events(path: &str) -> Vec<(f64, f32, f32, f32)> {
    let content = fs::read_to_string(path).unwrap();
    content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            let timestamp = parts[0].parse::<f64>().unwrap();
            let x = parts[1].parse::<f32>().unwrap();
            let y = parts[2].parse::<f32>().unwrap();
            let pressure = parts[3].parse::<f32>().unwrap();
            (timestamp, x, y, pressure)
        })
        .collect()
}

/// Test surface that records draw_dab calls.
struct RecordingSurface {
    calls: Vec<DabParams>,
}

impl Surface for RecordingSurface {
    fn draw_dab(&mut self, params: &DabParams) -> bool {
        self.calls.push(*params);
        true
    }
    fn get_color(&mut self, _x: f32, _y: f32, _radius: f32, _paint: f32) -> (f32, f32, f32, f32) {
        (0.0, 0.0, 0.0, 1.0)
    }
    fn begin_atomic(&mut self) {}
    fn end_atomic(&mut self) -> Rectangles {
        Default::default()
    }
    fn save_png(&mut self, _path: &std::path::Path, _x: i32, _y: i32, _w: i32, _h: i32) {}
}

#[test]
fn test_replay_events_smoke() {
    // Load a brush
    let brush_json = fs::read_to_string("tests/brushes/bulk.myb").unwrap();
    let mut brush = Brush::new();
    brush.from_string(&brush_json).expect("brush load");

    // Replay events
    let events = load_events("tests/events/painting30sec.dat");
    let mut surface = RecordingSurface { calls: Vec::new() };
    let mut last_time = 0.0;

    for (time, x, y, pressure) in events {
        let dtime = time - last_time;
        let dtime = if dtime <= 0.0 { 0.0001 } else { dtime };
        last_time = time;

        brush.stroke_to(
            &mut surface,
            &mypaint::StrokeInputs {
                x: x,
                y: y,
                pressure: pressure,
                dtime: dtime,
                ..Default::default()
            },
        );
    }

    // Just verify we got some dabs
    assert!(
        !surface.calls.is_empty(),
        "should have drawn at least one dab"
    );

    // Behavior-preservation snapshot for milestone 3 (stroke.rs decomposition).
    // 在 P1 之后基线 = 1540。P2/P3/P4/P5 是 pure refactor，dab 总数和
    // 各 dab 的关键参数必须保持不变。
    //
    // 若以后有意改动行为（例如 RNG 序列、time-discretization 算法），
    // 需要更新这两个数字并在 commit message 里说明。
    const BASELINE_DAB_COUNT: usize = 1540;
    assert_eq!(
        surface.calls.len(),
        BASELINE_DAB_COUNT,
        "regression: dab count drifted from baseline (P1 baseline = {BASELINE_DAB_COUNT})"
    );

    // 抽样首尾若干 dab 的 (x, y) — RNG 顺序漂移会让这些值都变。
    // 浮点跨平台微差 → 用相对宽松的精度（1e-3 像素，远小于一个 dab 半径）。
    fn approx_eq(a: f32, b: f32) {
        assert!(
            (a - b).abs() < 1e-3,
            "value drift: got {a}, expected {b} (Δ = {})",
            (a - b).abs()
        );
    }
    // 首 3 个 dab 锁定 RNG 初始顺序 + 第一段 stroke 的 motion smoothing。
    approx_eq(surface.calls[0].x, 224.201721);
    approx_eq(surface.calls[0].y, 207.184906);
    approx_eq(surface.calls[1].x, 225.992981);
    approx_eq(surface.calls[2].x, 214.074112);
    // 末 1 个 dab 锁定 30 秒 replay 后期累积状态。
    let last = &surface.calls[BASELINE_DAB_COUNT - 1];
    approx_eq(last.x, 733.367920);
    approx_eq(last.y, 98.607506);

    // FNV-1a 64-bit rolling hash over per-dab non-positional params
    // (opaque, hardness, softness, alpha_eraser, aspect_ratio, angle,
    // lock_alpha, colorize, posterize, paint, radius). 每个值乘 10000
    // 再 round-to-i32 后入 hash —— 跨平台稳定（f32::round 是 half-away-from-zero
    // 的确定行为，1/10000 步进 << 任何这些值的合理变动）。
    //
    // 这个 hash 是 milestone 3 review 反馈：x/y 抽样能 catch 位置/计数漂移，
    // 但 color/opacity/hardness 等"非位置"参数的静默 drift 抓不住。hash
    // 用于 milestone 4+ 的 public API reshape 等可能间接改算法的 phase。
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash: u64 = FNV_OFFSET;
    let extend = |h: &mut u64, v: f32| {
        let scaled = (v * 10000.0).round() as i32 as u32;
        for b in scaled.to_le_bytes() {
            *h ^= b as u64;
            *h = h.wrapping_mul(FNV_PRIME);
        }
    };
    for d in &surface.calls {
        extend(&mut hash, d.opaque);
        extend(&mut hash, d.hardness);
        extend(&mut hash, d.softness);
        extend(&mut hash, d.alpha_eraser);
        extend(&mut hash, d.aspect_ratio);
        extend(&mut hash, d.angle);
        extend(&mut hash, d.lock_alpha);
        extend(&mut hash, d.colorize);
        extend(&mut hash, d.posterize);
        extend(&mut hash, d.paint);
        extend(&mut hash, d.radius);
    }
    const BASELINE_DAB_PARAMS_HASH: u64 = BASELINE_DAB_PARAMS_HASH_VALUE;
    assert_eq!(
        hash, BASELINE_DAB_PARAMS_HASH,
        "regression: dab params hash drifted (positional snapshot未抓住的 color/opacity/hardness/etc 漂移)"
    );
}

// Hash 基线值：在 milestone 3 P5 + review followup 后捕获。
// 后续 milestone 改算法时如果 hash 变化但变化合理（例如 RNG 重排），
// 需要在 commit message 里说明并更新本常量。
const BASELINE_DAB_PARAMS_HASH_VALUE: u64 = 13857306452861084430;

#[test]
fn test_replay_pressure_zero_does_not_paint() {
    let mut brush = Brush::new();
    brush.from_defaults();
    use mypaint::BrushSetting;
    brush.set_base_value(BrushSetting::RadiusLogarithmic, 2.0);
    brush.set_base_value(BrushSetting::Opaque, 1.0);
    brush.set_base_value(BrushSetting::Hardness, 0.8);

    let mut surface = RecordingSurface { calls: Vec::new() };

    // First call — reset, pressure=0 (pen hover)
    brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 100.0,
            y: 100.0,
            pressure: 0.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    assert!(
        surface.calls.is_empty(),
        "first call (reset) should not produce dabs"
    );

    // Second call — still hover, no movement needed
    brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 100.0,
            y: 100.0,
            pressure: 0.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    assert!(
        surface.calls.is_empty(),
        "pressure=0 should not produce dabs"
    );

    // Third call — pressure=1 with movement (5 pixels), should paint at least 1 dab
    brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 105.0,
            y: 100.0,
            pressure: 1.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 110.0,
            y: 100.0,
            pressure: 1.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    assert!(
        !surface.calls.is_empty(),
        "pressure=1 with movement should produce dabs"
    );
}

#[test]
fn test_debug_stroke_sequence() {
    let mut brush = Brush::new();
    brush.from_defaults();
    use mypaint::BrushSetting;
    brush.set_base_value(BrushSetting::RadiusLogarithmic, 2.0);
    brush.set_base_value(BrushSetting::Opaque, 1.0);
    brush.set_base_value(BrushSetting::Hardness, 0.8);
    brush.set_mapping_n(BrushSetting::OpaqueMultiply, 0, 2);
    brush.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 0, 0.0, 0.0);
    brush.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 1, 1.0, 1.0);

    let mut surface = RecordingSurface { calls: Vec::new() };

    // First call - should trigger reset (pen hover)
    let r1 = brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 100.0,
            y: 100.0,
            pressure: 0.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    eprintln!("Call 1: reset={r1}, dabs={}", surface.calls.len());

    // Second call - pressure=1, move to different position (start painting)
    let r2 = brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 105.0,
            y: 100.0,
            pressure: 1.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    eprintln!("Call 2: reset={r2}, dabs={}", surface.calls.len());

    // Third call - continue painting
    let r3 = brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 110.0,
            y: 100.0,
            pressure: 1.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    eprintln!("Call 3: reset={r3}, dabs={}", surface.calls.len());

    // Fourth call - still painting, moving further
    let r4 = brush.stroke_to(
        &mut surface,
        &mypaint::StrokeInputs {
            x: 115.0,
            y: 100.0,
            pressure: 1.0,
            dtime: 0.01,
            ..Default::default()
        },
    );
    eprintln!("Call 4: reset={r4}, dabs={}", surface.calls.len());

    assert!(
        !surface.calls.is_empty(),
        "should have drawn dabs by call {r4}, got {} dabs",
        surface.calls.len()
    );
}
