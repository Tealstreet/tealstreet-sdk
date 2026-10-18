use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::retained_json::RetainedJsonInput;

pub fn capture(raw: &str) -> RetainedJsonInput {
    js_from_str::<JsNumberInput>(raw)
        .unwrap()
        .decode::<RetainedJsonInput>()
        .unwrap()
}

pub fn text_at(raw: &str, path: &[&str]) -> Vec<u16> {
    let retained = capture(raw);
    let value = path
        .iter()
        .fold(&retained, |value, key| value.get(key).unwrap());
    value.as_text().unwrap().to_vec()
}
