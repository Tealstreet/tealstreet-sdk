include!("cardinality.rs.inc");
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, StoreSubscriberError};
use safe_cex_venue_bitrue::{read_helpers::*, wire::OpaqueValue};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc};
fn bits(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}
fn observe(value: Option<f64>) -> Value {
    value
        .map(|v| json!({"type":"number","bits":bits(v)}))
        .unwrap_or_else(|| json!({"type":"null"}))
}
fn native(m: &Market) -> Value {
    json!({"id":m.id,"symbol":m.symbol,"amountMin":bits(m.limits.amount.min),"amountMax":bits(safe_cex_num::numeric_value_to_number(&m.limits.amount.max)),"leverageMin":bits(m.limits.leverage.min),"leverageMax":bits(m.limits.leverage.max)})
}
#[test]
fn leverage_matches_actual_typescript_and_store() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/leverage-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual leverage oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&capture, "leverage");
    println!(
        "capture {}; {} error-message, {} bracket, {} update cases",
        capture["header"],
        capture["errors"].as_array().unwrap().len(),
        capture["brackets"].as_array().unwrap().len(),
        capture["updates"].as_array().unwrap().len()
    );
    assert!(capture["header"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!capture["header"]["arch"].as_str().unwrap().is_empty());
    assert!(
        js_from_str::<String>(capture["utf8Raw"].as_str().unwrap()).is_err(),
        "chosen leverage message UTF-8 refusal"
    );
    for key in ["bracketUtf8Raw", "symbolUtf8Raw"] {
        assert!(
            js_from_str::<OpaqueValue>(capture[key].as_str().unwrap()).is_err(),
            "chosen leverage UTF-8 refusal: {key}"
        );
    }
    for row in capture["errors"].as_array().unwrap() {
        let actual = observe(parse_max_leverage_from_error_message(row["input"].as_str()));
        native_cardinality(&row["result"], &actual, "leverage error");
        assert_eq!(
            actual, row["result"],
            "leverage native parity: parseMaxLeverageFromErrorMessage; {row}"
        );
    }
    for row in capture["brackets"].as_array().unwrap() {
        let response = js_from_str::<OpaqueValue>(row["raw"].as_str().unwrap())
            .unwrap()
            .value;
        let actual =
            match parse_max_leverage_from_bracket_response(Some(&response), row["symbol"].as_str())
            {
                Ok(value) => observe(value),
                Err(error) => json!({"type":"error","name":error.name}),
            };
        native_cardinality(&row["result"], &actual, "leverage result");
        assert_eq!(
            actual, row["result"],
            "leverage native parity: parseMaxLeverageFromBracketResponse; {row}"
        );
    }
    let large_count = capture["largeCount"].as_u64().unwrap() as usize;
    let large = json!({"data":{"brackets":(0..large_count).map(|i| json!({"initialLeverage":2+i%2})).collect::<Vec<_>>()}});
    assert_eq!(
        parse_max_leverage_from_bracket_response(Some(&large), None).unwrap(),
        Some(3.0),
        "chosen leverage reduction avoids source argument expansion"
    );
    assert_eq!(
        capture["largeResult"],
        json!({"type":"error","name":"RangeError"}),
        "source argument expansion RangeError"
    );
    drop(large);
    for row in capture["updates"].as_array().unwrap() {
        let store = DefaultStore::new();
        let market = |id: &str, symbol: &str, max: f64| {
            let raw = format!(
                r#"{{"id":"{id}","symbol":"{symbol}","base":"BTC","quote":"USDT","active":true,"precision":{{"amount":0.01,"price":0.02}},"limits":{{"amount":{{"min":3,"max":71}},"leverage":{{"min":2,"max":{max}}}}},"info":{{"sentinel":"preserve"}}}}"#
            );
            let mut market = js_from_str::<Market>(&raw).unwrap();
            if symbol == "BTC" {
                market.limits.leverage.max = f64::from_bits(
                    u64::from_str_radix(row["initial"].as_str().unwrap(), 16).unwrap(),
                );
            }
            market
        };
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![
                    market("ID-A", "BTC", 7.0),
                    market("ID-B", "ETH", 19.0),
                ]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let notifications = Rc::new(Cell::new(0_u32));
        let captured = notifications.clone();
        let fail_listener = row["failListener"].as_bool().unwrap();
        let recovered = row["recovered"].as_bool().unwrap();
        let _unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                captured.set(captured.get() + 1);
                if fail_listener {
                    let response = json!({"status":409,"data":{"msg":"listener-specific"}});
                    if recovered {
                        let mut original = ExchangeError::new("observer failure");
                        original.response = Some(response);
                        original.code =
                            Some(safe_cex_exchange::js_runtime::value::Js::str("E_RECOVERED"));
                        // IDs from prior Store boundaries may repeat the new
                        // Store-local id 0; they must not be deduplicated.
                        original.store_subscriber_ids = vec![17, 0];
                        Err(original.into())
                    } else {
                        Err(StoreSubscriberError::with_response(
                            "observer failure",
                            response,
                        ))
                    }
                } else {
                    Ok(())
                }
            }),
        );
        let later = Rc::new(Cell::new(0_u32));
        let later_capture = later.clone();
        let _later_unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                later_capture.set(later_capture.get() + 1);
                Ok(())
            }),
        );
        let before = store.get_markets_version();
        let maximum =
            f64::from_bits(u64::from_str_radix(row["maximum"].as_str().unwrap(), 16).unwrap());
        let actual = match set_market_max_leverage(&store, row["symbol"].as_str().unwrap(), maximum)
        {
            Ok(()) => json!({"type":"undefined"}),
            Err(error) => {
                if error.name == "TypeError" {
                    assert!(
                        error.store_subscriber_ids.is_empty(),
                        "non-notification adds no subscriber id"
                    );
                    json!({"type":"error","name":error.name})
                } else {
                    if recovered {
                        assert_eq!(
                            error.store_subscriber_ids,
                            [17, 0, 0],
                            "leverage subscriber identity: recovered"
                        );
                    } else {
                        assert_eq!(
                            error.store_subscriber_ids,
                            [0],
                            "leverage subscriber identity: fallback"
                        );
                    }
                    let mut observed = json!({"type":"error","name":error.name,"message":error.message,"response":error.response});
                    if let Some(code) = error.code.as_ref() {
                        observed["code"] = safe_cex_exchange::js_runtime::value::to_json(code)
                            .expect("finite fixture error code");
                    }
                    observed
                }
            }
        };
        native_cardinality(&row["result"], &actual, "leverage result");
        assert_eq!(
            actual, row["result"],
            "leverage native parity: setMarketMaxLeverage return; {row}"
        );
        assert_eq!(
            store.get_markets_version() - before,
            row["version"].as_f64().unwrap(),
            "leverage native parity: setMarketMaxLeverage update count; {row}"
        );
        assert_eq!(
            notifications.get() as u64,
            row["notifications"].as_u64().unwrap(),
            "leverage native parity: setMarketMaxLeverage notifications; {row}"
        );
        assert_eq!(
            later.get() as u64,
            row["laterNotifications"].as_u64().unwrap(),
            "leverage native parity: setMarketMaxLeverage fanout stops; {row}"
        );
        let markets = store.markets().unwrap();
        let expected_markets = row["markets"].as_array().unwrap();
        assert_eq!(
            expected_markets.len(),
            2,
            "BITRUE-SOURCE-CARDINALITY markets per case"
        );
        assert_eq!(
            markets.len(),
            expected_markets.len(),
            "BITRUE-LEVERAGE-CARDINALITY retained markets"
        );
        for index in 0..expected_markets.len() {
            let market = &markets[index];
            let expected = &expected_markets[index];
            native_cardinality(expected, &native(market), "leverage market");
            assert_eq!(
                native(market),
                *expected,
                "leverage native parity: setMarketMaxLeverage fields; {row}"
            );
            assert!(
                market
                    .info
                    .as_ref()
                    .unwrap()
                    .get("sentinel")
                    .unwrap()
                    .text_equals("preserve")
            );
        }
    }
    native_comparisons(6876, "leverage");
}
