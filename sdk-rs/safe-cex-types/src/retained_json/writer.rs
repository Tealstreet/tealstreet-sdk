use super::RetainedJsonInput;
use crate::js_coercion::{JsCoercionError, js_number_string};
use std::collections::HashMap;

impl RetainedJsonInput {
    pub fn json_stringify(&self) -> Result<Option<String>, JsCoercionError> {
        if matches!(self, Self::Undefined) {
            return Ok(None);
        }
        let mut output = String::new();
        write_retained(self, &mut output)?;
        Ok(Some(output))
    }
}
fn quoted_utf16(value: &[u16], output: &mut String) {
    let mut text = Vec::new();
    crate::js_coercion::json_quote_utf16(value, &mut text);
    output.push_str(&String::from_utf16(&text).expect("well-formed JSON quotation"));
}
fn number(value: f64, output: &mut String) {
    if value.is_finite() {
        output.push_str(&js_number_string(value));
    } else {
        output.push_str("null");
    }
}
fn write_retained(
    value: &crate::retained_json::RetainedJsonInput,
    output: &mut String,
) -> Result<(), JsCoercionError> {
    use crate::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Undefined | Input::Null => output.push_str("null"),
        Input::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Input::Float(value) => number(*value, output),
        Input::Lexeme(value) => number(
            value.parse().map_err(|_| JsCoercionError {
                name: "TypeError",
                message: "Invalid retained numeric lexeme",
            })?,
            output,
        ),
        Input::Text(value) => quoted_utf16(value, output),
        Input::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_retained(value, output)?;
            }
            output.push(']');
        }
        Input::Object(entries) => {
            let mut keys = Vec::new();
            let mut values = HashMap::new();
            for (key, value) in entries {
                if values.insert(key.as_slice(), value).is_none() {
                    keys.push(key.as_slice());
                }
            }
            output.push('{');
            let mut first = true;
            for key in crate::js_coercion::js_own_keys_utf16(keys) {
                let value = values[key];
                if matches!(value, Input::Undefined) {
                    continue;
                }
                if !first {
                    output.push(',');
                }
                first = false;
                quoted_utf16(key, output);
                output.push(':');
                write_retained(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}
