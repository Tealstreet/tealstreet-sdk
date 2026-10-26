//! Deterministic lodash 4.18.1 bounds/coercion semantics with a caller-supplied draw.
//! Adapted from lodash; see LODASH-LICENSE and the pinned live package oracle.
use crate::{JsCoercionError, JsPrimitive};
use serde_json::Value;

#[derive(Clone, Copy)]
enum Arg<'a> {
    Primitive(&'a JsPrimitive),
    Json(Option<&'a Value>),
}
impl<'a> Arg<'a> {
    fn undefined(self) -> bool {
        matches!(
            self,
            Self::Primitive(JsPrimitive::Undefined) | Self::Json(None)
        )
    }
    fn boolean(self) -> Option<bool> {
        match self {
            Self::Primitive(JsPrimitive::Boolean(v)) | Self::Json(Some(Value::Bool(v))) => Some(*v),
            _ => None,
        }
    }
    fn number(self) -> Option<f64> {
        match self {
            Self::Primitive(JsPrimitive::Number(v)) => Some(*v),
            Self::Json(Some(Value::Number(v))) => Some(
                v.as_f64()
                    .unwrap_or_else(|| v.to_string().parse().unwrap_or(f64::NAN)),
            ),
            _ => None,
        }
    }
    fn string(self) -> Option<&'a str> {
        match self {
            Self::Primitive(JsPrimitive::String(v)) | Self::Json(Some(Value::String(v))) => Some(v),
            _ => None,
        }
    }
    fn truthy(self) -> bool {
        match self {
            Self::Json(value) => crate::js_truthy(value),
            Self::Primitive(value) => match value {
                JsPrimitive::Undefined | JsPrimitive::Null => false,
                JsPrimitive::Boolean(v) => *v,
                JsPrimitive::Number(v) => *v != 0.0 && !v.is_nan(),
                JsPrimitive::String(v) => !v.is_empty(),
                JsPrimitive::BigInt(v) => v.sign() != num_bigint::Sign::NoSign,
                JsPrimitive::Symbol => true,
            },
        }
    }
    fn finite(self) -> Result<f64, JsCoercionError> {
        let value = match self {
            Self::Primitive(JsPrimitive::Symbol) => 0.0,
            Self::Primitive(JsPrimitive::BigInt(v)) if v.sign() == num_bigint::Sign::NoSign => 0.0,
            Self::Primitive(v) => crate::js_to_number(v)?,
            Self::Json(v) => crate::js_number(v)?,
        };
        Ok(if value.is_infinite() {
            f64::MAX.copysign(value)
        } else if value.is_nan() {
            0.0
        } else {
            value
        })
    }
}

fn same_value_zero(left: Arg<'_>, right: Option<&Value>) -> bool {
    if left.undefined() {
        return right.is_none();
    }
    if let Some(a) = left.number() {
        return Arg::Json(right)
            .number()
            .is_some_and(|b| a == b || a.is_nan() && b.is_nan());
    }
    if let Some(a) = left.string() {
        return right.and_then(Value::as_str) == Some(a);
    }
    if let Some(a) = left.boolean() {
        return right.and_then(Value::as_bool) == Some(a);
    }
    match (left, right) {
        (Arg::Primitive(JsPrimitive::Null), Some(Value::Null)) => true,
        (Arg::Json(Some(Value::Null)), Some(Value::Null)) => true,
        (Arg::Json(Some(a @ (Value::Array(_) | Value::Object(_)))), Some(b)) => std::ptr::eq(a, b),
        _ => false,
    }
}

fn iteratee_call(lower: Arg<'_>, upper: Arg<'_>, floating: Arg<'_>) -> bool {
    let Arg::Json(Some(object @ (Value::Array(_) | Value::Object(_)))) = floating else {
        return false;
    };
    let property = |key: &str| match object {
        Value::Object(values) => values.get(key),
        Value::Array(values) => crate::js_array_index(key).and_then(|i| values.get(i as usize)),
        _ => None,
    };
    if let Some(index) = upper.number() {
        let length = match object {
            Value::Array(values) => Some(values.len() as f64),
            Value::Object(values) => Arg::Json(values.get("length")).number(),
            _ => None,
        };
        if length.is_some_and(|len| {
            (0.0..=9007199254740991.0).contains(&len)
                && len.fract() == 0.0
                && index >= 0.0
                && index.fract() == 0.0
                && index < len
        }) {
            return same_value_zero(lower, property(&crate::js_number_string(index)));
        }
    } else if let Some(key) = upper.string() {
        if let Value::Array(values) = object
            && key == "length"
        {
            return lower.number().is_some_and(|n| n == values.len() as f64);
        }
        if let Some(value) = property(key) {
            return same_value_zero(lower, Some(value));
        }
    }
    false
}

fn random_args<'a>(
    mut lower: Arg<'a>,
    mut upper: Arg<'a>,
    mut floating: Arg<'a>,
    source: impl FnOnce() -> f64,
) -> Result<f64, JsCoercionError> {
    if floating.truthy() && floating.boolean().is_none() && iteratee_call(lower, upper, floating) {
        upper = Arg::Json(None);
        floating = Arg::Json(None);
    }
    if floating.undefined() {
        if upper.boolean().is_some() {
            floating = upper;
            upper = Arg::Json(None);
        } else if lower.boolean().is_some() {
            floating = lower;
            lower = Arg::Json(None);
        }
    }
    let (mut lower, mut upper) = if lower.undefined() && upper.undefined() {
        (0.0, 1.0)
    } else {
        let low = lower.finite()?;
        if upper.undefined() {
            (0.0, low)
        } else {
            (low, upper.finite()?)
        }
    };
    if lower > upper {
        std::mem::swap(&mut lower, &mut upper);
    }
    let draw = source();
    Ok(
        if floating.truthy() || lower % 1.0 != 0.0 || upper % 1.0 != 0.0 {
            // Lodash intentionally derives epsilon from the JS decimal spelling LENGTH of the draw.
            let epsilon =
                crate::parse_float(&format!("1e-{}", crate::js_number_string(draw).len() - 1));
            crate::math_min(lower + (draw * (upper - lower + epsilon)), upper)
        } else {
            lower + (draw * (upper - lower + 1.0)).floor()
        },
    )
}

/// Lodash random with primitive arguments; Undefined models an omitted argument.
/// The injected source is called once after coercion, even for equal bounds; normally 0 <= draw < 1.
/// Integer/float selection, overloads and bound coercions are deterministic.
pub fn lodash_random(
    lower: &JsPrimitive,
    upper: &JsPrimitive,
    floating: &JsPrimitive,
    source: impl FnOnce() -> f64,
) -> Result<f64, JsCoercionError> {
    random_args(
        Arg::Primitive(lower),
        Arg::Primitive(upper),
        Arg::Primitive(floating),
        source,
    )
}

/// Lodash random over JSON values plus undefined, including JSON iteratee-call guards.
/// Callable host objects are outside this JSON adapter; object identity uses reference identity.
pub fn lodash_random_json(
    lower: Option<&Value>,
    upper: Option<&Value>,
    floating: Option<&Value>,
    source: impl FnOnce() -> f64,
) -> Result<f64, JsCoercionError> {
    random_args(
        Arg::Json(lower),
        Arg::Json(upper),
        Arg::Json(floating),
        source,
    )
}
