use safe_cex_venue_hyperliquid::{
    json_values::{JsonContainer, JsonObject, JsonValue},
    types::{AllMids, OrderStatus, UserRateLimit},
};
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
const RAW: &str = r#"{"2":"two","10":"ten","z":"zee","01":"leading","a":"aye","__proto__":"own"}"#;
const KEYS: [&str; 6] = ["2", "10", "z", "01", "a", "__proto__"];
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u16)
        .collect()
}
fn all_mids_native_order() {
    let value: AllMids = serde_json::from_str(RAW).unwrap();
    assert_eq!(value.get(&[50]).unwrap().0, [116, 119, 111]);
    assert_eq!(value.get(&[49, 48]).unwrap().0, [116, 101, 110]);
    assert_eq!(
        value.0.iter().map(|p| p.0.0.clone()).collect::<Vec<_>>(),
        KEYS.map(|s| s.encode_utf16().collect::<Vec<_>>()),
        "CORE-MAP native AllMids own key order"
    );
}
fn any_order(value: JsonContainer, name: &str) {
    let JsonContainer::Object(value) = value else {
        panic!("CORE-MAP native dictionary variant")
    };
    assert!(matches!(value.get(&[50]),Some(JsonValue::String(v))if v.0==[116,119,111]));
    assert!(matches!(value.get(&[49,48]),Some(JsonValue::String(v))if v.0==[116,101,110]));
    assert_eq!(
        value.0.iter().map(|p| p.0.0.clone()).collect::<Vec<_>>(),
        KEYS.map(|s| s.encode_utf16().collect::<Vec<_>>()),
        "CORE-MAP native {name} own key order"
    );
}
fn user_rate_limit_native_order() {
    any_order(
        serde_json::from_str::<UserRateLimit>(RAW).unwrap(),
        "UserRateLimit",
    )
}
fn order_status_native_order() {
    any_order(
        serde_json::from_str::<OrderStatus>(RAW).unwrap(),
        "OrderStatus",
    )
}
fn assert_object(actual: &JsonObject<JsonValue>, expected: &Value) {
    assert_eq!(expected[0], "object");
    let fields = expected[1].as_array().unwrap();
    assert_eq!(
        actual.0.len(),
        fields.len(),
        "CORE-MAP native independent object floor"
    );
    for ((key, value), expected) in actual.0.iter().zip(fields) {
        assert_eq!(
            key.0,
            units(&expected[0]),
            "CORE-MAP native ordered UTF16 key"
        );
        assert_value(value, &expected[1]);
    }
}
fn assert_array(actual: &[JsonValue], expected: &Value) {
    assert_eq!(expected[0], "array");
    let members = expected[1].as_array().unwrap();
    assert_eq!(
        actual.len(),
        members.len(),
        "CORE-MAP native independent array floor"
    );
    for (a, e) in actual.iter().zip(members) {
        assert_value(a, e)
    }
}
fn assert_value(actual: &JsonValue, expected: &Value) {
    match actual {
        JsonValue::Null => assert_eq!(expected[0], "null", "CORE-MAP native null variant"),
        JsonValue::Bool(v) => {
            assert_eq!(expected[0], "bool");
            assert_eq!(*v, expected[1].as_bool().unwrap(), "CORE-MAP native bool")
        }
        JsonValue::Number(v) => {
            assert_eq!(expected[0], "number");
            assert_eq!(
                format!("{:016x}", v.to_bits()),
                expected[1],
                "CORE-MAP native binary64 bits"
            )
        }
        JsonValue::String(v) => {
            assert_eq!(expected[0], "string");
            assert_eq!(v.0, units(&expected[1]), "CORE-MAP native UTF16 value")
        }
        JsonValue::Array(v) => assert_array(v, expected),
        JsonValue::Object(v) => assert_object(v, expected),
    }
}
fn assert_container(actual: &JsonContainer, expected: &Value) {
    match actual {
        JsonContainer::Object(v) => assert_object(v, expected),
        JsonContainer::Array(v) => assert_array(v, expected),
    }
}
fn project_value(actual: &JsonValue) -> Value {
    match actual {
        JsonValue::Null => json!(["null"]),
        JsonValue::Bool(v) => json!(["bool", v]),
        JsonValue::Number(v) => json!(["number", format!("{:016x}", v.to_bits())]),
        JsonValue::String(v) => json!(["string", v.0]),
        JsonValue::Array(v) => json!(["array", v.iter().map(project_value).collect::<Vec<_>>()]),
        JsonValue::Object(v) => project_object(v),
    }
}
fn project_object(v: &JsonObject<JsonValue>) -> Value {
    json!([
        "object",
        v.0.iter()
            .map(|(k, v)| json!([k.0, project_value(v)]))
            .collect::<Vec<_>>()
    ])
}
enum Model {
    Strings(AllMids),
    Any(JsonContainer),
}
impl Model {
    fn project(&self) -> Value {
        match self {
            Self::Strings(v) => json!([
                "object",
                v.0.iter()
                    .map(|(k, v)| json!([k.0, ["string", v.0]]))
                    .collect::<Vec<_>>()
            ]),
            Self::Any(JsonContainer::Object(v)) => project_object(v),
            Self::Any(JsonContainer::Array(v)) => {
                json!(["array", v.iter().map(project_value).collect::<Vec<_>>()])
            }
        }
    }
    fn wire(&self) -> String {
        match self {
            Self::Strings(v) => serde_json::to_string(v).unwrap(),
            Self::Any(v) => serde_json::to_string(v).unwrap(),
        }
    }
}
fn run(row: &Value) -> Option<Model> {
    let raw = row["raw"].as_str().unwrap();
    if row["model"] == "AllMids" {
        let actual = serde_json::from_str::<AllMids>(raw);
        if row["name"] == "last-invalid" {
            assert!(
                actual.is_err(),
                "CORE-MAP native invalid final typed duplicate"
            );
            return None;
        }
        let actual = actual.unwrap();
        let expected = row["expected"][1].as_array().unwrap();
        assert_eq!(
            actual.0.len(),
            expected.len(),
            "CORE-MAP native independent AllMids entry floor"
        );
        for ((key, value), expected) in actual.0.iter().zip(expected) {
            assert_eq!(
                key.0,
                units(&expected[0]),
                "CORE-MAP native AllMids ordered UTF16 key"
            );
            assert_eq!(expected[1][0], "string");
            assert_eq!(
                value.0,
                units(&expected[1][1]),
                "CORE-MAP native AllMids decoded UTF16 value"
            )
        }
        Some(Model::Strings(actual))
    } else {
        let actual = if row["model"] == "UserRateLimit" {
            serde_json::from_str::<UserRateLimit>(raw).unwrap()
        } else {
            assert_eq!(row["model"], "OrderStatus");
            serde_json::from_str::<OrderStatus>(raw).unwrap()
        };
        assert_container(&actual, &row["expected"]);
        Some(Model::Any(actual))
    }
}
#[test]
fn core_map_models_native_before_source() {
    all_mids_native_order();
    user_rate_limit_native_order();
    order_status_native_order();
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/core-map-model-inputs.json")).unwrap();
    assert_eq!(
        inputs.as_array().unwrap().len(),
        25,
        "CORE-MAP independent input floor"
    );
    let mut observed = vec![];
    let mut failed = vec![];
    for row in inputs.as_array().unwrap() {
        match catch_unwind(AssertUnwindSafe(|| run(row))) {
            Ok(v) => observed.push((row, v)),
            Err(_) => failed.push(format!("{}/{}", row["model"], row["name"])),
        }
    }
    assert!(failed.is_empty(), "CORE-MAP native failures {failed:?}");
    assert_eq!(observed.len(), 25, "CORE-MAP independently completed floor");
    let admitted = JsonValue::Array(vec![
        JsonValue::Number(-0.),
        JsonValue::Number(f64::NAN),
        JsonValue::Number(f64::INFINITY),
    ]);
    let JsonValue::Array(values) = &admitted else {
        unreachable!()
    };
    for (value, expected) in values.iter().zip([-0f64, f64::NAN, f64::INFINITY]) {
        let JsonValue::Number(value) = value else {
            panic!("CORE-MAP native admitted number variant")
        };
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "CORE-MAP native in-memory number bits"
        );
    }
    // Serialization begins only after every native DTO/value assertion above.
    let observations=observed.iter().map(|(row,model)|if let Some(model)=model{let wire=model.wire();let decoded:JsonValue=serde_json::from_str(&wire).unwrap();json!({"model":row["model"],"name":row["name"],"declared_valid":true,"native":model.project(),"wire":project_value(&decoded)})}else{json!({"model":row["model"],"name":row["name"],"declared_valid":false,"native":null,"wire":null})}).collect::<Vec<_>>();
    assert_eq!(
        serde_json::to_string(&admitted).unwrap(),
        "[0,null,null]",
        "CORE-MAP explicit numeric wire policy after native bits"
    );
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/core-map-model-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "CORE-MAP source {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(value["last_index"], 24, "CORE-MAP source stop");
    assert_eq!(
        value["semantic_cases"], 10,
        "CORE-MAP actual interface compiler floor"
    );
    assert_eq!(
        value["excluded_native_cases"], 7,
        "CORE-MAP source excluded native floor"
    );
    assert_eq!(value["admitted_numeric_wire"], "[0,null,null]");
    assert_eq!(
        value["cases"],
        json!(observations),
        "CORE-MAP wire comparison after every native assertion"
    );
}
