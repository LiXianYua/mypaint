/// A simple double-precision random number generator.
/// Ported from rng-double.c (uses a basic LCG).
pub struct RngDouble {
    state: u32,
}

impl RngDouble {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Returns a value in [0.0, 1.0).
    pub fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.state as f64) / (u32::MAX as f64)
    }
}
