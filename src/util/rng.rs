//! Knuth lagged-Fibonacci double-precision RNG.
//! 对应 libmypaint 的 rng-double.c (Knuth TAOCP Vol.2 §3.6 ex.15)。
//!
//! Low-quality settings (与 C 版默认一致):
//!     QUALITY=19, TT=7, KK=10, LL=7

const KK: usize = 10;
const LL: usize = 7;
const TT: i64 = 7;
const QUALITY: usize = 19;

/// Lagged-Fibonacci double-precision RNG. 与 C 版 RngDouble 行为等价。
pub struct RngDouble {
    ran_u: [f64; KK],
    ranf_arr_buf: [f64; QUALITY],
    /// 下一个待读的 buffer 索引；None = 需要 cycle (刚 seed 完)。
    next_idx: Option<usize>,
}

#[inline]
fn mod_sum(x: f64, y: f64) -> f64 {
    // (x + y) mod 1.0, matching C `((x)+(y))-(int)((x)+(y))`
    let s = x + y;
    s - (s as i64) as f64
}

#[inline]
fn is_odd(s: i64) -> bool {
    (s & 1) != 0
}

impl RngDouble {
    /// 创建一个 RNG，使用 seed 初始化。对应 rng_double_new。
    pub fn new(seed: i64) -> Self {
        let mut rng = Self {
            ran_u: [0.0; KK],
            ranf_arr_buf: [0.0; QUALITY],
            next_idx: None,
        };
        rng.set_seed(seed);
        rng
    }

    /// 重新设置种子。对应 rng_double_set_seed。
    pub fn set_seed(&mut self, seed: i64) {
        let mut u = [0.0f64; KK + KK - 1];
        // 2 to the -52
        let ulp = (1.0 / ((1_i64 << 30) as f64)) / ((1_i64 << 22) as f64);
        let mut ss = 2.0 * ulp * ((seed & 0x3fffffff) as f64 + 2.0);

        // bootstrap the buffer (51-bit cyclic shift)
        for j in 0..KK {
            u[j] = ss;
            ss += ss;
            if ss >= 1.0 {
                ss -= 1.0 - 2.0 * ulp;
            }
        }
        u[1] += ulp; // make u[1] (and only u[1]) "odd"

        let mut s = seed & 0x3fffffff;
        let mut t = TT - 1;
        while t > 0 {
            // "square"
            for j in (1..KK).rev() {
                u[j + j] = u[j];
                u[j + j - 1] = 0.0;
            }
            // 折叠回 u[0..KK]
            for j in (KK..=(KK + KK - 2)).rev() {
                u[j - (KK - LL)] = mod_sum(u[j - (KK - LL)], u[j]);
                u[j - KK] = mod_sum(u[j - KK], u[j]);
            }
            if is_odd(s) {
                // "multiply by z"
                for j in (1..=KK).rev() {
                    u[j] = u[j - 1];
                }
                u[0] = u[KK];
                u[LL] = mod_sum(u[LL], u[KK]);
            }
            if s != 0 {
                s >>= 1;
            } else {
                t -= 1;
            }
        }

        // 写回 ran_u
        for j in 0..LL {
            self.ran_u[j + KK - LL] = u[j];
        }
        for j in LL..KK {
            self.ran_u[j - LL] = u[j];
        }
        // warm up
        let mut warmup = [0.0f64; KK + KK - 1];
        for _ in 0..10 {
            self.get_array(&mut warmup, KK + KK - 1);
        }
        self.next_idx = None; // 首次 next() 需要 cycle
    }

    /// 生成一个 array 并更新内部状态。对应 rng_double_get_array。
    pub fn get_array(&mut self, aa: &mut [f64], n: usize) {
        for j in 0..KK {
            aa[j] = self.ran_u[j];
        }
        for j in KK..n {
            aa[j] = mod_sum(aa[j - KK], aa[j - LL]);
        }
        let mut j = n;
        for i in 0..LL {
            self.ran_u[i] = mod_sum(aa[j - KK], aa[j - LL]);
            j += 1;
        }
        for i in LL..KK {
            self.ran_u[i] = mod_sum(aa[j - KK], self.ran_u[i - LL]);
            j += 1;
        }
    }

    fn cycle(&mut self) -> f64 {
        let mut buf = [0.0f64; QUALITY];
        self.get_array(&mut buf, QUALITY);
        self.ranf_arr_buf = buf;
        self.next_idx = Some(1);
        self.ranf_arr_buf[0]
    }

    /// 返回下一个随机数 (0.0 ≤ x < 1.0)。对应 rng_double_next。
    pub fn next(&mut self) -> f64 {
        match self.next_idx {
            None => self.cycle(),
            Some(idx) if idx >= KK => self.cycle(),
            Some(idx) => {
                let v = self.ranf_arr_buf[idx];
                self.next_idx = Some(idx + 1);
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_range() {
        let mut rng = RngDouble::new(1000);
        for _ in 0..1000 {
            let v = rng.next();
            assert!(v >= 0.0 && v < 1.0, "value out of range: {v}");
        }
    }

    #[test]
    fn test_deterministic() {
        let mut a = RngDouble::new(42);
        let mut b = RngDouble::new(42);
        for _ in 0..50 {
            assert_eq!(a.next().to_bits(), b.next().to_bits());
        }
    }

    #[test]
    fn test_different_seeds() {
        let mut a = RngDouble::new(1);
        let mut b = RngDouble::new(2);
        let mut differs = false;
        for _ in 0..10 {
            if a.next() != b.next() {
                differs = true;
                break;
            }
        }
        assert!(differs, "different seeds should produce different output");
    }
}
