fn oracle_output() -> &'static [u8] {
    static OUTPUT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    OUTPUT.get_or_init(|| {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/response-oracle.cjs");
        let output = Command::new("node")
            .arg(script)
            .output()
            .expect("node required");
        assert!(
            output.status.success(),
            "TypeScript oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    })
}

#[test]
fn consuming_collection_decode_has_native_cardinality() {
    let expected: Value = serde_json::from_slice(oracle_output()).unwrap();
    cardinality::check(&expected, "response");
    let case = expected["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "info_distinct")
        .unwrap();
    let native: WeexExchangeInfoResponse =
        numeric_ingress::js_from_str(case["raw"].as_str().unwrap()).unwrap();
    let assets = native.assets.unwrap();
    let symbols = native.symbols.unwrap();
    assert_eq!(assets.len(), 1, "consuming::assets::native_length");
    assert_eq!(symbols.len(), 1, "consuming::symbols::native_length");
    let mut compared = 0;
    for record in assets.iter().chain(&symbols) {
        assert!(
            matches!(record, WeexExchangeInfoRecord::Object(_)),
            "consuming::record_kind"
        );
        compared += 1;
    }
    assert_eq!(compared, 2, "consuming::records::compared");
}

#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{self, JsNumberOutput};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_weex::response_types::{
    WeexExchangeInfoRecord, WeexExchangeInfoResponse, WeexKlineRow, WeexNumberOrString,
    WeexOrderResult,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn bits(value: f64) -> Value {
    json!({ "type": "number", "bits": format!("{:016x}", if value.is_nan() { 0x7ff8_0000_0000_0000 } else { value.to_bits() }) })
}
fn number_or_string(value: &WeexNumberOrString) -> Value {
    match value {
        WeexNumberOrString::Number(value) => bits(*value),
        WeexNumberOrString::String(value) => json!(value),
    }
}
fn kline(value: &WeexKlineRow) -> Value {
    // Native fields, not re-encoding the tuple through its Serialize implementation.
    json!({"kind": "array", "values": [
        number_or_string(&value.0),
        value.1,
        value.2,
        value.3,
        value.4,
        value.5,
        number_or_string(&value.6),
        value.7,
        number_or_string(&value.8),
        value.9,
        value.10
    ]})
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Observation {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<Observation>),
    Object(JsObjectMap<Observation>),
}
impl JsNumberOutput for Observation {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Null, Boolean(value), Number(value), String(value), Array(value), Object(value) });
    }
}
fn observe(value: &Observation) -> Value {
    match value {
        Observation::Null => Value::Null,
        Observation::Boolean(value) => json!(value),
        Observation::Number(value) => bits(*value),
        Observation::String(value) => json!(value),
        Observation::Array(values) => {
            json!({"kind":"array", "values": values.iter().map(observe).collect::<Vec<_>>() })
        }
        Observation::Object(values) => {
            json!({"kind":"object", "entries": values.iter().map(|(key, value)| json!([key, observe(value)])).collect::<Vec<_>>() })
        }
    }
}
fn info(value: &WeexExchangeInfoResponse) -> Value {
    let mut result = Vec::new();
    for (name, rows) in [("assets", &value.assets), ("symbols", &value.symbols)] {
        if let Some(rows) = rows {
            let rows: Vec<Value> = rows.iter().map(|row| match row {
                WeexExchangeInfoRecord::Object(row) => json!({"kind":"object", "entries": row.iter().map(|(key, input)| {
                    let native: Observation = input.clone().decode().expect("retained property");
                    json!([key, observe(&native)])
                }).collect::<Vec<_>>() }),
                WeexExchangeInfoRecord::Array(row) => json!({"kind":"array", "values": row.iter().map(|input| {
                    let native: Observation = input.clone().decode().expect("retained array element");
                    observe(&native)
                }).collect::<Vec<_>>() }),
            }).collect();
            result.push(json!([name, {"kind":"array", "values":rows}]));
        }
    }
    json!({"kind":"object", "entries":result})
}
fn order(value: &WeexOrderResult) -> Value {
    let mut result = Vec::new();
    // Explicit native field accesses defeat symmetric serde rename swaps.
    if let Some(v) = &value.order_id {
        result.push(json!(["orderId", v]));
    }
    if let Some(v) = &value.client_order_id {
        result.push(json!(["clientOrderId", v]));
    }
    if let Some(v) = value.success {
        result.push(json!(["success", v]));
    }
    if let Some(v) = &value.error_code {
        result.push(json!(["errorCode", v]));
    }
    if let Some(v) = &value.error_message {
        result.push(json!(["errorMessage", v]));
    }
    json!({"kind":"object", "entries":result})
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind")]
enum Tagged<T> {
    Row { data: T },
}
impl<T: JsNumberOutput> JsNumberOutput for Tagged<T> {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Row { data } });
    }
}
fn compare<T: DeserializeOwned + Serialize + JsNumberOutput>(
    case: &Value,
    capture: fn(&T) -> Value,
) -> usize {
    let raw = case["raw"].as_str().unwrap();
    let name = case["name"].as_str().unwrap();
    let accepted = case["accepted"].as_bool().unwrap() && !case["utf8Refusal"].as_bool().unwrap();
    let direct = serde_json::from_str::<T>(raw);
    let numeric = numeric_ingress::js_from_str::<T>(raw);
    let mut routes = vec![("direct_raw", direct), ("numeric_raw", numeric)];
    let tagged = format!("{{\"kind\":\"Row\",\"data\":{raw}}}");
    routes.push((
        "numeric_tagged",
        numeric_ingress::js_from_str::<Tagged<T>>(&tagged).map(|Tagged::Row { data }| data),
    ));
    if !case["utf8Refusal"].as_bool().unwrap() {
        let value: Value = serde_json::from_str(raw).unwrap();
        routes.push(("direct_value", serde_json::from_value::<T>(value.clone())));
        routes.push((
            "numeric_value",
            numeric_ingress::js_from_value::<T>(value.clone()),
        ));
        routes.push((
            "numeric_borrowed",
            numeric_ingress::js_from_value_ref::<T>(&value),
        ));
    }
    assert_eq!(
        routes.len(),
        if case["utf8Refusal"].as_bool().unwrap() {
            3
        } else {
            6
        },
        "cardinality::response::routes"
    );
    let mut compared = 0;
    for (route, decoded) in routes {
        if case["utf8Refusal"] == true {
            let decoded = decoded.expect("typed response keeps surrogate record");
            typed_utf16::assert_projection(raw, &decoded);
            compared += 1;
            continue;
        }
        assert_eq!(
            decoded.is_ok(),
            accepted,
            "shape::{name}::{route}: {:?}",
            decoded.as_ref().err()
        );
        if let Ok(decoded) = decoded {
            let observed = capture(&decoded);
            assert_eq!(observed, case["capture"], "native::{name}::{route}");
            let inventory: Value =
                serde_json::from_str(include_str!("cardinality-census.json")).unwrap();
            assert_eq!(
                cardinality::shape(&observed),
                inventory["response"]["captureShapes"][name],
                "cardinality::response::{name}::{route}::native_shape"
            );
        }
        compared += 1;
    }
    compared
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;

#[test]
fn response_types_match_actual_typescript() {
    let expected: Value = serde_json::from_slice(oracle_output()).unwrap();
    cardinality::check(&expected, "response");
    assert!(expected["header"]["arch"].is_string(), "capture_arch");
    assert!(
        expected["header"]["nodeMajor"].is_number(),
        "capture_node_major"
    );
    eprintln!("WEEX capture header: {}", expected["header"]);
    let optional = |name: &str, typ: Value| json!({"name": name, "optional": true, "type": {"union": [typ, "undefined"]}});
    // Schema comes from the live checker (including inherited properties).
    assert_eq!(
        expected["schema"]["WeexKlineRow"],
        json!({"tuple": [
            {"union":["number","string"]}, "string", "string", "string", "string", "string",
            {"union":["number","string"]}, "string", {"union":["number","string"]}, "string", "string"
        ]}),
        "schema::WeexKlineRow"
    );
    assert_eq!(
        expected["schema"]["WeexExchangeInfoResponse"],
        json!({"fields": [
            optional("assets", json!({"array":{"record":"any"}})),
            optional("symbols", json!({"array":{"record":"any"}}))
        ]}),
        "schema::WeexExchangeInfoResponse"
    );
    assert_eq!(
        expected["schema"]["WeexOrderResult"],
        json!({"fields": [
            optional("orderId", json!("string")), optional("clientOrderId", json!("string")),
            optional("success", json!("boolean")), optional("errorCode", json!("string")),
            optional("errorMessage", json!("string"))
        ]}),
        "schema::WeexOrderResult"
    );
    let depth_raw = expected["depth"]["raw"].as_str().unwrap();
    assert_eq!(expected["depth"]["sourceAccepted"], true);
    let error = numeric_ingress::js_from_str::<WeexExchangeInfoResponse>(depth_raw).unwrap_err();
    assert!(
        error.to_string().contains("recursion limit"),
        "chosen_depth_refusal: {error}"
    );
    let mut nested = json!(0);
    for _ in 0..150 {
        nested = json!([nested]);
    }
    let nested = json!({"assets":[{"nested":nested}]});
    for error in [
        serde_json::from_value::<WeexExchangeInfoResponse>(nested.clone()).unwrap_err(),
        numeric_ingress::js_from_value::<WeexExchangeInfoResponse>(nested.clone()).unwrap_err(),
        numeric_ingress::js_from_value_ref::<WeexExchangeInfoResponse>(&nested).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("recursion limit"),
            "chosen_depth_value_refusal: {error}"
        );
    }
    let order_depth_raw = expected["orderDepth"]["raw"].as_str().unwrap();
    assert_eq!(expected["orderDepth"]["sourceAccepted"], true);
    let order_nested = json!({"orderId":"ok", "extra": nested["assets"][0]["nested"].clone()});
    for error in [
        numeric_ingress::js_from_str::<WeexOrderResult>(order_depth_raw).unwrap_err(),
        serde_json::from_value::<WeexOrderResult>(order_nested.clone()).unwrap_err(),
        numeric_ingress::js_from_value::<WeexOrderResult>(order_nested.clone()).unwrap_err(),
        numeric_ingress::js_from_value_ref::<WeexOrderResult>(&order_nested).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("recursion limit"),
            "chosen_order_depth_refusal: {error}"
        );
    }
    let mut compared = 0;
    for case in expected["rows"].as_array().unwrap() {
        compared += match case["type"].as_str().unwrap() {
            "WeexKlineRow" => compare(case, kline),
            "WeexExchangeInfoResponse" => compare(case, info),
            "WeexOrderResult" => compare(case, order),
            other => panic!("unexpected source type: {other}"),
        };
    }
    assert_eq!(compared, 273, "cardinality::response::compared");
}
