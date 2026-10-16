//! ECMAScript operator semantics shared by venue host bindings: ToPrimitive,
//! ToNumeric, `+`, `-`, `*`, `/` and the abstract relational comparison.
use super::value::{Js, Thrown};
use num_bigint::BigInt;
use std::rc::Rc;

/// ToPrimitive with the default/number hint: objects become their ToString text
/// (this runtime has no user `valueOf`/`toString` overrides on plain objects).
pub fn to_primitive(value: &Js) -> Result<Js, Thrown> {
    Ok(match value {
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::BigInt(_) | Js::String(_) => {
            value.clone()
        }
        other => Js::String(other.to_js_string()?),
    })
}

/// ToNumeric: a BigInt stays a BigInt, everything else goes through ToNumber.
pub fn to_numeric(value: &Js) -> Result<Js, Thrown> {
    match to_primitive(value)? {
        b @ Js::BigInt(_) => Ok(b),
        p => Ok(Js::Number(p.to_number()?)),
    }
}

fn mixed() -> Thrown {
    Thrown::type_error("Cannot mix BigInt and other types, use explicit conversions")
}

fn numeric_pair(a: &Js, b: &Js) -> Result<(Js, Js), Thrown> {
    let (a, b) = (to_numeric(a)?, to_numeric(b)?);
    match (&a, &b) {
        (Js::BigInt(_), Js::BigInt(_)) | (Js::Number(_), Js::Number(_)) => Ok((a, b)),
        _ => Err(mixed()),
    }
}

/// `a + b`: string concatenation when either primitive is a string.
pub fn add(a: &Js, b: &Js) -> Result<Js, Thrown> {
    let (pa, pb) = (to_primitive(a)?, to_primitive(b)?);
    if pa.is_string() || pb.is_string() {
        let mut out = pa.to_js_string()?.to_vec();
        out.extend_from_slice(&pb.to_js_string()?);
        return Ok(Js::utf16(&out));
    }
    match numeric_pair(&pa, &pb)? {
        (Js::BigInt(x), Js::BigInt(y)) => Ok(Js::BigInt(Rc::new(&*x + &*y))),
        (x, y) => Ok(Js::Number(x.to_number()? + y.to_number()?)),
    }
}

pub fn subtract(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(x), Js::BigInt(y)) => Ok(Js::BigInt(Rc::new(&*x - &*y))),
        (x, y) => Ok(Js::Number(x.to_number()? - y.to_number()?)),
    }
}

pub fn multiply(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(x), Js::BigInt(y)) => Ok(Js::BigInt(Rc::new(&*x * &*y))),
        (x, y) => Ok(Js::Number(x.to_number()? * y.to_number()?)),
    }
}

pub fn divide(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(x), Js::BigInt(y)) => {
            if *y == BigInt::default() {
                return Err(Thrown::error("RangeError", "Division by zero"));
            }
            Ok(Js::BigInt(Rc::new(&*x / &*y)))
        }
        (x, y) => Ok(Js::Number(x.to_number()? / y.to_number()?)),
    }
}

/// Abstract relational comparison `a < b`; `None` is the undefined result (NaN).
fn less(a: &Js, b: &Js) -> Result<Option<bool>, Thrown> {
    let (pa, pb) = (to_primitive(a)?, to_primitive(b)?);
    if let (Js::String(x), Js::String(y)) = (&pa, &pb) {
        return Ok(Some(x[..] < y[..]));
    }
    let big_of = |v: &Js| -> Option<BigInt> {
        match v {
            Js::BigInt(n) => Some((**n).clone()),
            Js::String(s) => String::from_utf16_lossy(s).trim().parse().ok(),
            _ => None,
        }
    };
    match (&pa, &pb) {
        (Js::BigInt(_), _) | (_, Js::BigInt(_)) => {
            if let (Some(x), Some(y)) = (big_of(&pa), big_of(&pb)) {
                return Ok(Some(x < y));
            }
            let (x, y) = (
                match &pa {
                    Js::BigInt(n) => n.to_string().parse::<f64>().unwrap_or(f64::NAN),
                    other => other.to_number()?,
                },
                match &pb {
                    Js::BigInt(n) => n.to_string().parse::<f64>().unwrap_or(f64::NAN),
                    other => other.to_number()?,
                },
            );
            Ok(if x.is_nan() || y.is_nan() {
                None
            } else {
                Some(x < y)
            })
        }
        _ => {
            let (x, y) = (pa.to_number()?, pb.to_number()?);
            Ok(if x.is_nan() || y.is_nan() {
                None
            } else {
                Some(x < y)
            })
        }
    }
}

pub fn less_than(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(less(a, b)?.unwrap_or(false))
}
pub fn greater_than(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(less(b, a)?.unwrap_or(false))
}
pub fn less_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(less(b, a)?.is_some_and(|r| !r))
}
pub fn greater_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(less(a, b)?.is_some_and(|r| !r))
}
