use safe_cex_num::JsJsonValue as J;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::Value;

pub fn prepared(value: &Input) -> J {
    match value {
        Input::Undefined => J::Undefined,
        Input::Null => J::Null,
        Input::Bool(value) => J::Bool(*value),
        Input::Float(_) | Input::Lexeme(_) => J::Number(value.as_f64().unwrap()),
        Input::Text(units) => {
            J::String(String::from_utf16(units).expect("source observer UTF8 text"))
        }
        Input::Array(values) => J::Array(values.iter().map(prepared).collect()),
        Input::Object(values) => J::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        String::from_utf16(key).expect("source observer UTF8 key"),
                        prepared(value),
                    )
                })
                .collect(),
        ),
    }
}

pub fn observe(value: &(impl serde::Serialize + ?Sized), number: impl Fn(f64) -> Value) -> Value {
    fn walk(value: &Input, number: &impl Fn(f64) -> Value) -> Value {
        match value {
            Input::Undefined => serde_json::json!({"undefined":true}),
            Input::Null => Value::Null,
            Input::Bool(value) => Value::Bool(*value),
            Input::Float(_) | Input::Lexeme(_) => number(value.as_f64().unwrap()),
            Input::Text(units) => {
                Value::String(String::from_utf16(units).expect("source observer UTF8 text"))
            }
            Input::Array(values) => {
                Value::Array(values.iter().map(|value| walk(value, number)).collect())
            }
            Input::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        (
                            String::from_utf16(key).expect("source observer UTF8 key"),
                            walk(value, number),
                        )
                    })
                    .collect(),
            ),
        }
    }
    walk(
        &safe_cex_num::numeric_ingress::js_capture_output(value).unwrap(),
        &number,
    )
}
