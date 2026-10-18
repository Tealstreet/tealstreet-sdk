#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::{BothPositionSide, ExecutionPositionSide, Market, OrderSide, PositionSide};
use safe_cex_venue_weex::{
    fills::{normalize_execution, normalize_historical_trade},
    utils::WeexUtilityInput,
};
use serde_json::Value;
use std::{path::Path, process::Command};
fn input(raw: &str, route: usize) -> WeexUtilityInput {
    match route {
        0 => serde_json::from_str(raw).unwrap(),
        1 => js_from_str(raw).unwrap(),
        2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
        _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn numeric(n: f64, expected: &Value, label: &str) {
    assert_eq!(bits(n), expected["bits"].as_str().unwrap(), "{label}");
}
fn string<'a>(expected: &'a Value, key: &str) -> Option<&'a str> {
    expected.get(key).map(|v| v.as_str().unwrap())
}
fn side(expected: &Value) -> OrderSide {
    match expected.as_str().unwrap() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        other => panic!("unexpected side {other}"),
    }
}
fn position_side(expected: &Value) -> ExecutionPositionSide {
    match expected.as_str().unwrap() {
        "long" => ExecutionPositionSide::Side(PositionSide::Long),
        "short" => ExecutionPositionSide::Side(PositionSide::Short),
        "both" => ExecutionPositionSide::Both(BothPositionSide::Both),
        other => panic!("unexpected position side {other}"),
    }
}
#[test]
fn fills_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fills-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["node"].is_string());
    let market: Market = js_from_str(&corpus["market"].to_string()).unwrap();
    let ids = [
        "distinct",
        "aliases",
        "defaults",
        "id_zero",
        "id_null",
        "id_empty_array",
        "order_zero",
        "order_empty_array",
        "no_order",
        "commission_zero",
        "commission_invalid",
        "position_unknown",
        "nullish",
        "no_market",
        "symbol_empty",
        "side_invalid",
        "price_zero",
        "qty_zero",
        "time_invalid",
        "null",
        "price_error",
        "time_error",
        "id_error",
        "commission_error",
        "quote_error",
        "coercion",
        "numeric",
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 27, "fills::source_cases");
    let (mut outcomes, mut successes, mut nulls, mut errors, mut numeric_fields) = (0, 0, 0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(case["id"], ids[index], "fills::source_identity");
        let market = case["market"].as_bool().unwrap().then_some(&market);
        for route in 0..4 {
            let raw = input(case["raw"].as_str().unwrap(), route);
            for historical in [false, true] {
                let kind = if historical {
                    "historical"
                } else {
                    "execution"
                };
                let label = format!("fills::{kind}::{}::route_{route}", ids[index]);
                let expected = &case[kind];
                // Independent outcome inventory prevents a shortened or all-null oracle from passing.
                let is_error = [
                    "null",
                    "price_error",
                    "time_error",
                    "id_error",
                    "commission_error",
                ]
                .contains(&ids[index])
                    || historical && ids[index] == "quote_error";
                let is_null = [
                    "symbol_empty",
                    "side_invalid",
                    "price_zero",
                    "qty_zero",
                    "time_invalid",
                ]
                .contains(&ids[index])
                    || historical
                        && ["order_zero", "order_empty_array", "no_order"].contains(&ids[index]);
                assert_eq!(
                    expected.get("error").is_some(),
                    is_error,
                    "{label}::source_error_kind"
                );
                if !is_error {
                    assert_eq!(
                        expected["result"].is_null(),
                        is_null,
                        "{label}::source_null_kind"
                    );
                }
                let result = if historical {
                    normalize_historical_trade(&raw, market, |v| v).map(|v| {
                        v.map(|v| {
                            let e = &expected["result"];
                            assert_eq!(
                                v.order_id,
                                string(e, "orderId").unwrap(),
                                "{label}::orderId"
                            );
                            assert_eq!(
                                v.buyer,
                                e.get("buyer").map(|v| v.as_bool().unwrap()),
                                "{label}::buyer"
                            );
                            numeric(v.time, &e["time"], &format!("{label}::time"));
                            numeric_fields += 1;
                            numeric(
                                v.quote_qty.unwrap(),
                                &e["quoteQty"],
                                &format!("{label}::quoteQty"),
                            );
                            numeric_fields += 1;
                            (
                                v.id,
                                v.symbol,
                                Some(v.order_id),
                                v.price,
                                v.qty,
                                v.side,
                                Some(v.position_side),
                                v.realized_pnl,
                                v.maker,
                                v.commission,
                                v.commission_asset,
                            )
                        })
                    })
                } else {
                    normalize_execution(&raw, market, |v| v).map(|v| {
                        v.map(|v| {
                            numeric(
                                v.timestamp,
                                &expected["result"]["timestamp"],
                                &format!("{label}::timestamp"),
                            );
                            numeric_fields += 1;
                            assert!(
                                v.venue_reported_revenue.is_none()
                                    && v.venue_reported_revenue_asset.is_none(),
                                "{label}::absent_revenue"
                            );
                            (
                                v.id,
                                v.symbol,
                                v.order_id,
                                v.price,
                                v.qty,
                                v.side,
                                v.position_side,
                                v.realized_pnl,
                                v.maker,
                                v.commission,
                                v.commission_asset,
                            )
                        })
                    })
                };
                outcomes += 1;
                match result {
                    Err(error) => {
                        assert!(is_error, "{label}::unexpected_error");
                        assert_eq!(
                            error.name,
                            expected["error"]["name"].as_str().unwrap(),
                            "{label}::error_name"
                        );
                        assert_eq!(
                            error.message,
                            expected["error"]["message"].as_str().unwrap(),
                            "{label}::error_message"
                        );
                        errors += 1;
                    }
                    Ok(None) => {
                        assert!(is_null, "{label}::unexpected_null");
                        nulls += 1;
                    }
                    Ok(Some((
                        id,
                        symbol,
                        order_id,
                        price,
                        qty,
                        native_side,
                        native_position,
                        pnl,
                        maker,
                        commission,
                        asset,
                    ))) => {
                        assert!(!is_null && !is_error, "{label}::unexpected_success");
                        let e = &expected["result"];
                        assert_eq!(id.as_deref(), string(e, "id"), "{label}::id");
                        assert_eq!(symbol, string(e, "symbol").unwrap(), "{label}::symbol");
                        assert_eq!(
                            order_id.as_deref(),
                            string(e, "orderId"),
                            "{label}::orderId"
                        );
                        assert_eq!(native_side, side(&e["side"]), "{label}::side");
                        assert_eq!(
                            native_position,
                            Some(position_side(&e["positionSide"])),
                            "{label}::positionSide"
                        );
                        assert_eq!(
                            maker,
                            e.get("maker").map(|v| v.as_bool().unwrap()),
                            "{label}::maker"
                        );
                        assert_eq!(
                            asset.as_deref(),
                            string(e, "commissionAsset"),
                            "{label}::commissionAsset"
                        );
                        for (key, n) in [
                            ("price", price),
                            ("qty", qty),
                            ("realizedPnl", pnl.unwrap()),
                        ] {
                            numeric(n, &e[key], &format!("{label}::{key}"));
                            numeric_fields += 1;
                        }
                        assert_eq!(
                            commission.is_some(),
                            e.get("commission").is_some(),
                            "{label}::commission_presence"
                        );
                        if let Some(n) = commission {
                            numeric(n, &e["commission"], &format!("{label}::commission"));
                            numeric_fields += 1;
                        }
                        successes += 1;
                    }
                }
            }
        }
    }
    assert_eq!(outcomes, 216, "fills::outcomes");
    assert_eq!(successes, 120, "fills::successes");
    assert_eq!(nulls, 52, "fills::nulls");
    assert_eq!(errors, 44, "fills::errors");
    assert_eq!(numeric_fields, 548, "fills::numeric_fields");
    let refusals = corpus["shapeRefusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 4, "fills::shape_refusal_count");
    for (index, case) in refusals.iter().enumerate() {
        assert_eq!(
            case["id"],
            ["maker_null", "maker_string", "asset_number", "buyer_null"][index]
        );
        let input = input(case["raw"].as_str().unwrap(), 1);
        assert!(
            case["execution"]["result"].is_object() && case["historical"]["result"].is_object()
        );
        if index < 3 {
            assert!(
                normalize_execution(&input, Some(&market), |v| v).is_err(),
                "fills::execution_shape_refusal"
            );
        } else {
            assert!(
                normalize_execution(&input, Some(&market), |v| v)
                    .unwrap()
                    .is_some()
            );
        }
        assert!(
            normalize_historical_trade(&input, Some(&market), |v| v).is_err(),
            "fills::historical_shape_refusal"
        );
    }
    assert_eq!(corpus["callable"]["bits"], "401d000000000000");
    let native = retained_input::admit_utf16(r#"{"symbol":"\ud800"}"#);
    assert!(
        normalize_execution(&native, Some(&market), |value| value).is_err(),
        "fills UTF-8 model symbol refusal remains visible"
    );
}
