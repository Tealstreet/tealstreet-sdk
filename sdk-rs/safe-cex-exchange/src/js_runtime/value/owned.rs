//! Owned structured data that can cross native runtime threads without JSON.
use crate::{
    js_runtime::value::{self, Js},
    runtime::ExchangeError,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{ops::Index, rc::Rc};

#[derive(Debug, Clone, PartialEq)]
pub enum OwnedJsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Vec<Self>),
    Object(Vec<(Vec<u16>, Self)>),
    BigInt { negative: bool, words: Vec<u64> },
}
impl OwnedJsValue {
    pub fn string(value: &str) -> Self {
        Self::String(value.encode_utf16().collect())
    }
    pub fn object(fields: impl IntoIterator<Item = (impl AsRef<str>, Self)>) -> Self {
        Self::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.as_ref().encode_utf16().collect(), value))
                .collect(),
        )
    }
    pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Self, ExchangeError> {
        Self::from_js(&value::from_typed(value).map_err(|e| e.to_exchange_error())?)
    }
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ExchangeError> {
        value::to_typed(&self.to_js()).map_err(|e| e.to_exchange_error())
    }
    pub fn from_js(value: &Js) -> Result<Self, ExchangeError> {
        Self::capture(value, &mut vec![])
    }
    fn capture(value: &Js, ancestors: &mut Vec<Js>) -> Result<Self, ExchangeError> {
        let compound = matches!(value, Js::Array(_) | Js::Object(_));
        if compound {
            if ancestors.len() >= 512 {
                return Err(ExchangeError::new(
                    "native account value nesting exceeds 512",
                ));
            }
            if ancestors
                .iter()
                .any(|ancestor| value.same_reference(ancestor))
            {
                return Err(ExchangeError::new("native account value contains a cycle"));
            }
            ancestors.push(value.clone());
        }
        let captured = match value {
            Js::Undefined => Self::Undefined,
            Js::Null => Self::Null,
            Js::Bool(value) => Self::Bool(*value),
            Js::Number(value) => Self::Number(*value),
            Js::String(value) => Self::String(value.to_vec()),
            Js::BigInt(value) => {
                let (sign, words) = value.to_u64_digits();
                Self::BigInt {
                    negative: sign == num_bigint::Sign::Minus,
                    words,
                }
            }
            Js::Array(_) => Self::Array(
                value
                    .array_values()
                    .map_err(|e| e.to_exchange_error())?
                    .expect("array variant has values")
                    .iter()
                    .map(|value| Self::capture(value, ancestors))
                    .collect::<Result<_, _>>()?,
            ),
            Js::Object(_) => Self::Object(
                value
                    .own_entries()
                    .map_err(|e| e.to_exchange_error())?
                    .into_iter()
                    .map(|(key, value)| {
                        Self::capture(&value, ancestors).map(|value| (key.to_vec(), value))
                    })
                    .collect::<Result<_, _>>()?,
            ),
            _ => {
                return Err(ExchangeError::new(
                    "native account values require structured data",
                ));
            }
        };
        if compound {
            ancestors.pop();
        }
        Ok(captured)
    }
    pub fn to_js(&self) -> Js {
        match self {
            Self::Undefined => Js::Undefined,
            Self::Null => Js::Null,
            Self::Bool(value) => Js::Bool(*value),
            Self::Number(value) => Js::Number(*value),
            Self::String(value) => Js::String(value.clone().into()),
            Self::Array(values) => Js::array(values.iter().map(Self::to_js).collect()),
            Self::Object(fields) => {
                let object = Js::object(vec![]);
                for (key, value) in fields {
                    object
                        .define_key(key, value.to_js())
                        .expect("fresh native data property");
                }
                object
            }
            Self::BigInt { negative, words } => Js::BigInt(Rc::new(num_bigint::BigInt::new(
                if *negative {
                    num_bigint::Sign::Minus
                } else {
                    num_bigint::Sign::Plus
                },
                words
                    .iter()
                    .flat_map(|word| [*word as u32, (*word >> 32) as u32])
                    .collect(),
            ))),
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Self::Number(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields
                .iter()
                .find(|(name, _)| name.iter().copied().eq(key.encode_utf16()))
                .map(|(_, value)| value),
            _ => None,
        }
    }
}
impl From<Value> for OwnedJsValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(value),
            Value::Number(value) => {
                Self::Number(value.to_string().parse().expect("JSON numeric lexeme"))
            }
            Value::String(value) => Self::string(&value),
            Value::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            Value::Object(fields) => Self::object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value))),
            ),
        }
    }
}
impl Index<usize> for OwnedJsValue {
    type Output = Self;
    fn index(&self, index: usize) -> &Self {
        if let Self::Array(values) = self {
            values.get(index).unwrap_or(&Self::Undefined)
        } else {
            &Self::Undefined
        }
    }
}
impl Index<&str> for OwnedJsValue {
    type Output = Self;
    fn index(&self, key: &str) -> &Self {
        self.get(key).unwrap_or(&Self::Undefined)
    }
}

#[derive(Clone)]
pub struct OwnedCallbackError {
    value: OwnedJsValue,
}
impl OwnedCallbackError {
    pub fn new(value: OwnedJsValue) -> Self {
        Self { value }
    }
    pub fn value(&self) -> &OwnedJsValue {
        &self.value
    }
}
impl std::fmt::Debug for OwnedCallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedCallbackError").finish_non_exhaustive()
    }
}
impl std::fmt::Display for OwnedCallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value.get("message") {
            Some(OwnedJsValue::String(message)) => f.write_str(&String::from_utf16_lossy(message)),
            _ => f.write_str("native callback rejected"),
        }
    }
}
impl std::error::Error for OwnedCallbackError {}
