use super::WorkerDataPost;
use js_sys::{Array, Object, Reflect};
use safe_cex_exchange::{
    js_runtime::value::Js,
    runtime::{ExchangeError, ExchangeErrorOrigin},
};
use std::rc::Rc;
use wasm_bindgen::JsValue;

pub type BrowserDataPost = Rc<dyn Fn(&JsValue) -> Result<(), ExchangeError>>;
#[derive(Debug)]
struct BrowserDataCause(JsValue);
impl std::fmt::Display for BrowserDataCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for BrowserDataCause {}
fn failure(value: JsValue) -> ExchangeError {
    let mut error = ExchangeError::new(format!("{value:?}"));
    error.origin = Some(ExchangeErrorOrigin::new(BrowserDataCause(value)));
    error
}
fn string(units: &[u16]) -> JsValue {
    js_sys::JsString::from_char_code(units).into()
}
fn property(object: &Object, key: &JsValue, value: &JsValue) -> Result<(), ExchangeError> {
    let descriptor = Object::new();
    Reflect::set(&descriptor, &JsValue::from_str("value"), value).map_err(failure)?;
    for key in ["enumerable", "writable", "configurable"] {
        Reflect::set(&descriptor, &JsValue::from_str(key), &JsValue::TRUE).map_err(failure)?;
    }
    if !Reflect::define_property(object, key, &descriptor).map_err(failure)? {
        return Err(ExchangeError::new("Worker DATA property definition failed"));
    }
    Ok(())
}
// Typed result graphs contain primitives, arrays and enumerable object data.
// Cache objects before walking children so shared references and cycles survive.
fn materialize(value: &Js, seen: &mut Vec<(Js, JsValue)>) -> Result<JsValue, ExchangeError> {
    match value {
        Js::Undefined => Ok(JsValue::UNDEFINED),
        Js::Null => Ok(JsValue::NULL),
        Js::Bool(value) => Ok(JsValue::from_bool(*value)),
        Js::Number(value) => Ok(JsValue::from_f64(*value)),
        Js::String(value) => Ok(string(value)),
        Js::BigInt(value) => js_sys::BigInt::new(&JsValue::from_str(&value.to_string()))
            .map(JsValue::from)
            .map_err(|e| failure(e.into())),
        Js::Array(values) => {
            if let Some((_, v)) = seen.iter().find(|(old, _)| old.strict_equals(value)) {
                return Ok(v.clone());
            }
            let values = values.borrow().clone();
            let array = Array::new_with_length(
                values
                    .len()
                    .try_into()
                    .map_err(|_| ExchangeError::new("Worker DATA array too large"))?,
            );
            let output: JsValue = array.clone().into();
            seen.push((value.clone(), output.clone()));
            for (index, child) in values.iter().enumerate() {
                if let Some(child) = child {
                    array.set(index as u32, materialize(child, seen)?);
                }
            }
            Ok(output)
        }
        Js::Object(_) => {
            if let Some((_, v)) = seen.iter().find(|(old, _)| old.strict_equals(value)) {
                return Ok(v.clone());
            }
            let object = if matches!(value, Js::Object(object) if object.class == "Error") {
                Object::from(js_sys::Error::new(""))
            } else {
                Object::new()
            };
            let output: JsValue = object.clone().into();
            seen.push((value.clone(), output.clone()));
            for (key, child) in value.own_entries().map_err(|e| e.to_exchange_error())? {
                property(&object, &string(&key), &materialize(&child, seen)?)?;
            }
            Ok(output)
        }
        Js::Map(values) => {
            if let Some((_, v)) = seen.iter().find(|(old, _)| old.strict_equals(value)) {
                return Ok(v.clone());
            }
            let entries: Vec<_> = values.borrow().iter().cloned().collect();
            let map = js_sys::Map::new();
            let output: JsValue = map.clone().into();
            seen.push((value.clone(), output.clone()));
            for (key, child) in entries {
                map.set(&materialize(&key, seen)?, &materialize(&child, seen)?);
            }
            Ok(output)
        }
        Js::Set(values) => {
            if let Some((_, v)) = seen.iter().find(|(old, _)| old.strict_equals(value)) {
                return Ok(v.clone());
            }
            let entries: Vec<_> = values.borrow().iter().cloned().collect();
            let set = js_sys::Set::new(&JsValue::UNDEFINED);
            let output: JsValue = set.clone().into();
            seen.push((value.clone(), output.clone()));
            for child in entries {
                set.add(&materialize(&child, seen)?);
            }
            Ok(output)
        }
        Js::SymbolIterator | Js::Builtin(_) | Js::Function(_) | Js::Promise(_) => {
            let mut error = ExchangeError::new("Unsupported runtime value at worker DATA host");
            error.name = "DataCloneError".into();
            Err(error)
        }
    }
}
pub(super) fn create(post: BrowserDataPost) -> WorkerDataPost {
    WorkerDataPost::new(Rc::new(move |payload| {
        post(&materialize(&payload.into_message()?, &mut Vec::new())?)
    }))
}
