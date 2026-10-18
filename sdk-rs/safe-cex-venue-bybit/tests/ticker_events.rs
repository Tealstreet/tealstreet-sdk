use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Ticker;
use safe_cex_venue_bybit::ws_public::handle_tickers_event;
use serde_json::Value;

#[test]
fn ticker_notification_retains_subscriber_identity_after_committed_patch() {
    use safe_cex_store::StoreMutationError;
    use safe_cex_types::StoreSubscriberError;
    use safe_cex_venue_bybit::ws_public::TickerEventError;
    use std::{cell::Cell, rc::Rc};
    let store = DefaultStore::new();
    let ticker: Ticker = serde_json::from_value(serde_json::json!({
        "id":"BTCUSDT", "symbol":"BTCUSDT", "bid":1, "ask":2,
        "last":3, "mark":4, "index":5, "percentage":6, "openInterest":7,
        "fundingRate":8, "fundingTime":9, "volume":10, "quoteVolume":11
    }))
    .unwrap();
    Store::update(
        &store,
        StoreUpdate {
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let cause =
        StoreSubscriberError::with_response("listener refused", serde_json::json!({"code":419}))
            .with_cause(Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
                "original cause",
            )));
    let thrown = cause.clone();
    let first = Rc::new(Cell::new(0));
    let observed = first.clone();
    let _first_subscription = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            observed.set(observed.get() + 1);
            Err(thrown.clone())
        }),
    );
    let second = Rc::new(Cell::new(0));
    let observed = second.clone();
    let _second_subscription = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let mut original_id = None;
    for (is_spot, last) in [(false, 21.), (true, 22.)] {
        let error = handle_tickers_event(
            &store,
            &serde_json::json!({"data":{"symbol":"BTCUSDT","lastPrice":last}}),
            is_spot,
            false,
        )
        .unwrap_err();
        let TickerEventError::Store(StoreMutationError::Notification(notification)) = error else {
            panic!("ticker notification must retain typed subscriber identity: {error:?}");
        };
        assert_eq!(
            notification.cause(),
            &cause,
            "ticker original subscriber cause"
        );
        assert_eq!(
            *original_id.get_or_insert(notification.subscriber_id()),
            notification.subscriber_id(),
            "ticker stable subscriber identity"
        );
        assert_eq!(
            store.tickers().unwrap()[0].last,
            last,
            "ticker patch commits before notification"
        );
    }
    assert_eq!(first.get(), 2);
    assert_eq!(second.get(), 0, "ticker failure skips later subscriber");
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn ticker_events_match_live_source_with_native_nonfinite_patches() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-event-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        fixtures["keys"],
        serde_json::json!([
            "bid",
            "ask",
            "last",
            "mark",
            "index",
            "percentage",
            "openInterest",
            "fundingRate",
            "fundingTime",
            "volume",
            "quoteVolume"
        ]),
        "TICKER-CARDINALITY source field inventory"
    );
    assert_eq!(
        fixtures["cases"].as_array().unwrap().len(),
        1944,
        "TICKER-CARDINALITY source total"
    );
    let mut comparisons = 0;
    let mut retained_nonfinite = 0;
    let mut inherited_key_comparisons = 0;
    for (i, case) in fixtures["cases"].as_array().unwrap().iter().enumerate() {
        let initial: Ticker = serde_json::from_value(case["initial"].clone()).unwrap();
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                tickers: Some(vec![initial]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let notifications = std::rc::Rc::new(std::cell::Cell::new(0u64));
        let observed = notifications.clone();
        let _unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                observed.set(observed.get() + 1);
                Ok(())
            }),
        );
        let result = handle_tickers_event(
            &store,
            &case["frame"],
            case["variant"] == "bybitspot",
            case["variant"] == "bybit" && case["category"] == "inverse",
        );
        let symbol = case["frame"]["data"]["symbol"].as_str();
        if matches!(symbol, Some("constructor" | "__proto__" | "toString")) {
            assert_eq!(case["error"], "Cannot convert object to primitive value");
            inherited_key_comparisons += 1;
        }
        assert_eq!(
            result.err().map(|error| error.to_string()).as_deref(),
            case.get("error").and_then(Value::as_str),
            "case {i}: {case}"
        );
        assert_eq!(
            notifications.get(),
            case["notifications"].as_u64().unwrap(),
            "notification case {i}: {case}"
        );
        let tickers = store.tickers().unwrap();
        let ticker = &tickers[0];
        let numbers = [
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.mark,
            ticker.index.unwrap(),
            ticker.percentage,
            ticker.open_interest,
            ticker.funding_rate,
            ticker.funding_time,
            ticker.volume,
            ticker.quote_volume,
        ];
        assert_eq!(
            fixtures["keys"].as_array().unwrap().len(),
            numbers.len(),
            "TICKER-CARDINALITY expected/native field count"
        );
        for (key, number) in fixtures["keys"].as_array().unwrap().iter().zip(numbers) {
            comparisons += 1;
            let key = key.as_str().unwrap();
            let expected = &case["result"][key];
            if matches!(expected.as_str(), Some("NaN" | "Infinity" | "-Infinity")) {
                match expected.as_str().unwrap() {
                    "NaN" => assert!(number.is_nan(), "NaN overwrite case {i} {key}"),
                    "Infinity" => {
                        assert_eq!(number, f64::INFINITY, "Infinity overwrite case {i} {key}")
                    }
                    "-Infinity" => assert_eq!(
                        number,
                        f64::NEG_INFINITY,
                        "negative Infinity overwrite case {i} {key}"
                    ),
                    _ => unreachable!(),
                }
                retained_nonfinite += 1;
            } else {
                if expected == "-0" {
                    assert!(number == 0. && number.is_sign_negative(), "case {i} {key}");
                } else {
                    assert_eq!(
                        number,
                        expected.as_f64().unwrap(),
                        "case {i} field {key}: {case}"
                    );
                }
            }
        }
    }
    assert!(
        retained_nonfinite > 0,
        "source nonfinite overwrite witness disappeared"
    );
    assert_eq!(inherited_key_comparisons, 12);
    assert_eq!(comparisons, 21384, "TICKER-CARDINALITY native comparisons");
}
