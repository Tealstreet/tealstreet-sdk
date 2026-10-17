use safe_cex_types::{ExecutionPositionSide, OrderSide, PositionSide};
use safe_cex_venue_woo::transaction::parse_transaction_history_row;
use serde_json::{Value, json};
use std::process::Command;

fn bits(value: f64) -> Value {
    json!({"$bits": format!("{:016x}", if value.is_nan() { 0x7ff8_0000_0000_0000 } else { value.to_bits() })})
}
fn optional<T>(value: Option<T>, encode: impl FnOnce(T) -> Value) -> Value {
    value.map_or_else(|| json!({"$undefined":true}), encode)
}
fn pack(value: &Value) -> Value {
    match value {
        Value::Number(_) => bits(safe_cex_num::js_number(Some(value)).unwrap()),
        Value::Array(values) => Value::Array(values.iter().map(pack).collect()),
        Value::Object(values) => {
            Value::Object(values.iter().map(|(k, v)| (k.clone(), pack(v))).collect())
        }
        value => value.clone(),
    }
}
#[test]
fn transaction_native_fields_match_actual_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/transaction-oracle.cjs"
        ))
        .output()
        .expect("transaction source process");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(capture["architecture"].as_str().is_some());
    assert!(capture["nodeMajor"].as_str().is_some());
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 272, "TRANSACTION source case total");
    let mut compared = 0;
    let mut native_fields = 0;
    let mut classes = [0usize; 3];
    for index in 0..272 {
        let case = cases
            .get(index)
            .expect("TRANSACTION independently required case");
        let input = safe_cex_transport::how::json_parse(case["raw"].as_str().unwrap());
        let actual =
            parse_transaction_history_row(input.into_value().unwrap().as_object().unwrap());
        if let Some(error) = case.get("error") {
            classes[0] += 1;
            assert_eq!(
                actual.err().as_deref(),
                error.as_str(),
                "TRANSACTION coercion error {}",
                case["label"]
            );
        } else if case["ok"].is_null() {
            classes[1] += 1;
            assert!(
                actual.unwrap().is_none(),
                "TRANSACTION rejected row {}",
                case["label"]
            );
        } else {
            let row = actual.unwrap().expect("TRANSACTION accepted row");
            let expected = &case["ok"];
            classes[2] += 1;
            assert_eq!(
                expected.as_object().unwrap().len(),
                13,
                "TRANSACTION source field inventory"
            );
            assert_eq!(row.symbol, expected["symbol"], "TRANSACTION native symbol");
            assert_eq!(
                match row.side {
                    OrderSide::Buy => "buy",
                    OrderSide::Sell => "sell",
                },
                expected["side"],
                "TRANSACTION native side"
            );
            for (field, value) in [
                ("price", row.price),
                ("qty", row.qty),
                ("timestampMilliseconds", row.timestamp_milliseconds),
                ("timestampSeconds", row.timestamp_seconds),
            ] {
                assert_eq!(
                    bits(value),
                    expected[field],
                    "TRANSACTION native {field} {}",
                    case["label"]
                );
            }
            assert_eq!(
                optional(row.order_id, Value::String),
                expected["orderId"],
                "TRANSACTION native orderId"
            );
            assert_eq!(
                optional(row.fill_id, Value::String),
                expected["fillId"],
                "TRANSACTION native fillId"
            );
            assert_eq!(
                optional(row.maker, Value::Bool),
                expected["maker"],
                "TRANSACTION native maker"
            );
            assert_eq!(
                optional(row.commission, bits),
                expected["commission"],
                "TRANSACTION native commission"
            );
            assert_eq!(
                optional(row.realized_pnl, bits),
                expected["realizedPnl"],
                "TRANSACTION native realizedPnl"
            );
            assert_eq!(
                optional(row.commission_asset, |v| pack(&v)),
                expected["commissionAsset"],
                "TRANSACTION native commissionAsset"
            );
            assert_eq!(
                match row.position_side {
                    ExecutionPositionSide::Side(PositionSide::Long) => "long",
                    ExecutionPositionSide::Side(PositionSide::Short) => "short",
                    ExecutionPositionSide::Both(_) => "both",
                },
                expected["positionSide"],
                "TRANSACTION native positionSide"
            );
            native_fields += 13;
        }
        compared += 1;
    }
    assert_eq!(compared, 272, "TRANSACTION native comparison total");
    assert_eq!(
        classes,
        [13, 86, 173],
        "TRANSACTION source outcome inventory"
    );
    assert_eq!(
        native_fields, 2249,
        "TRANSACTION native field comparison total"
    );
    let mut input = json!({"symbol":"PERP_BTC_USDT","side":"BUY","price":17.125,"quantity":3.75,"timestamp":1700000000123i64,"feeAsset":{"label":"before"}});
    let row = parse_transaction_history_row(input.as_object().unwrap())
        .unwrap()
        .unwrap();
    input["feeAsset"]["label"] = json!("after");
    assert_eq!(
        row.commission_asset.as_ref().unwrap(),
        &capture["alias"]["before"],
        "TRANSACTION owned asset snapshot"
    );
    assert_eq!(
        input["feeAsset"], capture["alias"]["after"],
        "TRANSACTION source live asset view"
    );
}
