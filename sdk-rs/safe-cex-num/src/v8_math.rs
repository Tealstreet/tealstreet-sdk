//! V8 13.6.233 fdlibm arithmetic with measured native contraction profiles.
//! Source: https://github.com/v8/v8/blob/13.6.233/src/base/ieee754.cc
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// Original source modified significantly by Google Inc.
// Copyright 2016 the V8 project authors. All rights reserved.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}
fn with_high(x: f64, hi: i32) -> f64 {
    f64::from_bits((x.to_bits() & 0xffff_ffff) | (u64::from(hi as u32) << 32))
}

fn v8_log<const FUSED: bool>(mut x: f64) -> f64 {
    let madd = multiply_add::<FUSED>;
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const LG1: f64 = 6.666666666666735130e-01;
    const LG2: f64 = 3.999999999940941908e-01;
    const LG3: f64 = 2.857142874366239149e-01;
    const LG4: f64 = 2.222219843214978396e-01;
    const LG5: f64 = 1.818357216161805012e-01;
    const LG6: f64 = 1.531383769920937332e-01;
    const LG7: f64 = 1.479819860511658591e-01;
    let mut hx = high(x);
    let lx = x.to_bits() as u32;
    let mut k = 0;
    if hx < 0x00100000 {
        if ((hx as u32 & 0x7fffffff) | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= 1.80143985094819840000e16;
        hx = high(x);
    }
    if hx >= 0x7ff00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000fffff;
    let mut i = (hx + 0x95f64) & 0x100000;
    x = with_high(x, hx | (i ^ 0x3ff00000));
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000fffff & (2 + hx)) < 3 {
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                madd(dk, LN2_HI, dk * LN2_LO)
            };
        }
        let r = f * f * (madd(-0.33333333333333333_f64, f, 0.5));
        return if k == 0 {
            f - r
        } else {
            madd(dk, LN2_HI, -(madd(dk, -LN2_LO, r) - f))
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * madd(w, madd(w, LG6, LG4), LG2);
    let t2 = z * madd(w, madd(w, madd(w, LG7, LG5), LG3), LG1);
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - madd(-s, hfsq + r, hfsq)
        } else {
            madd(dk, LN2_HI, -((hfsq - madd(s, hfsq + r, dk * LN2_LO)) - f))
        }
    } else if k == 0 {
        madd(-s, f - r, f)
    } else {
        madd(dk, LN2_HI, -(madd(s, f - r, -(dk * LN2_LO)) - f))
    }
}

/// JavaScript Math.log10 matching Node/V8's native contraction profile.
/// Darwin arm64 contracts multiply/add; Linux x64 retains separate operations.
pub fn math_log10(x: f64) -> f64 {
    math_log10_profile::<{ cfg!(all(target_arch = "aarch64", target_os = "macos")) }>(x)
}

#[inline]
fn multiply_add<const FUSED: bool>(a: f64, b: f64, c: f64) -> f64 {
    if FUSED { a.mul_add(b, c) } else { a * b + c }
}

fn math_log10_profile<const FUSED: bool>(mut x: f64) -> f64 {
    let madd = multiply_add::<FUSED>;
    const IVLN10: f64 = 4.34294481903251816668e-01;
    const LOG10_2HI: f64 = 3.01029995663611771306e-01;
    const LOG10_2LO: f64 = 3.69423907715893078616e-13;
    let mut hx = high(x);
    let mut lx = x.to_bits() as u32;
    let mut k = 0;
    if hx < 0x00100000 {
        if ((hx as u32 & 0x7fffffff) | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= 1.80143985094819840000e16;
        hx = high(x);
        lx = x.to_bits() as u32;
    }
    if hx >= 0x7ff00000 {
        return x + x;
    }
    if hx == 0x3ff00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32) >> 31) as i32;
    hx = (hx & 0x000fffff) | ((0x3ff - i) << 20);
    let y = f64::from(k + i);
    x = with_high(x, hx);
    let z = madd(y, LOG10_2LO, IVLN10 * v8_log::<FUSED>(x));
    madd(y, LOG10_2HI, z)
}

#[cfg(test)]
mod tests {
    use super::math_log10_profile;
    #[test]
    fn linux_x64_log10_keeps_uncontracted_rounding_at_decimal_boundary() {
        // Node24.20.0/V8 13.6.233.17 Linux x64 CI witness. The arm64
        // contraction result is one ULP lower at this exact input.
        let input = f64::from_bits(0x0d45_d989_3228_0e41);
        assert_eq!(
            math_log10_profile::<false>(input).to_bits(),
            (-244.0_f64).to_bits()
        );
        assert_eq!(
            math_log10_profile::<true>(input).to_bits(),
            (-244.00000000000003_f64).to_bits()
        );
    }
}
