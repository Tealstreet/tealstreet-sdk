#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Ticker;
use safe_cex_venue_okx::public_socket::{
    handle_funding_rate_events, handle_index_ticker_events, handle_mark_price_events,
    handle_open_interest_events, handle_ticker_events,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn captured_equal(n: f64, expected: &str) -> bool {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            n.to_bits()
        }
    ) == expected
}
#[test]
fn comparison_retains_signed_zero_identity() {
    assert!(captured_equal(-0.0, "8000000000000000"));
    assert!(
        !captured_equal(-0.0, "0000000000000000"),
        "comparison signed zero identity"
    );
}
#[test]
fn comparison_retains_finite_payload_identity() {
    assert!(captured_equal(1.25, "3ff4000000000000"));
    assert!(
        !captured_equal(1.25, "4004000000000000"),
        "comparison finite payload identity"
    );
    assert!(captured_equal(-f64::NAN, "7ff8000000000000"));
}
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn maintained_typescript_first_ticker_update_numeric_bits() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ticker-events-oracle.mjs"
    );
    let oracle = std::process::Command::new("node")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    for (flag, expected) in [
        (
            "--mutate-ignore-signed-zero",
            "capture signed zero identity",
        ),
        ("--mutate-ignore-finite", "capture finite payload identity"),
    ] {
        let control = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ticker-events-oracle.mjs"
            ))
            .arg(flag)
            .output()
            .unwrap();
        assert_eq!(control.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&control.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&control.stderr)
        );
    }
    let mut cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    let cardinality_control = std::env::var("OKX_CARDINALITY_CONTROL").ok();
    let bypass_field_counts = cardinality_control.as_deref() == Some("bypass-field-counts");
    if cardinality_control.as_deref() == Some("drop-handler") {
        let replacement = cases
            .iter()
            .find(|case| case["method"] == "handleFundingRateEvents")
            .unwrap()
            .clone();
        for case in &mut cases {
            if case["method"] == "handleOpenInterestEvents" {
                *case = replacement.clone();
            }
        }
    } else if let Some(mode) = cardinality_control.as_deref() {
        for case in &mut cases {
            for call in case["calls"].as_array_mut().unwrap() {
                let patch = call["patch"].as_object_mut().unwrap();
                match mode {
                    "drop-fields" | "bypass-field-counts" => patch.clear(),
                    "drop-one-field" => {
                        let key = patch.keys().next().unwrap().clone();
                        patch.remove(&key);
                    }
                    _ => panic!("unknown cardinality control"),
                }
            }
        }
    }
    assert_eq!(cases.len(), 209);
    cardinality::assert_groups(
        &cases,
        "ticker_events profiles",
        |case| case["method"].as_str().unwrap().to_owned(),
        &[
            ("handleOpenInterestEvents", 19),
            ("handleFundingRateEvents", 38),
            ("handleMarkPriceEvents", 19),
            ("handleIndexTickerEvents", 19),
            ("handleTickerEvents", 114),
        ],
    );
    let source_calls: usize = cases
        .iter()
        .map(|case| case["calls"].as_array().unwrap().len())
        .sum();
    let source_fields: usize = cases
        .iter()
        .flat_map(|case| case["calls"].as_array().unwrap())
        .map(|call| call["patch"].as_object().unwrap().len())
        .sum();
    assert_eq!(source_calls, 216, "TICKER-CARDINALITY source write total");
    if !bypass_field_counts {
        assert_eq!(source_fields, 792, "TICKER-CARDINALITY source field total");
    }
    let mut native_comparisons = 0;
    let mut native_writes = 0;
    for case in cases {
        let store = DefaultStore::new();
        let ticker:Ticker=serde_json::from_value(json!({"id":"BTC","symbol":"BTCUSDT","bid":1,"ask":1,"last":1,"mark":1,"index":1,"percentage":1,"openInterest":1,"fundingRate":1,"fundingTime":1,"volume":1,"quoteVolume":1})).unwrap();
        let mut second = ticker.clone();
        second.id = "ETH".into();
        second.symbol = "ETHUSDT".into();
        Store::update(
            &store,
            StoreUpdate {
                tickers: Some(vec![ticker, second]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let count = Rc::new(Cell::new(0));
        let calls = count.clone();
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let current_id = Rc::new(RefCell::new(String::from("BTC")));
        let log = trace.clone();
        let observed_id = current_id.clone();
        let unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                calls.set(calls.get() + 1);
                log.borrow_mut()
                    .push(format!("write:{}", observed_id.borrow()));
                Ok(())
            }),
        );
        let loaded = |id: &str| {
            trace.borrow_mut().push(format!("loaded:{id}"));
            *current_id.borrow_mut() = id.into();
            Ok(())
        };
        let result = match case["method"].as_str().unwrap() {
            "handleTickerEvents" => handle_ticker_events(&store, &case["input"]),
            "handleOpenInterestEvents" => handle_open_interest_events(&store, &case["input"]),
            "handleFundingRateEvents" => handle_funding_rate_events(&store, &case["input"]),
            "handleMarkPriceEvents" => handle_mark_price_events(&store, &case["input"], |id| {
                loaded(id.unwrap().as_str().unwrap())
            }),
            "handleIndexTickerEvents" => handle_index_ticker_events(
                &store,
                &case["input"],
                |id| {
                    trace
                        .borrow_mut()
                        .push(format!("select:{}", id.unwrap().as_str().unwrap()));
                    Ok(vec!["ETH", "BTC"])
                },
                loaded,
            ),
            _ => unreachable!(),
        };
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "loaded/select/write source order {}",
            case["input"]
        );
        if case["error"].is_null() {
            result.unwrap();
            let tickers = store.tickers().unwrap();
            assert_eq!(tickers.len(), 2, "TICKER-CARDINALITY native stored records");
            let fields: &[&str] = match case["method"].as_str().unwrap() {
                "handleTickerEvents" => {
                    &["bid", "ask", "last", "percentage", "volume", "quoteVolume"]
                }
                "handleFundingRateEvents" => &["fundingRate", "fundingTime"],
                "handleOpenInterestEvents" => &["openInterest"],
                "handleMarkPriceEvents" => &["mark"],
                "handleIndexTickerEvents" => &["index"],
                _ => unreachable!(),
            };
            let writes = if case["method"] == "handleIndexTickerEvents" {
                2
            } else {
                1
            };
            assert_eq!(
                case["calls"].as_array().unwrap().len(),
                writes,
                "TICKER-CARDINALITY source per-case writes"
            );
            assert_eq!(
                count.get(),
                writes,
                "TICKER-CARDINALITY native per-case writes"
            );
            native_writes += count.get();
            for operation_index in 0..writes {
                let operation = &case["calls"][operation_index];
                let ticker = tickers
                    .iter()
                    .find(|ticker| ticker.id == operation["id"].as_str().unwrap())
                    .unwrap();
                if !bypass_field_counts {
                    assert_eq!(
                        operation["patch"].as_object().unwrap().len(),
                        fields.len(),
                        "TICKER-CARDINALITY source per-write fields"
                    );
                }
                for &key in fields {
                    let bits = operation["patch"][key]
                        .as_str()
                        .expect("TICKER-CARDINALITY expected field identity");
                    let n = match key {
                        "openInterest" => ticker.open_interest,
                        "fundingRate" => ticker.funding_rate,
                        "fundingTime" => ticker.funding_time,
                        "mark" => ticker.mark,
                        "index" => ticker.index,
                        "bid" => ticker.bid,
                        "ask" => ticker.ask,
                        "last" => ticker.last,
                        "percentage" => ticker.percentage,
                        "volume" => ticker.volume,
                        "quoteVolume" => ticker.quote_volume,
                        _ => unreachable!(),
                    };
                    assert!(
                        captured_equal(n, bits),
                        "source ticker numeric bits {key}/{}",
                        case["input"]
                    );
                    native_comparisons += 1;
                }
            }
            assert_eq!(
                count.get(),
                case["calls"].as_array().unwrap().len(),
                "one notification per source write"
            );
        } else {
            assert_eq!(result.unwrap_err().name, case["error"].as_str().unwrap());
            assert_eq!(count.get(), 0, "coercion throw occurs before write");
        }
        unsubscribe();
    }
    assert_eq!(native_writes, 216, "TICKER-CARDINALITY native write total");
    assert_eq!(
        native_comparisons, 792,
        "TICKER-CARDINALITY native comparison total"
    );
    for (mutation, expected) in [
        ("--mutate-source", "source oiCcy field"),
        ("--mutate-volume", "source volume currency units"),
        ("--mutate-order", "source mark loaded before write"),
    ] {
        let child = std::process::Command::new("node")
            .arg(path)
            .arg(mutation)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&child.stderr).contains(expected),
            "source mutation failed for wrong reason: {}",
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

#[test]
fn notifying_ticker_writers_preserve_original_error_and_stop_after_commit() {
    for method in ["ticker", "interest", "funding", "mark", "index"] {
        let store = DefaultStore::new();
        let ticker: Ticker = serde_json::from_value(json!({"id":"BTC","symbol":"BTCUSDT","bid":1,"ask":1,"last":1,"mark":1,"index":1,"percentage":1,"openInterest":1,"fundingRate":1,"fundingTime":1,"volume":1,"quoteVolume":1})).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut expected = safe_cex_exchange::runtime::ExchangeError::new("listener rejection");
        expected.name = "TypeError".into();
        expected.store_subscriber_ids = vec![901];
        expected = expected.with_retained_response(json!({"subscriber":"original cause"}).into());
        let original = expected.clone();
        let unsubscribe =
            Store::subscribe(&store, Box::new(move |_, _| Err(original.clone().into())));
        let later = Rc::new(Cell::new(0));
        let count = later.clone();
        let unsubscribe_later = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                count.set(count.get() + 1);
                Ok(())
            }),
        );
        let input = json!({"data":[{"instId":"BTC","oiCcy":"9","fundingRate":"9","fundingTime":"10","markPx":"9","idxPx":"9","open24h":"3","last":"9","bidPx":"9","askPx":"10","volCcy24h":"11","vol24h":"12"}]});
        let result = match method {
            "ticker" => handle_ticker_events(&store, &input),
            "interest" => handle_open_interest_events(&store, &input),
            "funding" => handle_funding_rate_events(&store, &input),
            "mark" => handle_mark_price_events(&store, &input, |_| Ok(())),
            "index" => handle_index_ticker_events(
                &store,
                &input,
                |_| Ok(vec!["BTC", "unreached"]),
                |_| Ok(()),
            ),
            _ => unreachable!(),
        };
        let mut error = result.unwrap_err();
        let mut probe = safe_cex_store::model_patch::ModelPatch::default();
        probe.set_number("bid", 17.0);
        let notification = match Store::update_ticker(&store, "BTC", &probe).unwrap_err() {
            safe_cex_store::StoreMutationError::Notification(notification) => notification,
            other => panic!("expected subscriber failure: {other:?}"),
        };
        assert_eq!(
            error.store_subscriber_ids,
            [901, notification.subscriber_id()],
            "notification subscriber identities inner to outer {method}"
        );
        error.store_subscriber_ids = expected.store_subscriber_ids.clone();
        // The recovered error retains the opaque notification cause it came through.
        assert!(
            error.store_subscriber_cause.take().is_some(),
            "notification retains its Store cause {method}"
        );
        assert_eq!(
            error, expected,
            "notification original error identity {method}"
        );
        assert_eq!(
            later.get(),
            0,
            "notification later subscriber stop {method}"
        );
        let ticker = Store::tickers(&store)
            .unwrap()
            .iter()
            .find(|ticker| ticker.id == "BTC")
            .unwrap()
            .clone();
        let value = match method {
            "ticker" => ticker.last,
            "interest" => ticker.open_interest,
            "funding" => ticker.funding_rate,
            "mark" => ticker.mark,
            "index" => ticker.index,
            _ => unreachable!(),
        };
        assert_eq!(value, 9.0, "notification commit before rejection {method}");
        unsubscribe();
        unsubscribe_later();
    }
}

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn cardinality_controls_reject_empty_partial_and_count_bypass() {
    for (mode, assertion) in [
        ("drop-fields", "TICKER-CARDINALITY source field total"),
        ("drop-one-field", "TICKER-CARDINALITY source field total"),
        (
            "bypass-field-counts",
            "TICKER-CARDINALITY expected field identity",
        ),
        (
            "drop-handler",
            "SUBCORPUS-CARDINALITY ticker_events profiles",
        ),
    ] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "maintained_typescript_first_ticker_update_numeric_bits",
                "--include-ignored",
            ])
            .env("OKX_CARDINALITY_CONTROL", mode)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(101), "cardinality control {mode}");
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(
            output.contains(assertion),
            "cardinality control wrong assertion {mode}: {output}"
        );
    }
}
