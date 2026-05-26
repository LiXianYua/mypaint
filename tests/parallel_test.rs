//! 验证 parallel feature 下的并行 end_atomic 输出与串行一致。
//! 仅在启用 `parallel` feature 时编译/运行。

#![cfg(feature = "parallel")]

use mypaint::surface::fixed::FixedTiledSurface;
use mypaint::Brush;
use mypaint::BrushSetting;
use mypaint::Surface;

/// 跑同一组 stroke：一次用串行 end_atomic、一次用 end_atomic_parallel，
/// 比较采样点 RGBA 应当一致。
#[test]
fn parallel_matches_serial() {
    let make = || {
        let mut b = Brush::new();
        b.from_defaults();
        b.set_base_value(BrushSetting::RadiusLogarithmic, 2.0);
        b.set_base_value(BrushSetting::ColorH, 0.0);
        b.set_base_value(BrushSetting::ColorS, 1.0);
        b.set_base_value(BrushSetting::ColorV, 1.0);
        b
    };

    let stroke = |brush: &mut Brush, surf: &mut FixedTiledSurface| {
        brush.stroke_to(
            &mut **surf,
            10.0,
            10.0,
            0.0,
            0.0,
            0.0,
            0.01,
            1.0,
            0.0,
            0.0,
            false,
        );
        // 在多个 tile 上画一条线
        for i in 1..=80 {
            let x = 10.0 + i as f32 * 2.5;
            let y = 10.0 + i as f32 * 2.0;
            brush.stroke_to(&mut **surf, x, y, 1.0, 0.0, 0.0, 0.01, 1.0, 0.0, 0.0, false);
        }
    };

    // 串行
    let mut b1 = make();
    let mut s1 = FixedTiledSurface::new(256, 256);
    s1.begin_atomic();
    stroke(&mut b1, &mut s1);
    let _ = s1.end_atomic();
    let c1 = s1.get_color(100.0, 80.0, 5.0, 0.0);

    // 并行
    let mut b2 = make();
    let mut s2 = FixedTiledSurface::new(256, 256);
    s2.begin_atomic();
    stroke(&mut b2, &mut s2);
    let _ = s2.end_atomic_parallel();
    let c2 = s2.get_color(100.0, 80.0, 5.0, 0.0);

    eprintln!("serial:   r={} g={} b={} a={}", c1.0, c1.1, c1.2, c1.3);
    eprintln!("parallel: r={} g={} b={} a={}", c2.0, c2.1, c2.2, c2.3);

    // 像素一致（亚像素浮点误差 < 1e-3）
    let eps = 1e-3;
    assert!((c1.0 - c2.0).abs() < eps, "r mismatch");
    assert!((c1.1 - c2.1).abs() < eps, "g mismatch");
    assert!((c1.2 - c2.2).abs() < eps, "b mismatch");
    assert!((c1.3 - c2.3).abs() < eps, "a mismatch");
}
