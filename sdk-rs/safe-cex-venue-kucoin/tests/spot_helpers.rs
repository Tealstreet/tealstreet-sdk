use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_kucoin::spot_helpers::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
#[test]
fn live_spot_position_and_leverage_helpers() {
    let market = json!({"id":"market-BTC","symbol":"BTC-USDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":10000},"leverage":{"min":1,"max":7}}});
    let mut markets = vec![market.clone(), market.clone()];
    for (symbol, active) in [("ETH-USDT", true), ("INACTIVE", false), ("", true)] {
        let mut m = market.clone();
        m["id"] = json!(format!("market:{symbol}"));
        m["symbol"] = json!(symbol);
        m["active"] = json!(active);
        markets.push(m);
    }
    let mut cases = vec![];
    for symbols in [
        json!(null),
        json!([]),
        json!(["BTC-USDT"]),
        json!(["ETH-USDT", "BTC-USDT", "BTC-USDT"]),
        json!(["missing"]),
        json!([""]),
    ] {
        for symbol in ["BTC-USDT", "new", ""] {
            cases.push(json!({"markets":markets,"symbols":symbols,"symbol":symbol,"leverage":37,"side":"short","isolated":false,"hedged":true,"fetchedLeverage":{"BTC-USDT":{"buyLeverage":10,"sellLeverage":20,"marginType":"cross","positionMode":"hedged","isFetching":true},"other":{"buyLeverage":3,"sellLeverage":4,"isFetching":false}}}));
        }
    }
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/spot-helpers.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 18, "KUCOIN-CARDINALITY helper input cases");
    assert_eq!(expected.len(), 18, "KUCOIN-CARDINALITY helper source cases");
    assert_eq!(
        expected
            .iter()
            .map(|v| v["positions"].as_array().unwrap().len())
            .sum::<usize>(),
        30,
        "KUCOIN-CARDINALITY helper source position total"
    );
    assert_eq!(
        expected
            .iter()
            .map(|v| v["fetchedLeverage"].as_object().unwrap().len())
            .sum::<usize>(),
        48,
        "KUCOIN-CARDINALITY helper source leverage total"
    );
    let mut compared_positions = 0;
    let mut compared_entries = 0;
    for index in 0..18 {
        let case = &cases[index];
        let expected = &expected[index];
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(serde_json::from_value(case["markets"].clone()).unwrap()),
                fetched_leverage: Some(
                    serde_json::from_value(case["fetchedLeverage"].clone()).unwrap(),
                ),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let updates = Rc::new(Cell::new(0));
        let count = updates.clone();
        let unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, from_worker| {
                assert!(!from_worker);
                count.set(count.get() + 1);
                Ok(())
            }),
        );
        let symbols: Option<Vec<String>> = serde_json::from_value(case["symbols"].clone()).unwrap();
        let symbol = case["symbol"].as_str().unwrap();
        let positions = fetch_positions(&store, symbols.as_deref()).unwrap();
        let max = fetch_symbol_max_leverage(symbol);
        set_leverage(
            &store,
            symbol,
            37.0,
            Some(safe_cex_types::PositionSide::Short),
        )
        .unwrap();
        change_margin_type(false, Some(symbol));
        change_position_mode(true, Some(symbol));
        let position_count = [4, 0, 2, 3, 0, 1][index / 3];
        let leverage_count = [2, 3, 3][index % 3];
        assert_eq!(
            expected["positions"].as_array().unwrap().len(),
            position_count,
            "KUCOIN-CARDINALITY helper source positions per case"
        );
        assert_eq!(
            expected["fetchedLeverage"].as_object().unwrap().len(),
            leverage_count,
            "KUCOIN-CARDINALITY helper source leverage per case"
        );
        assert_eq!(
            positions.len(),
            position_count,
            "KUCOIN-CARDINALITY helper native retained positions"
        );
        assert_eq!(
            store.fetched_leverage().unwrap().len(),
            leverage_count,
            "KUCOIN-CARDINALITY helper native retained leverage"
        );
        compared_positions += positions.len();
        compared_entries += store.fetched_leverage().unwrap().len();
        let actual = json!({"positions":positions,"max":max,"fetchedLeverage":&store.fetched_leverage().unwrap(),"updates":updates.get()});
        compare(&actual, expected);
        unsubscribe();
    }
    assert_eq!(
        compared_positions, 30,
        "KUCOIN-CARDINALITY helper compared positions"
    );
    assert_eq!(
        compared_entries, 48,
        "KUCOIN-CARDINALITY helper compared leverage entries"
    );
}

fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                compare(value, b.get(key).expect(key));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}

#[test]
fn live_spot_store_notification_failure_finding() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/spot-store-notifications.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 4);
    for row in rows {
        let throws = row["throws"].as_bool().unwrap();
        assert_eq!(
            row["leverage"],
            json!({"buyLeverage":1,"sellLeverage":1}),
            "state commits before notification"
        );
        assert_eq!(
            row["trace"],
            if throws {
                json!([["first", 1]])
            } else {
                json!([["first", 1], ["second", 1]])
            },
            "subscriber throw skips the second listener"
        );
        assert_eq!(
            row["rejected"],
            json!(throws),
            "subscriber throw rejects setLeverage"
        );
        assert_eq!(
            row["sameError"],
            json!(throws),
            "original subscriber error identity escapes"
        );

        let store = DefaultStore::new();
        let removed = Store::subscribe(&store, Box::new(|_, _| Ok(())));
        removed();
        let sentinel = safe_cex_types::StoreSubscriberError::with_response(
            "subscriber failure",
            json!({"status":409}),
        )
        .with_cause(Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
            "original subscriber object",
        )));
        let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut unsubscribers = Vec::new();
        for name in ["first", "second"] {
            let trace = trace.clone();
            let sentinel = sentinel.clone();
            let symbol = row["symbol"].as_str().unwrap().to_owned();
            unsubscribers.push(Store::subscribe(
                &store,
                Box::new(move |state, _| {
                    trace
                        .borrow_mut()
                        .push((name, state.fetched_leverage[&symbol].buy_leverage));
                    if throws && name == "first" {
                        Err(sentinel.clone())
                    } else {
                        Ok(())
                    }
                }),
            ));
        }
        let result: Result<(), safe_cex_store::StoreMutationError> =
            set_leverage(&store, row["symbol"].as_str().unwrap(), 37.0, None);
        if throws {
            let safe_cex_store::StoreMutationError::Notification(error) = result.unwrap_err()
            else {
                panic!("notification origin lost")
            };
            assert_eq!(
                error.subscriber_id(),
                1,
                "opaque failing subscriber id retained after removal"
            );
            assert_eq!(
                error.cause(),
                &sentinel,
                "original subscriber cause escapes"
            );
            assert_eq!(
                &*trace.borrow(),
                &[("first", 1.0)],
                "Rust subscriber failure skips second"
            );
        } else {
            result.unwrap();
            assert_eq!(&*trace.borrow(), &[("first", 1.0), ("second", 1.0)]);
        }
        assert_eq!(
            store.fetched_leverage().unwrap()[row["symbol"].as_str().unwrap()].sell_leverage,
            1.0
        );
        for unsubscribe in unsubscribers {
            unsubscribe();
        }
    }
}

#[test]
fn spot_notification_source_control() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_store_notification_failure_finding",
            "--nocapture",
        ])
        .env("KUCOIN_NOTIFY_MUTATION", "continue")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("subscriber throw skips the second listener"),
        "{stderr}"
    );
}

#[test]
fn live_channel_send_notification_reachability_finding() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/socket-store-notifications.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            row["trace"],
            json!(["first"]),
            "channel send notification skips later subscriber"
        );
        assert_eq!(
            row["settled"],
            json!([{"rejected":true,"sameError":true}]),
            "unawaited reconnect keeps original subscriber rejection"
        );
        assert_eq!(
            row["returnedUndefined"], true,
            "sendChannelOp does not await reconnect"
        );
        assert_eq!(
            row["publicHealthy"], true,
            "public health commits before rejection"
        );
        assert_eq!(
            row["privateHealthy"], true,
            "private health commits before rejection"
        );
        assert_eq!(
            row["heartbeatDidNotReconnect"], true,
            "actual heartbeat guard skips notification"
        );
    }
}

#[test]
fn channel_send_notification_source_control() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_channel_send_notification_reachability_finding",
            "--nocapture",
        ])
        .env("KUCOIN_NOTIFY_MUTATION", "continue")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("channel send notification skips later subscriber"),
        "{stderr}"
    );
}
