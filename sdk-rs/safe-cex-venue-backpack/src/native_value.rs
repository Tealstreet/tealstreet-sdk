//! Live native record/array values used by the concrete Backpack adapter frames.
use crate::public_control::JsString;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    BigInt(String),
    String(JsString),
    Array(Rc<RefCell<Vec<Option<Value>>>>),
    Object(Rc<RefCell<Vec<(JsString, Value)>>>),
}
impl Value {
    pub fn field(&self, key: &str) -> Result<Self, ExchangeError> {
        match self {
            Self::Undefined | Self::Null => Err(ExchangeError::new(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(self, Self::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ))),
            Self::Object(o) => Ok(o
                .borrow()
                .iter()
                .rev()
                .find(|(k, _)| k == &key.encode_utf16().collect::<Vec<_>>())
                .map_or(Self::Undefined, |(_, v)| v.clone())),
            _ => Ok(Self::Undefined),
        }
    }
    pub fn array(&self) -> Result<Rc<RefCell<Vec<Option<Self>>>>, ExchangeError> {
        if let Self::Array(v) = self {
            Ok(v.clone())
        } else {
            Err(ExchangeError::new("Value is not an array"))
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(v) => *v,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::String(s) => !s.is_empty(),
            Self::BigInt(s) => s != "0",
            _ => true,
        }
    }
    pub fn string(&self) -> Result<JsString, ExchangeError> {
        Ok(match self {
            Self::Undefined => "undefined".encode_utf16().collect(),
            Self::Null => "null".encode_utf16().collect(),
            Self::Bool(v) => if *v { "true" } else { "false" }.encode_utf16().collect(),
            Self::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
            Self::String(s) => s.clone(),
            Self::BigInt(s) => s.encode_utf16().collect(),
            Self::Array(a) => {
                let values = a.borrow().clone();
                let mut text = vec![];
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        text.push(44);
                    }
                    if let Some(v) = v {
                        if !matches!(v, Self::Undefined | Self::Null) {
                            text.extend(v.string()?);
                        }
                    }
                }
                text
            }
            Self::Object(_) => {
                if !matches!(self.field("toString")?, Self::Undefined) {
                    return Err(ExchangeError::new(
                        "Cannot convert object to primitive value",
                    ));
                }
                "[object Object]".encode_utf16().collect()
            }
        })
    }
    pub fn utf8(&self) -> Result<String, ExchangeError> {
        String::from_utf16(&self.string()?)
            .map_err(|e| ExchangeError::new(format!("Native UTF-8 boundary: {e}")))
    }
    pub fn parse_float(&self) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(
            &self.string()?,
        )))
    }
    pub fn number(&self) -> Result<f64, ExchangeError> {
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
    pub fn same(&self, right: &Self) -> bool {
        match (self, right) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    pub fn from_prepared(v: JsJsonValue) -> Self {
        match v {
            JsJsonValue::Undefined => Self::Undefined,
            JsJsonValue::Null => Self::Null,
            JsJsonValue::Bool(b) => Self::Bool(b),
            JsJsonValue::Number(n) => Self::Number(n),
            JsJsonValue::String(s) => Self::String(s.encode_utf16().collect()),
            JsJsonValue::Array(a) => Self::Array(Rc::new(RefCell::new(
                a.into_iter()
                    .map(|v| Some(Self::from_prepared(v)))
                    .collect(),
            ))),
            JsJsonValue::Object(o) => Self::Object(Rc::new(RefCell::new(
                o.into_iter()
                    .map(|(k, v)| (k.encode_utf16().collect(), Self::from_prepared(v)))
                    .collect(),
            ))),
            JsJsonValue::BigInt(v) => Self::BigInt(v.to_string()),
        }
    }
    pub fn prepared(&self) -> Result<JsJsonValue, ExchangeError> {
        Ok(match self {
            Self::Undefined => JsJsonValue::Undefined,
            Self::Null => JsJsonValue::Null,
            Self::Bool(b) => JsJsonValue::Bool(*b),
            Self::Number(n) => JsJsonValue::Number(*n),
            Self::BigInt(n) => JsJsonValue::BigInt(
                n.parse()
                    .map_err(|e| ExchangeError::new(format!("Invalid BigInt: {e}")))?,
            ),
            Self::String(_) => JsJsonValue::String(self.utf8()?),
            Self::Array(a) => JsJsonValue::Array(
                a.borrow()
                    .iter()
                    .map(|v| v.as_ref().unwrap_or(&Self::Undefined).prepared())
                    .collect::<Result<_, _>>()?,
            ),
            Self::Object(o) => JsJsonValue::Object(
                o.borrow()
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            String::from_utf16(k).map_err(|e| ExchangeError::new(e.to_string()))?,
                            v.prepared()?,
                        ))
                    })
                    .collect::<Result<_, ExchangeError>>()?,
            ),
        })
    }
    pub fn object(fields: Vec<(&str, Self)>) -> Self {
        Self::Object(Rc::new(RefCell::new(
            fields
                .into_iter()
                .map(|(k, v)| (k.encode_utf16().collect(), v))
                .collect(),
        )))
    }
    pub fn spread(values: &[Self]) -> Result<Self, ExchangeError> {
        let out = Self::object(vec![]);
        let Self::Object(target) = &out else {
            unreachable!()
        };
        for v in values {
            if let Self::Object(o) = v {
                for (k, v) in &*o.borrow() {
                    let mut t = target.borrow_mut();
                    if let Some((_, old)) = t.iter_mut().find(|(key, _)| key == k) {
                        *old = v.clone();
                    } else {
                        t.push((k.clone(), v.clone()));
                    }
                }
            }
        }
        Ok(out)
    }
}
