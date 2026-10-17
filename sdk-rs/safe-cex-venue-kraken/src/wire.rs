//! Private wire shape: numeric events are f64 before untagged buffering.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use safe_cex_types::{object_map::JsObjectMap, retained_json::RetainedJsonInput};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Serialize)]
#[serde(untagged)]
enum Wire {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Wire>),
    Object(JsObjectMap<Wire>),
}
impl JsNumberOutput for Wire {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {
            Null, Bool(value), Number(value), Text(value), Array(value), Object(value)
        });
    }
}
impl Wire {
    fn retained(self) -> RetainedJsonInput {
        match self {
            Self::Null => RetainedJsonInput::Null,
            Self::Bool(v) => RetainedJsonInput::Bool(v),
            Self::Number(v) => RetainedJsonInput::Float(v),
            Self::Text(v) => RetainedJsonInput::Text(v.encode_utf16().collect()),
            Self::Array(v) => RetainedJsonInput::Array(v.into_iter().map(Self::retained).collect()),
            Self::Object(v) => RetainedJsonInput::Object(
                v.into_iter()
                    .map(|(k, v)| (k.encode_utf16().collect(), v.retained()))
                    .collect(),
            ),
        }
    }
}
pub(crate) fn decode(input: JsNumberInput) -> Result<Value, serde_json::Error> {
    input.decode::<Wire>()?.retained().into_value()
}
pub(crate) fn text(value: Option<&Value>) -> Result<String, safe_cex_num::JsCoercionError> {
    match value {
        None | Some(Value::Null) => Ok(String::new()),
        value => safe_cex_num::js_string(value),
    }
}
