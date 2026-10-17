//! ECMAScript semantics the host families share: operators delegate to
//! `js_runtime::ops`; iteration uses the shared iterator protocol. String-hint
//! ToString, HasProperty and CopyDataProperties remain local operations.
use super::value::{to_account, to_js, to_signing};
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};

use safe_cex_exchange::js_runtime::{
    builtins, ops as runtime,
    value::{Js, Str, Thrown, array_index, utf16},
};
use std::cell::RefCell;

/// ToString: ToPrimitive with the string hint (toString before valueOf), which
/// `js_runtime::ops::to_primitive` (default/number hint only) does not cover.
pub(crate) fn to_string(value: &Js) -> Result<Str, Thrown> {
    if !value.is_object() {
        return value.to_js_string();
    }
    let mut callable = false;
    for name in ["toString", "valueOf"] {
        let method = value.get(name)?;
        if method.is_callable() {
            callable = true;
            let result = method.call(value.clone(), vec![])?;
            if !result.is_object() {
                return result.to_js_string();
            }
        }
    }
    if callable {
        return Err(Thrown::type_error(
            "Cannot convert object to primitive value",
        ));
    }
    value.to_js_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// The binary arithmetic operators: ToPrimitive left then right, then the
/// primitive operation (string concatenation for `+`).
pub(crate) fn arithmetic(op: Arithmetic, left: &Js, right: &Js) -> Result<Js, Thrown> {
    let left = runtime::to_primitive(left)?;
    let right = runtime::to_primitive(right)?;
    match op {
        Arithmetic::Add => runtime::add(&left, &right),
        Arithmetic::Subtract => runtime::subtract(&left, &right),
        Arithmetic::Multiply => runtime::multiply(&left, &right),
        Arithmetic::Divide => runtime::divide(&left, &right),
        Arithmetic::Remainder => runtime::remainder(&left, &right),
    }
}

/// Unary `-`; Number negation keeps -0.
pub(crate) fn negate(value: &Js) -> Result<Js, Thrown> {
    runtime::negate(&runtime::to_primitive(value)?)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Relation {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// The relational operators: ToPrimitive(number) left then right, then IsLessThan.
pub(crate) fn relation(op: Relation, left: &Js, right: &Js) -> Result<bool, Thrown> {
    let left = runtime::to_primitive(left)?;
    let right = runtime::to_primitive(right)?;
    match op {
        Relation::Less => runtime::less_than(&left, &right),
        Relation::LessOrEqual => runtime::less_equal(&left, &right),
        Relation::Greater => runtime::greater_than(&left, &right),
        Relation::GreaterOrEqual => runtime::greater_equal(&left, &right),
    }
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

/// A retained record over the shared intrinsic iterator protocol.
pub(crate) struct JsIterator(RefCell<Option<(Js, Js)>>);

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

impl JsIterator {
    pub(crate) fn open(value: &Js) -> Result<Self, Thrown> {
        if value.is_nullish() {
            return Err(not_iterable(value));
        }
        let method = value.get_property(&Js::SymbolIterator)?;
        if method.is_nullish() {
            return Err(not_iterable(value));
        }
        let iterator = method.call(value.clone(), vec![])?;
        if !iterator.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        let next = iterator.get("next")?;
        Ok(Self(RefCell::new(Some((iterator, next)))))
    }
    pub(crate) fn step(&self) -> Result<Option<Js>, Thrown> {
        let Some((iterator, next)) = self.0.borrow().clone() else {
            return Ok(None);
        };
        let result = (|| {
            let step = next.call(iterator, vec![])?;
            if !step.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
            if step.get("done")?.truthy() {
                Ok(None)
            } else {
                step.get("value").map(Some)
            }
        })();
        if !matches!(&result, Ok(Some(_))) {
            self.0.borrow_mut().take();
        }
        result
    }
    pub(crate) fn close(&self) -> Result<(), Thrown> {
        let Some((iterator, _)) = self.0.borrow_mut().take() else {
            return Ok(());
        };
        let method = iterator.get("return")?;
        if !method.is_nullish() {
            let result = method.call(iterator, vec![])?;
            if !result.is_object() {
                return Err(Thrown::type_error(
                    "Iterator return result is not an object",
                ));
            }
        }
        Ok(())
    }
}

/// Every value of an iterable, in order (array spread / argument spread).
pub(crate) fn spread(value: &Js) -> Result<Vec<Js>, Thrown> {
    let iterator = JsIterator::open(value)?;
    let mut values = vec![];
    while let Some(value) = iterator.step()? {
        values.push(value);
    }
    Ok(values)
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
    while let Some(item) = iterator.step().map_err(to_signing)? {
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
    use std::rc::Rc;

    fn big(text: &str) -> Js {
        Js::BigInt(Rc::new(text.parse().unwrap()))
    }

    #[test]
    fn operators_follow_ecmascript() {
        let valued = Js::object(vec![(
            "valueOf",
            Js::function("valueOf", |_, _| Ok(Js::Number(5.0))),
        )]);
        let sum = arithmetic(Arithmetic::Add, &valued, &Js::Number(1.0)).unwrap();
        assert!(sum.strict_equals(&Js::Number(6.0)));
        assert!(
            relation(
                Relation::Less,
                &big("9007199254740993"),
                &Js::Number(9007199254740994.0)
            )
            .unwrap()
        );
        assert!(
            !relation(
                Relation::Less,
                &big("9007199254740993"),
                &Js::Number(9007199254740992.0)
            )
            .unwrap()
        );
        assert!(relation(Relation::GreaterOrEqual, &big("16"), &Js::str("0x10")).unwrap());
        assert!(
            !relation(
                Relation::LessOrEqual,
                &Js::Number(f64::NAN),
                &Js::Number(1.0)
            )
            .unwrap()
        );
        assert!(arithmetic(Arithmetic::Remainder, &big("1"), &big("0")).is_err());
        assert!(arithmetic(Arithmetic::Subtract, &big("1"), &Js::Number(1.0)).is_err());
        let zero = negate(&Js::Number(0.0)).unwrap().to_number().unwrap();
        assert!(zero == 0.0 && zero.is_sign_negative());
    }

    #[test]
    fn map_values_use_the_live_shared_iterator() {
        let map = Js::new_map();
        let set = map.get("set").unwrap();
        set.call(map.clone(), vec![Js::str("a"), Js::Number(1.0)])
            .unwrap();
        set.call(map.clone(), vec![Js::str("b"), Js::Number(2.0)])
            .unwrap();
        let values = map
            .get("values")
            .unwrap()
            .call(map.clone(), vec![])
            .unwrap();
        let iterator = JsIterator::open(&values).unwrap();
        assert!(
            iterator
                .step()
                .unwrap()
                .unwrap()
                .strict_equals(&Js::Number(1.0))
        );
        map.get("delete")
            .unwrap()
            .call(map.clone(), vec![Js::str("b")])
            .unwrap();
        set.call(map.clone(), vec![Js::str("c"), Js::Number(-0.0)])
            .unwrap();
        let Js::Number(number) = iterator.step().unwrap().unwrap() else {
            panic!("native number")
        };
        assert_eq!(number.to_bits(), (-0.0_f64).to_bits());
        assert!(iterator.step().unwrap().is_none());
        set.call(map.clone(), vec![Js::str("d"), Js::Number(4.0)])
            .unwrap();
        assert!(iterator.step().unwrap().is_none());
    }

    #[test]
    fn iterator_next_keeps_the_original_thrown_value() {
        let thrown = Js::object(vec![("marker", Js::Number(7.0))]);
        let captured = thrown.clone();
        let iterator = Js::object(vec![(
            "next",
            Js::function("next", move |_, _| Err(Thrown::new(captured.clone()))),
        )]);
        // Retain the iterator and next function exactly as open() does.
        let record = JsIterator(RefCell::new(Some((
            iterator.clone(),
            iterator.get("next").unwrap(),
        ))));
        assert!(record.step().unwrap_err().value.strict_equals(&thrown));
        assert!(record.step().unwrap().is_none());
    }

    #[test]
    fn string_iteration_yields_code_points_and_stays_done() {
        let iterator = JsIterator::open(&Js::str("a\u{1F600}")).unwrap();
        assert_eq!(iterator.step().unwrap().unwrap().to_display(), "a");
        assert_eq!(iterator.step().unwrap().unwrap().to_display(), "\u{1F600}");
        assert!(iterator.step().unwrap().is_none() && iterator.step().unwrap().is_none());
        assert!(JsIterator::open(&Js::object(vec![])).is_err());
    }
}
