use crate::util::rng::RngDouble;

pub const WGM_EPSILON: f32 = 0.001;
pub const M_PI: f32 = std::f32::consts::PI;

#[inline]
pub fn max3(a: f32, b: f32, c: f32) -> f32 {
    if a > b {
        if a > c { a } else { c }
    } else {
        if b > c { b } else { c }
    }
}

#[inline]
pub fn min3(a: f32, b: f32, c: f32) -> f32 {
    if a < b {
        if a < c { a } else { c }
    } else {
        if b < c { b } else { c }
    }
}

/// Arithmetic modulo — handles negative dividends correctly.
/// Corresponds to `mod_arith` in helpers.c:75.
#[inline]
pub fn mod_arith(a: f32, n: f32) -> f32 {
    a - n * (a / n).floor()
}

/// Smallest angular difference between two angles in degrees.
/// Corresponds to `smallest_angular_difference` in helpers.c:82.
#[inline]
pub fn smallest_angular_difference(angle_a: f32, angle_b: f32) -> f32 {
    let mut a = angle_b - angle_a;
    a = mod_arith(a + 180.0, 360.0) - 180.0;
    a += if a > 180.0 {
        -360.0
    } else if a < -180.0 {
        360.0
    } else {
        0.0
    };
    a
}

/// Gaussian random noise approximation (sum of 4 uniform samples).
/// Corresponds to `rand_gauss` in helpers.c:62.
pub fn rand_gauss(rng: &mut RngDouble) -> f32 {
    let sum: f64 = (0..4).map(|_| rng.next()).sum();
    (sum * 1.73205080757 - 3.46410161514) as f32
}
