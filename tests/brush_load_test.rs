use std::fs;
use libmypaint::Brush;

#[test]
fn test_load_bulk_brush() {
    let json = fs::read_to_string("tests/brushes/bulk.myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(result, "bulk.myb should load successfully");
}

#[test]
fn test_load_charcoal_brush() {
    let json = fs::read_to_string("tests/brushes/charcoal.myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(result, "charcoal.myb should load successfully");
}

#[test]
fn test_load_impressionism_brush() {
    let json = fs::read_to_string("tests/brushes/impressionism.myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(result, "impressionism.myb should load successfully");
}

#[test]
fn test_load_missing_version_fails() {
    let json = fs::read_to_string("tests/brushes/bad/missing_version.bad-myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(!result, "missing_version should fail to load");
}

#[test]
fn test_load_empty_fails() {
    let json = fs::read_to_string("tests/brushes/bad/empty.bad-myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(!result, "empty brush should fail to load");
}

#[test]
fn test_load_truncated_fails() {
    let json = fs::read_to_string("tests/brushes/bad/truncated.bad-myb").unwrap();
    let mut brush = Brush::new();
    let result = brush.from_string(&json);
    assert!(!result, "truncated brush should fail to load");
}

#[test]
fn test_brush_from_defaults() {
    let mut brush = Brush::new();
    brush.from_defaults();
    // After from_defaults, opaque should be 1.0 (default)
    use libmypaint::BrushSetting;
    assert!((brush.get_base_value(BrushSetting::Opaque) - 1.0).abs() < 1e-6);
}
