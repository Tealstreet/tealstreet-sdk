#[path = "support/history_field_identity.rs"]
mod native_identity;
use safe_cex_venue_woo::order_history::*;
use serde::Serialize;
use serde_json::{Value, json};
fn wire<T: Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.map(|v| serde_json::to_value(v).unwrap())
}
fn same(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                assert!(b.contains_key(k), "{k}");
                same(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn live_history_projection_preserves_time_units_fee_sign_and_zero_pnl() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/history.json")).unwrap();
    assert_eq!(
        fixture["cases"].as_array().unwrap().len(),
        634,
        "WOO-CARDINALITY history source total"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = if case["missing"] == true {
            None
        } else {
            Some(&case["input"])
        };
        let result = match case["name"].as_str().unwrap() {
            "toMilliseconds" => wire(to_milliseconds(input)),
            "toOptionalNumber" => wire(to_optional_number(input)),
            "hasValue" => wire(has_value(input)),
            "toNumber" => wire(to_number(input)),
            "mapWooHistoricalOrder" => wire(
                map_woo_historical_order(case["input"].as_object().unwrap(), |id| {
                    if case["resolveError"] == true {
                        Err("resolver failed".into())
                    } else {
                        Ok((id == "PERP_BTC_USDT").then(|| "BTCUSDT".into()))
                    }
                })
                .inspect(|value| {
                    if let Some(order) = value {
                        native_identity::check(order, &case["ok"]);
                    }
                }),
            ),
            name => panic!("{name}"),
        };
        if let Some(error) = case.get("error") {
            assert_eq!(result, Err(error.as_str().unwrap().into()), "{case}");
        } else {
            let actual = result.unwrap_or_else(|e| panic!("{case}: {e}"));
            same(&actual, &case["ok"]);
        }
    }
}
#[test]
fn resolver_empty_string_falls_back_without_dropping_order() {
    let row = json!({"order_id":"1","symbol":"RAW","created_time":"1.001","side":"BUY"});
    let order = map_woo_historical_order(row.as_object().unwrap(), |_| Ok(Some(String::new())))
        .unwrap()
        .unwrap();
    assert_eq!(order.symbol, "RAW");
    assert_eq!(order.created_time, 1000.0);
}

#[test]
fn finite_seconds_overflow_is_preserved_without_a_panic() {
    assert_eq!(to_milliseconds(Some(&json!(1e308))).unwrap(), f64::INFINITY);
}

#[test]
fn asset_truthiness_retains_overflow_before_string_projection() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/history-assets.json")).unwrap();
    for case in &cases {
        let field = case["field"].as_str().unwrap();
        let lexeme = case["lexeme"].as_str().unwrap();
        let mut row = json!({"order_id":"1","symbol":"RAW","created_time":"1.001","side":"BUY"})
            .as_object()
            .unwrap()
            .clone();
        row.insert(field.into(), serde_json::from_str(lexeme).unwrap());
        let order = map_woo_historical_order(&row, |_| Ok(None))
            .unwrap()
            .unwrap();
        let asset = if field == "fee_asset" {
            order.fee_asset
        } else {
            order.rebate_asset
        };
        assert_eq!(
            asset.as_deref(),
            case["asset"].as_str(),
            "{field} {lexeme}: source truthiness before String"
        );
    }
    assert_eq!(cases.len(), 14);
}
