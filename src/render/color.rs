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

// =============================================================================
// HCY color space — 对应 helpers.c:370-517
// libmypaint brush 引擎自己不调用这两个函数，但 helpers.h 中有声明，
// 为完整 ABI 翻译而保留。
// =============================================================================

const HCY_RED_LUMA: f32 = 0.2162;
const HCY_GREEN_LUMA: f32 = 0.7152;
const HCY_BLUE_LUMA: f32 = 0.0722;

/// RGB → HCY (in-place). 对应 helpers.c:rgb_to_hcy_float。
pub fn rgb_to_hcy(r: &mut f32, g: &mut f32, b: &mut f32) {
    let rr = *r;
    let gg = *g;
    let bb = *b;

    let y = HCY_RED_LUMA * rr + HCY_GREEN_LUMA * gg + HCY_BLUE_LUMA * bb;
    let p = max3(rr, gg, bb);
    let n = min3(rr, gg, bb);
    let d = p - n;

    let h = if n == p {
        0.0
    } else if p == rr {
        let h0 = (gg - bb) / d;
        if h0 < 0.0 {
            h0 + 6.0
        } else {
            h0
        }
    } else if p == gg {
        ((bb - rr) / d) + 2.0
    } else {
        ((rr - gg) / d) + 4.0
    };
    let h = h / 6.0;
    let h = h - h.floor();

    let c = if rr == gg && gg == bb {
        0.0
    } else {
        ((y - n) / y).max((p - y) / (1.0 - y))
    };

    *r = h;
    *g = c;
    *b = y;
}

/// HCY → RGB (in-place). 对应 helpers.c:hcy_to_rgb_float。
pub fn hcy_to_rgb(h: &mut f32, c: &mut f32, y: &mut f32) {
    let mut hh = *h;
    let cc = c.clamp(0.0, 1.0);
    let yy = y.clamp(0.0, 1.0);

    hh = hh - hh.floor();

    if cc == 0.0 {
        // achromatic
        *h = yy;
        *c = yy;
        // *y 保持
        return;
    }

    hh = hh - hh.floor();
    hh *= 6.0;

    let (th, tm) = if hh < 1.0 {
        (hh, HCY_RED_LUMA + HCY_GREEN_LUMA * hh)
    } else if hh < 2.0 {
        let th = 2.0 - hh;
        (th, HCY_GREEN_LUMA + HCY_RED_LUMA * th)
    } else if hh < 3.0 {
        let th = hh - 2.0;
        (th, HCY_GREEN_LUMA + HCY_BLUE_LUMA * th)
    } else if hh < 4.0 {
        let th = 4.0 - hh;
        (th, HCY_BLUE_LUMA + HCY_GREEN_LUMA * th)
    } else if hh < 5.0 {
        let th = hh - 4.0;
        (th, HCY_BLUE_LUMA + HCY_RED_LUMA * th)
    } else {
        let th = 6.0 - hh;
        (th, HCY_RED_LUMA + HCY_BLUE_LUMA * th)
    };

    let (p, o, n) = if tm >= yy {
        let p = yy + yy * cc * (1.0 - tm) / tm;
        let o = yy + yy * cc * (th - tm) / tm;
        let n = yy - yy * cc;
        (p, o, n)
    } else {
        let p = yy + (1.0 - yy) * cc;
        let o = yy + (1.0 - yy) * cc * (th - tm) / (1.0 - tm);
        let n = yy - (1.0 - yy) * cc * tm / (1.0 - tm);
        (p, o, n)
    };

    let (r, g, b) = if hh < 1.0 {
        (p, o, n)
    } else if hh < 2.0 {
        (o, p, n)
    } else if hh < 3.0 {
        (n, p, o)
    } else if hh < 4.0 {
        (n, o, p)
    } else if hh < 5.0 {
        (o, n, p)
    } else {
        (p, n, o)
    };

    *h = r;
    *c = g;
    *y = b;
}

#[cfg(test)]
mod hcy_tests {
    use super::*;

    #[test]
    fn hcy_roundtrip_pure_red() {
        let (mut r, mut g, mut b) = (1.0, 0.0, 0.0);
        rgb_to_hcy(&mut r, &mut g, &mut b);
        hcy_to_rgb(&mut r, &mut g, &mut b);
        assert!((r - 1.0).abs() < 1e-3, "r={r}");
        assert!(g.abs() < 1e-3, "g={g}");
        assert!(b.abs() < 1e-3, "b={b}");
    }

    #[test]
    fn hcy_roundtrip_mid_gray() {
        // 灰色：c=0 应该保持 r=g=b=y（注意 C luma 系数和 ≈ 1.0036）
        let (mut r, mut g, mut b) = (0.5, 0.5, 0.5);
        rgb_to_hcy(&mut r, &mut g, &mut b);
        // c 应该为 0
        assert!(g.abs() < 1e-5, "expected c=0, got {g}");
        // y ≈ 0.5018（luma 系数和略大于 1）
        assert!((b - 0.5018).abs() < 1e-3, "expected y≈0.5018, got {b}");
        // achromatic 路径下 hcy_to_rgb 把 r=g=b=y
        hcy_to_rgb(&mut r, &mut g, &mut b);
        assert!((r - b).abs() < 1e-5);
        assert!((g - b).abs() < 1e-5);
    }
}

// max3 / min3 imported from crate::util::helpers at top of file
