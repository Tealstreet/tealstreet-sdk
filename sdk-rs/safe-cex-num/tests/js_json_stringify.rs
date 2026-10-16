#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{JsJsonValue, js_json_stringify};
use serde_json::{Value, json};
fn decode(v: &Value) -> JsJsonValue {
    match v[0].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "number" => JsJsonValue::Number(v[1].as_str().unwrap().parse().unwrap()),
        "string" => JsJsonValue::String(v[1].as_str().unwrap().into()),
        "bigint" => JsJsonValue::BigInt(v[1].as_str().unwrap().parse().unwrap()),
        "array" => JsJsonValue::Array(v[1].as_array().unwrap().iter().map(decode).collect()),
        "object" => JsJsonValue::Object(
            v[1].as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_str().unwrap().into(), decode(&v[1])))
                .collect(),
        ),
        "json" => JsJsonValue::from(serde_json::from_str::<Value>(v[1].as_str().unwrap()).unwrap()),
        _ => unreachable!(),
    }
}
#[test]
fn stringify_matches_live_node_bytes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_json_stringify.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 1914, "stringify", |row| {
        let actual = match js_json_stringify(&decode(&row["input"])) {
            Ok(text) => json!({"ok":text}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        assert_eq!(actual, row["expected"], "{row}");
    });
}
