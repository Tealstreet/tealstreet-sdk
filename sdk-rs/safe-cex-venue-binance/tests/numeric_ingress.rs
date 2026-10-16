use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_binance::{binance_types as futures, binancepm_types as pm};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

#[test]
fn raw_numeric_overflow_is_accepted_without_losing_negative_zero() {
    let record = serde_json::from_str::<futures::BinanceIncomeHistoryOptions>(
        r#"{"limit":1e400,"startTime":-0}"#,
    )
    .unwrap();
    assert_eq!(record.limit.unwrap().to_bits(), f64::INFINITY.to_bits());
    assert_eq!(record.start_time.unwrap().to_bits(), (-0.0_f64).to_bits());
}

fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    route: usize,
) -> Result<T, serde_json::Error> {
    match route {
        0 => serde_json::from_str(raw),
        1 => serde_json::from_slice(raw.as_bytes()),
        2 => serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()),
        3 => T::deserialize(&serde_json::from_str::<Value>(raw).unwrap()),
        4 => js_from_str(raw),
        5 => js_from_value(serde_json::from_str::<Value>(raw).unwrap()),
        6 => js_from_value_ref(&serde_json::from_str::<Value>(raw).unwrap()),
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn numeric_record_fields_match_node_before_serialization() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/numeric-ingress.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("Node numeric ingress oracle");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 162);
    for case in cases {
        let variant = case["variant"].as_str().unwrap();
        let record = case["record"].as_str().unwrap();
        let key = case["key"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        for route in 0..7 {
            let actual: Result<f64, serde_json::Error> = match (variant, record) {
                ("binance", "GET_POSITION_RISK_RESPONSE") => {
                    decode::<futures::GetPositionRiskResponseRow>(raw, route).map(|row| match key {
                        "updateTime" => row.update_time,
                        "adl" => row.adl.unwrap(),
                        "adlQuantile" => row.adl_quantile.unwrap(),
                        _ => panic!("unclassified futures numeric field {key}"),
                    })
                }
                ("binancepm", "GET_POSITION_RISK_RESPONSE") => {
                    decode::<pm::GetPositionRiskResponseRow>(raw, route).map(|row| match key {
                        "updateTime" => row.update_time,
                        "adlQuantile" => row.adl_quantile,
                        _ => panic!("unclassified PM numeric field {key}"),
                    })
                }
                ("binance", "BinanceIncomeRecord") => {
                    assert_eq!(key, "time");
                    decode::<futures::BinanceIncomeRecord>(raw, route).map(|row| row.time)
                }
                ("binance", "BinanceIncomeHistoryOptions") => decode::<
                    futures::BinanceIncomeHistoryOptions,
                >(raw, route)
                .map(|row| match key {
                    "startTime" => row.start_time.unwrap(),
                    "endTime" => row.end_time.unwrap(),
                    "limit" => row.limit.unwrap(),
                    _ => panic!("unclassified income numeric field {key}"),
                }),
                _ => panic!("unclassified source record {variant}:{record}"),
            };
            let actual = actual.unwrap_or_else(|error| panic!("numeric ingress acceptance {variant}:{record}.{key} lexeme={} route={route}: {error}", case["lexeme"]));
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                case["bits"].as_str().unwrap(),
                "numeric ingress {variant}:{record}.{key} lexeme={} route={route}",
                case["lexeme"]
            );
        }
    }
}

#[test]
fn finding_value_rebuffering_loses_negative_zero() {
    use serde::Deserialize;
    let value: Value = serde_json::from_str("-0").unwrap();
    let direct = serde_json::from_value::<f64>(value.clone()).unwrap();
    let buffered = Value::deserialize(value).unwrap();
    let indirect = serde_json::from_value::<f64>(buffered).unwrap();
    assert_eq!(direct.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(
        indirect.to_bits(),
        0.0_f64.to_bits(),
        "FINDING must flip when rebuffering retains -0"
    );
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_wire_field_identity_survives_every_decode_route() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/field-identity.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4);
    // Read each public field directly. Whole-record Serialize could hide a
    // decoder/serializer pair that accidentally applies the same permutation.
    macro_rules! project {
        ($row:ident; $($wire:literal => $field:ident),+ $(,)?) => {
            serde_json::json!({$($wire: $row.$field),+})
        };
    }
    for case in cases {
        let variant = case["variant"].as_str().unwrap();
        let record = case["record"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        for route in 0..7 {
            let actual = match (variant, record) {
                ("binance", "GET_POSITION_RISK_RESPONSE") => {
                    let row = decode::<futures::GetPositionRiskResponseRow>(raw, route).unwrap();
                    project!(row; "symbol"=>symbol,"positionAmt"=>position_amt,"entryPrice"=>entry_price,
                        "markPrice"=>mark_price,"unRealizedProfit"=>un_realized_profit,"liquidationPrice"=>liquidation_price,
                        "leverage"=>leverage,"maxNotionalValue"=>max_notional_value,"marginType"=>margin_type,
                        "isolatedMargin"=>isolated_margin,"isAutoAddMargin"=>is_auto_add_margin,"positionSide"=>position_side,
                        "notional"=>notional,"isolatedWallet"=>isolated_wallet,"updateTime"=>update_time,
                        "isolated"=>isolated,"adl"=>adl,"adlQuantile"=>adl_quantile,"initialMargin"=>initial_margin,
                        "positionInitialMargin"=>position_initial_margin,"openOrderInitialMargin"=>open_order_initial_margin)
                }
                ("binancepm", "GET_POSITION_RISK_RESPONSE") => {
                    let row = decode::<pm::GetPositionRiskResponseRow>(raw, route).unwrap();
                    project!(row; "symbol"=>symbol,"positionAmt"=>position_amt,"entryPrice"=>entry_price,
                        "markPrice"=>mark_price,"unRealizedProfit"=>un_realized_profit,"liquidationPrice"=>liquidation_price,
                        "leverage"=>leverage,"maxNotionalValue"=>max_notional_value,"marginType"=>margin_type,
                        "isolatedMargin"=>isolated_margin,"isAutoAddMargin"=>is_auto_add_margin,"positionSide"=>position_side,
                        "notional"=>notional,"isolatedWallet"=>isolated_wallet,"updateTime"=>update_time,
                        "isolated"=>isolated,"adlQuantile"=>adl_quantile)
                }
                ("binance", "BinanceIncomeRecord") => {
                    let row = decode::<futures::BinanceIncomeRecord>(raw, route).unwrap();
                    project!(row; "symbol"=>symbol,"incomeType"=>income_type,"income"=>income,"asset"=>asset,
                        "info"=>info,"time"=>time,"tranId"=>tran_id,"tradeId"=>trade_id)
                }
                ("binance", "BinanceIncomeHistoryOptions") => {
                    let row = decode::<futures::BinanceIncomeHistoryOptions>(raw, route).unwrap();
                    project!(row; "symbol"=>symbol,"incomeType"=>income_type,"startTime"=>start_time,"endTime"=>end_time,"limit"=>limit)
                }
                _ => panic!("unclassified public record {variant}:{record}"),
            };
            assert_eq!(
                actual, case["expected"],
                "public wire field identity {variant}:{record} route={route}"
            );
        }
    }
}
