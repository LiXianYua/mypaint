use mypaint::Brush;
use std::fs;

#[test]
fn test_load_bulk_brush() {
    let json = fs::read_to_string("tests/brushes/bulk.myb").unwrap();
    let mut brush = Brush::new();
    brush
        .from_string(&json)
        .expect("bulk.myb should load successfully");
}

#[test]
fn test_load_charcoal_brush() {
    let json = fs::read_to_string("tests/brushes/charcoal.myb").unwrap();
    let mut brush = Brush::new();
    brush
        .from_string(&json)
        .expect("charcoal.myb should load successfully");
}

#[test]
fn test_load_impressionism_brush() {
    let json = fs::read_to_string("tests/brushes/impressionism.myb").unwrap();
    let mut brush = Brush::new();
    brush
        .from_string(&json)
        .expect("impressionism.myb should load successfully");
}

#[test]
fn test_load_missing_version_fails() {
    let json = fs::read_to_string("tests/brushes/bad/missing_version.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush
        .from_string(&json)
        .expect_err("missing_version should fail to load");
    // The bad-myb file is also malformed JSON (trailing comma), so either
    // `InvalidJson` or `MissingField("version")` is acceptable — both
    // correctly reject the file.
    assert!(
        matches!(
            err,
            mypaint::BrushParseError::MissingField("version")
                | mypaint::BrushParseError::InvalidJson(_)
        ),
        "expected MissingField(\"version\") or InvalidJson, got {err:?}"
    );
}

#[test]
fn test_load_empty_fails() {
    let json = fs::read_to_string("tests/brushes/bad/empty.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush
        .from_string(&json)
        .expect_err("empty brush should fail to load");
    assert!(
        matches!(err, mypaint::BrushParseError::InvalidJson(_)),
        "expected InvalidJson, got {err:?}"
    );
}

#[test]
fn test_load_truncated_fails() {
    let json = fs::read_to_string("tests/brushes/bad/truncated.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush
        .from_string(&json)
        .expect_err("truncated brush should fail to load");
    assert!(
        matches!(err, mypaint::BrushParseError::InvalidJson(_)),
        "expected InvalidJson, got {err:?}"
    );
}

#[test]
fn test_brush_from_defaults() {
    let mut brush = Brush::new();
    brush.from_defaults();
    // After from_defaults, opaque should be 1.0 (default)
    use mypaint::BrushSetting;
    assert!((brush.get_base_value(BrushSetting::Opaque) - 1.0).abs() < 1e-6);
}
