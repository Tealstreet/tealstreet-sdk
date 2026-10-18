use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::StoreSubscriberError;
use safe_cex_venue_mexc::{
    utils::{normalize_market, normalize_ticker},
    ws::subscription_key,
    ws_private::{
        MexcFillEvent, MexcPrivateHandlers, private_feed_filter_payload, private_ping_payload,
    },
    ws_public::{handle_depth_frame, handle_ticker_frame, kline_handler_key},
};
use serde_json::{Map, Value, json};
use std::{cell::RefCell, rc::Rc};

#[test]
fn mexc_subscription_key_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ws_oracle.mjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC WS oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 5, "subscription key case census");
    for case in expected {
        let input = &case["input"];
        let method = input["method"].as_str().unwrap();
        let actual = subscription_key(method, input.get("param")).unwrap();
        assert_eq!(actual, case["output"].as_str().unwrap(), "{input}");
    }
}

#[test]
fn mexc_subscription_key_typescript_mutation_is_detected_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_subscription_key_matches_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_WS_TS_CONTROL", "subscription-param")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_subscription_key_matches_live_typescript ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_subscription_key_omits_absent_parameter_but_keeps_null() {
    assert_eq!(
        subscription_key("ping", None).unwrap(),
        r#"{"method":"ping"}"#
    );
    assert_eq!(
        subscription_key("method", Some(&json!(null))).unwrap(),
        r#"{"method":"method","param":null}"#
    );
}

#[test]
fn mexc_private_feed_subscription_includes_every_source_filter() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_filter_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private filter oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(private_feed_filter_payload(), expected);
}

#[test]
fn mexc_private_feed_subscription_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_feed_subscription_includes_every_source_filter",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_FILTER_CONTROL", "remove-asset")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_private_feed_subscription_includes_every_source_filter ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_private_ping_matches_source_frame() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_ping_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private ping oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(private_ping_payload(), expected);
}

#[test]
fn mexc_private_ping_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_ping_matches_source_frame",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_PING_CONTROL", "wrong-method")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_private_ping_matches_source_frame ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_public_ticker_handler_patches_only_source_fields() {
    let store = DefaultStore::new();
    let market_raw: Map<String, Value> = serde_json::from_value(json!({
        "symbol":"BTC_USDT", "baseCoin":"BTC", "quoteCoin":"USDT",
        "contractSize":0.0001, "volUnit":2, "priceUnit":0.1,
        "minVol":3, "maxVol":100000, "minLeverage":4, "maxLeverage":125
    }))
    .unwrap();
    let market =
        normalize_market(&serde_json::Value::Object((&market_raw).clone()).into()).unwrap();
    let local_to_utc = |value| value;
    let initial_raw: Map<String, Value> = serde_json::from_value(json!({
        "lastPrice":"10", "bid1":"9", "ask1":"11", "symbol":"BTC_USDT"
    }))
    .unwrap();
    let initial = normalize_ticker(&initial_raw, &market, &local_to_utc).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![initial]),
            ..StoreUpdate::default()
        },
        false,
    )
    .unwrap();
    let frame = json!({
        "symbol":"BTC_USDT",
        "data":{"lastPrice":"12","bid1":"11","ask1":"13","fairPrice":"12.5","indexPrice":"12.25","riseFallRate":"0.1","holdVol":"2","fundingRate":"0.001","fundingTime":1700000000000_i64,"volume24":"3","amount24":"36"}
    });
    assert!(handle_ticker_frame(&store, &frame, &local_to_utc).unwrap());
    let tickers = store.tickers_by_symbol();
    let ticker = &tickers["BTCUSDT"];
    assert_eq!(ticker.last, 12.0);
    assert_eq!(ticker.bid, 11.0);
    assert_eq!(ticker.ask, 13.0);
    assert_eq!(ticker.mark, 12.5);
    assert_eq!(ticker.index, Some(12.25));
    assert_eq!(ticker.percentage, 10.0);
    assert_eq!(ticker.open_interest, 0.0025);
    assert_eq!(ticker.funding_rate, 0.001);
    assert_eq!(ticker.volume.to_bits(), (3.0_f64 * 0.0001).to_bits());
    assert_eq!(ticker.quote_volume, 36.0, "ticker field bits quoteVolume");
    assert_eq!(ticker.funding_time, 1_700_000_000_000.0);
    assert!(
        !handle_ticker_frame(
            &store,
            &json!({"symbol":"NO_MARKET","data":{}}),
            &local_to_utc
        )
        .unwrap()
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_ticker_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC ticker source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual = json!({
        "bid":ticker.bid, "ask":ticker.ask, "last":ticker.last, "mark":ticker.mark,
        "index":ticker.index, "percentage":ticker.percentage, "openInterest":ticker.open_interest,
        "fundingRate":ticker.funding_rate, "fundingTime":ticker.funding_time,
        "volume":ticker.volume, "quoteVolume":ticker.quote_volume
    });
    for (key, value) in actual.as_object().unwrap() {
        assert_eq!(
            format!("{:016x}", value.as_f64().unwrap().to_bits()),
            expected["fields"][key].as_str().unwrap(),
            "ticker field bits {key}"
        );
    }
}

#[test]
fn mexc_public_ticker_handler_typescript_mutation_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_public_ticker_handler_patches_only_source_fields",
            "--nocapture",
        ])
        .env("MEXC_WS_TS_CONTROL", "ticker-volume")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_public_ticker_handler_patches_only_source_fields ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_public_depth_handler_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_depth_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC depth source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5, "public depth handler corpus census");

    let store = DefaultStore::new();
    let market_raw: Map<String, Value> = serde_json::from_value(json!({
        "symbol":"BTC_USDT", "baseCoin":"BTC", "quoteCoin":"USDT",
        "contractSize":0.0001, "volUnit":2, "priceUnit":0.1,
        "minVol":3, "maxVol":100000, "minLeverage":4, "maxLeverage":125
    }))
    .unwrap();
    let market =
        normalize_market(&serde_json::Value::Object((&market_raw).clone()).into()).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            ..StoreUpdate::default()
        },
        false,
    )
    .unwrap();

    for case in cases {
        let actual = handle_depth_frame(&store, &case["frame"]).unwrap().map(
            |(symbol, handler_key, book)| {
                let encode_side = |side: &[safe_cex_types::OrderBookOrders]| {
                    Value::Array(
                        side.iter()
                            .map(|level| {
                                json!([
                                    format!("{:016x}", level.price.to_bits()),
                                    format!("{:016x}", level.amount.to_bits()),
                                    format!("{:016x}", level.total.to_bits())
                                ])
                            })
                            .collect(),
                    )
                };
                json!({
                    "symbol":symbol,
                    "route":handler_key,
                    "book":{"bids":encode_side(&book.bids),"asks":encode_side(&book.asks)}
                })
            },
        );
        assert_eq!(
            actual.unwrap_or(Value::Null),
            case["output"],
            "depth frame {}",
            case["frame"]
        );
    }
}

#[test]
fn mexc_public_depth_handler_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_public_depth_handler_matches_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_WS_DEPTH_TS_CONTROL", "skip-normalizer")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_public_depth_handler_matches_live_typescript ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_public_kline_handler_key_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_kline_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC kline source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 6, "public kline handler corpus census");
    for case in cases {
        assert_eq!(
            kline_handler_key(&case["frame"]).unwrap(),
            case["key"].as_str().unwrap(),
            "kline frame {}",
            case["frame"]
        );
    }
}

#[test]
fn mexc_public_kline_handler_key_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_public_kline_handler_key_matches_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_WS_KLINE_TS_CONTROL", "nullish")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_public_kline_handler_key_matches_live_typescript ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_private_order_handler_matches_source_order_and_rejection_hook() {
    let store = DefaultStore::new();
    let rejected = Rc::new(RefCell::new(Vec::new()));
    let rejected_hook = rejected.clone();
    let local_to_utc = Rc::new(|value| value);
    let handlers = MexcPrivateHandlers::new(
        local_to_utc,
        None,
        Rc::new(move |id, code| {
            rejected_hook.borrow_mut().push(json!([id, code]));
            Ok(())
        }),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        Rc::new(|_| Ok(())),
    );
    handlers
        .handle_order(
            &store,
            &json!({"data":[{"orderId":"123","symbol":"BTC_USDT","side":1,"state":4,"errorCode":510,"dealVol":0,"vol":2,"price":10}]}),
        )
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_order_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private order source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let orders = store.orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "123");
    assert_eq!(orders[0].status, safe_cex_types::OrderStatus::Canceled);
    let actual_rejected = rejected.borrow();
    assert_eq!(actual_rejected.len(), 1);
    assert_eq!(actual_rejected[0][0], expected["rejected"][0][0]);
    assert_eq!(
        actual_rejected[0][1].as_f64(),
        expected["rejected"][0][1].as_f64()
    );
    assert_eq!(
        expected["events"],
        json!([["orders", "123"], ["status", "123", "canceled"]])
    );
}

#[test]
fn mexc_private_order_handler_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_order_handler_matches_source_order_and_rejection_hook",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_ORDER_CONTROL", "rejection-state")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains(
            "mexc_private_order_handler_matches_source_order_and_rejection_hook ... FAILED"
        ),
        "{output}"
    );
}

#[test]
fn mexc_private_fill_handler_emits_normalized_fill_event() {
    let events = Rc::new(RefCell::new(Vec::<MexcFillEvent>::new()));
    let event_sink = events.clone();
    let handlers = MexcPrivateHandlers::new(
        Rc::new(|value| value),
        None,
        Rc::new(|_, _| Ok(())),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        Rc::new(move |event| {
            event_sink.borrow_mut().push(event);
            Ok(())
        }),
    );
    let frame = json!({"data":[{
        "tradeId":"fill-1", "orderId":"order-1", "symbol":"BTC_USDT",
        "side":1, "price":"12.5", "vol":"2", "fee":"0.25",
        "feeCurrency":"USDT", "profit":"1.5", "createTime":1700000000000_i64,
        "role":2
    }]});
    handlers.handle_fill(&DefaultStore::new(), &frame).unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_fill_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private fill source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual = events.borrow();
    assert_eq!(actual.len(), 1);
    let actual = &actual[0];
    let expected = &expected[0];
    assert_eq!(actual.id, "fill-1", "FILL native id");
    assert_eq!(actual.order_id, "order-1", "FILL native orderId");
    assert_eq!(actual.symbol, "BTCUSDT", "FILL native symbol");
    assert_eq!(
        actual.side,
        safe_cex_types::OrderSide::Buy,
        "FILL native side"
    );
    for (key, value, number) in [
        ("price", actual.price, 12.5f64),
        ("amount", actual.amount, 2.0),
        ("fee", actual.fee, 0.25),
        ("realizedPnl", actual.realized_pnl, 1.5),
    ] {
        assert_eq!(
            value.to_bits(),
            number.to_bits(),
            "FILL native numeric {key}"
        );
    }
    for (key, value) in [
        ("id", actual.id.as_str()),
        ("orderId", actual.order_id.as_str()),
        ("symbol", actual.symbol.as_str()),
        ("side", "buy"),
    ] {
        assert_eq!(Some(value), expected[key].as_str(), "fill field {key}");
    }
    for (key, value) in [
        ("price", actual.price),
        ("amount", actual.amount),
        ("fee", actual.fee),
        ("realizedPnl", actual.realized_pnl),
    ] {
        assert_eq!(
            Some(value),
            expected[key].as_f64(),
            "fill numeric field {key}"
        );
    }
}

#[test]
fn mexc_private_fill_handler_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_fill_handler_emits_normalized_fill_event",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_FILL_CONTROL", "fee-value")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_private_fill_handler_emits_normalized_fill_event ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_private_fill_missing_fields_match_native_source_values() {
    let events = Rc::new(RefCell::new(Vec::<MexcFillEvent>::new()));
    let event_sink = events.clone();
    let handlers = MexcPrivateHandlers::new(
        Rc::new(|value| value),
        None,
        Rc::new(|_, _| Ok(())),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        Rc::new(move |event| {
            event_sink.borrow_mut().push(event);
            Ok(())
        }),
    );
    handlers
        .handle_fill(
            &DefaultStore::new(),
            &json!({"data":[{"tradeId":"fill-2","symbol":"BTC_USDT","side":1,"price":12.5,"vol":2,"createTime":1700000000000_i64}]}),
        )
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_fill_missing_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private fill missing-field oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let actual = &events.borrow()[0];
    for key in ["orderId", "fee", "realizedPnl"] {
        assert_eq!(expected["ownProperties"][key], true);
    }
    assert_eq!(actual.order_id, "", "FILL native retains empty orderId");
    assert_eq!(actual.fee, 0.0, "FILL native missing fee is zero");
    assert_eq!(actual.realized_pnl, 0.0, "FILL native missing pnl is zero");
    assert_eq!(expected["serialized"]["orderId"], "");
    assert_eq!(expected["serialized"]["fee"], 0);
    assert_eq!(expected["serialized"]["realizedPnl"], 0);
}

#[test]
fn mexc_private_plan_and_stop_handlers_preserve_seen_and_cancel_shields() {
    let store = DefaultStore::new();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen_hook = seen.clone();
    let handlers = MexcPrivateHandlers::new(
        Rc::new(|value| value),
        None,
        Rc::new(|_, _| Ok(())),
        Rc::new(move |id| seen_hook.borrow_mut().push(id)),
        Rc::new(|id| id == "cancel-1"),
        Rc::new(|_| Ok(())),
    );
    handlers
        .handle_plan_order(
            &store,
            &json!({"data":[
                {"id":"plan-1","symbol":"BTC_USDT","side":2,"state":1,"vol":3,"price":10,"triggerPrice":9,"triggerType":4},
                {"id":"cancel-1","symbol":"BTC_USDT","side":2,"state":1,"vol":3,"price":10,"triggerPrice":9,"triggerType":4}
            ]}),
        )
        .unwrap();
    handlers
        .handle_stop_order(
            &store,
            &json!({"data":[{"id":"stop-1","symbol":"BTC_USDT","side":0,"state":1,"vol":4,"stopLossPrice":9,"positionType":2,"triggerSide":3}]}),
        )
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_stop_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private plan/stop source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected_seen: Vec<String> = expected["seen"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(*seen.borrow(), expected_seen);
    let orders = store.orders();
    let mut actual_ids: Vec<_> = orders.iter().map(|order| order.id.clone()).collect();
    actual_ids.sort();
    let mut expected_ids: Vec<_> = expected["orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    expected_ids.sort();
    assert_eq!(actual_ids, expected_ids);
}

#[test]
fn mexc_private_plan_stop_detect_typescript_shield_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_plan_and_stop_handlers_preserve_seen_and_cancel_shields",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_STOP_CONTROL", "disable-cancel-shield")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains(
            "mexc_private_plan_and_stop_handlers_preserve_seen_and_cancel_shields ... FAILED"
        ),
        "{output}"
    );
}

#[test]
fn mexc_private_position_handler_matches_normalized_fields() {
    let store = DefaultStore::new();
    let handlers = MexcPrivateHandlers::new(
        Rc::new(|value| value),
        None,
        Rc::new(|_, _| Ok(())),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        Rc::new(|_| Ok(())),
    );
    handlers
        .handle_position(
            &store,
            &json!({"data":[{
                "symbol":"BTC_USDT", "positionType":2, "holdVol":"3",
                "holdAvgPrice":"12.5", "unrealized":"1.25", "realised":"0.5",
                "liquidatePrice":"8", "im":"2", "leverage":"10",
                "openType":1, "updateTime":1700000000000_i64
            }]}),
        )
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_position_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC private position source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let positions = store.positions();
    assert_eq!(positions.len(), 1);
    let actual = &positions[0];
    assert_eq!(actual.id, expected["id"].as_str().unwrap());
    assert_eq!(actual.symbol, expected["symbol"].as_str().unwrap());
    assert_eq!(serde_json::to_value(actual.side).unwrap(), expected["side"]);
    for (key, value) in [
        ("entryPrice", actual.entry_price),
        ("notional", actual.notional),
        ("leverage", actual.leverage),
        ("unrealizedPnl", actual.unrealized_pnl),
        ("realizedPnl", actual.realized_pnl.unwrap()),
        ("contracts", actual.contracts),
        ("liquidationPrice", actual.liquidation_price),
        ("marginSize", actual.margin_size.unwrap()),
        ("lastUpdate", actual.last_update.unwrap()),
    ] {
        assert_eq!(
            value.to_bits(),
            u64::from_str_radix(expected["numericBits"][key].as_str().unwrap(), 16).unwrap(),
            "position field bits {key}"
        );
    }
}

#[test]
fn mexc_websocket_store_notifications_propagate_after_commit() {
    let store = DefaultStore::new();
    let market_raw: Map<String, Value> = serde_json::from_value(json!({
        "symbol":"BTC_USDT", "baseCoin":"BTC", "quoteCoin":"USDT",
        "contractSize":1, "volUnit":2, "priceUnit":0.1,
        "minVol":1, "maxVol":100000, "minLeverage":1, "maxLeverage":100
    }))
    .unwrap();
    let market =
        normalize_market(&serde_json::Value::Object((&market_raw).clone()).into()).unwrap();
    let initial = normalize_ticker(
        &serde_json::from_value(
            json!({"lastPrice":"10","bid1":"9","ask1":"11","symbol":"BTC_USDT"}),
        )
        .unwrap(),
        &market,
        &|value| value,
    )
    .unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![initial]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = calls.clone();
    let _first = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            first_calls.borrow_mut().push("first");
            Err(StoreSubscriberError::new("subscriber failed"))
        }),
    );
    let second_calls = calls.clone();
    let _second = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            second_calls.borrow_mut().push("second");
            Ok(())
        }),
    );
    let error = handle_ticker_frame(
        &store,
        &json!({"symbol":"BTC_USDT","data":{"lastPrice":"12","bid1":"11","ask1":"13"}}),
        &|value| value,
    )
    .unwrap_err();
    assert_eq!(error.message, "subscriber failed");
    assert_eq!(
        error.store_subscriber_ids,
        [0],
        "ticker failure retains Store-local subscriber identity"
    );
    assert_eq!(store.tickers_by_symbol()["BTCUSDT"].bid, 11.0);
    assert_eq!(*calls.borrow(), ["first"]);

    let position_store = DefaultStore::new();
    let position_calls = Rc::new(RefCell::new(Vec::new()));
    let first_position_calls = position_calls.clone();
    let _first = Store::subscribe(
        &position_store,
        Box::new(move |_, _| {
            first_position_calls.borrow_mut().push("first");
            Err(StoreSubscriberError::new("position subscriber failed"))
        }),
    );
    let second_position_calls = position_calls.clone();
    let _second = Store::subscribe(
        &position_store,
        Box::new(move |_, _| {
            second_position_calls.borrow_mut().push("second");
            Ok(())
        }),
    );
    let handlers = MexcPrivateHandlers::new(
        Rc::new(|value| value),
        None,
        Rc::new(|_, _| Ok(())),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        Rc::new(|_| Ok(())),
    );
    let error = handlers
        .handle_position(
            &position_store,
            &json!({"data":[{"symbol":"BTC_USDT","positionType":2,"holdVol":"3","holdAvgPrice":"12.5","unrealized":"1.25","realised":"0.5","liquidatePrice":"8","im":"2","leverage":"10","openType":1,"updateTime":1700000000000_i64}]}),
        )
        .unwrap_err();
    assert_eq!(error.message, "position subscriber failed");
    assert_eq!(
        error.store_subscriber_ids,
        [0],
        "position failure retains Store-local subscriber identity"
    );
    assert_eq!(position_store.positions().len(), 1);
    assert_eq!(*position_calls.borrow(), ["first"]);
}

#[test]
fn mexc_private_position_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_private_position_handler_matches_normalized_fields",
            "--nocapture",
        ])
        .env("MEXC_WS_PRIVATE_POSITION_CONTROL", "entry-price")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_private_position_handler_matches_normalized_fields ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_private_fill_boundary_native_fields_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_private_fill_boundary_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = report["rows"].as_array().unwrap();
    let labels = [
        "missing-order-id",
        "fallback-number",
        "amount-overflow",
        "ignored-asset",
        "populated-sell",
        "contract-only",
        "lone-symbol",
        "coercion-error",
        "null-row",
        "null-frame",
        "emitter-error",
        "skip-zero",
    ];
    assert_eq!(rows.len(), 12, "FILL independent boundary case floor");
    assert_eq!(
        report["total"], 9,
        "FILL independent Source emit-call count"
    );
    let mut native_calls = 0;
    for (row, label) in rows.iter().zip(labels) {
        assert_eq!(row["label"], label);
        let input = row["inputJson"].as_str().unwrap();
        if label == "lone-symbol" {
            assert_eq!(
                row["events"][0]["symbolUnits"],
                json!([0xd800]),
                "FILL materialized Source symbol units"
            );
            assert!(
                serde_json::from_str::<Value>(input).is_err(),
                "FILL native whole input refuses lone symbol unit"
            );
            continue;
        }
        let frame: Value = serde_json::from_str(input).unwrap();
        if label == "ignored-asset" {
            let error = safe_cex_venue_mexc::utils::normalize_execution(
                frame["data"][0].as_object().unwrap(),
                None,
                &|value| value,
            )
            .unwrap_err();
            assert_eq!(
                error.name, "TypeError",
                "FILL public Execution boundary remains closed"
            );
            assert_eq!(
                error.message,
                "MEXC commissionAsset is outside the shared Execution string boundary"
            );
        }
        let store = DefaultStore::new();
        if let Some(size) = row["contractSize"].as_f64() {
            let raw: serde_json::Map<String, Value> = serde_json::from_value(json!({"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","contractSize":1,"volUnit":2,"priceUnit":0.1,"minVol":1,"maxVol":1000,"minLeverage":1,"maxLeverage":100})).unwrap();
            let mut market =
                normalize_market(&serde_json::Value::Object((&raw).clone()).into()).unwrap();
            market.contract_size = Some(size);
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(vec![market]),
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        }
        let events = Rc::new(RefCell::new(Vec::<MexcFillEvent>::new()));
        let sink = events.clone();
        let mut failure = safe_cex_exchange::runtime::ExchangeError::new("fill listener failed");
        failure.name = "TypeError".into();
        failure.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(197)));
        let thrown = failure.clone();
        let handlers = MexcPrivateHandlers::new(
            Rc::new(|value| value),
            None,
            Rc::new(|_, _| Ok(())),
            Rc::new(|_| {}),
            Rc::new(|_| false),
            Rc::new(move |event| {
                sink.borrow_mut().push(event);
                if label == "emitter-error" {
                    Err(thrown.clone())
                } else {
                    Ok(())
                }
            }),
        );
        let result = handlers.handle_fill(&store, &frame);
        if ["coercion-error", "null-row", "null-frame", "emitter-error"].contains(&label) {
            let error = result.unwrap_err();
            assert_eq!(
                error.name,
                row["error"]["name"].as_str().unwrap(),
                "FILL native error name {label}"
            );
            assert_eq!(
                error.message,
                row["error"]["message"].as_str().unwrap(),
                "FILL native error message {label}"
            );
            if label == "emitter-error" {
                assert_eq!(error, failure, "FILL native emitter failure unchanged")
            }
        } else {
            result.unwrap();
        }
        let events = events.borrow();
        let expected = row["events"].as_array().unwrap();
        assert_eq!(
            events.len(),
            if ["coercion-error", "null-row", "null-frame"].contains(&label) {
                0
            } else {
                1
            },
            "FILL native independent emit count {label}"
        );
        assert_eq!(events.len(), expected.len());
        native_calls += events.len();
        for (event, source) in events.iter().zip(expected) {
            assert_eq!(
                event.id.encode_utf16().collect::<Vec<_>>(),
                serde_json::from_value::<Vec<u16>>(source["idUnits"].clone()).unwrap(),
                "FILL native id {label}"
            );
            assert_eq!(
                event.order_id,
                if label == "populated-sell" {
                    "sell-order"
                } else {
                    ""
                },
                "FILL native orderId {label}"
            );
            assert_eq!(
                event.order_id.encode_utf16().collect::<Vec<_>>(),
                serde_json::from_value::<Vec<u16>>(source["orderIdUnits"].clone()).unwrap()
            );
            assert_eq!(event.symbol, "BTCUSDT", "FILL native symbol {label}");
            assert_eq!(
                event.symbol.encode_utf16().collect::<Vec<_>>(),
                serde_json::from_value::<Vec<u16>>(source["symbolUnits"].clone()).unwrap()
            );
            assert_eq!(
                event.side,
                if label == "populated-sell" {
                    safe_cex_types::OrderSide::Sell
                } else {
                    safe_cex_types::OrderSide::Buy
                },
                "FILL native side {label}"
            );
            assert_eq!(
                source["fields"].as_array().unwrap().len(),
                4,
                "FILL independent Source numeric field count"
            );
            for ((key, value), field) in [
                ("price", event.price),
                ("amount", event.amount),
                ("fee", event.fee),
                ("realizedPnl", event.realized_pnl),
            ]
            .into_iter()
            .zip(source["fields"].as_array().unwrap())
            {
                assert_eq!(field["key"], key);
                assert_eq!(value.is_nan(), field["nan"].as_bool().unwrap());
                if !value.is_nan() {
                    assert_eq!(
                        format!("{:016x}", value.to_bits()),
                        field["bits"].as_str().unwrap(),
                        "FILL native numeric bits {label}.{key}"
                    )
                }
            }
            if label == "amount-overflow" {
                assert_eq!(
                    event.amount,
                    f64::INFINITY,
                    "FILL native emitter retains Infinity"
                );
            }
            // Secondary encoding follows native id/enum/number/error assertions.
            let wire = serde_json::to_value(event).unwrap();
            assert_eq!(
                wire.as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                [
                    "id",
                    "orderId",
                    "symbol",
                    "side",
                    "price",
                    "amount",
                    "fee",
                    "realizedPnl"
                ],
                "FILL secondary own-key order"
            );
            let wire = safe_cex_num::js_json_stringify(&wire.into())
                .unwrap()
                .unwrap();
            assert_eq!(
                wire,
                source["wire"].as_str().unwrap(),
                "FILL secondary event wire {label}"
            );
        }
    }
    assert_eq!(
        native_calls, 8,
        "FILL independent native emit-call count; lone-symbol refused before method"
    );
}
