include!("cardinality.rs.inc");
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
};
use safe_cex_venue_bitrue::wire::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;

trait Native {
    fn native(&self) -> Value;
}
impl Native for f64 {
    fn native(&self) -> Value {
        json!({"number":format!("{:016x}",if self.is_nan(){0x7ff8000000000000}else{self.to_bits()})})
    }
}
impl Native for String {
    fn native(&self) -> Value {
        Value::String(self.clone())
    }
}
impl Native for bool {
    fn native(&self) -> Value {
        Value::Bool(*self)
    }
}
impl<T: Native> Native for Vec<T> {
    fn native(&self) -> Value {
        Value::Array(self.iter().map(Native::native).collect())
    }
}
impl<A: Native, B: Native> Native for (A, B) {
    fn native(&self) -> Value {
        json!([self.0.native(), self.1.native()])
    }
}
impl Native for Value {
    fn native(&self) -> Value {
        match self {
            Value::Number(_) => js_value_as_f64(self).unwrap().native(),
            Value::Array(a) => a.native(),
            Value::Object(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), v.native())).collect())
            }
            _ => self.clone(),
        }
    }
}
include!("native_types.rs.inc");
fn check<T: DeserializeOwned + Serialize + JsNumberOutput + Native>(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let label = case["label"].as_str().unwrap();
    let input = case["input"].as_str().unwrap();
    if label == "chosen UTF-8 surrogate refusal" {
        typed_utf16::assert_projection(
            input,
            &(serde_json::from_str::<T>(input)).expect("typed surrogate record is retained"),
        );
        typed_utf16::assert_projection(
            input,
            &(js_from_str::<T>(input)).expect("typed surrogate record is retained"),
        );
        return;
    }
    let value: Value = serde_json::from_str(input).unwrap();
    let routes = [
        ("public raw", serde_json::from_str::<T>(input)),
        ("public owned", serde_json::from_value::<T>(value.clone())),
        ("public borrowed", T::deserialize(&value)),
        ("shared raw", js_from_str::<T>(input)),
        ("shared owned", js_from_value::<T>(value.clone())),
        ("shared borrowed", js_from_value_ref::<T>(&value)),
    ];
    for (route, result) in routes {
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "type shape: {name} {label} {route}: {}",
            result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
        if let Ok(native) = result {
            native_cardinality(&case["expected"], &native.native(), "wire");
            assert_eq!(
                native.native(),
                case["expected"],
                "native type parity: {name} {label} {route}"
            );
        }
    }
}
#[test]
fn declared_types_match_live_source_and_native_values() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/types-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live type oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stderr));
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&Value::Array(cases.clone()), "wire");
    for case in &cases {
        match case["name"].as_str().unwrap() {
            "BITRUE_ORDER_TYPE" => check::<BitrueOrderType>(case),
            "FETCH_ACCOUNT_RESPONSE" => check::<FetchAccountResponse>(case),
            "FETCH_CONTRACTS_RESPONSE" => check::<FetchContractsResponse>(case),
            "FETCH_LEVERAGE_BRACKET_RESPONSE" => check::<FetchLeverageBracketResponse>(case),
            "FETCH_KLINE_RESPONSE" => check::<FetchKlineResponse>(case),
            "ORDER_BOOK_WS_UPDATE" => check::<OrderBookWsUpdate>(case),
            "TICKER_WS_UPDATE" => check::<TickerWsUpdate>(case),
            "FETCH_ORDERS_RESPONSE" => check::<FetchOrdersResponse>(case),
            "FETCH_MY_TRADES_RESPONSE" => check::<FetchMyTradesResponse>(case),
            "WS_ORDER_TRADE_UPDATE" => check::<WsOrderTradeUpdate>(case),
            name => panic!("unmapped live TS type {name}"),
        }
    }
    println!(
        "{} cases across 10 actual declarations; 6 ingress routes for representable input, raw-only surrogate refusals",
        cases.len()
    );
    native_comparisons(594, "wire");
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
