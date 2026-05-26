//! Error types for the brush engine.

use thiserror::Error;

/// Errors returned by [`crate::Brush::from_string`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BrushParseError {
    /// The input was not valid JSON.
    #[error("invalid brush JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// A required top-level field was missing.
    #[error("missing required field: `{0}`")]
    MissingField(&'static str),

    /// The `version` field was present but not equal to 3.
    #[error("unsupported brush version: expected 3, got {0}")]
    UnsupportedVersion(i64),

    /// A field had the wrong JSON type.
    #[error("field `{field}` has wrong type (expected {expected})")]
    WrongFieldType {
        field: &'static str,
        expected: &'static str,
    },
}

/// Runtime errors for brush state mutations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BrushError {
    /// Smudge buckets have not been allocated (the brush was created
    /// without smudge bucket capacity, e.g. via [`crate::Brush::new`]).
    #[error("smudge buckets not allocated for this brush")]
    SmudgeBucketsNotAllocated,

    /// The requested smudge bucket index is out of range.
    #[error("smudge bucket index {index} out of range (allocated {len})")]
    SmudgeBucketIndexOutOfRange { index: usize, len: usize },
}
