use crate::util::helpers::WGM_EPSILON;

// 10-bin spectral primaries (from helpers.c:49-59)
const SPECTRAL_R: [f32; 10] = [
    0.009281362787953, 0.009732627042016, 0.011254252737167, 0.015105578649573,
    0.024797924177217, 0.083622585502406, 0.977865045723212, 1.0,
    0.999961046144372, 0.999999992756822,
];
const SPECTRAL_G: [f32; 10] = [
    0.002854127435775, 0.003917589679914, 0.012132151699187, 0.748259205918013,
    1.0, 0.865695937531795, 0.037477469241101, 0.022816789725717,
    0.021747419446456, 0.021384940572308,
];
const SPECTRAL_B: [f32; 10] = [
    0.537052150373386, 0.546646402401469, 0.575501819073983, 0.258778829633924,
    0.041709923751716, 0.012662638828324, 0.007485593127390, 0.006766900622462,
    0.006699764779016, 0.006676219883241,
];

// 3x10 transform matrix (from helpers.c:39-47)
const T_MATRIX: [[f32; 10]; 3] = [
    [0.026595621243689, 0.049779426257903, 0.022449850859496, -0.218453689278271,
     -0.256894883201278, 0.445881722194840, 0.772365886289756, 0.194498761382537,
     0.014038157587820, 0.007687264480513],
    [-0.032601672674412, -0.061021043498478, -0.052490001018404, 0.206659098273522,
     0.572496335158169, 0.317837248815438, -0.021216624031211, -0.019387668756117,
     -0.001521339050858, -0.000835181622534],
    [0.339475473216284, 0.635401374177222, 0.771520797089589, 0.113222640692379,
     -0.055251113343776, -0.048222578468680, -0.012966666339586, -0.001523814504223,
     -0.000094718948810, -0.000051604594741],
];

/// Convert RGB to 10-bin spectral distribution.
/// Corresponds to `rgb_to_spectral` in helpers.c:521.
#[inline]
pub fn rgb_to_spectral(r: f32, g: f32, b: f32) -> [f32; 10] {
    let offset = 1.0 - WGM_EPSILON;
    let r = r * offset + WGM_EPSILON;
    let g = g * offset + WGM_EPSILON;
    let b = b * offset + WGM_EPSILON;
    let mut spectral = [0.0; 10];
    for i in 0..10 {
        spectral[i] = SPECTRAL_R[i] * r + SPECTRAL_G[i] * g + SPECTRAL_B[i] * b;
    }
    spectral
}

/// Convert 10-bin spectral distribution to RGB.
/// Corresponds to `spectral_to_rgb` in helpers.c:547.
#[inline]
pub fn spectral_to_rgb(spectral: &[f32; 10]) -> (f32, f32, f32) {
    let offset = 1.0 - WGM_EPSILON;
    let mut tmp = [0.0; 3];
    for i in 0..10 {
        for ch in 0..3 {
            tmp[ch] += T_MATRIX[ch][i] * spectral[i];
        }
    }
    (
        ((tmp[0] - WGM_EPSILON) / offset).clamp(0.0, 1.0),
        ((tmp[1] - WGM_EPSILON) / offset).clamp(0.0, 1.0),
        ((tmp[2] - WGM_EPSILON) / offset).clamp(0.0, 1.0),
    )
}

/// Mix two RGBA colors using weighted geometric mean (spectral) + linear fallback.
/// `a` = smudge state color, `b` = brush/canvas color
/// `fac` = how much of `a` (0..1), `paint_mode` = 0=linear, 1=spectral
/// Corresponds to `mix_colors` in helpers.c:564.
pub fn mix_colors(
    a: &[f32; 4],
    b: &[f32; 4],
    fac: f32,
    paint_mode: f32,
) -> [f32; 4] {
    let opa_a = fac;
    let opa_b = 1.0 - opa_a;
    let result_alpha = (opa_a * a[3] + opa_b * b[3]).clamp(0.0, 1.0);

    let sfac_a = if a[3] == 0.0 { 0.0 } else { opa_a * a[3] / (a[3] + b[3] * opa_b) };
    let sfac_b = 1.0 - sfac_a;

    let mut rgb = [0.0; 3];

    if paint_mode > 0.0 {
        let spec_a = rgb_to_spectral(a[0], a[1], a[2]);
        let spec_b = rgb_to_spectral(b[0], b[1], b[2]);
        let mut spectral_mix = [0.0; 10];
        for i in 0..10 {
            spectral_mix[i] = spec_a[i].powf(sfac_a) * spec_b[i].powf(sfac_b);
        }
        let (r, g, b_) = spectral_to_rgb(&spectral_mix);
        rgb = [r, g, b_];
    }

    if paint_mode < 1.0 {
        for i in 0..3 {
            rgb[i] = rgb[i] * paint_mode + (1.0 - paint_mode) * (a[i] * opa_a + b[i] * opa_b);
        }
    }

    [rgb[0], rgb[1], rgb[2], result_alpha]
}
