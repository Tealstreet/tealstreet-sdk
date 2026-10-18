use safe_cex_store::{
    returns::{calculate_relative_returns, calculate_returns},
    risk::{build_orders_risk_maps, calculate_orders_risk, calculate_risk_at_price, format_pnl},
};
use safe_cex_types::{Market, Order, Position, Ticker};
use serde_json::Value;
fn number(value: &Value) -> f64 {
    match value["$number"].as_str() {
        Some("-0") => -0.0,
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        _ => value.as_f64().unwrap(),
    }
}
fn encoded(value: f64) -> Value {
    if value.is_nan() {
        serde_json::json!({"$number":"NaN"})
    } else if value == 0.0 && value.is_sign_negative() {
        serde_json::json!({"$number":"-0"})
    } else if value.is_infinite() {
        serde_json::json!({"$number":if value.is_sign_negative(){"-Infinity"}else{"Infinity"}})
    } else {
        serde_json::json!(value)
    }
}
fn assert_json(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Object(b))
            if b.get("$number").and_then(Value::as_str) == Some("-0") =>
        {
            assert_eq!(a.as_f64().unwrap().to_bits(), (-0.0_f64).to_bits())
        }
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{a} != {b}"
        ),
        (Value::Object(a), Value::Object(b)) => {
            // preserve_order changes Map's backing store; object key order is not domain data.
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>()
            );
            for (key, value) in a {
                assert_json(value, &b[key]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_json(a, b);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
#[test]
fn json_comparator_ignores_only_object_key_order() {
    let actual: Value = serde_json::from_str(r#"{"z":{"b":2,"a":1},"a":[1,2]}"#).unwrap();
    let reordered: Value = serde_json::from_str(r#"{"a":[1,2],"z":{"a":1,"b":2}}"#).unwrap();
    assert_json(&actual, &reordered);
    for (actual, expected) in [
        (serde_json::json!({}), serde_json::json!({"missing":null})),
        (serde_json::json!({"extra":null}), serde_json::json!({})),
        (serde_json::json!({"a":1}), serde_json::json!({"b":1})),
        (
            serde_json::json!({"nested":{"x":1}}),
            serde_json::json!({"nested":{"x":2}}),
        ),
        (serde_json::json!([1, 2]), serde_json::json!([2, 1])),
        (serde_json::json!([1]), serde_json::json!([1, 2])),
        (serde_json::json!(0.0), serde_json::json!(-0.0)),
        (serde_json::json!(0.0), serde_json::json!({"$number":"-0"})),
        (
            serde_json::json!(1.0),
            serde_json::json!(f64::from_bits(1.0_f64.to_bits() + 1)),
        ),
    ] {
        assert!(
            std::panic::catch_unwind(|| assert_json(&actual, &expected)).is_err(),
            "unexpected match: {actual} and {expected}"
        );
    }
}

#[test]
#[ignore = "Runs maintained TypeScript through Node; no committed generated corpus"]
fn live_typescript_risk_and_returns() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/tests/derived_oracle.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    // Pin each independent corpus: empty or truncated generator output must fail.
    for (key, count) in [
        ("risks", 104),
        ("returns", 90),
        ("specialReturns", 10),
        ("formats", 126),
        ("prototypeFailures", 6),
    ] {
        assert_eq!(
            cases[key].as_array().unwrap().len(),
            count,
            "{key} corpus size"
        );
    }
    let mut crosshair_count = 0;
    let mut profile_key_orders = std::collections::BTreeSet::new();
    for case in cases["risks"].as_array().unwrap() {
        let input = &case["input"];
        let markets: Vec<Market> = serde_json::from_value(input["markets"].clone()).unwrap();
        let tickers: Vec<Ticker> = serde_json::from_value(input["tickers"].clone()).unwrap();
        let positions: Vec<Position> = serde_json::from_value(input["positions"].clone()).unwrap();
        let orders: Vec<Order> = input["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|raw| {
                let mut copy = raw.clone();
                copy["price"] = serde_json::json!(0.0);
                let mut order: Order = serde_json::from_value(copy).unwrap();
                order.price = (number(&raw["price"])).into();
                order
            })
            .collect();
        let risk = calculate_orders_risk(&markets, &tickers, &positions, &orders, None).unwrap();
        for (id, profiles) in &risk {
            for (index, profile) in profiles.iter().enumerate() {
                let expected = &case["risk"][id][index];
                for (field, actual) in [
                    ("orderPnl", profile.order_pnl),
                    ("totalRealizedPnl", profile.total_realized_pnl),
                    ("rollingUnrealizedPnl", profile.rolling_unrealized_pnl),
                    ("positionSize", profile.position_size),
                ] {
                    assert_json(&encoded(actual), &expected[field]);
                }
                assert_eq!(
                    profile.order_line_text.as_deref(),
                    expected["orderLineText"].as_str(),
                    "RISK-NATIVE profile text identity"
                );
                if let Some(fee) = profile.fee {
                    assert_json(&encoded(fee), &expected["fee"]);
                }
            }
        }
        let serialized = serde_json::to_value(&risk).unwrap();
        assert_json(&serialized, &case["risk"]);
        for (id, rows) in serialized.as_object().unwrap() {
            for (index, row) in rows.as_array().unwrap().iter().enumerate() {
                profile_key_orders
                    .insert(row.as_object().unwrap().keys().cloned().collect::<Vec<_>>());
                assert_eq!(
                    row.as_object().unwrap().keys().collect::<Vec<_>>(),
                    case["risk"][id][index]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>(),
                    "RISK-PROFILE source branch field order: {id}/{index}"
                );
            }
        }
        let maps = build_orders_risk_maps(&markets, &tickers);
        assert_eq!(
            calculate_orders_risk(&markets, &tickers, &positions, &orders, Some(&maps)).unwrap(),
            risk
        );
        assert_eq!(case["crosshairs"].as_array().unwrap().len(), 8);
        for crosshair in case["crosshairs"].as_array().unwrap() {
            crosshair_count += 1;
            assert_json(
                &calculate_risk_at_price(crosshair["price"].as_f64().unwrap(),"BTC",&risk,&positions,&orders).map_or(Value::Null,|r|serde_json::json!({
                    "positionSize":encoded(r.position_size),"positionSide":r.position_side,"unrealizedPnl":encoded(r.unrealized_pnl),"realizedPnl":encoded(r.realized_pnl)
                })),
                &crosshair["expected"],
            );
        }
    }
    assert_eq!(
        profile_key_orders.len(),
        3,
        "RISK-PROFILE corpus must cover full-stop, size-last and size-before-fee branches"
    );
    for case in cases["prototypeFailures"].as_array().unwrap() {
        let input = &case["input"];
        let markets: Vec<Market> = serde_json::from_value(input["markets"].clone()).unwrap();
        let tickers: Vec<Ticker> = serde_json::from_value(input["tickers"].clone()).unwrap();
        let positions: Vec<Position> = serde_json::from_value(input["positions"].clone()).unwrap();
        let orders: Vec<Order> = serde_json::from_value(input["orders"].clone()).unwrap();
        let failed = calculate_orders_risk(&markets, &tickers, &positions, &orders, None).is_err();
        assert_eq!(failed, case["failed"].as_bool().unwrap());
    }
    for case in cases["returns"].as_array().unwrap() {
        let position: Position = serde_json::from_value(case["position"].clone()).unwrap();
        assert_json(
            &serde_json::to_value(calculate_returns(
                &position,
                (!case["override"].is_null()).then(|| number(&case["override"])),
            ))
            .unwrap(),
            &case["expected"],
        );
        assert_json(
            &serde_json::to_value(calculate_relative_returns(
                &position,
                case["exitPrice"].as_f64().unwrap(),
            ))
            .unwrap(),
            &case["relative"],
        );
    }
    for case in cases["specialReturns"].as_array().unwrap() {
        let mut position: Position =
            serde_json::from_value(cases["returns"][0]["position"].clone()).unwrap();
        let args = &case["args"];
        position.entry_price = number(&args["entry"]);
        position.contracts = number(&args["contracts"]);
        position.leverage = number(&args["leverage"]);
        position.unrealized_pnl = number(&args["upnl"]);
        let actual = calculate_returns(&position, args.get("override").map(number));
        let relative = calculate_relative_returns(&position, number(&args["exit"]));
        assert_json(
            &serde_json::json!({"roe":encoded(actual.roe),"roi":encoded(actual.roi)}),
            &case["expected"],
        );
        assert_json(
            &serde_json::json!({"roe":encoded(relative.roe),"roi":encoded(relative.roi)}),
            &case["relative"],
        );
    }
    for case in cases["formats"].as_array().unwrap() {
        assert_eq!(
            format_pnl(
                number(&case["value"]),
                case["precision"].as_f64(),
                case["prefix"].as_str()
            )
            .unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
    assert_eq!(crosshair_count, 832);
    println!(
        "{} risk cases / {crosshair_count} crosshair prices; {} finite + {} nonfinite returns pairs; {} formatter cases + {} prototype failures verified against live TS",
        cases["risks"].as_array().unwrap().len(),
        cases["returns"].as_array().unwrap().len(),
        cases["specialReturns"].as_array().unwrap().len(),
        cases["formats"].as_array().unwrap().len(),
        cases["prototypeFailures"].as_array().unwrap().len()
    );
}
