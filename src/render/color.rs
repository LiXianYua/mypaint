use crate::util::helpers::max3;
use crate::util::helpers::min3;

/// HSV → RGB (in-place). Corresponds to `hsv_to_rgb_float` in helpers.c:150.
pub fn hsv_to_rgb(h: &mut f32, s: &mut f32, v: &mut f32) {
    *h = *h - (*h).floor();
    *s = s.clamp(0.0, 1.0);
    *v = v.clamp(0.0, 1.0);

    if *s == 0.0 {
        *h = *v;
        *s = *v;
        // *v 保持不变（与 C 版一致）
        return;
    }

    let mut hue = *h;
    if hue == 1.0 {
        hue = 0.0;
    }
    hue *= 6.0;
    let i = hue as i32;
    let f = hue - i as f32;
    let w = *v * (1.0 - *s);
    let q = *v * (1.0 - (*s * f));
    let t = *v * (1.0 - (*s * (1.0 - f)));

    let (r, g, b) = match i {
        0 => (*v, t, w),
        1 => (q, *v, w),
        2 => (w, *v, t),
        3 => (w, q, *v),
        4 => (t, w, *v),
        _ => (*v, w, q),
    };
    *h = r;
    *s = g;
    *v = b;
}

/// RGB → HSV (in-place). Corresponds to `rgb_to_hsv_float` in helpers.c:93.
pub fn rgb_to_hsv(r: &mut f32, g: &mut f32, b: &mut f32) {
    *r = r.clamp(0.0, 1.0);
    *g = g.clamp(0.0, 1.0);
    *b = b.clamp(0.0, 1.0);

    let max = max3(*r, *g, *b);
    let min = min3(*r, *g, *b);
    let delta = max - min;

    if delta > 0.0001 {
        let s = delta / max;
        let h = if *r == max {
            let mut h = (*g - *b) / delta;
            if h < 0.0 {
                h += 6.0;
            }
            h
        } else if *g == max {
            2.0 + (*b - *r) / delta
        } else {
            4.0 + (*r - *g) / delta
        };
        *r = h / 6.0;
        *g = s;
        *b = max;
    } else {
        *r = 0.0;
        *g = 0.0;
        *b = max;
    }
}

/// RGB → HSL (in-place). Corresponds to `rgb_to_hsl_float` in helpers.c:230.
pub fn rgb_to_hsl(r: &mut f32, g: &mut f32, b: &mut f32) {
    *r = r.clamp(0.0, 1.0);
    *g = g.clamp(0.0, 1.0);
    *b = b.clamp(0.0, 1.0);

    let max = max3(*r, *g, *b);
    let min = min3(*r, *g, *b);
    let l = (max + min) / 2.0;

    if max == min {
        *r = 0.0;
        *g = 0.0;
        *b = l;
        return;
    }

    let s = if l <= 0.5 {
        (max - min) / (max + min)
    } else {
        (max - min) / (2.0 - max - min)
    };

    let delta = if max - min == 0.0 { 1.0 } else { max - min };
    let h = if *r == max {
        (*g - *b) / delta
    } else if *g == max {
        2.0 + (*b - *r) / delta
    } else {
        4.0 + (*r - *g) / delta
    };
    let mut h = h / 6.0;
    if h < 0.0 {
        h += 1.0;
    }

    *r = h;
    *g = s;
    *b = l;
}

fn hsl_value(n1: f32, n2: f32, mut hue: f32) -> f32 {
    if hue > 6.0 {
        hue -= 6.0;
    } else if hue < 0.0 {
        hue += 6.0;
    }
    if hue < 1.0 {
        n1 + (n2 - n1) * hue
    } else if hue < 3.0 {
        n2
    } else if hue < 4.0 {
        n1 + (n2 - n1) * (4.0 - hue)
    } else {
        n1
    }
}

/// HSL → RGB (in-place). Corresponds to `hsl_to_rgb_float` in helpers.c:328.
pub fn hsl_to_rgb(h: &mut f32, s: &mut f32, l: &mut f32) {
    *h = *h - (*h).floor();
    *s = s.clamp(0.0, 1.0);
    *l = l.clamp(0.0, 1.0);

    if *s == 0.0 {
        *h = *l;
        *s = *l;
        // *l 保持不变
        return;
    }

    let m2 = if *l <= 0.5 {
        *l * (1.0 + *s)
    } else {
        *l + *s - *l * *s
    };
    let m1 = 2.0 * *l - m2;

    let r = hsl_value(m1, m2, *h * 6.0 + 2.0);
    let g = hsl_value(m1, m2, *h * 6.0);
    let b = hsl_value(m1, m2, *h * 6.0 - 2.0);
    *h = r;
    *s = g;
    *l = b;
}
