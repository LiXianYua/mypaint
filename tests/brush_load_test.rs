//! Tests for `Brush::from_string` covering each `BrushParseError` variant
//! plus the happy paths.

use mypaint::{Brush, BrushParseError};
use std::fs;

// ============================================================================
// Happy paths — real .myb fixtures
// ============================================================================

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

// ============================================================================
// Error variants — `MissingField`
// ============================================================================

#[test]
fn test_load_missing_version_fails_with_missing_field() {
    // Fixture is now clean JSON (no trailing comma), so the only failure
    // mode is the absent `version` key. Strict assert to lock that in.
    let json = fs::read_to_string("tests/brushes/bad/missing_version.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush.from_string(&json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::MissingField("version")),
        "expected MissingField(\"version\"), got {err:?}"
    );
}

#[test]
fn test_load_missing_settings_fails_with_missing_field() {
    let json = fs::read_to_string("tests/brushes/bad/missing_settings.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush.from_string(&json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::MissingField("settings")),
        "expected MissingField(\"settings\"), got {err:?}"
    );
}

// ============================================================================
// Error variants — `InvalidJson`
// ============================================================================

#[test]
fn test_load_empty_fails_with_invalid_json() {
    let json = fs::read_to_string("tests/brushes/bad/empty.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush.from_string(&json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::InvalidJson(_)),
        "expected InvalidJson, got {err:?}"
    );
}

#[test]
fn test_load_truncated_fails_with_invalid_json() {
    let json = fs::read_to_string("tests/brushes/bad/truncated.bad-myb").unwrap();
    let mut brush = Brush::new();
    let err = brush.from_string(&json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::InvalidJson(_)),
        "expected InvalidJson, got {err:?}"
    );
}

// ============================================================================
// Error variants — `UnsupportedVersion`
// ============================================================================

#[test]
fn test_load_version_2_fails() {
    let json = r#"{"version": 2, "settings": {}}"#;
    let mut brush = Brush::new();
    let err = brush.from_string(json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::UnsupportedVersion(2)),
        "expected UnsupportedVersion(2), got {err:?}"
    );
}

#[test]
fn test_load_version_99_fails() {
    let json = r#"{"version": 99, "settings": {}}"#;
    let mut brush = Brush::new();
    let err = brush.from_string(json).expect_err("should fail");
    assert!(
        matches!(err, BrushParseError::UnsupportedVersion(99)),
        "expected UnsupportedVersion(99), got {err:?}"
    );
}

// ============================================================================
// Error variants — `WrongFieldType`
// ============================================================================

#[test]
fn test_load_settings_array_fails_with_wrong_type() {
    let json = r#"{"version": 3, "settings": []}"#;
    let mut brush = Brush::new();
    let err = brush.from_string(json).expect_err("should fail");
    assert!(
        matches!(
            err,
            BrushParseError::WrongFieldType {
                field: "settings",
                expected: "object"
            }
        ),
        "expected WrongFieldType {{ field: \"settings\", expected: \"object\" }}, got {err:?}"
    );
}

// ============================================================================
// Deliberate divergence from C upstream — empty settings is OK
// ============================================================================

#[test]
fn test_load_empty_settings_succeeds() {
    // C `libmypaint` returns FALSE when no setting was updated (conflates
    // "no settings to apply" with "every setting failed"). Rust treats
    // only structural errors as failures, so an empty `settings: {}` is
    // a valid (if useless) brush. Lock this divergence in with a test.
    let json = r#"{"version": 3, "settings": {}}"#;
    let mut brush = Brush::new();
    brush
        .from_string(json)
        .expect("empty settings should succeed");
}

// ============================================================================
// Defaults sanity
// ============================================================================

#[test]
fn test_brush_from_defaults() {
    let mut brush = Brush::new();
    brush.from_defaults();
    // After from_defaults, opaque should be 1.0 (default)
    use mypaint::BrushSetting;
    assert!((brush.get_base_value(BrushSetting::Opaque) - 1.0).abs() < 1e-6);
}
