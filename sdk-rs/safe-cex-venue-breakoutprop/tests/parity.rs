use safe_cex_num::{
    TableHit, js_table_lookup,
    numeric_ingress::{
        JsNumberOutput, js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
    },
};
use safe_cex_venue_breakoutprop::{exchange::*, types::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Command, sync::LazyLock};
const SOURCE_CENSUS: &str = include_str!("support/parity-census.json");
include!("support/cardinality.rs");
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let output = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/oracle.cjs"))
        .output()
        .expect("run actual TypeScript oracle");
    assert!(
        output.status.success(),
        "TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_source_census(&corpus, SOURCE_CENSUS);
    corpus
});
trait Observe {
    fn observe(&self) -> Value;
}
impl Observe for f64 {
    fn observe(&self) -> Value {
        json!({"number": format!("{:016x}", if self.is_nan() {0x7ff8000000000000} else {self.to_bits()})})
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
        Value::Array(self.iter().map(Observe::observe).collect())
    }
}
impl Observe for Value {
    fn observe(&self) -> Value {
        match self {
            Value::Number(_) => js_value_as_f64(self).unwrap().observe(),
            Value::Array(a) => a.observe(),
            Value::Object(o) => {
                Value::Object(o.iter().map(|(k, v)| (k.clone(), v.observe())).collect())
            }
            v => v.clone(),
        }
    }
}
include!("support/native_fields.rs");
fn compare<T: DeserializeOwned + Serialize + JsNumberOutput + Observe>(case: &Value) -> usize {
    let raw = case["raw"].as_str().unwrap();
    let mut comparisons = 0;
    // Finite typed leaves permit a direct serde decode before numeric ingress.
    // A consuming decoder mutation must reach OUR native assertion, rather than
    // being caught only by a parser or numeric-ingress consistency check.
    if case["profile"] == -1 {
        let decoded: T = serde_json::from_str(raw).expect("finite source record decodes");
        let native = decoded.observe();
        assert_native_cardinality(
            &native,
            &case["expected"],
            "CONSUMING-DECODE native_fields_and_bits",
        );
        assert_eq!(
            native,
            case["expected"],
            "DECODED-NATIVE full fields and cardinality; native_fields_and_bits: row_{}",
            case["name"].as_str().unwrap()
        );
        comparisons += 1;
    }
    let owned: Value = serde_json::from_str(raw).unwrap();
    let decoded = [
        js_from_str::<T>(raw).unwrap(),
        js_from_value(owned.clone()).unwrap(),
        js_from_value_ref(&owned).unwrap(),
    ];
    for (path, value) in decoded.iter().enumerate() {
        let native = value.observe();
        assert_native_cardinality(&native, &case["expected"], "native_fields_and_bits");
        assert_eq!(
            native,
            case["expected"],
            "native_fields_and_bits: row_{} profile {} ingress {path}",
            case["name"].as_str().unwrap(),
            case["profile"]
        );
        comparisons += 1;
    }
    comparisons
}
fn dispatch(case: &Value) -> usize {
    match case["name"].as_str().unwrap() {
        "LOGIN_RESPONSE" => compare::<LoginResponse>(case),
        "USER_RESPONSE" => compare::<UserResponse>(case),
        "PORTFOLIO_RESPONSE" => compare::<PortfolioResponse>(case),
        "INSTRUMENTS_RESPONSE" => compare::<InstrumentsResponse>(case),
        "INSTRUMENT_DETAILS_RESPONSE" => compare::<InstrumentDetailsResponse>(case),
        "TICKER_RESPONSE" => compare::<TickerResponse>(case),
        "CANDLE_RESPONSE" => compare::<CandleResponse>(case),
        other => panic!("unhandled TS type {other}"),
    }
}
#[test]
fn native_fields_and_bits() {
    let mut comparisons = 0;
    eprintln!("types oracle runtime: {}", ORACLE["runtime"]);
    for case in ORACLE["cases"].as_array().unwrap() {
        comparisons += dispatch(case);
    }
    assert_eq!(comparisons, 175, "COMPARISON-COUNT parity");
}
#[test]
fn surrogate_boundary() {
    for case in ORACLE["surrogates"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        match case["name"].as_str().unwrap() {
            "LOGIN_RESPONSE" => {
                typed_utf16::assert_projection(raw, &js_from_str::<LoginResponse>(raw).unwrap())
            }
            "USER_RESPONSE" => {
                typed_utf16::assert_projection(raw, &js_from_str::<UserResponse>(raw).unwrap())
            }
            "PORTFOLIO_RESPONSE" => {
                typed_utf16::assert_projection(raw, &js_from_str::<PortfolioResponse>(raw).unwrap())
            }
            "INSTRUMENTS_RESPONSE" => typed_utf16::assert_projection(
                raw,
                &js_from_str::<InstrumentsResponse>(raw).unwrap(),
            ),
            "INSTRUMENT_DETAILS_RESPONSE" => typed_utf16::assert_projection(
                raw,
                &js_from_str::<InstrumentDetailsResponse>(raw).unwrap(),
            ),
            "TICKER_RESPONSE" => {
                typed_utf16::assert_projection(raw, &js_from_str::<TickerResponse>(raw).unwrap())
            }
            "CANDLE_RESPONSE" => {
                typed_utf16::assert_projection(raw, &js_from_str::<CandleResponse>(raw).unwrap())
            }
            _ => panic!("type inventory"),
        };
    }
}
#[test]
fn constants_and_lookup() {
    let tables = [
        ("ENDPOINTS", ENDPOINTS),
        ("INTERVAL", INTERVAL),
        ("ORDER_TYPE", ORDER_TYPE),
        ("ORDER_SIDE", ORDER_SIDE),
        ("ORDER_STATUS", ORDER_STATUS),
        ("POSITION_SIDE", POSITION_SIDE),
    ];
    let mut actual = serde_json::Map::new();
    for (name, table) in tables {
        assert_eq!(
            json!(table),
            ORACLE["orderedConstants"][name],
            "constant_{name}"
        );
        actual.insert(
            name.into(),
            json!(table.iter().copied().collect::<BTreeMap<_, _>>()),
        );
        for (key, expected) in ORACLE["tables"][name].as_object().unwrap() {
            let observed = match js_table_lookup(table, key) {
                TableHit::Mapped(v) => json!({"kind":"string","text":v}),
                TableHit::Missing => json!({"kind":"undefined","text":null}),
                TableHit::Inherited(v) => {
                    json!({"kind":if key=="__proto__" {"object"} else {"function"},"text":v.js_string()})
                }
            };
            assert_eq!(&observed, expected, "table_lookup: {name}.{key}");
        }
    }
    actual.insert("RECV_WINDOW".into(), json!(RECV_WINDOW));
    actual.insert("BASE_URL".into(), json!(BASE_URL));
    actual.insert(
        "GRAFANA_ENDPOINTS_EVERY".into(),
        json!(GRAFANA_ENDPOINTS_EVERY),
    );
    actual.insert(
        "GRAFANA_ENDPOINTS_PERIODIC".into(),
        json!(GRAFANA_ENDPOINTS_PERIODIC),
    );
    for (name, value) in &actual {
        let native = value.observe();
        assert_native_cardinality(&native, &ORACLE["constants"][name], "constant cardinality");
        assert_eq!(
            native,
            ORACLE["constants"][name].observe(),
            "constant_{name}"
        );
    }
    // Observe numbers before comparison; arbitrary-precision JSON distinguishes 20000.0 from 20000.
    assert_eq!(
        Value::Object(actual).observe(),
        ORACLE["constants"].observe(),
        "actual_typescript_constants"
    );
}
#[test]
fn source_helpers() {
    for case in ORACLE["timeouts"].as_array().unwrap() {
        let result = match parse_dx_session_timeout_ms(case["arg"].as_str().unwrap()) {
            Ok(v) => json!({"ok":v.observe()}),
            Err(e) => json!({"error":e}),
        };
        assert_eq!(
            result, case["result"],
            "session_timeout_bits: {}",
            case["arg"]
        );
    }
    for case in ORACLE["orderTypes"].as_array().unwrap() {
        let result = match to_dx_order_type(case["arg"].as_str().unwrap()) {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e}),
        };
        assert_eq!(result, case["result"], "order_type_branch: {}", case["arg"]);
    }
}

#[test]
fn helper_surrogate_boundary() {
    for case in ORACLE["helperSurrogates"].as_array().unwrap() {
        // Both source helpers interpolate the lone surrogate into their error.
        assert_eq!(case["errorRetainsSurrogate"], true);
        assert!(
            serde_json::from_str::<String>(case["raw"].as_str().unwrap()).is_err(),
            "helper_utf8_boundary: {}",
            case["name"]
        );
    }
}

#[test]
fn unknown_field_boundary() {
    for case in ORACLE["unknownCases"].as_array().unwrap() {
        assert_eq!(case["sourceRetainsUnknown"], "retained by TypeScript");
        assert!(case["added"].as_u64().unwrap() > 0);
        dispatch(case);
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
