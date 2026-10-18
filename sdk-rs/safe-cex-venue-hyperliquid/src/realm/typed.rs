//! Adapt the family realm's owned DTO snapshots to the shared native typed codec.
//! JSON wire serialization remains separate; f64 NaN/infinities/-0 retain bits.
use super::{
    Realm,
    value::{PropKey, Value},
};
use safe_cex_exchange::js_runtime::value::{self as shared, Js};

impl Realm {
    pub(crate) fn capture_typed_value(&mut self, value: &Value, depth: usize) -> Result<Js, Value> {
        if depth >= 128 {
            return Err(self.type_error("typed value recursion limit exceeded"));
        }
        Ok(match value {
            Value::Undefined | Value::Symbol(_) => Js::Undefined,
            Value::Null => Js::Null,
            Value::Bool(value) => Js::Bool(*value),
            Value::Number(value) => Js::Number(*value),
            Value::String(value) => Js::String(value.clone()),
            Value::Object(_) if value.is_callable() => Js::Undefined,
            Value::Object(_) if value.promise().is_some() => Js::Null,
            Value::Object(_) if value.is_array() => {
                let length = self.get_str(value, "length")?;
                let length = self.to_number(&length)? as usize;
                let mut entries = Vec::with_capacity(length);
                for index in 0..length {
                    let item = self.index(value, &Value::Number(index as f64))?;
                    entries.push(self.capture_typed_value(&item, depth + 1)?);
                }
                Js::array(entries)
            }
            Value::Object(_) => {
                let object = Js::object(vec![]);
                for (key, value) in self.own_entries(value)? {
                    let Some(units) = key.units() else { continue };
                    let value = self.capture_typed_value(&value, depth + 1)?;
                    object
                        .define_key(units, value)
                        .map_err(|error| self.typed_error(error))?;
                }
                object
            }
        })
    }
    pub(crate) fn typed_error(&self, error: shared::Thrown) -> Value {
        let message = error
            .value
            .get("message")
            .and_then(|value| value.to_js_string())
            .map(|units| String::from_utf16_lossy(&units))
            .unwrap_or_else(|_| format!("{:?}", error.value));
        self.type_error(&format!("typed boundary: {message}"))
    }
    pub(crate) fn restore_typed_value(&self, value: &Js) -> Result<Value, Value> {
        Ok(match value {
            Js::Undefined => Value::Undefined,
            Js::Null => Value::Null,
            Js::Bool(value) => Value::Bool(*value),
            Js::Number(value) => Value::Number(*value),
            Js::String(value) => Value::String(value.clone()),
            Js::Array(_) => self.new_array(
                value.array_values().map_err(|error|self.typed_error(error))?.expect("array variant").iter().map(|value|self.restore_typed_value(value)).collect::<Result<_,_>>()?
            ),
            Js::Object(_) => {
                let result = self.new_object();
                for (key, value) in value
                    .own_entries()
                    .map_err(|error| self.typed_error(error))?
                {
                    result.object().unwrap().borrow_mut().put(
                        PropKey::String(key),
                        self.restore_typed_value(&value)?,
                        true,
                    );
                }
                result
            }
            _ => return Err(self.type_error("typed serializer returned an executable value")),
        })
    }
}
