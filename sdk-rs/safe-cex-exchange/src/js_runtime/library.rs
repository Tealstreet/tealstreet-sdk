//! The npm and safe-cex utility modules venue bodies import by name (lodash,
//! p-iteration, safe-math, bignumber.js, dayjs, qs, create-hmac, nanoid and
//! safe-cex/src/utils), as thin wrappers over the existing Rust ports.
use super::{
    ops,
    promise::{Promise, await_js},
    scope::{Scope, ScopeShared, scope_of},
    value::{Js, Thrown, arg},
};
use num_bigint::BigInt;
use std::{
    cmp::Ordering,
    rc::{Rc, Weak},
};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn func(name: &str, f: impl Fn(Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
    Js::function(name, move |_, a| f(a))
}

fn list(v: &Js) -> Vec<Js> {
    v.array_items()
        .map(|xs| xs.into_iter().map(|x| x.unwrap_or(Js::Undefined)).collect())
        .unwrap_or_default()
}

fn call(f: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
    if !f.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            f.to_display()
        )));
    }
    f.call(Js::Undefined, args)
}

/// lodash iteratee shorthand: a function, a property path, or identity.
fn iteratee(f: &Js, value: Js) -> Result<Js, Thrown> {
    if f.is_callable() {
        return f.call(Js::Undefined, vec![value]);
    }
    if f.is_undefined() || matches!(f, Js::Null) {
        return Ok(value);
    }
    let mut out = value;
    for part in text(f)?.split('.') {
        if out.is_nullish() {
            return Ok(Js::Undefined);
        }
        out = out.get(part)?;
    }
    Ok(out)
}

fn in_operator(obj: &Js, key: &str) -> Result<bool, Thrown> {
    if !obj.is_object() {
        let rhs = if matches!(obj, Js::SymbolIterator) {
            "Symbol(Symbol.iterator)".to_owned()
        } else {
            obj.to_display()
        };
        return Err(Thrown::type_error(&format!(
            "Cannot use 'in' operator to search for '{key}' in {rhs}"
        )));
    }
    if obj.has_own(key) {
        return Ok(true);
    }
    match obj {
        Js::Object(o) => match o.proto.borrow().as_ref() {
            Some(proto) if !matches!(proto, Js::Null) => in_operator(proto, key),
            _ => Ok(false),
        },
        Js::Array(_) => Ok(key == "length"),
        _ => Ok(false),
    }
}

/// lodash compareAscending: undefined, then null, then NaN sort last.
fn compare_ascending(a: &Js, b: &Js) -> Result<Ordering, Thrown> {
    if a.strict_equals(b) {
        return Ok(Ordering::Equal);
    }
    let reflexive = |v: &Js| !matches!(v, Js::Number(n) if n.is_nan());
    let (a_def, a_null, a_refl) = (!a.is_undefined(), matches!(a, Js::Null), reflexive(a));
    let (b_def, b_null, b_refl) = (!b.is_undefined(), matches!(b, Js::Null), reflexive(b));
    if (!b_null && ops::greater_than(a, b)?)
        || (a_null && b_def && b_refl)
        || (!a_def && b_refl)
        || !a_refl
    {
        return Ok(Ordering::Greater);
    }
    if (!a_null && ops::less_than(a, b)?)
        || (b_null && a_def && a_refl)
        || (!b_def && a_refl)
        || !b_refl
    {
        return Ok(Ordering::Less);
    }
    Ok(Ordering::Equal)
}

fn uniq_by(values: Vec<Js>, by: &Js) -> Result<Js, Thrown> {
    let mut seen: Vec<Js> = vec![];
    let mut out = vec![];
    for v in values {
        let key = iteratee(by, v.clone())?;
        if !seen.iter().any(|s| s.same_value_zero(&key)) {
            seen.push(key);
            out.push(v);
        }
    }
    Ok(Js::array(out))
}

fn order_by(collection: &Js, iteratees: &Js, orders: &Js) -> Result<Js, Thrown> {
    let values = if collection.is_array() {
        list(collection)
    } else if collection.is_nullish() {
        vec![]
    } else {
        collection
            .own_entries()?
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    };
    let keys = match iteratees {
        Js::Array(_) => list(iteratees),
        Js::Undefined | Js::Null => vec![],
        other => vec![other.clone()],
    };
    let keys = if keys.is_empty() {
        vec![Js::Undefined]
    } else {
        keys
    };
    let descending = match orders {
        Js::Array(_) => list(orders),
        Js::Undefined | Js::Null => vec![],
        other => vec![other.clone()],
    }
    .iter()
    .map(|o| {
        o.as_str()
            .is_some_and(|s| String::from_utf16_lossy(s) == "desc")
    })
    .collect::<Vec<_>>();
    let mut rows = vec![];
    for (index, value) in values.into_iter().enumerate() {
        let criteria = keys
            .iter()
            .map(|k| iteratee(k, value.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        rows.push((criteria, index, value));
    }
    super::sorting::stable_sort(&mut rows, |(a, ai, _), (b, bi, _)| {
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            match compare_ascending(x, y)? {
                Ordering::Equal => continue,
                order => {
                    return Ok(if descending.get(i) == Some(&true) {
                        order.reverse()
                    } else {
                        order
                    });
                }
            }
        }
        Ok(ai.cmp(bi))
    })?;
    Ok(Js::array(rows.into_iter().map(|(_, _, v)| v).collect()))
}

/// Finite bignumber.js value: `coefficient * 10^-scale`, retaining zero's sign.
#[derive(Clone, Debug, PartialEq)]
struct Decimal {
    coefficient: BigInt,
    scale: i64,
    negative_zero: bool,
}

impl Decimal {
    fn parse(source: &str) -> Option<Decimal> {
        let s = safe_cex_num::js_trim(source);
        let (negative, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (mantissa, exponent) = match s.find(['e', 'E']) {
            Some(at) => (&s[..at], s[at + 1..].parse::<i64>().ok()?),
            None => (s, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let mut coefficient: BigInt = format!("{whole}{fraction}").parse().ok()?;
        if negative {
            coefficient = -coefficient;
        }
        Some(
            Decimal {
                coefficient,
                scale: fraction.len() as i64 - exponent,
                negative_zero: negative,
            }
            .normalize(),
        )
    }
    fn normalize(mut self) -> Self {
        let ten = BigInt::from(10);
        if self.coefficient == BigInt::default() {
            self.scale = 0;
            return self;
        }
        while self.scale > 0 && (&self.coefficient % &ten) == BigInt::default() {
            self.coefficient /= &ten;
            self.scale -= 1;
        }
        if self.scale < 0 {
            self.coefficient *= ten.pow((-self.scale) as u32);
            self.scale = 0;
        }
        self
    }
    fn aligned(&self, other: &Self) -> (BigInt, BigInt, i64) {
        let scale = self.scale.max(other.scale);
        let ten = BigInt::from(10);
        (
            &self.coefficient * ten.pow((scale - self.scale) as u32),
            &other.coefficient * ten.pow((scale - other.scale) as u32),
            scale,
        )
    }
    fn plus(&self, other: &Self) -> Self {
        let (a, b, scale) = self.aligned(other);
        Decimal {
            coefficient: a + b,
            scale,
            negative_zero: self.negative() && other.negative(),
        }
        .normalize()
    }
    fn minus(&self, other: &Self) -> Self {
        let (a, b, scale) = self.aligned(other);
        Decimal {
            coefficient: a - b,
            scale,
            negative_zero: self.negative() && !other.negative(),
        }
        .normalize()
    }
    fn times(&self, other: &Self) -> Self {
        Decimal {
            coefficient: &self.coefficient * &other.coefficient,
            scale: self.scale + other.scale,
            negative_zero: self.negative() != other.negative(),
        }
        .normalize()
    }
    /// ROUND_DOWN quotient of finite values with a nonzero divisor.
    fn integer_quotient(&self, other: &Self) -> Self {
        let (a, b, _) = self.aligned(other);
        Decimal {
            coefficient: a / b,
            scale: 0,
            negative_zero: self.negative() != other.negative(),
        }
    }
    fn compare(&self, other: &Self) -> Ordering {
        let (a, b, _) = self.aligned(other);
        a.cmp(&b)
    }
    fn digits(&self) -> String {
        self.coefficient.magnitude().to_string()
    }
    fn negative(&self) -> bool {
        self.coefficient < BigInt::default()
            || self.coefficient == BigInt::default() && self.negative_zero
    }
    /// Plain notation; `dp` rounds ROUND_HALF_UP and pads to that many places.
    fn fixed(&self, dp: Option<i64>) -> String {
        let value = match dp {
            Some(dp) if dp < self.scale => {
                let drop = (self.scale - dp) as u32;
                let ten = BigInt::from(10);
                let divisor = ten.pow(drop);
                let magnitude = BigInt::from(self.coefficient.magnitude().clone());
                let mut kept = &magnitude / &divisor;
                if (&magnitude % &divisor) * 2 >= divisor {
                    kept += 1;
                }
                Decimal {
                    coefficient: if self.negative() { -kept } else { kept },
                    scale: dp,
                    negative_zero: self.negative(),
                }
            }
            _ => self.clone(),
        };
        let digits = value.digits();
        let places = dp.unwrap_or(value.scale).max(0) as usize;
        let pad = places.saturating_sub(value.scale as usize);
        let scale = value.scale as usize;
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let split = padded.len() - scale;
        let mut body = padded[..split].to_owned();
        if places > 0 {
            body.push('.');
            body.push_str(&padded[split..]);
            body.push_str(&"0".repeat(pad));
        }
        // An originally negative nonzero value keeps its sign when rounding to
        // zero. An actual negative zero formats unsigned, unlike valueOf/JSON.
        let signed = self.negative() && self.coefficient != BigInt::default();
        format!("{}{body}", if signed { "-" } else { "" })
    }
    /// bignumber.js toString: exponential outside EXPONENTIAL_AT [-7, 20].
    fn display(&self) -> String {
        let digits = self.digits();
        let exponent = digits.len() as i64 - 1 - self.scale;
        if self.coefficient == BigInt::default() || (-7 < exponent && exponent < 21) {
            return self.fixed(None);
        }
        let digits = digits.trim_end_matches('0');
        let mantissa = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits.to_owned()
        };
        format!(
            "{}{mantissa}e{}{exponent}",
            if self.negative() { "-" } else { "" },
            if exponent > 0 { "+" } else { "" }
        )
    }
}

#[derive(Clone, Debug)]
enum BigNumberValue {
    Finite(Decimal),
    Infinity { negative: bool },
    NaN,
}

impl BigNumberValue {
    fn parse(source: &str) -> Self {
        match safe_cex_num::js_trim(source) {
            "Infinity" | "+Infinity" => Self::Infinity { negative: false },
            "-Infinity" => Self::Infinity { negative: true },
            _ => Decimal::parse(source).map_or(Self::NaN, Self::Finite),
        }
    }
    fn from_number(number: f64) -> Self {
        if number == 0.0 && number.is_sign_negative() {
            Self::zero(true)
        } else {
            Self::parse(&safe_cex_num::js_number_string(number))
        }
    }
    fn zero(negative: bool) -> Self {
        Self::Finite(Decimal {
            coefficient: BigInt::default(),
            scale: 0,
            negative_zero: negative,
        })
    }
    fn is_zero(&self) -> bool {
        matches!(self, Self::Finite(d) if d.coefficient == BigInt::default())
    }
    fn negative(&self) -> bool {
        match self {
            Self::Finite(d) => d.negative(),
            Self::Infinity { negative } => *negative,
            Self::NaN => false,
        }
    }
    fn negated(&self) -> Self {
        match self {
            Self::Finite(d) => Self::Finite(Decimal {
                coefficient: -&d.coefficient,
                scale: d.scale,
                negative_zero: !d.negative(),
            }),
            Self::Infinity { negative } => Self::Infinity {
                negative: !negative,
            },
            Self::NaN => Self::NaN,
        }
    }
    fn plus(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::NaN, _) | (_, Self::NaN) => Self::NaN,
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a.plus(b)),
            (Self::Infinity { negative: a }, Self::Infinity { negative: b }) if a != b => Self::NaN,
            (Self::Infinity { .. }, _) => self.clone(),
            (_, Self::Infinity { .. }) => other.clone(),
        }
    }
    fn minus(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a.minus(b)),
            _ => self.plus(&other.negated()),
        }
    }
    fn times(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::NaN, _) | (_, Self::NaN) => Self::NaN,
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a.times(b)),
            _ if self.is_zero() || other.is_zero() => Self::NaN,
            _ => Self::Infinity {
                negative: self.negative() != other.negative(),
            },
        }
    }
    fn integer_quotient(&self, other: &Self) -> Self {
        let negative = self.negative() != other.negative();
        match (self, other) {
            (Self::NaN, _) | (_, Self::NaN) | (Self::Infinity { .. }, Self::Infinity { .. }) => {
                Self::NaN
            }
            (Self::Finite(_), Self::Infinity { .. }) => Self::zero(negative),
            (Self::Infinity { .. }, Self::Finite(_)) => Self::Infinity { negative },
            (Self::Finite(a), Self::Finite(b)) => {
                if other.is_zero() {
                    if self.is_zero() {
                        Self::NaN
                    } else {
                        Self::Infinity { negative }
                    }
                } else {
                    Self::Finite(a.integer_quotient(b))
                }
            }
        }
    }
    fn compare(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::NaN, _) | (_, Self::NaN) => None,
            (Self::Finite(a), Self::Finite(b)) => Some(a.compare(b)),
            (Self::Infinity { negative: a }, Self::Infinity { negative: b }) => Some(b.cmp(a)),
            (Self::Infinity { negative }, _) => Some(if *negative {
                Ordering::Less
            } else {
                Ordering::Greater
            }),
            (_, Self::Infinity { negative }) => Some(if *negative {
                Ordering::Greater
            } else {
                Ordering::Less
            }),
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Finite(d) => d.display(),
            Self::Infinity { negative: true } => "-Infinity".into(),
            Self::Infinity { negative: false } => "Infinity".into(),
            Self::NaN => "NaN".into(),
        }
    }
    fn value_string(&self) -> String {
        if self.is_zero() && self.negative() {
            "-0".into()
        } else {
            self.display()
        }
    }
    fn fixed(&self, dp: Option<i64>) -> String {
        match self {
            Self::Finite(d) => d.fixed(dp),
            _ => self.display(),
        }
    }
}

fn decimal_of(v: &Js) -> Result<BigNumberValue, Thrown> {
    match v {
        Js::Object(_) if v.get("_isBigNumber")?.truthy() => {
            // Unlike toString, valueOf preserves the sign of a cloned zero.
            Ok(BigNumberValue::parse(&text(
                &v.call_method("valueOf", vec![])?,
            )?))
        }
        Js::String(s) => Ok(BigNumberValue::parse(&String::from_utf16_lossy(s))),
        Js::Number(n) => Ok(BigNumberValue::from_number(*n)),
        other => Ok(BigNumberValue::from_number(other.to_number()?)),
    }
}

fn big_number(value: BigNumberValue) -> Js {
    let object = Js::object_with_class("BigNumber", vec![("_isBigNumber", Js::Bool(true))]);
    let binary = |name: &'static str,
                  op: fn(&BigNumberValue, &BigNumberValue) -> BigNumberValue| {
        let value = value.clone();
        (
            name,
            func(name, move |a| {
                Ok(big_number(op(&value, &decimal_of(&arg(&a, 0))?)))
            }),
        )
    };
    let compare = |name: &'static str, accept: fn(Ordering) -> bool| {
        let value = value.clone();
        (
            name,
            func(name, move |a| {
                Ok(Js::Bool(
                    value.compare(&decimal_of(&arg(&a, 0))?).is_some_and(accept),
                ))
            }),
        )
    };
    let (fixed, string, number) = (value.clone(), value.clone(), value.clone());
    let members = vec![
        binary("plus", BigNumberValue::plus),
        binary("minus", BigNumberValue::minus),
        binary("multipliedBy", BigNumberValue::times),
        binary("times", BigNumberValue::times),
        binary("dividedToIntegerBy", BigNumberValue::integer_quotient),
        binary("idiv", BigNumberValue::integer_quotient),
        compare("lt", |o| o == Ordering::Less),
        compare("lte", |o| o != Ordering::Greater),
        compare("gt", |o| o == Ordering::Greater),
        compare("gte", |o| o != Ordering::Less),
        compare("eq", |o| o == Ordering::Equal),
        compare("isEqualTo", |o| o == Ordering::Equal),
        (
            "toFixed",
            func("toFixed", move |a| {
                let dp = arg(&a, 0);
                let dp = if dp.is_undefined() {
                    None
                } else {
                    Some(dp.to_number()? as i64)
                };
                Ok(Js::str(&fixed.fixed(dp)))
            }),
        ),
        (
            "toString",
            func("toString", move |_| Ok(Js::str(&string.display()))),
        ),
        (
            "toNumber",
            func("toNumber", move |_| {
                Ok(Js::Number(safe_cex_num::number_from_string(
                    &number.value_string(),
                )))
            }),
        ),
    ];
    for (k, v) in members {
        let _ = object.define(k, v);
    }
    for name in ["valueOf", "toJSON"] {
        let shown = value.clone();
        let _ = object.define(
            name,
            func(name, move |_| Ok(Js::str(&shown.value_string()))),
        );
    }
    let finite = matches!(&value, BigNumberValue::Finite(_));
    let _ = object.define("isFinite", func("isFinite", move |_| Ok(Js::Bool(finite))));
    let _ = object.define(
        "isNaN",
        func("isNaN", move |_| {
            Ok(Js::Bool(matches!(value, BigNumberValue::NaN)))
        }),
    );
    object
}

fn extreme(args: &[Js], pick: Ordering) -> Result<Js, Thrown> {
    let mut best: Option<BigNumberValue> = None;
    for v in args {
        let d = decimal_of(v)?;
        if matches!(d, BigNumberValue::NaN) {
            return Ok(big_number(d));
        }
        if best.as_ref().is_none_or(|b| {
            d.compare(b) == Some(pick)
                || d.is_zero()
                    && b.is_zero()
                    && d.negative() != b.negative()
                    && d.negative() == (pick == Ordering::Less)
        }) {
            best = Some(d);
        }
    }
    Ok(big_number(best.unwrap_or(BigNumberValue::NaN)))
}

const DAY_MS: f64 = 86_400_000.0;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}

/// dayjs `add(n, unit)` on the UTC calendar (dayjs uses the host zone).
fn dayjs_add(ms: f64, amount: f64, unit: &str) -> f64 {
    if !ms.is_finite() {
        return f64::NAN;
    }
    let unit = pretty_unit(unit);
    let days = ms.div_euclid(DAY_MS) as i64;
    let in_day = ms.rem_euclid(DAY_MS);
    let (y, m, d) = civil_from_days(days);
    let months = |delta: f64| {
        let total = y * 12 + (m - 1) + delta.trunc() as i64;
        let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
        let nd = d.min(days_in_month(ny, nm));
        days_from_civil(ny, nm, nd) as f64 * DAY_MS + in_day
    };
    match unit.as_str() {
        "month" => months(amount),
        "year" => months(amount.trunc() * 12.0),
        "day" | "date" => ms + safe_cex_num::math_round(amount) * DAY_MS,
        "week" => ms + safe_cex_num::math_round(amount * 7.0) * DAY_MS,
        "hour" => ms + amount * 3_600_000.0,
        "minute" => ms + amount * 60_000.0,
        "second" => ms + amount * 1_000.0,
        _ => ms + amount,
    }
}

/// Date#toISOString: `YYYY-MM-DDTHH:mm:ss.sssZ` in UTC; None for an invalid date.
fn iso_string(ms: f64) -> Option<String> {
    if !ms.is_finite() {
        return None;
    }
    let days = ms.div_euclid(DAY_MS) as i64;
    let in_day = ms.rem_euclid(DAY_MS) as i64;
    let (y, m, d) = civil_from_days(days);
    let (h, min, s, milli) = (
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000,
    );
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{milli:03}Z"
    ))
}

fn dayjs_ms(v: &Js, now: f64) -> Result<f64, Thrown> {
    Ok(match v {
        Js::Undefined => now,
        Js::Null => f64::NAN,
        Js::Number(n) => *n,
        Js::Object(_) if v.get("_isDayjs")?.truthy() => {
            v.call_method("valueOf", vec![])?.to_number()?
        }
        Js::String(s) => parse_iso(&String::from_utf16_lossy(s)).unwrap_or(f64::NAN),
        other => other.to_number()?,
    })
}

/// `dayjs(input)`: a facade input is cloned with its utc-plugin state, as `clone()` does.
fn dayjs_from(v: &Js, now: f64) -> Result<Js, Thrown> {
    if let Some(state) = DayjsState::of(v)? {
        return Ok(dayjs_with(state));
    }
    Ok(dayjs(dayjs_ms(v, now)?))
}

/// `YYYY-MM-DD[THH:mm[:ss[.sss]]][Z]`, read as UTC.
fn parse_iso(s: &str) -> Option<f64> {
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once(['T', ' ']).unwrap_or((s, ""));
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (
        parts.next()??,
        parts.next().flatten().unwrap_or(1),
        parts.next().flatten().unwrap_or(1),
    );
    let mut ms = days_from_civil(y, m, d) as f64 * DAY_MS;
    if !time.is_empty() {
        let mut t = time.split(':');
        let h: f64 = t.next()?.parse().ok()?;
        let min: f64 = t.next().unwrap_or("0").parse().ok()?;
        let sec: f64 = t.next().unwrap_or("0").parse().ok()?;
        ms += h * 3_600_000.0 + min * 60_000.0 + (sec * 1000.0).round();
    }
    Some(ms)
}

/// dayjs + utc plugin instance fields in a UTC host zone: `$d` is the wall clock,
/// `$offset`/`$u`/`$x.$localOffset` follow dayjs 1.11 `plugin/utc` exactly.
#[derive(Clone)]
struct DayjsState {
    wall: f64,
    offset: Option<Js>,
    utc: bool,
    local_offset: Option<f64>,
}

impl DayjsState {
    const FIELDS: [&'static str; 4] = ["$d", "$offset", "$u", "$localOffset"];

    fn plain(ms: f64) -> Self {
        DayjsState {
            wall: safe_cex_num::js_time_clip(ms),
            offset: None,
            utc: false,
            local_offset: None,
        }
    }
    /// utc plugin `valueOf`: offset addition retains ordinary JS coercion.
    fn value_of(&self) -> Result<f64, Thrown> {
        let Some(offset) = &self.offset else {
            return Ok(self.wall);
        };
        let local = self
            .local_offset
            .filter(|v| *v != 0.0 && !v.is_nan())
            .unwrap_or(if self.wall.is_nan() { f64::NAN } else { 0.0 });
        let total = super::ops::add(offset, &Js::Number(local))?.to_number()?;
        Ok(self.wall - total * 60_000.0)
    }
    fn utc_offset(&self) -> Js {
        if self.utc {
            Js::Number(0.0)
        } else if let Some(offset) = &self.offset {
            offset.clone()
        } else {
            Js::Number(if self.wall.is_nan() { f64::NAN } else { -0.0 })
        }
    }
    fn timezone_offset(&self) -> Result<f64, Thrown> {
        Ok(if self.value_of()?.is_finite() {
            0.0
        } else {
            f64::NAN
        })
    }
    fn with_wall(&self, wall: f64) -> Self {
        DayjsState {
            wall: safe_cex_num::js_time_clip(wall),
            ..self.clone()
        }
    }
    fn of(v: &Js) -> Result<Option<Self>, Thrown> {
        if !matches!(v, Js::Object(o) if o.class == "Dayjs") {
            return Ok(None);
        }
        let [wall, offset, utc, local_offset] = Self::FIELDS.map(|key| v.get(key));
        let optional = |v: Js| (!v.is_undefined()).then(|| v.to_number()).transpose();
        Ok(Some(DayjsState {
            wall: wall?.to_number()?,
            offset: {
                let value = offset?;
                (!value.is_undefined()).then_some(value)
            },
            utc: utc?.truthy(),
            local_offset: optional(local_offset?)?,
        }))
    }
}

/// dayjs `prettyUnit`: shorthand table, else lowercase with one trailing `s` removed.
fn pretty_unit(unit: &str) -> String {
    match unit {
        "M" => "month".into(),
        "y" => "year".into(),
        "w" => "week".into(),
        "d" => "day".into(),
        "D" => "date".into(),
        "h" => "hour".into(),
        "m" => "minute".into(),
        "s" => "second".into(),
        "ms" => "millisecond".into(),
        "Q" => "quarter".into(),
        other => {
            let lower = other.to_lowercase();
            lower.strip_suffix('s').map_or(lower.clone(), str::to_owned)
        }
    }
}

/// dayjs `startOf(unit)` on the wall clock; en locale weeks start on Sunday and
/// units without a core case (quarter, isoWeek, unknown) return a clone.
fn dayjs_start_of(wall: f64, unit: &str, start: bool) -> f64 {
    if !wall.is_finite() {
        return f64::NAN;
    }
    let days = wall.div_euclid(DAY_MS) as i64;
    let in_day = wall.rem_euclid(DAY_MS);
    let (y, m, _) = civil_from_days(days);
    let day = |d: i64| d as f64 * DAY_MS;
    let (begin, next) = match unit {
        "year" => (
            day(days_from_civil(y, 1, 1)),
            day(days_from_civil(y + 1, 1, 1)),
        ),
        "month" => (
            day(days_from_civil(y, m, 1)),
            day(days_from_civil(
                y + i64::from(m == 12),
                if m == 12 { 1 } else { m + 1 },
                1,
            )),
        ),
        "week" => {
            let first = days - (days + 4).rem_euclid(7);
            (day(first), day(first + 7))
        }
        "day" | "date" => (day(days), day(days + 1)),
        "hour" | "minute" | "second" => {
            let length = match unit {
                "hour" => 3_600_000.0,
                "minute" => 60_000.0,
                _ => 1_000.0,
            };
            let begin = day(days) + in_day - in_day % length;
            (begin, begin + length)
        }
        _ => return wall,
    };
    if start { begin } else { next - 1.0 }
}

fn dayjs_offset(input: &[u16]) -> Option<f64> {
    let digit = |unit: u16| (48..=57).contains(&unit);
    let index = (0..input.len()).find(|&i| {
        matches!(input[i], 43 | 45)
            && input.get(i + 1).is_some_and(|v| digit(*v))
            && input.get(i + 2).is_some_and(|v| digit(*v))
    })?;
    let hour = f64::from((input[index + 1] - 48) * 10 + input[index + 2] - 48);
    let minute = index + 3 + usize::from(input.get(index + 3) == Some(&58));
    if !input.get(minute).is_some_and(|v| digit(*v))
        || !input.get(minute + 1).is_some_and(|v| digit(*v))
    {
        return Some(f64::NAN);
    }
    let total = hour * 60.0 + f64::from((input[minute] - 48) * 10 + input[minute + 1] - 48);
    Some(if total == 0.0 || input[index] == 43 {
        total
    } else {
        -total
    })
}

fn dayjs(ms: f64) -> Js {
    dayjs_with(DayjsState::plain(ms))
}

fn dayjs_with(state: DayjsState) -> Js {
    let at = |f: fn(f64) -> f64| {
        let state = state.clone();
        func("", move |_| Ok(Js::Number(f(state.value_of()?))))
    };
    let shift = |sign: f64| {
        let state = state.clone();
        func("", move |a| {
            let amount = arg(&a, 0).to_number()?;
            let unit = arg(&a, 1);
            let unit = if unit.is_undefined() {
                String::new()
            } else {
                text(&unit)?
            };
            Ok(dayjs_with(state.with_wall(dayjs_add(
                state.wall,
                sign * amount,
                &unit,
            ))))
        })
    };
    let order = |want: Ordering| {
        let state = state.clone();
        func("", move |a| {
            let other = dayjs_ms(&arg(&a, 0), f64::NAN)?;
            Ok(Js::Bool(
                state.value_of()?.partial_cmp(&other) == Some(want),
            ))
        })
    };
    let offset_state = state.clone();
    let utc_offset = Js::function("utcOffset", move |this, a| {
        let state = &offset_state;
        let input = match arg(&a, 0) {
            Js::Undefined => return Ok(state.utc_offset()),
            Js::String(value) => match dayjs_offset(&value) {
                Some(offset) => Js::Number(offset),
                None => return Ok(this),
            },
            value => value,
        };
        let offset = if input.to_number()?.abs() <= 16.0 {
            Js::Number(input.to_number()? * 60.0)
        } else {
            input
        };
        let keep_local_time = arg(&a, 1).truthy();
        if offset.strict_equals(&Js::Number(0.0)) {
            let utc = DayjsState {
                utc: true,
                ..DayjsState::plain(state.value_of()?)
            };
            return Ok(dayjs_with(if keep_local_time {
                utc.with_wall(dayjs_add(
                    utc.wall,
                    state.utc_offset().to_number()?,
                    "minute",
                ))
            } else {
                utc
            }));
        }
        if keep_local_time {
            return Ok(dayjs_with(DayjsState {
                offset: Some(offset),
                utc: false,
                ..state.clone()
            }));
        }
        let local_timezone_offset = if state.utc {
            state.timezone_offset()?
        } else {
            -state.utc_offset().to_number()?
        };
        let local = DayjsState::plain(state.value_of()?);
        let amount = super::ops::add(&offset, &Js::Number(local_timezone_offset))?.to_number()?;
        Ok(dayjs_with(DayjsState {
            wall: safe_cex_num::js_time_clip(dayjs_add(local.wall, amount, "minute")),
            offset: Some(offset),
            utc: false,
            local_offset: Some(local_timezone_offset),
        }))
    });
    let start_state = state.clone();
    let start_of = func("startOf", move |a| {
        let unit = arg(&a, 0);
        let unit = if unit.is_undefined() {
            String::new()
        } else {
            text(&unit)?
        };
        let flag = arg(&a, 1);
        Ok(dayjs_with(start_state.with_wall(dayjs_start_of(
            start_state.wall,
            &pretty_unit(&unit),
            flag.is_undefined() || flag.truthy(),
        ))))
    });
    let iso_state = state.clone();
    let object = Js::object_with_class(
        "Dayjs",
        vec![
            ("_isDayjs", Js::Bool(true)),
            ("valueOf", at(|ms| ms)),
            ("unix", at(|ms| (ms / 1000.0).floor())),
            ("add", shift(1.0)),
            ("subtract", shift(-1.0)),
            ("isBefore", order(Ordering::Less)),
            ("isAfter", order(Ordering::Greater)),
            ("utcOffset", utc_offset),
            ("startOf", start_of),
            (
                "toISOString",
                func("toISOString", move |_| {
                    iso_string(iso_state.value_of()?)
                        .map(|s| Js::str(&s))
                        .ok_or_else(|| Thrown::error("RangeError", "Invalid time value"))
                }),
            ),
        ],
    );
    let optional = |v: Option<f64>| v.map_or(Js::Undefined, Js::Number);
    let fields = [
        Js::Number(state.wall),
        state.offset.unwrap_or(Js::Undefined),
        Js::Bool(state.utc),
        optional(state.local_offset),
    ];
    if let Js::Object(data) = &object {
        let mut props = data.props.borrow_mut();
        for (key, value) in DayjsState::FIELDS.into_iter().zip(fields) {
            props.insert(
                key.encode_utf16().collect::<Vec<u16>>().into(),
                super::value::Prop::DataDescriptor {
                    value,
                    writable: false,
                    enumerable: false,
                    configurable: false,
                },
            );
        }
    }
    object
}

// The option-bearing qs calls used by venue signers and params serializers.
struct QsOptions {
    arrays: String,
    encode: bool,
    form: bool,
    sort: Js,
}
impl QsOptions {
    fn read(value: &Js) -> Result<Self, Thrown> {
        let get = |key: &str| {
            if value.is_nullish() {
                Ok(Js::Undefined)
            } else {
                value.get(key)
            }
        };
        let arrays = text(&get("arrayFormat")?)?;
        let format = get("format")?;
        if !format.is_undefined() && !matches!(text(&format)?.as_str(), "RFC3986" | "RFC1738") {
            return Err(Thrown::type_error("Unknown format option provided."));
        }
        Ok(Self {
            arrays,
            encode: !matches!(get("encode")?, Js::Bool(false)),
            form: format.strict_equals(&Js::str("RFC1738")),
            sort: get("sort")?,
        })
    }
    fn encode(&self, value: &str) -> String {
        if !self.encode {
            return if self.form {
                value.replace("%20", "+")
            } else {
                value.to_owned()
            };
        }
        let mut output = String::new();
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric()
                || b"-._~".contains(&byte)
                || (self.form && b"()".contains(&byte))
            {
                output.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(output, "%{byte:02X}").expect("writing to String");
            }
        }
        if self.form {
            output.replace("%20", "+")
        } else {
            output
        }
    }
    fn keys(&self, value: &Js) -> Result<Vec<super::value::Str>, Thrown> {
        let mut keys = value.own_keys();
        if self.sort.is_callable() {
            // Stable insertion avoids Rust sort's total-order precondition on JS comparators.
            // Read values after sorting keys, as qs does.
            for index in 1..keys.len() {
                let mut at = index;
                while at > 0 {
                    let order = self
                        .sort
                        .call(
                            Js::Undefined,
                            vec![Js::utf16(&keys[at - 1]), Js::utf16(&keys[at])],
                        )?
                        .to_number()?;
                    if order <= 0.0 || order.is_nan() {
                        break;
                    }
                    keys.swap(at - 1, at);
                    at -= 1;
                }
            }
        }
        Ok(keys)
    }
}
fn qs_pairs(
    prefix: &str,
    value: &Js,
    options: &QsOptions,
    out: &mut Vec<String>,
) -> Result<(), Thrown> {
    match value {
        Js::Undefined => {}
        Js::Null => out.push(format!("{}=", options.encode(prefix))),
        Js::Array(_) if options.arrays == "comma" => {
            if !list(value).is_empty() {
                out.push(format!(
                    "{}={}",
                    options.encode(prefix),
                    options.encode(&text(value)?)
                ));
            }
        }
        Js::Array(_) | Js::Object(_) => {
            for key in options.keys(value)? {
                let next = if value.is_array() && options.arrays == "repeat" {
                    prefix.to_owned()
                } else if value.is_array() && options.arrays == "brackets" {
                    format!("{prefix}[]")
                } else {
                    format!("{prefix}[{}]", String::from_utf16_lossy(&key))
                };
                qs_pairs(&next, &value.get_key(&key)?, options, out)?;
            }
        }
        other => out.push(format!(
            "{}={}",
            options.encode(prefix),
            options.encode(&text(other)?)
        )),
    }
    Ok(())
}
pub fn qs_stringify(value: &Js, options: &Js) -> Result<String, Thrown> {
    let options = QsOptions::read(options)?;
    let mut out = vec![];
    if matches!(value, Js::Object(_) | Js::Array(_)) {
        for key in options.keys(value)? {
            qs_pairs(
                &String::from_utf16_lossy(&key),
                &value.get_key(&key)?,
                &options,
                &mut out,
            )?;
        }
    }
    Ok(out.join("&"))
}

fn decode_uri_component(s: &[u16]) -> Result<Vec<u16>, Thrown> {
    let malformed = || Thrown::error("URIError", "URI malformed");
    let hex = |i: usize| -> Result<u8, Thrown> {
        let pair = s.get(i + 1..i + 3).ok_or_else(malformed)?;
        let pair = String::from_utf16(pair).map_err(|_| malformed())?;
        u8::from_str_radix(&pair, 16).map_err(|_| malformed())
    };
    let mut out = vec![];
    let mut i = 0;
    while i < s.len() {
        if s[i] != u16::from(b'%') {
            out.push(s[i]);
            i += 1;
            continue;
        }
        let first = hex(i)?;
        let width = match first {
            0x00..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(malformed()),
        };
        let mut bytes = vec![first];
        for k in 1..width {
            if s.get(i + 3 * k) != Some(&u16::from(b'%')) {
                return Err(malformed());
            }
            bytes.push(hex(i + 3 * k)?);
        }
        let decoded = std::str::from_utf8(&bytes).map_err(|_| malformed())?;
        out.extend(decoded.encode_utf16());
        i += 3 * width;
    }
    Ok(out)
}

const URL_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

fn nanoid<X>(scope: &Scope<X>, size: usize) -> String {
    let runtime = scope.exchange_runtime();
    (0..size)
        .map(|_| char::from(URL_ALPHABET[(runtime.random() * 64.0) as usize & 63]))
        .collect()
}

fn sleep_promise<X>(scope: &Scope<X>, ms: f64) -> Js {
    let runtime = scope.exchange_runtime();
    let delay = if ms.is_finite() && ms > 0.0 {
        ms as u64
    } else {
        0
    };
    let sleeper = runtime.clone();
    Js::Promise(Promise::spawn(
        &runtime,
        Box::pin(async move {
            sleeper.sleep(delay).await?;
            Ok(Js::Undefined)
        }),
    ))
}

fn update_order_book(book: &Js, side: &Js, update: Js, upsert: bool) -> Result<Js, Thrown> {
    let levels = book.get(&text(side)?)?;
    let descending = side.strict_equals(&Js::str("bids"));
    // The source evaluates update.price before entering binarySearchPrice.
    let price = update.get("price")?;
    let (mut left, mut right) = (0.0, levels.get("length")?.to_number()? - 1.0);
    let mut found = None;
    while left <= right {
        // Source midpoint is (left + right) >>> 1, including Uint32 wrapping.
        let mid = ((left + right).trunc().rem_euclid(4_294_967_296.0) as u32 >> 1) as f64;
        let mid_price = levels.get(&Js::Number(mid).to_display())?.get("price")?;
        if mid_price.strict_equals(&price) {
            found = Some(mid);
            break;
        }
        let after = if descending {
            ops::greater_than(&mid_price, &price)?
        } else {
            ops::less_than(&mid_price, &price)?
        };
        if after {
            left = mid + 1.0;
        } else {
            right = mid - 1.0;
        }
    }
    let index = match found {
        None => {
            if !upsert || update.get("amount")?.strict_equals(&Js::Number(0.0)) {
                return Ok(Js::Number(-1.0));
            }
            let Js::Array(rows) = &levels else {
                return Err(Thrown::type_error("orderBook side is not an array"));
            };
            let mut rows = rows.borrow_mut();
            // splice clamps against the current length, after callback effects.
            let insert_at = (left as usize).min(rows.len());
            rows.insert(insert_at, Some(update));
            left
        }
        Some(index) => {
            if update.get("amount")?.strict_equals(&Js::Number(0.0)) {
                let Js::Array(rows) = &levels else {
                    return Err(Thrown::type_error("orderBook side is not an array"));
                };
                let removed = {
                    let mut rows = rows.borrow_mut();
                    (index < rows.len() as f64).then(|| rows.remove(index as usize))
                };
                drop(removed);
                return Ok(Js::Number(-1.0));
            }
            let level = levels.get(&Js::Number(index).to_display())?;
            level.set("amount", update.get("amount")?)?;
            index
        }
    };
    Ok(Js::Number(index))
}

// lodash times: ToInteger count, castFunction (non-callables are identity),
// and ordered calls with one index argument and an undefined receiver.
fn lodash_times(args: &[Js]) -> Result<Js, Thrown> {
    let count = arg(args, 0).to_number()?.trunc();
    if !(1. ..=9_007_199_254_740_991.).contains(&count) {
        return Ok(Js::array(vec![]));
    }
    let iteratee = arg(args, 1);
    let invoke = |index: u64| {
        let value = Js::Number(index as f64);
        if iteratee.is_callable() {
            iteratee.call(Js::Undefined, vec![value])
        } else {
            Ok(value)
        }
    };
    const MAX_ARRAY_LENGTH: u64 = 4_294_967_295;
    let count = count as u64;
    let mut values = vec![];
    for index in 0..count.min(MAX_ARRAY_LENGTH) {
        values.push(invoke(index)?);
    }
    // Preserve lodash's counter/bound arithmetic beyond its result-array limit.
    let mut index = MAX_ARRAY_LENGTH;
    let remaining = count.saturating_sub(MAX_ARRAY_LENGTH);
    loop {
        index += 1;
        if index >= remaining {
            break;
        }
        invoke(index)?;
    }
    Ok(Js::array(values))
}

fn define_on(owner: &Js, key: &str, value: Js) {
    let _ = owner.define(key, value);
}

/// Install the shared utility modules next to `install_builtins`' globals.
pub fn install_library<X: 'static>(g: &Js, rt: Weak<ScopeShared<X>>) {
    let def = |k: &str, v: Js| define_on(g, k, v);
    def("times", func("times", |args| lodash_times(&args)));
    let scoped = |name: &'static str, f: fn(&Scope<X>, Vec<Js>) -> Result<Js, Thrown>| -> Js {
        let rt = rt.clone();
        Js::function(name, move |_, a| f(&scope_of(&rt)?, a))
    };

    def(
        "throttle",
        scoped("throttle", |rt, a| {
            let function = arg(&a, 0);
            if !function.is_callable() {
                return Err(Thrown::type_error("Expected a function"));
            }
            let wait = arg(&a, 1).to_number()?;
            let wait = if wait.is_nan() { 0. } else { wait };
            let options = arg(&a, 2);
            let leading = !options.is_object()
                || !in_operator(&options, "leading")?
                || options.get("leading")?.truthy();
            let trailing = !options.is_object()
                || !in_operator(&options, "trailing")?
                || options.get("trailing")?.truthy();
            Ok(super::throttle::new(rt, function, wait, leading, trailing))
        }),
    );

    if let Ok(number) = g.get("Number") {
        if let Ok(parse_int) = g.get("parseInt") {
            define_on(&number, "parseInt", parse_int);
        }
    }
    def(
        "decodeURIComponent",
        func("decodeURIComponent", |a| {
            Ok(Js::utf16(&decode_uri_component(
                &arg(&a, 0).to_js_string()?,
            )?))
        }),
    );

    let chunk = func("chunk", |a| {
        let values = list(&arg(&a, 0));
        let size = arg(&a, 1);
        let size = if size.is_undefined() {
            1.0
        } else {
            size.to_number()?.trunc()
        };
        if values.is_empty() || size.partial_cmp(&1.0).is_none_or(Ordering::is_lt) {
            return Ok(Js::array(vec![]));
        }
        Ok(Js::array(
            values
                .chunks(size.min(usize::MAX as f64) as usize)
                .map(|c| Js::array(c.to_vec()))
                .collect(),
        ))
    });
    let flatten = func("flatten", |a| {
        let mut out = vec![];
        for v in list(&arg(&a, 0)) {
            match v {
                Js::Array(_) => out.extend(list(&v)),
                other => out.push(other),
            }
        }
        Ok(Js::array(out))
    });
    let partition = func("partition", |a| {
        let (mut yes, mut no) = (vec![], vec![]);
        let by = arg(&a, 1);
        for v in list(&arg(&a, 0)) {
            if iteratee(&by, v.clone())?.truthy() {
                yes.push(v);
            } else {
                no.push(v);
            }
        }
        Ok(Js::array(vec![Js::array(yes), Js::array(no)]))
    });
    let sum_by = func("sumBy", |a| {
        let values = list(&arg(&a, 0));
        if values.is_empty() {
            return Ok(Js::Number(0.0));
        }
        let by = arg(&a, 1);
        let mut total = Js::Undefined;
        for v in values {
            let current = iteratee(&by, v)?;
            if !current.is_undefined() {
                total = if total.is_undefined() {
                    current
                } else {
                    ops::add(&total, &current)?
                };
            }
        }
        Ok(total)
    });
    let uniq = func("uniq", |a| uniq_by(list(&arg(&a, 0)), &Js::Undefined));
    let uniq_by_fn = func("uniqBy", |a| uniq_by(list(&arg(&a, 0)), &arg(&a, 1)));
    let order_by_fn = func("orderBy", |a| {
        order_by(&arg(&a, 0), &arg(&a, 1), &arg(&a, 2))
    });
    let lodash = Js::object(vec![]);
    for (name, f) in [
        ("chunk", chunk),
        ("flatten", flatten),
        ("partition", partition),
        ("sumBy", sum_by),
        ("uniq", uniq),
        ("uniqBy", uniq_by_fn),
        ("orderBy", order_by_fn),
    ] {
        def(name, f.clone());
        define_on(&lodash, name, f);
    }
    def("lodash", lodash);

    let p_map = scoped("map", |s, a| {
        let f = arg(&a, 1);
        let values = list(&arg(&a, 0));
        let array = arg(&a, 0);
        let started: Vec<Result<Js, Thrown>> = values
            .into_iter()
            .enumerate()
            .map(|(i, v)| call(&f, vec![v, Js::Number(i as f64), array.clone()]))
            .collect();
        Ok(s.spawn(Box::pin(async move {
            let mut out = vec![];
            for r in started {
                out.push(await_js(r?).await?);
            }
            Ok(Js::array(out))
        })))
    });
    let series = |collect: bool| {
        let rt = rt.clone();
        Js::function("series", move |_, a| {
            let s = scope_of(&rt)?;
            let array = arg(&a, 0);
            let f = arg(&a, 1);
            Ok(s.spawn(Box::pin(async move {
                let mut out = vec![];
                let len = list(&array).len();
                for i in 0..len {
                    let Some(Some(v)) = array.array_items().and_then(|xs| xs.get(i).cloned())
                    else {
                        out.push(Js::Undefined);
                        continue;
                    };
                    out.push(
                        await_js(call(&f, vec![v, Js::Number(i as f64), array.clone()])?).await?,
                    );
                }
                Ok(if collect {
                    Js::array(out)
                } else {
                    Js::Undefined
                })
            })))
        })
    };
    let (map_series, for_each_series) = (series(true), series(false));
    def("mapSeries", map_series.clone());
    def("forEachSeries", for_each_series.clone());
    def(
        "p-iteration",
        Js::object(vec![
            ("map", p_map),
            ("mapSeries", map_series),
            ("forEachSeries", for_each_series),
        ]),
    );

    let numeric = |name: &'static str, f: fn(f64, f64) -> f64| {
        func(name, move |a| {
            Ok(Js::Number(f(
                arg(&a, 0).to_number()?,
                arg(&a, 1).to_number()?,
            )))
        })
    };
    def("add", numeric("add", safe_cex_num::add));
    def("subtract", numeric("subtract", safe_cex_num::subtract));
    def("multiply", numeric("multiply", safe_cex_num::multiply));
    def(
        "adjust",
        func("adjust", |a| {
            let value = arg(&a, 0).to_number()?;
            Ok(Js::Number(match arg(&a, 1) {
                Js::String(s) => safe_cex_num::adjust(value, String::from_utf16_lossy(&s)),
                step => safe_cex_num::adjust(value, step.to_number()?),
            }))
        }),
    );
    def(
        "roundUSD",
        func("roundUSD", |a| {
            Ok(Js::Number(safe_cex_num::round_usd(arg(&a, 0).to_number()?)))
        }),
    );
    def(
        "oldestCursorId",
        func("oldestCursorId", |a| {
            let field = text(&arg(&a, 1))?;
            let mut ids = vec![];
            for row in list(&arg(&a, 0)) {
                let raw = if row.is_nullish() {
                    Js::Undefined
                } else {
                    row.get(&field)?
                };
                if !raw.is_nullish() {
                    ids.push(text(&raw)?);
                }
            }
            Ok(safe_cex_num::oldest_cursor_id(&ids).map_or(Js::Undefined, |id| Js::str(&id)))
        }),
    );
    def(
        "createPositionId",
        func("createPositionId", |a| {
            let (symbol, side, mode) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
            if mode.strict_equals(&Js::str("oneway")) || !side.truthy() {
                return Ok(symbol);
            }
            ops::add(
                &ops::add(&Js::String(symbol.to_js_string()?), &Js::str(":"))?,
                &Js::String(side.to_js_string()?),
            )
        }),
    );
    def(
        "normalizeExecutionPositionSide",
        func("normalizeExecutionPositionSide", |a| {
            let raw = arg(&a, 0);
            let value = if raw.is_nullish() {
                String::new()
            } else {
                text(&raw)?
            };
            Ok(Js::str(
                match safe_cex_num::js_trim(&value).to_lowercase().as_str() {
                    "long" | "open long" | "close long" => "long",
                    "short" | "open short" | "close short" => "short",
                    _ => "both",
                },
            ))
        }),
    );
    def(
        "omitUndefined",
        func("omitUndefined", |a| {
            let out = Js::object(vec![]);
            let source = arg(&a, 0);
            if !source.is_nullish() {
                for (k, v) in source.own_entries()? {
                    if !v.is_undefined() {
                        out.define_key(&k, v)?;
                    }
                }
            }
            Ok(out)
        }),
    );
    def(
        "inverseObj",
        func("inverseObj", |a| {
            let source = arg(&a, 0);
            if source.is_nullish() {
                return Err(Thrown::type_error(
                    "Cannot convert undefined or null to object",
                ));
            }
            let out = Js::object(vec![]);
            for (k, v) in source.own_entries()? {
                out.set_key(&v.to_js_string()?, Js::String(k))?;
            }
            Ok(out)
        }),
    );
    def(
        "calcOrderBookTotal",
        func("calcOrderBookTotal", |a| {
            let book = arg(&a, 0);
            // Destructure both sides before any amount getter can mutate the book.
            let bids = book.get("bids")?;
            let asks = book.get("asks")?;
            for levels in [bids, asks] {
                let mut total = Js::Number(0.0);
                let mut index = 0.0;
                while ops::less_than(&Js::Number(index), &levels.get("length")?)? {
                    let key = Js::Number(index).to_display();
                    total = ops::add(&total, &levels.get(&key)?.get("amount")?)?;
                    // Re-read the row after the amount getter, as the source does.
                    levels.get(&key)?.set("total", total.clone())?;
                    index += 1.0;
                }
            }
            Ok(Js::Undefined)
        }),
    );
    def(
        "updateOrderBook",
        func("updateOrderBook", |a| {
            let upsert = arg(&a, 3);
            update_order_book(
                &arg(&a, 0),
                &arg(&a, 1),
                arg(&a, 2),
                upsert.is_undefined() || upsert.truthy(),
            )
        }),
    );
    def(
        "v",
        func("v", |a| {
            let obj = arg(&a, 0);
            if obj.is_nullish() {
                return Ok(Js::Undefined);
            }
            let key = arg(&a, 1);
            if !obj.is_object() && !key.is_string() {
                return Err(Thrown::type_error(
                    "Right-hand side of 'in' is not an object",
                ));
            }
            let key = text(&key)?;
            for k in [
                key.clone(),
                safe_cex_num::lodash_snake_case(&key),
                safe_cex_num::lodash_camel_case(&key),
            ] {
                if in_operator(&obj, &k)? {
                    return obj.get(&k);
                }
            }
            Ok(Js::Undefined)
        }),
    );
    def(
        "mergeUpdatesUsingLastUpdate",
        scoped("mergeUpdatesUsingLastUpdate", |s, a| {
            let now = s.now();
            let existing = list(&arg(&a, 1));
            let cutoff = arg(&a, 2);
            let cutoff = if cutoff.is_undefined() {
                1000.0
            } else {
                cutoff.to_number()?
            };
            let mut merged = vec![];
            for update in list(&arg(&a, 0)) {
                let id = update.get("id")?;
                let mut found = Js::Undefined;
                for e in &existing {
                    if e.get("id")?.strict_equals(&id) {
                        found = e.clone();
                        break;
                    }
                }
                let last = if found.is_nullish() {
                    Js::Undefined
                } else {
                    found.get("lastUpdate")?
                };
                let fresh = last.truthy()
                    && (now - last.to_number()?).partial_cmp(&cutoff) != Some(Ordering::Greater);
                merged.push(if fresh { found } else { update });
            }
            merged.extend(existing);
            uniq_by(merged, &Js::str("id"))
        }),
    );

    def(
        "BigNumber",
        func("BigNumber", |a| Ok(big_number(decimal_of(&arg(&a, 0))?))),
    );
    if let Ok(ctor) = g.get("BigNumber") {
        for (name, pick) in [
            ("minimum", Ordering::Less),
            ("min", Ordering::Less),
            ("maximum", Ordering::Greater),
            ("max", Ordering::Greater),
        ] {
            define_on(&ctor, name, func(name, move |a| extreme(&a, pick)));
        }
    }
    def("dayjs", {
        let rt = rt.clone();
        Js::function("dayjs", move |_, a| {
            let input = arg(&a, 0);
            let now = if input.is_undefined() {
                scope_of(&rt)?.now()
            } else {
                f64::NAN
            };
            dayjs_from(&input, now)
        })
    });
    if let Ok(clock) = g.get("virtualClock") {
        define_on(
            &clock,
            "getCurrentTime",
            func("getCurrentTime", |_| {
                Ok(dayjs(
                    safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time(),
                ))
            }),
        );
    }
    def(
        "qs",
        Js::object(vec![(
            "stringify",
            func("stringify", |a| {
                Ok(Js::str(&qs_stringify(&arg(&a, 0), &arg(&a, 1))?))
            }),
        )]),
    );
    def(
        "createHmac",
        func("createHmac", |a| {
            let algorithm = text(&arg(&a, 0))?;
            if algorithm != "sha256" {
                return Err(Thrown::error(
                    "Error",
                    &format!("Digest method not supported: {algorithm}"),
                ));
            }
            let key = text(&arg(&a, 1))?;
            let payload = Rc::new(std::cell::RefCell::new(String::new()));
            let hmac = Js::object_with_class("Hmac", vec![]);
            let (sink, this) = (payload.clone(), hmac.clone());
            define_on(
                &hmac,
                "update",
                func("update", move |a| {
                    sink.borrow_mut().push_str(&text(&arg(&a, 0))?);
                    Ok(this.clone())
                }),
            );
            define_on(
                &hmac,
                "digest",
                func("digest", move |a| {
                    let encoding = match text(&arg(&a, 0))?.as_str() {
                        "hex" => safe_cex_crypto::Encoding::Hex,
                        _ => safe_cex_crypto::Encoding::Base64,
                    };
                    Ok(Js::str(&safe_cex_crypto::hmac_sha256(
                        key.as_bytes(),
                        payload.borrow().as_bytes(),
                        encoding,
                    )))
                }),
            );
            Ok(hmac)
        }),
    );
    def(
        "nanoid",
        scoped("nanoid", |s, a| {
            let size = arg(&a, 0);
            let size = if size.is_undefined() {
                21.0
            } else {
                size.to_number()?
            };
            Ok(Js::str(&nanoid(s, size.max(0.0) as usize)))
        }),
    );
    def(
        "uuid",
        scoped("uuid", |s, _| {
            Ok(Js::str(&nanoid(s, 21).replace(['-', '_'], "")))
        }),
    );
    def(
        "sleep",
        scoped("sleep", |s, a| {
            Ok(sleep_promise(s, arg(&a, 0).to_number()?))
        }),
    );
    def(
        "loop",
        scoped("loop", |s, a| {
            let f = arg(&a, 0);
            let min = arg(&a, 1).to_number()?;
            if min == f64::INFINITY {
                return Ok(s.spawn(Box::pin(async { Ok(Js::Undefined) })));
            }
            let interval = if min > 0.0 {
                min
            } else {
                s.exchange_runtime().console(
                    "warn",
                    "[safe-cex] loop() called without a minInterval — defaulting to 5000ms.",
                );
                5000.0
            };
            let slept = sleep_promise(s, interval);
            let scope = s.clone();
            Ok(s.spawn(Box::pin(async move {
                await_js(slept).await?;
                scope.set_timeout(Rc::new(move || call(&f, vec![]).map(|_| ())), 0.0);
                Ok(Js::Undefined)
            })))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globals() -> Js {
        let g = Js::object(vec![]);
        super::super::globals::install_builtins(&g, Weak::<ScopeShared<()>>::new());
        install_library(&g, Weak::<ScopeShared<()>>::new());
        g
    }
    fn run(g: &Js, name: &str, args: Vec<Js>) -> Js {
        call(&g.get(name).unwrap(), args).unwrap()
    }
    fn json(v: &Js) -> String {
        s(&globals()
            .get("JSON")
            .unwrap()
            .call_method("stringify", vec![v.clone()])
            .unwrap())
    }
    fn s(v: &Js) -> String {
        text(v).unwrap()
    }

    #[test]
    fn lodash_times_preserves_count_callback_and_throw_identity() {
        use std::cell::RefCell;
        let g = globals();
        assert!(
            g.get("times").unwrap().is_callable(),
            "shared lodash times import"
        );
        assert_eq!(json(&run(&g, "times", vec![Js::Number(3.9)])), "[0,1,2]");
        let calls = Rc::new(RefCell::new(vec![]));
        let seen = calls.clone();
        let thrown = Js::object(vec![("marker", Js::str("callback rejection"))]);
        let failure = thrown.clone();
        let callback = Js::function("iteratee", move |this, args| {
            assert!(this.is_undefined());
            assert_eq!(args.len(), 1);
            let index = args[0].to_number()?;
            seen.borrow_mut().push(index);
            if index == 1. {
                Err(Thrown::new(failure.clone()))
            } else {
                Ok(Js::Number(index))
            }
        });
        let result = g
            .get("times")
            .unwrap()
            .call(Js::Undefined, vec![Js::str("3"), callback]);
        assert!(result.unwrap_err().value.strict_equals(&thrown));
        assert_eq!(*calls.borrow(), [0., 1.]);
        for n in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.,
            -1.,
            9007199254740992.,
        ] {
            assert_eq!(json(&run(&g, "times", vec![Js::Number(n)])), "[]");
        }
    }

    #[test]
    fn lodash_times_matches_installed_source() {
        use std::cell::RefCell;
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_lodash_times_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "lodash times source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        let g = globals();
        for case in cases {
            let input = &case["input"];
            let count = match input["special"].as_str() {
                Some("NaN") => Js::Number(f64::NAN),
                Some("Infinity") => Js::Number(f64::INFINITY),
                Some("-Infinity") => Js::Number(f64::NEG_INFINITY),
                Some("-0") => Js::Number(-0.),
                Some(_) => unreachable!(),
                None => input
                    .get("count")
                    .map_or(Js::Undefined, super::super::value::from_json),
            };
            let calls = Rc::new(RefCell::new(vec![]));
            let seen = calls.clone();
            let callback = match case["kind"].as_str().unwrap() {
                "absent" => Js::Undefined,
                "number" => Js::Number(7.),
                "property" => Js::str("name"),
                "function" => Js::function("iteratee", move |this, args| {
                    let index = args[0].to_number()?;
                    seen.borrow_mut().push(serde_json::json!({"index":index as u64,"arity":args.len(),"undefinedThis":this.is_undefined()}));
                    Ok(Js::Number(index * 10.))
                }),
                _ => unreachable!(),
            };
            let values = run(&g, "times", vec![count, callback]);
            let actual: serde_json::Value = serde_json::from_str(&json(&values)).unwrap();
            assert_eq!(actual, case["values"], "source times values: {case}");
            assert_eq!(
                serde_json::json!(*calls.borrow()),
                case["calls"],
                "source times calls: {case}"
            );
        }
    }

    #[test]
    fn big_number_is_exact_decimal() {
        let g = globals();
        let n = |x: Js| run(&g, "BigNumber", vec![x]);
        let sum = n(Js::str("0.1"))
            .call_method("plus", vec![Js::Number(0.2)])
            .unwrap();
        assert_eq!(s(&sum.call_method("toString", vec![]).unwrap()), "0.3");
        let step = n(Js::Number(0.01));
        let lot = n(Js::Number(1000.005))
            .call_method("dividedToIntegerBy", vec![step.clone()])
            .unwrap()
            .call_method("multipliedBy", vec![step])
            .unwrap();
        assert_eq!(s(&lot.call_method("toFixed", vec![]).unwrap()), "1000");
        let fixed = |x: &str, dp: f64| {
            s(&n(Js::str(x))
                .call_method("toFixed", vec![Js::Number(dp)])
                .unwrap())
        };
        assert_eq!(fixed("1.005", 2.0), "1.01");
        assert_eq!(fixed("-1.005", 2.0), "-1.01");
        assert_eq!(fixed("1.5", 3.0), "1.500");
        let shown = |x: f64| s(&n(Js::Number(x)).call_method("toString", vec![]).unwrap());
        assert_eq!(shown(1e21), "1e+21");
        assert_eq!(shown(1e-7), "1e-7");
        assert_eq!(shown(123.25), "123.25");
        let min = call(
            &g.get("BigNumber").unwrap().get("minimum").unwrap(),
            vec![n(Js::Number(3.0)), Js::Number(2.5)],
        )
        .unwrap();
        assert_eq!(
            min.call_method("toNumber", vec![]).unwrap().as_f64(),
            Some(2.5)
        );
        assert!(
            n(Js::str("abc"))
                .call_method("toNumber", vec![])
                .unwrap()
                .as_f64()
                .unwrap()
                .is_nan()
        );
        assert!(
            !n(Js::Number(f64::NAN))
                .call_method("gt", vec![Js::Number(0.0)])
                .unwrap()
                .truthy()
        );
    }

    #[test]
    fn dayjs_calendar_units_clamp_month_end() {
        let jan31 = 1_706_659_200_000.0;
        assert_eq!(dayjs_add(jan31, 1.0, "M"), 1_709_164_800_000.0);
        assert_eq!(dayjs_add(jan31, -2.0, "d"), jan31 - 2.0 * DAY_MS);
        assert_eq!(dayjs_add(jan31, -1.0, "w"), jan31 - 7.0 * DAY_MS);
        assert_eq!(dayjs_add(jan31, -300.0, "m"), jan31 - 18_000_000.0);
        assert_eq!(dayjs_add(jan31, 1.0, "y"), 1_738_281_600_000.0);
        assert_eq!(
            dayjs(jan31).call_method("unix", vec![]).unwrap().as_f64(),
            Some(1_706_659_200.0)
        );
        assert_eq!(parse_iso("2024-01-31T00:00:00Z"), Some(jan31));
    }

    #[test]
    fn qs_and_uri_component() {
        let params = Js::object(vec![
            ("a", Js::Number(1.0)),
            ("b", Js::array(vec![Js::Number(1.0), Js::Number(2.0)])),
            ("c", Js::Undefined),
            ("d", Js::Null),
            ("e", Js::str("x y,")),
        ]);
        let encoded = qs_stringify(&params, &Js::Undefined).unwrap();
        assert_eq!(encoded, "a=1&b%5B0%5D=1&b%5B1%5D=2&d=&e=x%20y%2C");
        let decoded = decode_uri_component(&super::super::value::utf16(&encoded)).unwrap();
        assert_eq!(
            String::from_utf16_lossy(&decoded),
            "a=1&b[0]=1&b[1]=2&d=&e=x y,"
        );
        let euro = decode_uri_component(&super::super::value::utf16("%E2%82%AC")).unwrap();
        assert_eq!(String::from_utf16_lossy(&euro), "€");
        assert!(decode_uri_component(&super::super::value::utf16("%E2")).is_err());
    }

    #[test]
    fn lodash_and_utils() {
        let g = globals();
        let nums = |xs: &[f64]| Js::array(xs.iter().map(|x| Js::Number(*x)).collect());
        assert_eq!(
            json(&run(
                &g,
                "chunk",
                vec![nums(&[1.0, 2.0, 3.0]), Js::Number(2.0)]
            )),
            "[[1,2],[3]]"
        );
        assert_eq!(
            json(&run(&g, "chunk", vec![nums(&[1.0]), Js::Number(0.0)])),
            "[]"
        );
        let nested = Js::array(vec![
            nums(&[1.0]),
            Js::Number(2.0),
            Js::array(vec![nums(&[3.0])]),
        ]);
        assert_eq!(json(&run(&g, "flatten", vec![nested])), "[1,2,[3]]");
        let rows = |xs: &[Js]| {
            Js::array(
                xs.iter()
                    .map(|x| Js::object(vec![("a", x.clone())]))
                    .collect(),
            )
        };
        let data = rows(&[Js::Number(2.0), Js::Undefined, Js::Number(1.0), Js::Null]);
        assert_eq!(
            json(&run(
                &g,
                "sumBy",
                vec![rows(&[Js::Number(1.0), Js::Number(2.0)]), Js::str("a")]
            )),
            "3"
        );
        assert_eq!(json(&run(&g, "sumBy", vec![Js::array(vec![])])), "0");
        let asc = run(
            &g,
            "orderBy",
            vec![
                data.clone(),
                Js::array(vec![Js::str("a")]),
                Js::array(vec![Js::str("asc")]),
            ],
        );
        assert_eq!(json(&asc), r#"[{"a":1},{"a":2},{"a":null},{}]"#);
        let desc = run(&g, "orderBy", vec![data, Js::str("a"), Js::str("desc")]);
        assert_eq!(json(&desc), r#"[{},{"a":null},{"a":2},{"a":1}]"#);
        let split = run(
            &g,
            "partition",
            vec![
                nums(&[0.0, 1.0, 2.0]),
                Js::function("", |_, a| Ok(arg(&a, 0))),
            ],
        );
        assert_eq!(json(&split), "[[1,2],[0]]");
        assert_eq!(
            json(&run(
                &g,
                "uniq",
                vec![nums(&[1.0, 1.0, f64::NAN, f64::NAN, 2.0])]
            )),
            "[1,null,2]"
        );
        let by_id = rows(&[Js::Number(1.0), Js::Number(1.0), Js::Number(2.0)]);
        assert_eq!(
            json(&run(&g, "uniqBy", vec![by_id, Js::str("a")])),
            r#"[{"a":1},{"a":2}]"#
        );
        let side = Js::object(vec![("Buy", Js::str("buy"))]);
        assert_eq!(json(&run(&g, "inverseObj", vec![side])), r#"{"buy":"Buy"}"#);
        let omitted = Js::object(vec![("a", Js::Undefined), ("b", Js::Null)]);
        assert_eq!(
            json(&run(&g, "omitUndefined", vec![omitted])),
            r#"{"b":null}"#
        );
        let id = |side: Js, mode: Js| {
            s(&run(
                &g,
                "createPositionId",
                vec![Js::str("BTC"), side, mode],
            ))
        };
        assert_eq!(id(Js::str("long"), Js::str("hedged")), "BTC:long");
        assert_eq!(id(Js::str("long"), Js::str("oneway")), "BTC");
        assert_eq!(id(Js::Undefined, Js::str("hedged")), "BTC");
        assert_eq!(
            s(&run(
                &g,
                "normalizeExecutionPositionSide",
                vec![Js::str(" Close LONG ")]
            )),
            "long"
        );
        assert_eq!(
            s(&run(
                &g,
                "normalizeExecutionPositionSide",
                vec![Js::str("buy")]
            )),
            "both"
        );
        let cursor = rows(&[Js::str("120"), Js::str("99"), Js::str("x"), Js::Null]);
        assert_eq!(
            s(&run(&g, "oldestCursorId", vec![cursor, Js::str("a")])),
            "99"
        );
        let record = Js::object(vec![
            ("order_id", Js::Number(7.0)),
            ("clientId", Js::Number(8.0)),
        ]);
        assert_eq!(
            json(&run(&g, "v", vec![record.clone(), Js::str("orderId")])),
            "7"
        );
        assert_eq!(
            json(&run(&g, "v", vec![record.clone(), Js::str("client_id")])),
            "8"
        );
        assert!(run(&g, "v", vec![record, Js::str("missing")]).is_undefined());
        let finite = g.get("Number").unwrap().get("isFinite").unwrap();
        assert!(!call(&finite, vec![Js::str("1")]).unwrap().truthy());
        let hmac = run(&g, "createHmac", vec![Js::str("sha256"), Js::str("key")])
            .call_method(
                "update",
                vec![Js::str("The quick brown fox jumps over the lazy dog")],
            )
            .unwrap()
            .call_method("digest", vec![Js::str("hex")])
            .unwrap();
        assert_eq!(
            s(&hmac),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn order_book_upsert_keeps_sort() {
        let g = globals();
        let level =
            |p: f64, a: f64| Js::object(vec![("price", Js::Number(p)), ("amount", Js::Number(a))]);
        let book = Js::object(vec![
            ("bids", Js::array(vec![level(3.0, 1.0), level(1.0, 1.0)])),
            ("asks", Js::array(vec![level(4.0, 1.0)])),
        ]);
        let up = |side: &str, l: Js| {
            run(&g, "updateOrderBook", vec![book.clone(), Js::str(side), l]).as_f64()
        };
        assert_eq!(up("bids", level(2.0, 5.0)), Some(1.0));
        assert_eq!(up("bids", level(3.0, 0.0)), Some(-1.0));
        assert_eq!(up("asks", level(4.0, 2.0)), Some(0.0));
        run(&g, "calcOrderBookTotal", vec![book.clone()]);
        assert_eq!(
            json(&book),
            r#"{"bids":[{"price":2,"amount":5,"total":5},{"price":1,"amount":1,"total":6}],"asks":[{"price":4,"amount":2,"total":2}]}"#
        );
    }
}
