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
    brush.from_string(&brush_json);

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
            x,
            y,
            pressure,
            0.0,
            0.0,
            dtime,
            1.0,
            0.0,
            0.0,
            false,
        );
    }

    // Just verify we got some dabs
    assert!(
        !surface.calls.is_empty(),
        "should have drawn at least one dab"
    );
}

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
        100.0,
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
    assert!(
        surface.calls.is_empty(),
        "first call (reset) should not produce dabs"
    );

    // Second call — still hover, no movement needed
    brush.stroke_to(
        &mut surface,
        100.0,
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
    assert!(
        surface.calls.is_empty(),
        "pressure=0 should not produce dabs"
    );

    // Third call — pressure=1 with movement (5 pixels), should paint at least 1 dab
    brush.stroke_to(
        &mut surface,
        105.0,
        100.0,
        1.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    brush.stroke_to(
        &mut surface,
        110.0,
        100.0,
        1.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
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
        100.0,
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
    eprintln!("Call 1: reset={r1}, dabs={}", surface.calls.len());

    // Second call - pressure=1, move to different position (start painting)
    let r2 = brush.stroke_to(
        &mut surface,
        105.0,
        100.0,
        1.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    eprintln!("Call 2: reset={r2}, dabs={}", surface.calls.len());

    // Third call - continue painting
    let r3 = brush.stroke_to(
        &mut surface,
        110.0,
        100.0,
        1.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    eprintln!("Call 3: reset={r3}, dabs={}", surface.calls.len());

    // Fourth call - still painting, moving further
    let r4 = brush.stroke_to(
        &mut surface,
        115.0,
        100.0,
        1.0,
        0.0,
        0.0,
        0.01,
        1.0,
        0.0,
        0.0,
        false,
    );
    eprintln!("Call 4: reset={r4}, dabs={}", surface.calls.len());

    assert!(
        !surface.calls.is_empty(),
        "should have drawn dabs by call {r4}, got {} dabs",
        surface.calls.len()
    );
}
