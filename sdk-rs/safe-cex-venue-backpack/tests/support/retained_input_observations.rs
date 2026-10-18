use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::retained_json::RetainedJsonInput;

pub fn capture(raw: &str) -> RetainedJsonInput {
    js_from_str::<JsNumberInput>(raw)
        .unwrap()
        .decode::<RetainedJsonInput>()
        .unwrap()
}

pub fn string_units(value: &RetainedJsonInput) -> Vec<Vec<u16>> {
    match value {
        RetainedJsonInput::Text(units) => vec![units.clone()],
        RetainedJsonInput::Array(values) => values.iter().flat_map(string_units).collect(),
        RetainedJsonInput::Object(fields) => fields
            .iter()
            .flat_map(|(key, value)| std::iter::once(key.clone()).chain(string_units(value)))
            .collect(),
        _ => vec![],
    }
}
