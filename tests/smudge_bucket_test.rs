//! Tests for `Brush::set_smudge_bucket_state` covering each `BrushError`
//! variant plus the happy path roundtrip.

use mypaint::{Brush, BrushError};

#[test]
fn set_smudge_bucket_state_without_buckets_errors() {
    // `Brush::new()` allocates no smudge buckets.
    let mut brush = Brush::new();
    let err = brush
        .set_smudge_bucket_state(0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        .expect_err("expected error for un-allocated buckets");
    assert!(
        matches!(err, BrushError::SmudgeBucketsNotAllocated),
        "expected SmudgeBucketsNotAllocated, got {err:?}"
    );
}

#[test]
fn set_smudge_bucket_state_index_out_of_range_errors() {
    let mut brush = Brush::new_with_buckets(4);
    // index 4 is one past the end of a 4-bucket array.
    let err = brush
        .set_smudge_bucket_state(4, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        .expect_err("expected out-of-range error");
    assert!(
        matches!(
            err,
            BrushError::SmudgeBucketIndexOutOfRange { index: 4, len: 4 }
        ),
        "expected SmudgeBucketIndexOutOfRange {{ index: 4, len: 4 }}, got {err:?}"
    );
}

#[test]
fn set_smudge_bucket_state_roundtrip() {
    let mut brush = Brush::new_with_buckets(4);
    brush
        .set_smudge_bucket_state(2, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9)
        .expect("set should succeed");

    let (r, g, b, a, pr, pg, pb, pa, rec) = brush
        .get_smudge_bucket_state(2)
        .expect("get should return Some for index in range");

    assert_eq!((r, g, b, a), (0.1, 0.2, 0.3, 0.4));
    assert_eq!((pr, pg, pb, pa), (0.5, 0.6, 0.7, 0.8));
    assert_eq!(rec, 0.9);
}
