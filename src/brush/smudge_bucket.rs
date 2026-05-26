//! `SmudgeBucket` newtype: replaces the bare `[f32; 9]` representation with a
//! named-field struct so call sites can't accidentally swap indices (e.g.
//! `SMUDGE_A` = 3 vs `PREV_COL_A` = 7).
//!
//! Public API (`Brush::set_smudge_bucket_state` / `Brush::get_smudge_bucket_state`,
//! FFI) is unchanged — they take/return 9 raw f32s and convert internally
//! via [`SmudgeBucket::from_array`] / [`SmudgeBucket::to_array`].

/// One smudge bucket: the current smudge color (RGBA), the previously sampled
/// color (RGBA), and a "recentness" decay value.
///
/// Field layout matches the C upstream's
/// `SMUDGE_R/G/B/A, PREV_COL_R/G/B/A, PREV_COL_RECENTNESS` order:
/// `to_array()[0..4]` is `smudge`, `[4..8]` is `prev`, `[8]` is `recentness`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SmudgeBucket {
    /// Current smudge color (RGBA, 0..=1).
    pub smudge: [f32; 4],
    /// Previously sampled surface color (RGBA, 0..=1).
    pub prev: [f32; 4],
    /// Decay factor for `prev` freshness. 0 = first init pending, 1 = fresh.
    pub recentness: f32,
}

impl SmudgeBucket {
    pub(crate) const fn zero() -> Self {
        Self {
            smudge: [0.0; 4],
            prev: [0.0; 4],
            recentness: 0.0,
        }
    }

    /// Build from the 9-float FFI layout
    /// `[smudge_r, smudge_g, smudge_b, smudge_a, prev_r, prev_g, prev_b, prev_a, recentness]`.
    pub(crate) const fn from_array(a: [f32; 9]) -> Self {
        Self {
            smudge: [a[0], a[1], a[2], a[3]],
            prev: [a[4], a[5], a[6], a[7]],
            recentness: a[8],
        }
    }

    /// Inverse of [`Self::from_array`].
    pub(crate) const fn to_array(&self) -> [f32; 9] {
        [
            self.smudge[0],
            self.smudge[1],
            self.smudge[2],
            self.smudge[3],
            self.prev[0],
            self.prev[1],
            self.prev[2],
            self.prev[3],
            self.recentness,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_from_to_array() {
        let arr = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        let bucket = SmudgeBucket::from_array(arr);
        assert_eq!(bucket.smudge, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(bucket.prev, [0.5, 0.6, 0.7, 0.8]);
        assert_eq!(bucket.recentness, 0.9);
        assert_eq!(bucket.to_array(), arr);
    }

    #[test]
    fn zero_bucket_is_all_zero() {
        let z = SmudgeBucket::zero();
        assert_eq!(z.to_array(), [0.0; 9]);
        assert_eq!(z, SmudgeBucket::default());
    }
}
