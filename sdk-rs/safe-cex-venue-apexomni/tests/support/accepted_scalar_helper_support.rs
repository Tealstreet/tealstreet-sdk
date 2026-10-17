pub(crate) fn float(value: &serde_json::Value) -> f64 {
    value.as_str().unwrap().parse().unwrap()
}
pub(crate) fn native_bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
pub(crate) fn outcome(result: Result<(), String>) -> serde_json::Value {
    match result {
        Ok(()) => serde_json::json!({"ok":true}),
        Err(message) => serde_json::json!({"ok":false,"message":message}),
    }
}
pub(crate) fn prepared(value: &serde_json::Value) -> safe_cex_num::JsJsonValue {
    use safe_cex_num::JsJsonValue as J;
    match value["kind"].as_str().unwrap() {
        "undefined" => J::Undefined,
        "null" => J::Null,
        "boolean" => J::Bool(value["value"].as_bool().unwrap()),
        "number" => J::Number(float(&value["value"])),
        "string" => J::String(value["value"].as_str().unwrap().to_owned()),
        "bigint" => J::BigInt(value["value"].as_str().unwrap().parse().unwrap()),
        "array" => J::Array(
            value["value"]
                .as_array()
                .unwrap()
                .iter()
                .map(prepared)
                .collect(),
        ),
        "object" => J::Object(
            value["value"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().to_owned(), prepared(&pair[1])))
                .collect(),
        ),
        _ => panic!("unreviewed prepared fixture"),
    }
}
pub(crate) fn check(
    name: &str,
    count: usize,
    observe: impl Fn(&serde_json::Value) -> serde_json::Value,
) {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("accepted_scalar_helper_fixtures.json")).unwrap();
    assert_eq!(
        fixtures.as_object().unwrap().len(),
        23,
        "fixed reviewed owner census"
    );
    let cases = fixtures[name].as_array().unwrap();
    assert_eq!(
        cases.len(),
        count,
        "fixed independently authored case census"
    );
    let mut executed = 0;
    for case in cases {
        executed += 1;
        assert_eq!(
            observe(&case["input"]),
            case["output"],
            "{name}: independent native helper output input={}",
            case["input"]
        );
    }
    assert_eq!(executed, count, "actual native comparison count");
}
