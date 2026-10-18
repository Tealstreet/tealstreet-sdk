//! Live native record/array values used by the concrete Backpack adapter frames.
use crate::public_control::JsString;
/// Adapter frames use the shared runtime graph directly, including callback aliases.
pub use safe_cex_exchange::js_runtime::value::Js as Value;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use std::rc::Rc;

/// The adapter's explicit prepared-data/UTF-8 boundary over the shared graph.
/// These operations do not allocate a second live value representation.
pub trait NativeValue: Sized {
    fn field(&self, key: &str) -> Result<Self, ExchangeError>;
    fn array(&self) -> Result<Self, ExchangeError>;
    fn string(&self) -> Result<JsString, ExchangeError>;
    fn utf8(&self) -> Result<String, ExchangeError>;
    fn float(&self) -> Result<f64, ExchangeError>;
    fn number(&self) -> Result<f64, ExchangeError>;
    fn same(&self, right: &Self) -> bool;
    fn from_prepared(value: JsJsonValue) -> Self;
    fn prepared(&self) -> Result<JsJsonValue, ExchangeError>;
    fn spread(values: &[Self]) -> Result<Self, ExchangeError>;
}

impl NativeValue for Value {
    fn field(&self, key: &str) -> Result<Self, ExchangeError> {
        match self {
            Self::Undefined | Self::Null => Err(ExchangeError::new(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(self, Self::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ))),
            Self::Object(_) if self.has_own(key) => self.get(key).map_err(Into::into),
            _ => Ok(Self::Undefined),
        }
    }
    fn array(&self) -> Result<Self, ExchangeError> {
        if matches!(self, Self::Array(_)) {
            Ok(self.clone())
        } else {
            Err(ExchangeError::new("Value is not an array"))
        }
    }
    fn string(&self) -> Result<JsString, ExchangeError> {
        match self {
            Self::Array(_) => {
                let items = self.array_items()?.expect("array variant");
                let mut text = vec![];
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        text.push(44);
                    }
                    if let Some(item) = item
                        && !item.is_nullish()
                    {
                        text.extend(item.string()?);
                    }
                }
                Ok(text)
            }
            Self::Object(_) => {
                // The prepared object profile excludes executable coercion hooks.
                if !self.field("toString")?.is_undefined() {
                    return Err(ExchangeError::new(
                        "Cannot convert object to primitive value",
                    ));
                }
                Ok("[object Object]".encode_utf16().collect())
            }
            Self::Function(_) | Self::Promise(_) | Self::Map(_) | Self::Set(_) => Err(
                ExchangeError::new("Value is outside the prepared-data boundary"),
            ),
            _ => Ok(self.to_js_string()?.to_vec()),
        }
    }
    fn utf8(&self) -> Result<String, ExchangeError> {
        String::from_utf16(&self.string()?)
            .map_err(|e| ExchangeError::new(format!("Native UTF-8 boundary: {e}")))
    }
    fn float(&self) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(
            &self.string()?,
        )))
    }
    fn number(&self) -> Result<f64, ExchangeError> {
        Ok(match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Number(n) => *n,
            _ => safe_cex_num::number_from_string(&String::from_utf16_lossy(&self.string()?)),
        })
    }
    fn same(&self, right: &Self) -> bool {
        self.strict_equals(right)
    }
    fn from_prepared(v: JsJsonValue) -> Self {
        match v {
            JsJsonValue::Undefined => Self::Undefined,
            JsJsonValue::Null => Self::Null,
            JsJsonValue::Bool(b) => Self::Bool(b),
            JsJsonValue::Number(n) => Self::Number(n),
            JsJsonValue::String(s) => Self::String(s.encode_utf16().collect()),
            JsJsonValue::Array(a) => Self::array(a.into_iter().map(Self::from_prepared).collect()),
            JsJsonValue::Object(o) => {
                let value = Self::object(vec![]);
                for (key, item) in o {
                    value
                        .define(&key, Self::from_prepared(item))
                        .expect("fresh object");
                }
                value
            }
            JsJsonValue::BigInt(v) => Self::BigInt(Rc::new(v)),
        }
    }
    fn prepared(&self) -> Result<JsJsonValue, ExchangeError> {
        Ok(match self {
            Self::Undefined => JsJsonValue::Undefined,
            Self::Null => JsJsonValue::Null,
            Self::Bool(b) => JsJsonValue::Bool(*b),
            Self::Number(n) => JsJsonValue::Number(*n),
            Self::BigInt(n) => JsJsonValue::BigInt((**n).clone()),
            Self::String(_) => JsJsonValue::String(self.utf8()?),
            Self::Array(_) => JsJsonValue::Array(
                self.array_items()?
                    .expect("array variant")
                    .iter()
                    .map(|v| v.as_ref().unwrap_or(&Self::Undefined).prepared())
                    .collect::<Result<_, _>>()?,
            ),
            Self::Object(_) => JsJsonValue::Object(
                self.own_entries()?
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            String::from_utf16(k).map_err(|e| ExchangeError::new(e.to_string()))?,
                            v.prepared()?,
                        ))
                    })
                    .collect::<Result<_, ExchangeError>>()?,
            ),
            _ => {
                return Err(ExchangeError::new(
                    "Value is outside the prepared-data boundary",
                ));
            }
        })
    }
    fn spread(values: &[Self]) -> Result<Self, ExchangeError> {
        let out = Self::object(vec![]);
        for value in values {
            if matches!(value, Self::Object(_)) {
                for (key, item) in value.own_entries()? {
                    out.define_key(&key, item)?;
                }
            }
        }
        Ok(out)
    }
}
