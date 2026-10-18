use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};

pub fn observe(value: &(impl serde::Serialize + ?Sized)) -> Value {
    fn walk(value: &Input) -> Value {
        match value {
            Input::Undefined => json!({"$undefined":true}),
            Input::Null => Value::Null,
            Input::Bool(value) => json!(value),
            Input::Float(_) | Input::Lexeme(_) => {
                let value = value.as_f64().unwrap();
                json!({"number":format!("{:016x}",if value.is_nan(){0x7ff8000000000000}else{value.to_bits()})})
            }
            Input::Text(units) => {
                json!(String::from_utf16(units).expect("source observer UTF8 text"))
            }
            Input::Array(values) => Value::Array(values.iter().map(walk).collect()),
            Input::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| {
                        (
                            String::from_utf16(key).expect("source observer UTF8 key"),
                            walk(value),
                        )
                    })
                    .collect(),
            ),
        }
    }
    walk(&safe_cex_num::numeric_ingress::js_capture_output(value).unwrap())
}
