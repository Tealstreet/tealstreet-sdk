use safe_cex_types::{OrderTimeInForce, OrderType};
use safe_cex_venue_hyperliquid::order_inference::*;
use serde_json::Value;

#[test]
fn actual_source_inference_preserves_native_fields_and_precedence() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-inference.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "order inference source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 62, "INFERENCE fixed source total");
    let mut totals = [0; 3];
    let mut sources = std::collections::BTreeSet::new();
    for index in 0..62 {
        let row = cases.get(index).expect("INFERENCE native index coverage");
        let operation = row["operation"].as_str().unwrap();
        let category = match operation {
            "infer" => 0,
            "number" => 1,
            "tif" => 2,
            other => panic!("unknown operation {other}"),
        };
        totals[category] += 1;
        let raw = row["raw"].as_str().unwrap();
        if row["representationRefusal"] == true {
            assert!(
                serde_json::from_str::<Value>(raw).is_err(),
                "INFERENCE shared UTF-8 refusal {raw}"
            );
            assert!(
                row.get("error").is_none(),
                "source accepts lone surrogate input {raw}"
            );
            continue;
        }
        let input: Value = serde_json::from_str(raw).unwrap();
        if operation == "tif" {
            let native = parse_hyperliquid_order_time_in_force(Some(&input));
            let expected = match row["tif"].as_str() {
                Some("PostOnly") => Some(OrderTimeInForce::PostOnly),
                Some("GoodTillCancel") => Some(OrderTimeInForce::GoodTillCancel),
                None => None,
                other => panic!("unknown source tif {other:?}"),
            };
            assert_eq!(native, expected, "INFERENCE native tif {raw}");
            continue;
        }
        if operation == "number" {
            match to_number_or_zero(Some(&input)) {
                Ok(native) => {
                    assert!(
                        row.get("error").is_none(),
                        "unexpected source helper error {raw}"
                    );
                    assert_eq!(
                        native.to_bits(),
                        u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap(),
                        "INFERENCE native number bits {raw}"
                    );
                }
                Err(error) => {
                    assert_eq!(
                        error.name,
                        row["errorName"].as_str().unwrap(),
                        "INFERENCE native number error name {raw}"
                    );
                    assert_eq!(
                        error.message,
                        row["error"].as_str().unwrap(),
                        "INFERENCE native number error message {raw}"
                    );
                }
            }
            continue;
        }
        match infer_hyperliquid_order_type_and_price(&input) {
            Ok(native) => {
                assert!(
                    row.get("error").is_none(),
                    "unexpected source inference error {raw}"
                );
                let expected_type = match row["type"].as_str().unwrap() {
                    "limit" => OrderType::Limit,
                    "market" => OrderType::Market,
                    "take_profit_market" => OrderType::TakeProfit,
                    "stop_market" => OrderType::StopLoss,
                    other => panic!("unknown source order type {other}"),
                };
                assert_eq!(
                    native.r#type, expected_type,
                    "INFERENCE native order type {raw}"
                );
                assert_eq!(
                    native.price.to_bits(),
                    u64::from_str_radix(row["priceBits"].as_str().unwrap(), 16).unwrap(),
                    "INFERENCE native price bits {raw}"
                );
                let label = row["source"].as_str().unwrap();
                let expected_source = match label {
                    "orderType=Limit" => HyperliquidOrderInferenceSource::OrderTypeLimit,
                    "orderType=TP" => HyperliquidOrderInferenceSource::OrderTypeTp,
                    "orderType=SL" => HyperliquidOrderInferenceSource::OrderTypeSl,
                    "triggerPx+tpsl=tp" => HyperliquidOrderInferenceSource::TriggerPxTpslTp,
                    "triggerPx+tpsl=sl" => HyperliquidOrderInferenceSource::TriggerPxTpslSl,
                    "triggerPx-only" => HyperliquidOrderInferenceSource::TriggerPxOnly,
                    "no-orderType-fallback" => HyperliquidOrderInferenceSource::NoOrderTypeFallback,
                    "fallback-market" => HyperliquidOrderInferenceSource::FallbackMarket,
                    other => panic!("unknown source classification {other}"),
                };
                assert_eq!(
                    native.source, expected_source,
                    "INFERENCE native source label {raw}"
                );
                sources.insert(label.to_owned());
            }
            Err(error) => {
                assert_eq!(
                    error.name,
                    row["errorName"].as_str().unwrap(),
                    "INFERENCE native error name {raw}"
                );
                assert_eq!(
                    error.message,
                    row["error"].as_str().unwrap(),
                    "INFERENCE native error message {raw}"
                );
            }
        }
    }
    assert_eq!(
        totals,
        [34, 16, 12],
        "INFERENCE independent operation totals"
    );
    assert_eq!(sources.len(), 8, "INFERENCE all declared source variants");
    assert_eq!(
        parse_hyperliquid_order_time_in_force(None),
        None,
        "INFERENCE missing tif"
    );
    assert_eq!(
        to_number_or_zero(None).unwrap().to_bits(),
        0,
        "INFERENCE missing numeric input"
    );
}
