//! 端到端测试：用真正的 FixedTiledSurface 渲染并验证像素被写入。

use libmypaint::surface::fixed::FixedTiledSurface;
use libmypaint::Brush;
use libmypaint::BrushSetting;
use libmypaint::Surface;

#[test]
fn draw_on_tiled_surface_produces_nonzero_pixels() {
    let mut brush = Brush::new();
    brush.from_defaults();
    // 红色，半径 ~7px
    brush.set_base_value(BrushSetting::RadiusLogarithmic, 2.0);
    brush.set_base_value(BrushSetting::Opaque, 1.0);
    brush.set_base_value(BrushSetting::Hardness, 0.8);
    brush.set_base_value(BrushSetting::ColorH, 0.0); // 红
    brush.set_base_value(BrushSetting::ColorS, 1.0);
    brush.set_base_value(BrushSetting::ColorV, 1.0);
    brush.set_mapping_n(BrushSetting::OpaqueMultiply, 0, 2);
    brush.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 0, 0.0, 0.0);
    brush.set_mapping_point(BrushSetting::OpaqueMultiply, 0, 1, 1.0, 1.0);

    let mut surface = FixedTiledSurface::new(128, 128);

    // 触发 reset
    surface.begin_atomic();
    brush.stroke_to(
        &mut *surface,
        60.0,
        60.0,
        0.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    // 几次有压力的 stroke
    for i in 1..=20 {
        let x = 60.0 + i as f32 * 1.0;
        brush.stroke_to(
            &mut *surface,
            x,
            60.0,
            1.0,
            0.0,
            0.0,
            0.01,
            1.0,
            0.0,
            0.0,
            false,
        );
    }
    let _roi = surface.end_atomic();

    // 取一个采样点的颜色，应该是红色（或至少有 alpha）
    let (r, g, b, a) = surface.get_color(70.0, 60.0, 3.0, 0.0);
    eprintln!("sampled color at (70,60): r={r}, g={g}, b={b}, a={a}");
    assert!(
        a > 0.01,
        "expected non-zero alpha at brush trail, got a={a}"
    );
    // 红色通道应当占主导
    assert!(
        r > g && r > b,
        "expected red dominant, got r={r}, g={g}, b={b}"
    );
}
