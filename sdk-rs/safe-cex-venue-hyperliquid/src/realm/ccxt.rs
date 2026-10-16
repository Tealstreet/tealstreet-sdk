//! @tealstreet/ccxt-compat precision helpers (functions/number.ts,
//! functions/ccxt_exchange.ts, functions/type.ts safeString), ported for the
//! realm: the ported venue bodies call them as imports.
use super::{
    Realm,
    value::{Thrown, Value},
};

pub(crate) const TRUNCATE: f64 = 0.0;
pub(crate) const ROUND: f64 = 1.0;
pub(crate) const DECIMAL_PLACES: f64 = 2.0;
pub(crate) const SIGNIFICANT_DIGITS: f64 = 3.0;
pub(crate) const TICK_SIZE: f64 = 4.0;
pub(crate) const NO_PADDING: f64 = 5.0;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

/// numberToString for a number: JS spelling without scientific notation.
pub(crate) fn number_to_string(x: f64) -> String {
    let s = safe_cex_num::js_number_string(x);
    if x.abs() < 1.0 {
        let mut parts = s.splitn(2, "e-");
        let mantissa = parts.next().unwrap_or("");
        let exponent = parts
            .next()
            .map(|e| safe_cex_num::parse_int(e, None))
            .unwrap_or(f64::NAN);
        let n = mantissa.replacen('.', "", 1);
        let neg = s.starts_with('-');
        if exponent.is_finite() && exponent != 0.0 {
            let zeros = "0".repeat((exponent as usize).saturating_sub(1));
            let digits = if neg { &n[1..] } else { &n[..] };
            return format!("{}0.{zeros}{digits}", if neg { "-" } else { "" });
        }
    } else {
        let parts: Vec<&str> = s.splitn(2, 'e').collect();
        if parts.len() == 2 && !parts[1].is_empty() {
            let mut e = safe_cex_num::parse_int(parts[1], None);
            let m: Vec<&str> = parts[0].splitn(2, '.').collect();
            let mut part = "";
            if m.len() == 2 && !m[1].is_empty() {
                e -= m[1].len() as f64;
                part = m[1];
            }
            return format!("{}{part}{}", m[0], "0".repeat(e.max(0.0) as usize));
        }
    }
    s
}

pub(crate) fn precision_from_string(s: &str) -> f64 {
    if s.contains('e') || s.contains('E') {
        // str.replace(/\d\.?\d*[eE]/, '') — the first mantissa-and-marker run.
        let re = regex::Regex::new(r"\d\.?\d*[eE]").expect("static pattern");
        let rest = re.replacen(s, 1, "");
        return -safe_cex_num::parse_int(&rest, None);
    }
    let trimmed = s.trim_end_matches('0');
    match trimmed.split_once('.') {
        Some((_, after)) => after.encode_utf16().count() as f64,
        None => 0.0,
    }
}

pub(crate) fn parse_precision(precision: &str) -> String {
    let n = safe_cex_num::parse_int(precision, None);
    if n == 0.0 {
        return "1".into();
    }
    let zeros = if n.is_finite() && n > 1.0 {
        "0".repeat(n as usize - 1)
    } else {
        String::new()
    };
    format!("0.{zeros}1")
}

/// A decimalToPrecision operand: the source accepts numbers and strings.
#[derive(Clone)]
pub(crate) enum Operand {
    Number(f64),
    Text(String),
}
impl Operand {
    fn number(&self) -> f64 {
        match self {
            Operand::Number(n) => *n,
            Operand::Text(t) => safe_cex_num::number_from_string(t),
        }
    }
    fn string(&self) -> String {
        match self {
            Operand::Number(n) => number_to_string(*n),
            Operand::Text(t) => t.clone(),
        }
    }
}

pub(crate) fn decimal_to_precision(
    x: Operand,
    rounding: f64,
    digits: Operand,
    counting: f64,
    padding: f64,
) -> Result<String, String> {
    let mut digits_n = digits.number();
    if counting == TICK_SIZE {
        if digits_n <= 0.0 {
            return Err("TICK_SIZE cant be used with negative or zero numPrecisionDigits".into());
        }
    }
    if digits_n < 0.0 {
        let to_nearest = safe_cex_num::math_pow(10.0, -digits_n);
        if rounding == ROUND {
            let inner = decimal_to_precision(
                Operand::Number(x.number() / to_nearest),
                rounding,
                Operand::Number(0.0),
                counting,
                padding,
            )?;
            return Ok(safe_cex_num::js_number_string(
                to_nearest * safe_cex_num::number_from_string(&inner),
            ));
        }
        if rounding == TRUNCATE {
            let v = x.number();
            return Ok(safe_cex_num::js_number_string(v - v % to_nearest));
        }
    }
    if counting == TICK_SIZE {
        let precision_string = decimal_to_precision(
            Operand::Number(digits_n),
            ROUND,
            Operand::Number(22.0),
            DECIMAL_PLACES,
            NO_PADDING,
        )?;
        let new_digits = precision_from_string(&precision_string);
        let mut xv = x.number();
        let missing = xv % digits_n;
        let missing = safe_cex_num::number_from_string(&decimal_to_precision(
            Operand::Number(missing),
            ROUND,
            Operand::Number(8.0),
            DECIMAL_PLACES,
            NO_PADDING,
        )?);
        let fp_error = decimal_to_precision(
            Operand::Number(missing / digits_n),
            ROUND,
            Operand::Number(new_digits.max(8.0)),
            DECIMAL_PLACES,
            NO_PADDING,
        )?;
        if precision_from_string(&fp_error) != 0.0 {
            if rounding == ROUND {
                if xv > 0.0 {
                    xv = if missing >= digits_n / 2.0 {
                        xv - missing + digits_n
                    } else {
                        xv - missing
                    };
                } else {
                    xv = if missing >= digits_n / 2.0 {
                        xv - missing
                    } else {
                        xv - missing - digits_n
                    };
                }
            } else if rounding == TRUNCATE {
                xv -= missing;
            }
        }
        return decimal_to_precision(
            Operand::Number(xv),
            ROUND,
            Operand::Number(new_digits),
            DECIMAL_PLACES,
            padding,
        );
    }
    digits_n = digits_n.trunc();
    let s = x.string();
    let bytes: Vec<u8> = s.bytes().collect();
    let negative = bytes.first() == Some(&b'-');
    let str_start = usize::from(negative);
    let str_end = bytes.len();
    let str_dot = bytes.iter().position(|b| *b == b'.').unwrap_or(str_end);
    let has_dot = str_dot < str_end;
    let len = str_end - str_start + usize::from(!has_dot);
    let mut chars = vec![b'0'; len];
    let mut after_dot = len;
    let mut digits_start: isize = -1;
    let mut i = 1usize;
    let mut j = str_start;
    while j < str_end {
        let c = bytes[j];
        if c == b'.' {
            after_dot = i;
            j += 1;
            continue;
        } else if !c.is_ascii_digit() {
            return Err(format!(
                "{s}: invalid number (contains an illegal character '{}')",
                c as char
            ));
        }
        chars[i] = c;
        if c != b'0' && digits_start < 0 {
            digits_start = i as isize;
        }
        i += 1;
        j += 1;
    }
    if digits_start < 0 {
        digits_start = 1;
    }
    let mut precision_start = if counting == DECIMAL_PLACES {
        after_dot as isize
    } else {
        digits_start
    };
    let mut precision_end = precision_start + digits_n as isize;
    let mut digits_end: isize = -1;
    let mut all_zeros = true;
    let mut sign_needed = negative;
    let mut memo = 0u8;
    for idx in (0..chars.len()).rev() {
        let mut c = chars[idx];
        if idx != 0 {
            c += memo;
            if idx as isize >= precision_start + digits_n as isize {
                let ceil = rounding == ROUND && c >= b'5' && !(c == b'5' && memo != 0);
                c = if ceil { b'9' + 1 } else { b'0' };
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
        chars[idx] = c;
        if c != b'0' {
            all_zeros = false;
            digits_start = idx as isize;
            digits_end = if digits_end < 0 {
                idx as isize + 1
            } else {
                digits_end
            };
        }
    }
    if counting == SIGNIFICANT_DIGITS {
        precision_start = digits_start;
        precision_end = precision_start + digits_n as isize;
    }
    if all_zeros {
        sign_needed = false;
    }
    let after_dot_i = after_dot as isize;
    let read_start = if digits_start >= after_dot_i || all_zeros {
        after_dot_i - 1
    } else {
        digits_start
    };
    let read_end = if digits_end < after_dot_i {
        after_dot_i
    } else {
        digits_end
    };
    let n_after_dot = (read_end - after_dot_i).max(0);
    let actual_length = read_end - read_start;
    let desired_length = if padding == NO_PADDING {
        actual_length
    } else {
        precision_end - read_start
    };
    let pad = (desired_length - actual_length).max(0);
    let is_integer = n_after_dot + pad == 0;
    let mut out = String::new();
    if sign_needed {
        out.push('-');
    }
    for k in read_start..after_dot_i {
        out.push(chars[k as usize] as char);
    }
    if !is_integer {
        out.push('.');
    }
    for k in after_dot_i..after_dot_i + n_after_dot {
        out.push(chars[k as usize] as char);
    }
    for _ in 0..pad {
        out.push('0');
    }
    Ok(out)
}

fn operand(r: &mut Realm, v: &Value) -> Result<Operand, Thrown> {
    Ok(match v {
        Value::Number(n) => Operand::Number(*n),
        Value::String(u) => Operand::Text(String::from_utf16_lossy(u)),
        other => Operand::Number(r.to_number(other)?),
    })
}

pub(crate) fn bind(realm: &Realm) {
    for (name, value) in [
        ("TRUNCATE", TRUNCATE),
        ("ROUND", ROUND),
        ("DECIMAL_PLACES", DECIMAL_PLACES),
        ("SIGNIFICANT_DIGITS", SIGNIFICANT_DIGITS),
        ("TICK_SIZE", TICK_SIZE),
        ("NO_PADDING", NO_PADDING),
    ] {
        realm.bind_import(name, Value::Number(value));
    }
    realm.bind_import(
        "numberToString",
        realm.native("numberToString", 1, |r, _, a| match arg(a, 0) {
            Value::Undefined => Ok(Value::Undefined),
            Value::Number(n) => Ok(Value::str(&number_to_string(n))),
            other => r.call_method(&other, "toString", &[]),
        }),
    );
    realm.bind_import(
        "precisionFromString",
        realm.native("precisionFromString", 1, |r, _, a| {
            let s = r.to_rust_string(&arg(a, 0))?;
            Ok(Value::Number(precision_from_string(&s)))
        }),
    );
    realm.bind_import(
        "parsePrecision",
        realm.native("parsePrecision", 1, |r, _, a| match arg(a, 0) {
            Value::Undefined => Ok(Value::Undefined),
            v => {
                let s = r.to_rust_string(&v)?;
                Ok(Value::str(&parse_precision(&s)))
            }
        }),
    );
    realm.bind_import(
        "decimalToPrecision",
        realm.native("decimalToPrecision", 5, |r, _, a| {
            let x = operand(r, &arg(a, 0))?;
            let rounding = r.to_number(&arg(a, 1))?;
            let digits = operand(r, &arg(a, 2))?;
            let counting = match arg(a, 3) {
                Value::Undefined => DECIMAL_PLACES,
                v => r.to_number(&v)?,
            };
            let padding = match arg(a, 4) {
                Value::Undefined => NO_PADDING,
                v => r.to_number(&v)?,
            };
            decimal_to_precision(x, rounding, digits, counting, padding)
                .map(|s| Value::str(&s))
                .map_err(|e| r.error(&e))
        }),
    );
    // type.ts safeString: prop() then isStringCoercible.
    realm.bind_import(
        "safeString",
        realm.native("safeString", 3, |r, _, a| {
            let o = arg(a, 0);
            let x = if o.object().is_some() {
                let k = r.to_property_key(&arg(a, 1))?;
                let v = r.get(&o, &k)?;
                let empty = matches!(&v, Value::String(u) if u.is_empty());
                if empty || matches!(v, Value::Null) {
                    Value::Undefined
                } else {
                    v
                }
            } else {
                Value::Undefined
            };
            let coercible = match &x {
                Value::Number(n) => {
                    n.is_finite() || {
                        let f = r.get_str(&x, "toString")?;
                        f.truthy()
                    }
                }
                v if v.is_nullish() => false,
                v => r.get_str(v, "toString")?.truthy(),
            };
            if coercible {
                Ok(Value::String(r.to_string(&x)?))
            } else {
                Ok(arg(a, 2))
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(x: &str, rounding: f64, digits: u8, counting: f64) -> String {
        decimal_to_precision(
            Operand::Text(x.into()),
            rounding,
            Operand::Number(digits as f64),
            counting,
            NO_PADDING,
        )
        .unwrap()
    }
    #[test]
    fn decimal_to_precision_matches_ccxt_examples() {
        // Spot checks of the ported algorithm; not yet compared against the TS source.
        assert_eq!(d("12.3456000", ROUND, 2, DECIMAL_PLACES), "12.35");
        assert_eq!(d("12.3456", ROUND, 1, DECIMAL_PLACES), "12.3");
        assert_eq!(d("12.3456", TRUNCATE, 2, DECIMAL_PLACES), "12.34");
        assert_eq!(
            d("0.000123456789", ROUND, 3, SIGNIFICANT_DIGITS),
            "0.000123"
        );
        assert_eq!(d("0.00098765", ROUND, 2, SIGNIFICANT_DIGITS), "0.00099");
        assert_eq!(
            d("-0.000123456789", ROUND, 8, DECIMAL_PLACES),
            "-0.00012346"
        );
        assert_eq!(d("999.999", ROUND, 2, DECIMAL_PLACES), "1000");
        assert_eq!(d("1.45", ROUND, 1, DECIMAL_PLACES), "1.5");
        assert_eq!(number_to_string(1e-7), "0.0000001");
        assert_eq!(number_to_string(1e21), "1000000000000000000000");
        assert_eq!(precision_from_string("0.00010"), 4.0);
        assert_eq!(precision_from_string("1e-4"), 4.0);
        assert_eq!(parse_precision("3"), "0.001");
    }
}
