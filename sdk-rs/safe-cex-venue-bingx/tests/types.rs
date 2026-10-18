use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_types::retained_json::RetainedJsonInput;
use safe_cex_venue_bingx::types::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

#[path = "support/native_fields.rs"]
mod native_fields;

trait Observe {
    fn observe(&self) -> Value;
}
impl Observe for f64 {
    fn observe(&self) -> Value {
        let bits = if self.is_nan() {
            0x7ff8000000000000
        } else {
            self.to_bits()
        };
        json!({"number": format!("{bits:016x}")})
    }
}
impl Observe for String {
    fn observe(&self) -> Value {
        json!(self)
    }
}
impl Observe for bool {
    fn observe(&self) -> Value {
        json!(self)
    }
}
impl<T: Observe> Observe for Vec<T> {
    fn observe(&self) -> Value {
        self.iter().map(Observe::observe).collect()
    }
}
impl<T: Observe> Observe for Option<T> {
    fn observe(&self) -> Value {
        self.as_ref().map_or(Value::Null, Observe::observe)
    }
}
fn oracle() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/types_oracle.cjs"
            ))
            .output()
            .expect("execute actual TypeScript declarations");
        assert!(
            output.status.success(),
            "types oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("types oracle JSON")
    })
}
fn assert_native_cardinality(native: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Array(rows) => {
            let actual = native.as_array().expect("native collection representation");
            assert_eq!(
                actual.len(),
                rows.len(),
                "BINGX-CARDINALITY native array length {path}"
            );
            for (index, row) in rows.iter().enumerate() {
                assert_native_cardinality(&actual[index], row, &format!("{path}/{index}"));
            }
        }
        Value::Object(fields) => {
            for (key, value) in fields {
                assert_native_cardinality(&native[key], value, &format!("{path}/{key}"));
            }
        }
        _ => {}
    }
}

fn verify<T: DeserializeOwned + Serialize + JsNumberOutput + Observe>(case: &Value) -> usize {
    let raw = case["raw"].as_str().unwrap();
    let name = case["name"].as_str().unwrap();
    let label = case["label"].as_str().unwrap();
    if case["boundary"] == "utf8" {
        assert_eq!(
            case["valid"], true,
            "TypeScript accepts lone surrogate {name}/{label}"
        );
        assert!(
            case["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| (0xd800..=0xdfff).contains(&u.as_u64().unwrap())),
            "utf16 native observation {name}/{label}"
        );
        assert!(
            case["wire"].as_str().unwrap().contains("\\ud"),
            "TypeScript retains surrogate {name}/{label}"
        );
        let (path, _) = label
            .strip_prefix("utf8_boundary:")
            .unwrap()
            .split_once(':')
            .unwrap();
        let units: Vec<u16> = case["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
            .collect();
        for (route, retained) in [
            (
                "canonical",
                RetainedJsonInput::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap(),
            ),
            (
                "raw_capture",
                serde_json::from_str::<RetainedJsonInput>(raw).unwrap(),
            ),
        ] {
            let mut selected = &retained;
            for key in path.split('/') {
                selected = if let Some(rows) = selected.as_array() {
                    &rows[key.parse::<usize>().unwrap()]
                } else {
                    selected.get(key).unwrap()
                };
            }
            assert_eq!(
                selected.as_text().unwrap(),
                units,
                "retained UTF-16 units {name}/{label}/{route}"
            );
            assert_eq!(
                retained.json_stringify().unwrap().unwrap(),
                case["wire"].as_str().unwrap(),
                "retained wire identity {name}/{label}/{route}"
            );
        }
        // The source changes exactly one String in its distinct-field fixture.
        // Keep every other native member observation, including numeric bits.
        let mut expected = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|baseline| baseline["name"] == name && baseline["label"] == "distinct_fields")
            .unwrap()["native"]
            .clone();
        let typed = String::from_utf16_lossy(&units);
        assert!(
            typed.contains('\u{fffd}'),
            "typed replacement {name}/{label}"
        );
        *expected.pointer_mut(&format!("/{path}")).unwrap() = json!(typed);
        for (route, decoded) in [
            (
                "shared_raw",
                js_from_str::<T>(raw).map_err(|error| error.to_string()),
            ),
            (
                "bare_raw",
                serde_json::from_str::<T>(raw).map_err(|error| error.to_string()),
            ),
        ] {
            let decoded = decoded.unwrap_or_else(|error| {
                panic!("typed UTF-8 decode {name}/{label}/{route}: {error}")
            });
            let native = decoded.observe();
            assert_native_cardinality(&native, &expected, &format!("{name}/{label}/{route}"));
            assert_eq!(
                native, expected,
                "typed UTF-8 native fields {name}/{label}/{route}"
            );
        }
        return 2;
    }
    if case["valid"] == true
        && matches!(name, "FETCH_BALANCE_RESPONSE" | "FETCH_POSITIONS_RESPONSE")
    {
        assert_eq!(
            case["native"]["data"].as_array().unwrap().len(),
            2,
            "BINGX-CARDINALITY source per-case rows {name}/{label}"
        );
    }
    let retained: Value = serde_json::from_str(raw).unwrap();
    let routes = [
        (
            "shared_raw",
            js_from_str::<T>(raw).map_err(|e| e.to_string()),
        ),
        (
            "shared_owned",
            js_from_value::<T>(retained.clone()).map_err(|e| e.to_string()),
        ),
        (
            "shared_borrowed",
            js_from_value_ref::<T>(&retained).map_err(|e| e.to_string()),
        ),
        (
            "bare_raw",
            serde_json::from_str::<T>(raw).map_err(|e| e.to_string()),
        ),
        (
            "bare_value",
            serde_json::from_value::<T>(retained).map_err(|e| e.to_string()),
        ),
    ];
    let mut compared_routes = 0;
    for (route, decoded) in routes {
        if case["valid"] == true {
            let decoded =
                decoded.unwrap_or_else(|e| panic!("type_decode {name}/{label}/{route}: {e}"));
            // Direct native member assertions, independent of serde rename metadata.
            let native = decoded.observe();
            assert_native_cardinality(&native, &case["native"], &format!("{name}/{label}/{route}"));
            assert_eq!(
                native, case["native"],
                "native_fields {name}/{label}/{route}"
            );
            let wire = safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(
                serde_json::to_value(&decoded).unwrap(),
            ))
            .unwrap()
            .unwrap();
            assert_eq!(
                wire,
                case["wire"].as_str().unwrap(),
                "wire_shape {name}/{label}/{route}"
            );
        } else {
            assert!(decoded.is_err(), "type_rejection {name}/{label}/{route}");
        }
        compared_routes += 1;
    }
    assert_eq!(compared_routes, 5, "BINGX-CARDINALITY per-case routes");
    compared_routes
}

#[test]
fn types_match_actual_typescript() {
    let corpus = oracle();
    println!("TypeScript oracle runtime: {}", corpus["runtime"]);
    assert_eq!(corpus["heritage"], true, "inherited_field_census");
    assert_eq!(
        corpus["names"],
        json!([
            "BINGX_BALANCE",
            "FETCH_BALANCE_RESPONSE",
            "BINGX_ORDER_STATUS",
            "FETCH_LEVERAGE_RESPONSE",
            "FETCH_MARGIN_MODE_RESPONSE",
            "FETCH_HEDGE_MODE_RESPONSE",
            "ORDER_TRADE_UPDATE",
            "FETCH_POSITIONS_RESPONSE",
        ]),
        "declared_type_census"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 883, "BINGX-CARDINALITY total cases");
    for (name, total, boundaries) in [
        ("BINGX_BALANCE", 66, 27),
        ("FETCH_BALANCE_RESPONSE", 153, 57),
        ("BINGX_ORDER_STATUS", 13, 0),
        ("FETCH_LEVERAGE_RESPONSE", 105, 21),
        ("FETCH_MARGIN_MODE_RESPONSE", 30, 3),
        ("FETCH_HEDGE_MODE_RESPONSE", 40, 3),
        ("ORDER_TRADE_UPDATE", 171, 48),
        ("FETCH_POSITIONS_RESPONSE", 305, 93),
    ] {
        assert_eq!(
            cases.iter().filter(|c| c["name"] == name).count(),
            total,
            "BINGX-CARDINALITY per-type cases {name}"
        );
        assert_eq!(
            cases
                .iter()
                .filter(|c| c["name"] == name && c["boundary"] == "utf8")
                .count(),
            boundaries,
            "BINGX-CARDINALITY per-type boundaries {name}"
        );
    }
    let source_rows: usize = cases
        .iter()
        .filter(|c| {
            c["valid"] == true
                && c["boundary"] != "utf8"
                && matches!(
                    c["name"].as_str(),
                    Some("FETCH_BALANCE_RESPONSE" | "FETCH_POSITIONS_RESPONSE")
                )
        })
        .map(|c| c["native"]["data"].as_array().unwrap().len())
        .sum();
    assert_eq!(source_rows, 220, "BINGX-CARDINALITY source total rows");
    let mut compared_routes = 0;
    for case in cases {
        compared_routes += match case["name"].as_str().unwrap() {
            "BINGX_BALANCE" => verify::<BingxBalance>(case),
            "FETCH_BALANCE_RESPONSE" => verify::<FetchBalanceResponse>(case),
            "BINGX_ORDER_STATUS" => verify::<BingxOrderStatus>(case),
            "FETCH_LEVERAGE_RESPONSE" => verify::<FetchLeverageResponse>(case),
            "FETCH_MARGIN_MODE_RESPONSE" => verify::<FetchMarginModeResponse>(case),
            "FETCH_HEDGE_MODE_RESPONSE" => verify::<FetchHedgeModeResponse>(case),
            "ORDER_TRADE_UPDATE" => verify::<OrderTradeUpdate>(case),
            "FETCH_POSITIONS_RESPONSE" => verify::<FetchPositionsResponse>(case),
            name => panic!("unhandled declared type {name}"),
        };
    }
    assert_eq!(
        compared_routes, 3659,
        "BINGX-CARDINALITY executed route comparisons"
    );
    let cases = corpus["cases"].as_array().unwrap();
    let boundaries = cases
        .iter()
        .filter(|case| case["boundary"] == "utf8")
        .count();
    println!(
        "{} typed cases across five decode routes; {boundaries} UTF-8 witnesses across two raw routes",
        cases.len() - boundaries
    );
}
