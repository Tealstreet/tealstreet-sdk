//! ECMAScript operator semantics shared by venue host bindings: ToPrimitive,
//! ToNumeric, `+`, `-`, `*`, `/`, `%`, unary `-` and relational comparison.
use super::value::{Js, Thrown};
use num_bigint::BigInt;
use std::{cmp::Ordering, rc::Rc};

fn is_primitive(value: &Js) -> bool {
    matches!(
        value,
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::BigInt(_) | Js::String(_)
    )
}

/// ToPrimitive with the default/number hint (OrdinaryToPrimitive: valueOf, then
/// toString). Without either member the builtin Object/Array string form applies.
pub fn to_primitive(value: &Js) -> Result<Js, Thrown> {
    if matches!(value, Js::Object(_) | Js::Function(_)) {
        return value.ordinary_to_primitive(["valueOf", "toString"]);
    }
    if is_primitive(value) {
        return Ok(value.clone());
    }
    let mut callable = false;
    for name in ["valueOf", "toString"] {
        let method = value.get(name)?;
        if method.is_callable() {
            callable = true;
            let result = method.call(value.clone(), vec![])?;
            if is_primitive(&result) {
                return Ok(result);
            }
        }
    }
    if callable {
        return Err(Thrown::type_error(
            "Cannot convert object to primitive value",
        ));
    }
    Ok(Js::String(value.to_js_string()?))
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

fn big(n: BigInt) -> Js {
    Js::BigInt(Rc::new(n))
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
        (Js::BigInt(x), Js::BigInt(y)) => Ok(big(&*x + &*y)),
        (x, y) => Ok(Js::Number(x.to_number()? + y.to_number()?)),
    }
}

pub fn subtract(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(x), Js::BigInt(y)) => Ok(big(&*x - &*y)),
        (x, y) => Ok(Js::Number(x.to_number()? - y.to_number()?)),
    }
}

pub fn multiply(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(x), Js::BigInt(y)) => Ok(big(&*x * &*y)),
        (x, y) => Ok(Js::Number(x.to_number()? * y.to_number()?)),
    }
}

fn division_by_zero() -> Thrown {
    Thrown::error("RangeError", "Division by zero")
}

pub fn divide(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(_), Js::BigInt(y)) if *y == BigInt::default() => Err(division_by_zero()),
        (Js::BigInt(x), Js::BigInt(y)) => Ok(big(&*x / &*y)),
        (x, y) => Ok(Js::Number(x.to_number()? / y.to_number()?)),
    }
}

/// `a % b`: Number remainder keeps the dividend's sign (fmod); BigInt truncates.
pub fn remainder(a: &Js, b: &Js) -> Result<Js, Thrown> {
    match numeric_pair(a, b)? {
        (Js::BigInt(_), Js::BigInt(y)) if *y == BigInt::default() => Err(division_by_zero()),
        (Js::BigInt(x), Js::BigInt(y)) => Ok(big(&*x % &*y)),
        (x, y) => Ok(Js::Number(x.to_number()? % y.to_number()?)),
    }
}

/// Unary `-`: preserves -0 for Number and negates BigInt.
pub fn negate(a: &Js) -> Result<Js, Thrown> {
    match to_numeric(a)? {
        Js::BigInt(x) => Ok(big(-&*x)),
        n => Ok(Js::Number(-n.to_number()?)),
    }
}

/// StringToBigInt: JS-trimmed decimal (optionally signed) or 0x/0o/0b literal;
/// the empty string is 0n; anything else is not a BigInt.
pub fn string_to_bigint(text: &[u16]) -> Option<BigInt> {
    let text = String::from_utf16(text).ok()?;
    let text = safe_cex_num::js_trim(&text);
    if text.is_empty() {
        return Some(BigInt::default());
    }
    let radix = match text.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => 16,
        Some("0o") => 8,
        Some("0b") => 2,
        _ => 10,
    };
    let digits = if radix == 10 { text } else { &text[2..] };
    let unsigned = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    if unsigned.is_empty()
        || (radix != 10 && unsigned.len() != digits.len())
        || !unsigned.chars().all(|c| c.is_digit(radix))
    {
        return None;
    }
    BigInt::parse_bytes(digits.as_bytes(), radix)
}

/// Exact ordering of a BigInt against a Number; `None` when the Number is NaN.
fn compare_big_number(x: &BigInt, y: f64) -> Option<Ordering> {
    if y.is_nan() {
        return None;
    }
    if y.is_infinite() {
        return Some(if y > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let floor = y.floor();
    let floor_big: BigInt = format!("{floor:.0}").parse().ok()?;
    Some(match x.cmp(&floor_big) {
        Ordering::Equal if y > floor => Ordering::Less,
        other => other,
    })
}

/// Abstract relational comparison of `a` against `b`; `None` is the undefined result.
fn compare(a: &Js, b: &Js) -> Result<Option<Ordering>, Thrown> {
    let (pa, pb) = (to_primitive(a)?, to_primitive(b)?);
    Ok(match (&pa, &pb) {
        (Js::String(x), Js::String(y)) => Some(x[..].cmp(&y[..])),
        (Js::BigInt(x), Js::BigInt(y)) => Some(x.cmp(y)),
        (Js::BigInt(x), Js::String(s)) => string_to_bigint(s).map(|y| (**x).cmp(&y)),
        (Js::String(s), Js::BigInt(y)) => string_to_bigint(s).map(|x| x.cmp(y)),
        (Js::BigInt(x), other) => compare_big_number(x, other.to_number()?),
        (other, Js::BigInt(y)) => compare_big_number(y, other.to_number()?).map(Ordering::reverse),
        _ => pa.to_number()?.partial_cmp(&pb.to_number()?),
    })
}

pub fn less_than(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(compare(a, b)? == Some(Ordering::Less))
}
pub fn greater_than(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(compare(a, b)? == Some(Ordering::Greater))
}
pub fn less_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(matches!(
        compare(a, b)?,
        Some(Ordering::Less | Ordering::Equal)
    ))
}
pub fn greater_equal(a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(matches!(
        compare(a, b)?,
        Some(Ordering::Greater | Ordering::Equal)
    ))
}
