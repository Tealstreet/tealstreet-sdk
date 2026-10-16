//! ECMAScript semantics every host family shares that `js_runtime::ops` lacks:
//! live ToPrimitive, exact BigInt relations, `%`, unary `-`, built-in iteration,
//! HasProperty and CopyDataProperties. Primitive `+ - * /` delegate to js_runtime.
use super::value::{to_account, to_js, to_signing};
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use num_bigint::{BigInt, ToBigInt};
use safe_cex_exchange::js_runtime::{
    builtins,
    ops as runtime,
    value::{Js, Str, Thrown, array_index, utf16},
};
use std::{cell::RefCell, cmp::Ordering, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hint {
    Default,
    Number,
    String,
}

/// ToPrimitive via OrdinaryToPrimitive over live valueOf/toString. An absent
/// toString is the intrinsic one (js_runtime models no prototype methods).
pub(crate) fn to_primitive(value: &Js, hint: Hint) -> Result<Js, Thrown> {
    if !value.is_object() {
        return Ok(value.clone());
    }
    let order = if hint == Hint::String {
        ["toString", "valueOf"]
    } else {
        ["valueOf", "toString"]
    };
    for name in order {
        let method = value.get(name)?;
        if method.is_callable() {
            let result = method.call(value.clone(), vec![])?;
            if !result.is_object() {
                return Ok(result);
            }
        } else if name == "toString" && method.is_undefined() {
            return Ok(Js::String(value.to_js_string()?));
        }
    }
    Err(Thrown::type_error("Cannot convert object to primitive value"))
}

/// ToString through ToPrimitive(string).
pub(crate) fn to_string(value: &Js) -> Result<Str, Thrown> {
    to_primitive(value, Hint::String)?.to_js_string()
}

/// ToNumeric: `Js::BigInt` or `Js::Number`.
pub(crate) fn to_numeric(value: &Js) -> Result<Js, Thrown> {
    runtime::to_numeric(&to_primitive(value, Hint::Number)?)
}

/// ToLength, clamped to 2^53 - 1.
pub(crate) fn to_length(value: &Js) -> Result<usize, Thrown> {
    let n = to_primitive(value, Hint::Number)?.to_number()?;
    Ok(if n.is_nan() || n <= 0.0 {
        0
    } else {
        n.min(9_007_199_254_740_991.0) as usize
    })
}

fn mixed_bigint() -> Thrown {
    Thrown::type_error("Cannot mix BigInt and other types, use explicit conversions")
}

fn division_by_zero() -> Thrown {
    Thrown::error("RangeError", "Division by zero")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

fn remainder(left: &Js, right: &Js) -> Result<Js, Thrown> {
    match (runtime::to_numeric(left)?, runtime::to_numeric(right)?) {
        (Js::Number(a), Js::Number(b)) => Ok(Js::Number(a % b)),
        (Js::BigInt(_), Js::BigInt(b)) if *b == BigInt::default() => Err(division_by_zero()),
        (Js::BigInt(a), Js::BigInt(b)) => Ok(Js::BigInt(Rc::new(&*a % &*b))),
        _ => Err(mixed_bigint()),
    }
}

/// The binary arithmetic operators: ToPrimitive left then right, then the
/// primitive operation (string concatenation for `+`).
pub(crate) fn arithmetic(op: Arithmetic, left: &Js, right: &Js) -> Result<Js, Thrown> {
    let hint = if op == Arithmetic::Add {
        Hint::Default
    } else {
        Hint::Number
    };
    let left = to_primitive(left, hint)?;
    let right = to_primitive(right, hint)?;
    match op {
        Arithmetic::Add => runtime::add(&left, &right),
        Arithmetic::Subtract => runtime::subtract(&left, &right),
        Arithmetic::Multiply => runtime::multiply(&left, &right),
        Arithmetic::Divide => runtime::divide(&left, &right),
        Arithmetic::Remainder => remainder(&left, &right),
    }
}

/// Unary `-`; Number negation keeps -0.
pub(crate) fn negate(value: &Js) -> Result<Js, Thrown> {
    Ok(match to_numeric(value)? {
        Js::BigInt(b) => Js::BigInt(Rc::new(-&*b)),
        other => Js::Number(-other.to_number()?),
    })
}

/// StringToBigInt; `None` is the spec's undefined for unparsable text.
fn string_to_bigint(text: &[u16]) -> Option<BigInt> {
    let text = String::from_utf16(text).ok()?;
    let text = safe_cex_num::js_trim(&text);
    if text.is_empty() {
        return Some(BigInt::default());
    }
    let lower = text.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return None;
            }
            return BigInt::parse_bytes(digits.as_bytes(), radix);
        }
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.strip_prefix('+').unwrap_or(text).parse().ok()
}

/// Exact BigInt-vs-Number ordering; `None` for NaN.
fn compare_bigint_number(big: &BigInt, number: f64) -> Option<Ordering> {
    if number.is_nan() {
        return None;
    }
    if number.is_infinite() {
        return Some(if number > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let whole = number.trunc().to_bigint()?;
    Some(match big.cmp(&whole) {
        Ordering::Equal => 0.0.partial_cmp(&number.fract())?,
        other => other,
    })
}

/// IsLessThan over primitives where at least one is a BigInt; `None` is undefined.
fn bigint_less_than(x: &Js, y: &Js) -> Result<Option<bool>, Thrown> {
    Ok(match (x, y) {
        (Js::BigInt(a), Js::BigInt(b)) => Some(a < b),
        (Js::BigInt(a), Js::String(b)) => string_to_bigint(b).map(|b| **a < b),
        (Js::String(a), Js::BigInt(b)) => string_to_bigint(a).map(|a| a < **b),
        (Js::BigInt(a), other) => {
            compare_bigint_number(a, other.to_number()?).map(|o| o == Ordering::Less)
        }
        (other, Js::BigInt(b)) => {
            compare_bigint_number(b, other.to_number()?).map(|o| o == Ordering::Greater)
        }
        _ => unreachable!("bigint_less_than needs a BigInt operand"),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relation {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// The relational operators: ToPrimitive(number) left then right, then IsLessThan.
/// js_runtime compares BigInt through f64, so BigInt operands stay exact here.
pub(crate) fn relation(op: Relation, left: &Js, right: &Js) -> Result<bool, Thrown> {
    let left = to_primitive(left, Hint::Number)?;
    let right = to_primitive(right, Hint::Number)?;
    if !matches!(left, Js::BigInt(_)) && !matches!(right, Js::BigInt(_)) {
        return match op {
            Relation::Less => runtime::less_than(&left, &right),
            Relation::LessOrEqual => runtime::less_equal(&left, &right),
            Relation::Greater => runtime::greater_than(&left, &right),
            Relation::GreaterOrEqual => runtime::greater_equal(&left, &right),
        };
    }
    Ok(match op {
        Relation::Less => bigint_less_than(&left, &right)? == Some(true),
        Relation::Greater => bigint_less_than(&right, &left)? == Some(true),
        Relation::LessOrEqual => bigint_less_than(&right, &left)? == Some(false),
        Relation::GreaterOrEqual => bigint_less_than(&left, &right)? == Some(false),
    })
}

pub(crate) fn arithmetic_value(
    op: Arithmetic,
    left: AccountValue,
    right: AccountValue,
) -> SigningResult<AccountValue> {
    arithmetic(op, &to_js(&left), &to_js(&right))
        .map(|v| to_account(&v))
        .map_err(to_signing)
}

pub(crate) fn relation_value(
    op: Relation,
    left: AccountValue,
    right: AccountValue,
) -> SigningResult<bool> {
    relation(op, &to_js(&left), &to_js(&right)).map_err(to_signing)
}

/// A relational operator's result as a JS boolean value.
pub(crate) fn relation_bool(
    op: Relation,
    left: AccountValue,
    right: AccountValue,
) -> SigningResult<AccountValue> {
    relation_value(op, left, right).map(AccountValue::Boolean)
}

enum Cursor {
    Array {
        items: Rc<RefCell<Vec<Option<Js>>>>,
        next: usize,
    },
    Chars {
        units: Str,
        next: usize,
    },
    Map {
        entries: Rc<RefCell<Vec<(Js, Js)>>>,
        visited: Vec<Js>,
    },
    Set {
        values: Rc<RefCell<Vec<Js>>>,
        visited: Vec<Js>,
    },
}

/// A built-in iterator (Array, String, Map, Set). Arrays re-read their length
/// per step; Map/Set skip entries deleted before being reached and see appended
/// ones. None carries `return`, so IteratorClose keeps the original completion.
pub(crate) struct JsIterator(RefCell<Option<Cursor>>);

fn not_iterable(value: &Js) -> Thrown {
    Thrown::type_error(&match value {
        Js::Undefined | Js::Null => format!(
            "{} is not iterable (cannot read property Symbol(Symbol.iterator))",
            value.to_display()
        ),
        Js::Function(_) => "function is not iterable".into(),
        Js::Object(_) | Js::Promise(_) => "object is not iterable".into(),
        other => format!("{} is not iterable", other.to_display()),
    })
}

fn unvisited<T: Clone>(items: &[T], key: impl Fn(&T) -> &Js, visited: &[Js]) -> Option<T> {
    items
        .iter()
        .find(|item| !visited.iter().any(|v| v.same_value_zero(key(item))))
        .cloned()
}

impl Cursor {
    fn next_value(&mut self) -> Option<Js> {
        match self {
            Cursor::Array { items, next } => {
                let value = items.borrow().get(*next)?.clone().unwrap_or(Js::Undefined);
                *next += 1;
                Some(value)
            }
            Cursor::Chars { units, next } => {
                let first = *units.get(*next)?;
                let pair = (0xD800..0xDC00).contains(&first)
                    && units
                        .get(*next + 1)
                        .is_some_and(|u| (0xDC00..0xE000).contains(u));
                let width = if pair { 2 } else { 1 };
                let out = Js::utf16(&units[*next..*next + width]);
                *next += width;
                Some(out)
            }
            Cursor::Map { entries, visited } => {
                let (key, value) = unvisited(&entries.borrow(), |(k, _)| k, visited)?;
                visited.push(key.clone());
                Some(Js::array(vec![key, value]))
            }
            Cursor::Set { values, visited } => {
                let value = unvisited(&values.borrow(), |v| v, visited)?;
                visited.push(value.clone());
                Some(value)
            }
        }
    }
}

impl JsIterator {
    /// GetIterator(value, sync) over the runtime's iterable intrinsics.
    pub(crate) fn open(value: &Js) -> Result<Self, Thrown> {
        let cursor = match value {
            Js::Array(items) => Cursor::Array {
                items: items.clone(),
                next: 0,
            },
            Js::String(units) => Cursor::Chars {
                units: units.clone(),
                next: 0,
            },
            Js::Map(entries) => Cursor::Map {
                entries: entries.clone(),
                visited: vec![],
            },
            Js::Set(values) => Cursor::Set {
                values: values.clone(),
                visited: vec![],
            },
            other => return Err(not_iterable(other)),
        };
        Ok(Self(RefCell::new(Some(cursor))))
    }
    /// IteratorStepValue: `None` once done, and stays done.
    pub(crate) fn step(&self) -> Option<Js> {
        let mut slot = self.0.borrow_mut();
        let value = slot.as_mut()?.next_value();
        if value.is_none() {
            *slot = None;
        }
        value
    }
    pub(crate) fn close(&self) {
        *self.0.borrow_mut() = None;
    }
}

/// Every value of an iterable, in order (array spread / argument spread).
pub(crate) fn spread(value: &Js) -> Result<Vec<Js>, Thrown> {
    let iterator = JsIterator::open(value)?;
    Ok(std::iter::from_fn(|| iterator.step()).collect())
}

pub(crate) fn spread_values(value: &AccountValue) -> SigningResult<Vec<AccountValue>> {
    Ok(spread(&to_js(value))
        .map_err(to_signing)?
        .iter()
        .map(to_account)
        .collect())
}

/// for-of: an abrupt body completion propagates with the iterator left as-is.
pub(crate) fn for_of(
    value: &AccountValue,
    mut body: impl FnMut(AccountValue) -> SigningResult<()>,
) -> SigningResult<()> {
    let iterator = JsIterator::open(&to_js(value)).map_err(to_signing)?;
    while let Some(item) = iterator.step() {
        body(to_account(&item))?;
    }
    Ok(())
}

/// An own property of the value's runtime representation, by UTF-16 key.
pub(crate) fn has_own_key(value: &Js, key: &[u16]) -> bool {
    match value {
        Js::Array(items) => {
            array_index(key).is_some_and(|i| matches!(items.borrow().get(i), Some(Some(_))))
                || key == utf16("length").as_ref()
        }
        Js::String(units) => {
            array_index(key).is_some_and(|i| i < units.len()) || key == utf16("length").as_ref()
        }
        Js::Object(o) => o.props.borrow().contains_key(key),
        Js::Function(f) => f.props.borrow().contains_key(key),
        _ => false,
    }
}

/// HasProperty: own key, prototype chain, or an intrinsic builtin member.
pub(crate) fn has_property(value: &Js, key: &[u16]) -> bool {
    if has_own_key(value, key) {
        return true;
    }
    match value {
        Js::Object(o) => o
            .proto
            .borrow()
            .as_ref()
            .is_some_and(|proto| has_property(proto, key)),
        Js::Map(_) | Js::Set(_) if key == utf16("size").as_ref() => true,
        Js::Array(_) | Js::Map(_) | Js::Set(_) | Js::Promise(_) => {
            builtins::member(value, key).is_some()
        }
        _ => false,
    }
}

/// A fresh ordinary object with CopyDataProperties from each source in order.
pub(crate) fn copy_data(sources: &[Js]) -> Result<Js, Thrown> {
    let out = Js::object(vec![]);
    for source in sources.iter().filter(|s| !s.is_nullish()) {
        for (key, value) in source.own_entries()? {
            out.define_key(&key, value)?;
        }
    }
    Ok(out)
}

/// `{ ...source, k1: v1, ... }`: CopyDataProperties then ordered own defines.
pub(crate) fn spread_with(source: &Js, fields: &[(&str, AccountValue)]) -> Result<Js, Thrown> {
    let out = copy_data(std::slice::from_ref(source))?;
    for (key, value) in fields {
        out.define(key, to_js(value))?;
    }
    Ok(out)
}

pub(crate) fn spread_with_value(
    source: &AccountValue,
    fields: &[(&str, AccountValue)],
) -> SigningResult<AccountValue> {
    spread_with(&to_js(source), fields)
        .map(|v| to_account(&v))
        .map_err(to_signing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> Js {
        Js::BigInt(Rc::new(text.parse().unwrap()))
    }

    #[test]
    fn operators_follow_ecmascript() {
        let valued = Js::object(vec![("valueOf", Js::function("valueOf", |_, _| Ok(Js::Number(5.0))))]);
        let sum = arithmetic(Arithmetic::Add, &valued, &Js::Number(1.0)).unwrap();
        assert!(sum.strict_equals(&Js::Number(6.0)));
        assert!(relation(Relation::Less, &big("9007199254740993"), &Js::Number(9007199254740994.0)).unwrap());
        assert!(!relation(Relation::Less, &big("9007199254740993"), &Js::Number(9007199254740992.0)).unwrap());
        assert!(relation(Relation::GreaterOrEqual, &big("16"), &Js::str("0x10")).unwrap());
        assert!(!relation(Relation::LessOrEqual, &Js::Number(f64::NAN), &Js::Number(1.0)).unwrap());
        assert!(arithmetic(Arithmetic::Remainder, &big("1"), &big("0")).is_err());
        assert!(arithmetic(Arithmetic::Subtract, &big("1"), &Js::Number(1.0)).is_err());
        let zero = negate(&Js::Number(0.0)).unwrap().to_number().unwrap();
        assert!(zero == 0.0 && zero.is_sign_negative());
    }

    #[test]
    fn string_iteration_yields_code_points_and_stays_done() {
        let iterator = JsIterator::open(&Js::str("a\u{1F600}")).unwrap();
        assert_eq!(iterator.step().unwrap().to_display(), "a");
        assert_eq!(iterator.step().unwrap().to_display(), "\u{1F600}");
        assert!(iterator.step().is_none() && iterator.step().is_none());
        assert!(JsIterator::open(&Js::object(vec![])).is_err());
    }
}
