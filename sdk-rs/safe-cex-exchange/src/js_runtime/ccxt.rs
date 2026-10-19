//! Shared imports from ccxt-compat/functions/{ccxt_exchange,number,type}.ts.
use super::{
    ops,
    value::{Js, Thrown, arg},
};

fn num(n: f64) -> Js {
    Js::Number(n)
}
fn exact(v: &Js, n: f64) -> bool {
    v.strict_equals(&num(n))
}
fn integer(v: &Js) -> Result<f64, Thrown> {
    Ok(safe_cex_num::parse_int(
        &String::from_utf16_lossy(&v.to_js_string()?),
        None,
    ))
}
fn zeroes(n: f64) -> Result<String, Thrown> {
    if !n.is_finite() || n < 0. || n.fract() != 0. || n > 4_294_967_294. {
        return Err(Thrown::error("RangeError", "Invalid array length"));
    }
    let mut out = String::new();
    out.try_reserve(n as usize)
        .map_err(|_| Thrown::error("RangeError", "Invalid string length"))?;
    out.extend(std::iter::repeat_n('0', n as usize));
    Ok(out)
}
fn number_to_string(x: &Js) -> Result<Js, Thrown> {
    if x.is_undefined() {
        return Ok(Js::Undefined);
    }
    let s = x.call_method("toString", vec![])?;
    let Js::Number(n) = x else {
        return Ok(s);
    };
    if n.abs() < 1. {
        let parts = s.call_method("split", vec![Js::str("e-")])?;
        let digits = parts
            .get("0")?
            .call_method("replace", vec![Js::str("."), Js::str("")])?;
        let exponent = integer(&parts.get("1")?)?;
        let negative = s.get("0")?.strict_equals(&Js::str("-"));
        if exponent != 0. && !exponent.is_nan() {
            let prefix = Js::str(&format!(
                "{}0.{}",
                if negative { "-" } else { "" },
                zeroes(exponent - 1.)?
            ));
            return ops::add(
                &prefix,
                &digits.call_method("substring", vec![Js::Bool(negative)])?,
            );
        }
    } else {
        let parts = s.call_method("split", vec![Js::str("e")])?;
        if parts.get("1")?.truthy() {
            let mut exponent = integer(&parts.get("1")?)?;
            let m = parts.get("0")?.call_method("split", vec![Js::str(".")])?;
            let part = m.get("1")?;
            let part = if part.truthy() {
                exponent -= part.get("length")?.to_number()?;
                part
            } else {
                Js::str("")
            };
            let base = ops::add(&m.get("0")?, &part)?;
            return ops::add(&base, &Js::str(&zeroes(exponent)?));
        }
    }
    Ok(s)
}
fn parse_precision(x: &Js) -> Result<Js, Thrown> {
    if x.is_undefined() {
        return Ok(Js::Undefined);
    }
    let n = integer(x)?;
    if n == 0. {
        return Ok(Js::str("1"));
    }
    Ok(Js::str(&format!("0.{}1", zeroes((n - 1.).ceil().max(0.))?)))
}
fn safe_string(o: &Js, k: &Js, default: Js) -> Result<Js, Thrown> {
    let x = if o.type_of() == "object"
        && !matches!(o, Js::Null)
        && !o.get_property(k)?.strict_equals(&Js::str(""))
        && !matches!(o.get_property(k)?, Js::Null)
    {
        o.get_property(k)?
    } else {
        Js::Undefined
    };
    let coercible = (!x.is_nullish() && x.get("toString")?.truthy())
        || matches!(x,Js::Number(n) if n.is_finite());
    if coercible {
        Ok(if matches!(x, Js::SymbolIterator) {
            Js::str("Symbol(Symbol.iterator)")
        } else {
            Js::String(x.to_js_string()?)
        })
    } else {
        Ok(default)
    }
}
fn precision_from_string(s: &Js) -> Result<f64, Thrown> {
    let text = s.to_js_string()?;
    if text.iter().any(|u| *u == 101 || *u == 69) {
        let mut start = None;
        let mut end = 0;
        for i in 0..text.len() {
            if !(48..=57).contains(&text[i]) {
                continue;
            }
            let mut j = i + 1;
            if text.get(j) == Some(&46) {
                j += 1;
            }
            while text.get(j).is_some_and(|u| (48..=57).contains(u)) {
                j += 1;
            }
            if text.get(j).is_some_and(|u| *u == 101 || *u == 69) {
                start = Some(i);
                end = j + 1;
                break;
            }
        }
        let stripped = if let Some(start) = start {
            [&text[..start], &text[end..]].concat()
        } else {
            text.to_vec()
        };
        return Ok(-integer(&Js::utf16(&stripped))?);
    }
    let end = text.iter().rposition(|u| *u != 48).map_or(0, |i| i + 1);
    Ok(text[..end]
        .iter()
        .position(|u| *u == 46)
        .map_or(0., |i| (end - i - 1) as f64))
}
fn numeric_string(x: &Js) -> Result<Js, Thrown> {
    x.call_method("toString", vec![])
}
fn decimal_to_precision(
    mut x: Js,
    rounding: Js,
    mut digits: Js,
    counting: Js,
    padding: Js,
) -> Result<Js, Thrown> {
    if exact(&counting, 4.) {
        if digits.is_string() {
            digits = num(digits.parse_float()?);
        }
        if ops::less_equal(&digits, &num(0.))? {
            return Err(Thrown::error(
                "Error",
                "TICK_SIZE cant be used with negative or zero numPrecisionDigits",
            ));
        }
    }
    if ops::less_than(&digits, &num(0.))? {
        let nearest = num(safe_cex_num::math_pow(10., -digits.to_number()?));
        if exact(&rounding, 1.) {
            let inner = decimal_to_precision(
                ops::divide(&x, &nearest)?,
                rounding,
                num(0.),
                counting,
                padding,
            )?;
            return numeric_string(&ops::multiply(&nearest, &inner)?);
        }
        if exact(&rounding, 0.) {
            return numeric_string(&ops::subtract(&x, &ops::remainder(&x, &nearest)?)?);
        }
    }
    if exact(&counting, 4.) {
        let precision = decimal_to_precision(digits.clone(), num(1.), num(22.), num(2.), num(5.))?;
        let new_digits = precision_from_string(&precision)?;
        let missing = ops::remainder(&x, &digits)?;
        let missing =
            num(decimal_to_precision(missing, num(1.), num(8.), num(2.), num(5.))?.to_number()?);
        let fp_error = decimal_to_precision(
            ops::divide(&missing, &digits)?,
            num(1.),
            num(new_digits.max(8.)),
            num(2.),
            num(5.),
        )?;
        if precision_from_string(&fp_error)? != 0. {
            if exact(&rounding, 1.) {
                let positive = ops::greater_than(&x, &num(0.))?;
                let higher = ops::greater_equal(&missing, &ops::divide(&digits, &num(2.))?)?;
                if positive {
                    x = if higher {
                        ops::add(&ops::subtract(&x, &missing)?, &digits)?
                    } else {
                        ops::subtract(&x, &missing)?
                    };
                } else {
                    x = ops::subtract(&num(x.to_number()?), &missing)?;
                    if !higher {
                        x = ops::subtract(&x, &digits)?;
                    }
                }
            } else if exact(&rounding, 0.) {
                x = ops::subtract(&x, &missing)?;
            }
        }
        return decimal_to_precision(x, num(1.), num(new_digits), num(2.), padding);
    }
    let s = number_to_string(&x)?;
    let negative = s.get("0")?.strict_equals(&Js::str("-"));
    let start = usize::from(negative);
    let end = s.get("length")?.to_number()? as usize;
    let mut dot = 0;
    while dot < end {
        if s.get(&dot.to_string())?.strict_equals(&Js::str(".")) {
            break;
        }
        dot += 1;
    }
    let has_dot = dot < end;
    let len = end
        .checked_sub(start)
        .ok_or_else(|| Thrown::error("RangeError", "Invalid typed array length"))?
        + usize::from(!has_dot);
    let mut chars = vec![0u8; len];
    if let Some(first) = chars.first_mut() {
        *first = b'0';
    }
    let mut after_dot = len;
    let mut digits_start = -1isize;
    let mut i = 1usize;
    for j in start..end {
        let c = match &s {
            Js::String(units) => units[j] as f64,
            _ => s
                .call_method("charCodeAt", vec![num(j as f64)])?
                .to_number()?,
        };
        if c == 46. {
            after_dot = i;
            continue;
        }
        if !(48. ..=57.).contains(&c) {
            let mut message = s.to_js_string()?.to_vec();
            message.extend(": invalid number (contains an illegal character '".encode_utf16());
            message.extend(s.get(&(i - 1).to_string())?.to_js_string()?.iter().copied());
            message.extend("')".encode_utf16());
            return Err(Thrown::new(
                super::value::error_constructor("Error")
                    .call(Js::Undefined, vec![Js::utf16(&message)])?,
            ));
        }
        if let Some(slot) = chars.get_mut(i) {
            *slot = c as u8;
        }
        if c != 48. && digits_start < 0 {
            digits_start = i as isize;
        }
        i += 1;
    }
    if digits_start < 0 {
        digits_start = 1;
    }
    let mut precision_start = if exact(&counting, 2.) {
        after_dot as isize
    } else {
        digits_start
    };
    let mut precision_end = ops::add(&num(precision_start as f64), &digits)?;
    let mut digits_end = -1isize;
    let mut all_zero = true;
    let mut sign = negative;
    let mut memo = 0u8;
    for i in (0..chars.len()).rev() {
        let mut c = chars[i];
        if i != 0 {
            c += memo;
            if ops::greater_equal(
                &num(i as f64),
                &ops::add(&num(precision_start as f64), &digits)?,
            )? {
                c = if exact(&rounding, 1.) && c >= b'5' && !(c == b'5' && memo != 0) {
                    b'9' + 1
                } else {
                    b'0'
                };
            }
            if c > b'9' {
                c = b'0';
                memo = 1;
            } else {
                memo = 0;
            }
        } else if memo != 0 {
            c = b'1';
        }
        chars[i] = c;
        if c != b'0' {
            all_zero = false;
            digits_start = i as isize;
            if digits_end < 0 {
                digits_end = i as isize + 1;
            }
        }
    }
    if exact(&counting, 3.) {
        precision_start = digits_start;
        precision_end = ops::add(&num(precision_start as f64), &digits)?;
    }
    if all_zero {
        sign = false;
    }
    let after = after_dot as isize;
    let read_start = if digits_start >= after || all_zero {
        after - 1
    } else {
        digits_start
    };
    let read_end = digits_end.max(after);
    let n_after = (read_end - after).max(0) as f64;
    let actual = (read_end - read_start) as f64;
    let desired = if exact(&padding, 5.) {
        actual
    } else {
        ops::subtract(&precision_end, &num(read_start as f64))?.to_number()?
    };
    let padding_length = desired - actual;
    let pad = if padding_length.is_nan() {
        f64::NAN
    } else {
        padding_length.max(0.)
    };
    let before = f64::from(sign) + (after - read_start) as f64;
    let integer = n_after + pad == 0.;
    let length = before + f64::from(!integer) + n_after + pad;
    if length.is_infinite() || length < 0. {
        return Err(Thrown::error("RangeError", "Invalid typed array length"));
    }
    let mut out = vec![0u16; length as usize];
    if sign {
        if let Some(first) = out.first_mut() {
            *first = 45;
        }
    }
    for j in read_start..after {
        let idx = usize::from(sign) + (j - read_start) as usize;
        if let Some(slot) = out.get_mut(idx) {
            *slot = chars.get(j as usize).copied().unwrap_or(0) as u16;
        }
    }
    if !integer {
        if let Some(slot) = out.get_mut(before as usize) {
            *slot = 46;
        }
    }
    for j in 0..n_after as usize {
        if let Some(slot) = out.get_mut(before as usize + 1 + j) {
            *slot = chars.get(after_dot + j).copied().unwrap_or(0) as u16;
        }
    }
    for j in 0..pad.ceil() as usize {
        if let Some(slot) = out.get_mut(before as usize + 1 + n_after as usize + j) {
            *slot = 48;
        }
    }
    Ok(Js::utf16(&out))
}

pub(super) fn install(g: &Js) -> Result<(), Thrown> {
    for (name, value) in [
        ("TRUNCATE", 0.),
        ("ROUND", 1.),
        ("ROUND_UP", 2.),
        ("ROUND_DOWN", 3.),
        ("DECIMAL_PLACES", 2.),
        ("SIGNIFICANT_DIGITS", 3.),
        ("TICK_SIZE", 4.),
        ("NO_PADDING", 5.),
        ("PAD_WITH_ZERO", 6.),
    ] {
        g.define(name, num(value))?;
    }
    g.define(
        "numberToString",
        Js::function("numberToString", |_, a| number_to_string(&arg(&a, 0))),
    )?;
    g.define(
        "precisionFromString",
        Js::function("precisionFromString", |_, a| {
            precision_from_string(&arg(&a, 0)).map(num)
        }),
    )?;
    g.define(
        "parsePrecision",
        Js::function("parsePrecision", |_, a| parse_precision(&arg(&a, 0))),
    )?;
    g.define(
        "safeString",
        Js::function("safeString", |_, a| {
            safe_string(&arg(&a, 0), &arg(&a, 1), arg(&a, 2))
        }),
    )?;
    g.define(
        "decimalToPrecision",
        Js::function("decimalToPrecision", |_, a| {
            decimal_to_precision(
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                if arg(&a, 3).is_undefined() {
                    num(2.)
                } else {
                    arg(&a, 3)
                },
                if arg(&a, 4).is_undefined() {
                    num(5.)
                } else {
                    arg(&a, 4)
                },
            )
        }),
    )?;
    Ok(())
}
