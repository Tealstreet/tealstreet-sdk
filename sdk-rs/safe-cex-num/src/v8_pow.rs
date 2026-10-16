//! V8's enabled use_std_math_pow path, not its disabled legacy fdlibm path.
//! Reference: nodejs/node v24.20.0 deps/v8/src/numbers/ieee754.cc.

/// Math.pow matching V8's default platform-pow path and JS special cases.
/// Finite general powers inherit the host libm; the oracle pins measured parity.
pub fn math_pow(mut x: f64, y: f64) -> f64 {
    if y.is_nan() || (y.is_infinite() && x.abs() == 1.0) {
        return f64::NAN;
    }
    if x.is_nan() {
        x = f64::NAN;
    }
    if y == 2.0 {
        x * x
    } else if y == 0.5 {
        if x.is_infinite() {
            f64::INFINITY
        } else {
            (x + 0.0).sqrt()
        }
    } else {
        x.powf(y)
    }
}
