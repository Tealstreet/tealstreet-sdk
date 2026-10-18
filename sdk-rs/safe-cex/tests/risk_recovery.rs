//! Production engine and Store wiring. Worker processor remains owned by its slice.
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{
    Store, StoreUpdate,
    risk::{calculate_orders_risk, format_pnl},
};
use safe_cex_types::{Market, Order, Position, Ticker};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

fn calculate(
    input: &Value,
) -> Result<
    safe_cex_types::object_map::JsObjectMap<Vec<safe_cex_types::OrderRiskProfile>>,
    safe_cex_store::risk::RiskError,
> {
    let markets: Vec<Market> = serde_json::from_value(input["markets"].clone()).unwrap();
    let tickers: Vec<Ticker> = serde_json::from_value(input["tickers"].clone()).unwrap();
    let positions: Vec<Position> = serde_json::from_value(input["positions"].clone()).unwrap();
    let orders: Vec<Order> = serde_json::from_value(input["orders"].clone()).unwrap();
    calculate_orders_risk(&markets, &tickers, &positions, &orders, None)
}

fn assert_native_profiles(
    actual: &safe_cex_types::object_map::JsObjectMap<Vec<safe_cex_types::OrderRiskProfile>>,
    expected: &Value,
    native_numbers: &Value,
) {
    let expected = expected.as_object().unwrap();
    assert_eq!(
        actual.keys().map(String::as_str).collect::<Vec<_>>(),
        expected.keys().map(String::as_str).collect::<Vec<_>>(),
        "RISK-NATIVE-KEY-ORDER"
    );
    for (key, profiles) in actual {
        let expected = expected[key].as_array().unwrap();
        assert_eq!(
            profiles.len(),
            expected.len(),
            "independent source profile count"
        );
        for (index, (profile, source)) in profiles.iter().zip(expected).enumerate() {
            for (number, field) in [
                (profile.order_pnl, "orderPnl"),
                (profile.total_realized_pnl, "totalRealizedPnl"),
                (profile.rolling_unrealized_pnl, "rollingUnrealizedPnl"),
                (profile.position_size, "positionSize"),
            ] {
                assert_eq!(
                    number.to_bits(),
                    native_numbers[key][index][field]
                        .as_str()
                        .unwrap()
                        .parse::<u64>()
                        .unwrap(),
                    "RISK-NATIVE-BITS {key}/{field}"
                );
            }
            assert_eq!(
                profile.is_looking_like_a_full_stop,
                source
                    .get("isLookingLikeAFullStop")
                    .map(|v| v.as_bool().unwrap())
            );
            assert_eq!(
                profile.order_line_text.as_deref(),
                source.get("orderLineText").map(|v| v.as_str().unwrap())
            );
            assert_eq!(
                profile.fee.map(f64::to_bits),
                native_numbers[key][index].get("fee").map(|v| v
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap())
            );
            assert_eq!(
                profile.order_line_pos_side,
                match source["orderLinePosSide"].as_str().unwrap() {
                    "long" => safe_cex_types::PositionSide::Long,
                    "short" => safe_cex_types::PositionSide::Short,
                    _ => panic!("independent source side"),
                }
            );
            if profile.fee.is_some() {
                let keys = source
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                assert_eq!(
                    profile.position_size_before_fee,
                    keys.iter().position(|k| *k == "positionSize")
                        < keys.iter().position(|k| *k == "fee"),
                    "native profile construction preserves source enumeration branch"
                );
            }
        }
    }
}

fn assert_profile_values(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "risk numeric bits"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_profile_values(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            // This recovery gate measures values, not dictionary enumeration.
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>()
            );
            for (key, value) in a {
                assert_profile_values(value, &b[key]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn prototype_risk_error_is_catchable_and_store_clearing_is_observable() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/risk_recovery_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["rows"].as_array().unwrap().len(), 48);
    for row in corpus["rows"].as_array().unwrap() {
        let input = &row["input"];
        let mut good = input.clone();
        good["orders"] = serde_json::json!([input["orders"][0]]);
        good["positions"] = serde_json::json!([input["positions"][0]]);
        good["tickers"] = serde_json::json!([input["tickers"][0]]);
        let prior = calculate(&good).unwrap();
        assert!(!prior.is_empty());
        let real = DefaultStore::new();
        let host: &dyn Store = &real;
        host.update(
            StoreUpdate {
                orders_risk: Some(prior.into_iter().collect()),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation must succeed");
        let observations = Rc::new(RefCell::new(Vec::new()));
        let captured = observations.clone();
        let unsubscribe = host.subscribe(Box::new(move |data, _| {
            captured.borrow_mut().push(data.orders_risk.len());
            Ok(())
        }));
        let result = calculate(input);
        assert!(
            result.is_err(),
            "prototype key must produce recoverable risk error: {row}"
        );
        let error = result.err().unwrap();
        assert_eq!(error.name(), row["failure"]["name"].as_str().unwrap());
        assert_eq!(error.message(), row["failure"]["message"].as_str().unwrap());
        assert_eq!(error.key(), row["key"].as_str());
        assert!(error.member().is_some());
        // Explicit caller recovery, not a partial result or invented zero-valued profiles.
        host.update(
            StoreUpdate {
                orders_risk: Some(Default::default()),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation must succeed");
        assert!(
            host.orders_risk().unwrap().is_empty(),
            "risk must be visibly cleared"
        );
        assert_eq!(
            observations.borrow().last(),
            Some(&0),
            "clearing must notify the host"
        );
        unsubscribe();
    }
    assert_eq!(
        corpus["controls"].as_array().unwrap().len(),
        61,
        "independent control floor"
    );
    assert_eq!(
        corpus["ordering"].as_array().unwrap().len(),
        3,
        "independent ordering floor"
    );
    assert_eq!(
        corpus["formats"].as_array().unwrap().len(),
        14,
        "independent Intl option floor"
    );
    let controls = corpus["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let risk = calculate(&row["input"]).unwrap();
            assert_native_profiles(&risk, &row["expected"], &row["nativeNumbers"]);
            (risk, &row["expected"])
        })
        .collect::<Vec<_>>();
    for row in corpus["ordering"].as_array().unwrap() {
        let risk = calculate(&row["input"]).unwrap();
        assert_eq!(
            risk.keys().map(String::as_str).collect::<Vec<_>>(),
            row["expectedKeys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "risk own-key enumeration"
        );
    }
    for row in corpus["formats"].as_array().unwrap() {
        let precision: f64 = row["precision"].as_str().unwrap().parse().unwrap();
        let value: f64 = row["value"].as_str().unwrap().parse().unwrap();
        let result = format_pnl(value, Some(precision), None);
        if let Some(expected) = row["expected"].as_str() {
            assert_eq!(result.unwrap(), expected);
        } else {
            let error = result.expect_err("Intl options must return catchable RangeError");
            assert_eq!(error.name(), row["failure"]["name"].as_str().unwrap());
            assert_eq!(error.message(), row["failure"]["message"].as_str().unwrap());
        }
    }
    // Every native profile, error, key order and format assertion precedes wire views.
    for (risk, expected) in controls {
        let wire = serde_json::to_value(risk).unwrap();
        assert_profile_values(&wire, expected);
        for (key, profiles) in wire.as_object().unwrap() {
            for (profile, source) in profiles
                .as_array()
                .unwrap()
                .iter()
                .zip(expected[key].as_array().unwrap())
            {
                assert_eq!(
                    profile.as_object().unwrap().keys().collect::<Vec<_>>(),
                    source.as_object().unwrap().keys().collect::<Vec<_>>(),
                    "source profile field enumeration, including positionSize"
                );
            }
        }
    }
}
