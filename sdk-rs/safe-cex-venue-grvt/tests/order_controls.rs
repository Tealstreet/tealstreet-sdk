#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::*;
use safe_cex_venue_grvt::{
    builder::GrvtBuilderUnauthorizedError, order_controls::*, types::GrvtBuilderUnauthorizedReason,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("CONTROLS unexpectedly pending"),
    }
}
fn corpus() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-controls-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CONTROLS source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn order(id: &str) -> Order {
    serde_json::from_value(json!({"id":id,"status":"open","symbol":"BTC_USDT_PERP","type":"limit","side":"buy","price":13.25,"amount":6.5,"filled":0.5,"remaining":6,"reduceOnly":true,"lastUpdate":1009,"timeInForce":"ImmediateOrCancel"})).unwrap()
}
fn rejection(name: &str) -> ExchangeError {
    let mut error = ExchangeError::new("original");
    error.name = "TypeError".into();
    error.context = Some(json!({"original":true}));
    if matches!(name, "sdk-error" | "emit-error") {
        error = error.with_retained_response(
            (json!({"data":{"message":"full message","m":"lite message","code":17,"c":9}})).into(),
        );
    }
    if name == "lite-error" {
        error = error.with_retained_response(
            (json!({"data":{"message":"","m":"lite message","code":0,"c":3}})).into(),
        );
    }
    error
}
fn assert_error(error: Option<&ExchangeError>, expected: &Value, assertion: &str) {
    match error {
        None => assert!(expected.is_null(), "{assertion} unexpected success"),
        Some(error) => {
            assert_eq!(
                error.name,
                expected["name"].as_str().unwrap(),
                "{assertion} native name"
            );
            assert_eq!(
                error.message,
                expected["message"].as_str().unwrap(),
                "{assertion} native message"
            );
        }
    }
}

fn cancellations(method: &str) {
    let corpus = corpus();
    let rows = corpus["cancelRows"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "CANCEL independent total floor");
    let names = [
        "ok",
        "fallback",
        "no-client",
        "no-account",
        "endpoint-error",
        "sdk-error",
        "lite-error",
        "log-error",
        "emit-error",
    ];
    let mut visited = 0;
    for (index, row) in rows
        .iter()
        .filter(|row| row["method"] == method)
        .enumerate()
    {
        let name = names[index];
        assert_eq!(row["name"], name, "CANCEL input order");
        let effects = RefCell::new(Vec::new());
        let payloads = RefCell::new(Vec::new());
        let account = || Some(if name == "no-account" { "" } else { "sub-72" }.to_owned());
        let auth = || {
            effects.borrow_mut().push(json!(["endpoint"]));
            std::future::ready(if name == "endpoint-error" {
                Err(rejection(name))
            } else {
                Ok(json!({"token":"endpoint-31"}))
            })
        };
        let sdk = |payload: Value, endpoint: Value| {
            payloads
                .borrow_mut()
                .push((payload.clone(), endpoint.clone()));
            effects.borrow_mut().push(json!(["sdk", payload, endpoint]));
            std::future::ready(
                if matches!(name, "sdk-error" | "lite-error" | "emit-error") {
                    Err(rejection(name))
                } else {
                    Ok(())
                },
            )
        };
        let log = |message: &str, severity: LogSeverity| {
            effects
                .borrow_mut()
                .push(json!(["log", message, severity.as_str()]));
            if name == "log-error" {
                Err(ExchangeError::new("log-fail"))
            } else {
                Ok(())
            }
        };
        let emit = |message: &str| {
            effects.borrow_mut().push(json!(["emit", message]));
            if name == "emit-error" {
                Err(ExchangeError::new("emit-fail"))
            } else {
                Ok(())
            }
        };
        let current = order("nonce-17");
        let future: Pin<Box<dyn Future<Output = Result<(), ExchangeError>> + '_>> = match method {
            "cancelOrder" => Box::pin(cancel_order(
                &current,
                name != "no-client",
                account,
                Some(if name == "fallback" { "" } else { "0xAB" }),
                auth,
                sdk,
                log,
                emit,
            )),
            "cancelAllOrders" => Box::pin(cancel_all_orders(
                name != "no-client",
                account,
                auth,
                sdk,
                log,
                emit,
            )),
            "cancelSymbolOrders" => Box::pin(cancel_symbol_orders(
                "BTC_USDT_PERP",
                name != "no-client",
                account,
                auth,
                sdk,
                log,
                emit,
            )),
            _ => unreachable!(),
        };
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "CANCEL native eager effects"
        );
        let result = run(future);
        let expected_calls = usize::from(
            !matches!(name, "no-client" | "no-account" | "endpoint-error")
                && !(method == "cancelOrder" && name == "log-error"),
        );
        let payloads = payloads.borrow();
        assert_eq!(
            payloads.len(),
            expected_calls,
            "CANCEL independent SDK call count"
        );
        for (payload, endpoint) in payloads.iter() {
            assert_eq!(
                payload["sub_account_id"].as_str(),
                Some("sub-72"),
                "{}",
                if method == "cancelAllOrders" {
                    "CANCEL all native account"
                } else {
                    "CANCEL native account"
                }
            );
            assert_eq!(
                endpoint["token"].as_str(),
                Some("endpoint-31"),
                "CANCEL native endpoint"
            );
            if method == "cancelOrder" {
                let key = if name == "fallback" {
                    "client_order_id"
                } else {
                    "order_id"
                };
                let id = if name == "fallback" {
                    "nonce-17"
                } else {
                    "0xAB"
                };
                assert_eq!(
                    payload.get(key).and_then(Value::as_str),
                    Some(id),
                    "CANCEL single native payload key"
                );
                assert_eq!(
                    payload.as_object().unwrap().len(),
                    2,
                    "CANCEL single payload key count"
                );
            } else if method == "cancelSymbolOrders" {
                assert_eq!(
                    payload["instrument_id"].as_str(),
                    Some("BTC_USDT_Perp"),
                    "CANCEL symbol native instrument"
                );
                assert_eq!(
                    payload.as_object().unwrap().len(),
                    2,
                    "CANCEL symbol payload key count"
                );
            } else {
                assert_eq!(
                    payload.as_object().unwrap().len(),
                    1,
                    "CANCEL all payload key count"
                );
            }
        }
        assert_error(result.as_ref().err(), &row["error"], "CANCEL");
        if let Err(error) = result {
            assert!(
                error.response_json_view().is_none() && error.context.is_none(),
                "CANCEL fresh error metadata"
            );
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "CANCEL native effect order"
        );
        visited += 1;
    }
    assert_eq!(visited, 9, "CANCEL independent per-method floor");
}
#[test]
fn cancel_order_native_payload_and_errors() {
    cancellations("cancelOrder");
}
#[test]
fn cancel_all_native_payload_and_errors() {
    cancellations("cancelAllOrders");
}
#[test]
fn cancel_symbol_native_payload_and_errors() {
    cancellations("cancelSymbolOrders");
}

#[test]
fn cancel_batch_native_continuation() {
    let corpus = corpus();
    let rows = corpus["cancelBatchRows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "CANCEL batch independent floor");
    for (index, row) in rows.iter().enumerate() {
        let orders = if index == 2 {
            Vec::new()
        } else {
            vec![order("one"), order("two"), order("three")]
        };
        let effects = RefCell::new(Vec::new());
        let warnings = RefCell::new(Vec::new());
        let future = cancel_orders(
            &orders,
            |order| {
                effects.borrow_mut().push(json!(["cancel", order.id]));
                std::future::ready(if order.id == "two" {
                    Err(ExchangeError::new("cancel-fail"))
                } else {
                    Ok(())
                })
            },
            |message, severity| {
                warnings.borrow_mut().push(message.to_owned());
                effects
                    .borrow_mut()
                    .push(json!(["log", message, severity.as_str()]));
                if index == 1 {
                    Err(ExchangeError::new("log-fail"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "CANCEL batch eager first call"
        );
        let result = run(future);
        assert_eq!(
            *warnings.borrow(),
            if index == 2 {
                vec![]
            } else {
                vec!["Failed to cancel order two, continuing...".to_owned()]
            },
            "CANCEL batch native warning"
        );
        assert_error(result.as_ref().err(), &row["error"], "CANCEL batch");
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "CANCEL batch native continuation order"
        );
    }
}

#[test]
fn place_batch_native_results_and_builder_variant() {
    let corpus = corpus();
    let rows = corpus["placeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "BATCH independent floor");
    for (index, row) in rows.iter().enumerate() {
        let orders: Vec<PlaceOrderOpts> = if index == 4 {
            vec![]
        } else {
            ["ONE", "TWO", "THREE"]
                .into_iter()
                .map(|symbol| {
                    serde_json::from_value(
                        json!({"symbol":symbol,"type":"limit","side":"buy","amount":2}),
                    )
                    .unwrap()
                })
                .collect()
        };
        let effects = RefCell::new(Vec::new());
        let future = place_orders(
            &orders,
            |orders| {
                effects
                    .borrow_mut()
                    .push(json!(["preflight", orders.len()]));
                if index == 2 {
                    Err(ExchangeError::new("preflight-fail"))
                } else {
                    Ok(())
                }
            },
            |order| {
                effects.borrow_mut().push(json!(["place", order.symbol]));
                std::future::ready(if order.symbol == "TWO" {
                    if index == 1 {
                        Err(BatchOrderError::Builder(GrvtBuilderUnauthorizedError::new(
                            GrvtBuilderUnauthorizedReason::CapTooLow,
                            300.0,
                            Some(7.0),
                            Some("builder-fail"),
                        )))
                    } else {
                        Err(BatchOrderError::Exchange({
                            let mut error = ExchangeError::new("place-fail");
                            if index == 5 {
                                error.name = "GrvtBuilderUnauthorizedError".into();
                            }
                            error
                        }))
                    }
                } else {
                    Ok(if order.symbol == "ONE" {
                        vec!["id-a".into(), "id-b".into()]
                    } else {
                        vec!["id-c".into()]
                    })
                })
            },
            |message, severity| {
                effects
                    .borrow_mut()
                    .push(json!(["log", message, severity.as_str()]));
                if index == 3 {
                    Err(ExchangeError::new("log-fail"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "BATCH eager preflight and first call"
        );
        let result = run(future);
        if index == 1 {
            let Err(BatchOrderError::Builder(error)) = &result else {
                panic!("BATCH native builder variant");
            };
            assert_eq!(
                error.reason,
                GrvtBuilderUnauthorizedReason::CapTooLow,
                "BATCH native builder reason"
            );
            assert_eq!(error.required_raw, 300.0, "BATCH native required cap");
            assert_eq!(
                error.authorized_raw,
                Some(7.0),
                "BATCH native authorized cap"
            );
            assert_eq!(
                error.code,
                row["error"]["code"].as_str().unwrap(),
                "BATCH native builder code"
            );
            assert_eq!(
                error.message,
                row["error"]["message"].as_str().unwrap(),
                "BATCH native builder message"
            );
        } else {
            match &result {
                Ok(ids) => {
                    assert_eq!(
                        ids.len(),
                        if index == 4 { 0 } else { 3 },
                        "BATCH independent native ID count"
                    );
                    let expected: &[&str] = if index == 4 {
                        &[]
                    } else {
                        &["id-a", "id-b", "id-c"]
                    };
                    assert_eq!(
                        ids.iter().map(String::as_str).collect::<Vec<_>>(),
                        expected,
                        "BATCH native ordered IDs"
                    );
                    let source = row["result"].as_array().unwrap();
                    assert_eq!(
                        source.len(),
                        if index == 4 { 0 } else { 3 },
                        "BATCH independent source ID count"
                    );
                    assert_eq!(
                        ids.iter().map(String::as_str).collect::<Vec<_>>(),
                        source
                            .iter()
                            .map(|id| id.as_str().unwrap())
                            .collect::<Vec<_>>(),
                        "BATCH native source ordered IDs"
                    );
                }
                Err(BatchOrderError::Exchange(error)) => {
                    assert_error(Some(error), &row["error"], "BATCH")
                }
                Err(BatchOrderError::Builder(_)) => panic!("BATCH unexpected builder variant"),
            }
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "BATCH native sequence"
        );
    }
}

fn replacement_fields(native: &safe_cex_types::PreparedOrderOpts, expected: &Value) {
    assert_eq!(
        native.symbol,
        expected["symbol"].as_str().unwrap(),
        "UPDATE native symbol"
    );
    assert_eq!(
        native.r#type,
        serde_json::from_value::<OrderType>(expected["type"].clone()).unwrap(),
        "UPDATE native type"
    );
    assert_eq!(native.side, OrderSide::Buy, "UPDATE native side");
    assert_eq!(
        native.amount,
        expected["amount"].as_f64().unwrap(),
        "UPDATE native amount"
    );
    assert_eq!(
        native.price,
        expected["price"].as_f64(),
        "UPDATE native price"
    );
    assert_eq!(native.reduce_only, Some(true), "UPDATE native reduce only");
    assert_eq!(
        native.time_in_force,
        Some(OrderTimeInForce::ImmediateOrCancel),
        "UPDATE native time in force"
    );
    assert_eq!(
        native.trigger_price,
        expected.get("triggerPrice").and_then(Value::as_f64),
        "UPDATE native trigger price"
    );
    assert_eq!(
        native.close,
        expected.get("close").and_then(Value::as_bool),
        "UPDATE native close"
    );
    for (actual, key) in [
        (native.sl_trigger_type, "slTriggerType"),
        (native.tp_trigger_type, "tpTriggerType"),
    ] {
        assert_eq!(
            actual,
            expected
                .get(key)
                .map(
                    |value| serde_json::from_value::<OrderStopTriggerType>(value.clone()).unwrap()
                ),
            "UPDATE native {key}"
        );
    }
    assert!(
        native.trigger_type.is_none()
            && native.stop_loss.is_none()
            && native.take_profit.is_none()
            && native.client_order_id.is_none()
            && native.max_slippage_pct.is_none(),
        "UPDATE unassigned native options"
    );
}
#[test]
fn update_native_replacement_before_cancel() {
    let corpus = corpus();
    let rows = corpus["updateRows"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "UPDATE independent floor");
    let names = [
        "amount",
        "price",
        "both",
        "no-tif",
        "stop-price",
        "stop-amount",
        "take-price",
        "missing-trigger",
        "wrong-trigger",
        "missing-price",
        "zero-price",
        "incomplete",
        "split",
        "cancel-error",
        "place-error",
        "log-error",
        "emit-error",
        "null-info",
    ];
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[index], "UPDATE input order");
        let opts: UpdateOrderOpts = serde_json::from_value(row["input"].clone()).unwrap();
        let effects = RefCell::new(Vec::new());
        let placed = RefCell::new(Vec::new());
        let cancelled = RefCell::new(Vec::new());
        let future = update_order(
            &opts,
            |order| {
                cancelled.borrow_mut().push(order.id.clone());
                effects.borrow_mut().push(json!(["cancel", order.id]));
                std::future::ready(if index == 13 {
                    Err(rejection("sdk-error"))
                } else {
                    Ok(())
                })
            },
            |new| {
                placed.borrow_mut().push(new);
                effects.borrow_mut().push(json!(["place", 0]));
                std::future::ready(if index == 14 || index == 16 {
                    Err(rejection("lite-error"))
                } else {
                    Ok(vec!["new-a".into(), "new-b".into()])
                })
            },
            |message, severity| {
                effects
                    .borrow_mut()
                    .push(json!(["log", message, severity.as_str()]));
                if index == 15 {
                    Err(ExchangeError::new("log-fail"))
                } else {
                    Ok(())
                }
            },
            |message| {
                effects.borrow_mut().push(json!(["emit", message]));
                if index == 16 {
                    Err(ExchangeError::new("emit-fail"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            *effects.borrow(),
            row["eager"].as_array().unwrap().to_vec(),
            "UPDATE native eager preflight"
        );
        let result = run(future);
        let valid = matches!(index, 0 | 1 | 2 | 4 | 5 | 6 | 13 | 14 | 16);
        assert_eq!(
            cancelled.borrow().len(),
            usize::from(valid),
            "UPDATE native validation before cancel"
        );
        assert_eq!(
            placed.borrow().len(),
            usize::from(valid && index != 13),
            "UPDATE independent replacement call count"
        );
        let mut expected_effects = row["effects"].as_array().unwrap().to_vec();
        for effect in &mut expected_effects {
            if effect[0] == "place" {
                replacement_fields(&placed.borrow()[0], &effect[1]);
                effect[1] = json!(0);
            }
        }
        match &result {
            Ok(ids) => {
                assert_eq!(ids, &["new-a", "new-b"], "UPDATE native replacement IDs");
                assert!(row["error"].is_null(), "UPDATE source successful outcome");
            }
            Err(error) => assert_error(Some(error), &row["error"], "UPDATE"),
        }
        assert_eq!(
            *effects.borrow(),
            expected_effects,
            "UPDATE native effect order"
        );
    }
}

#[test]
fn update_declared_typed_key_count_boundary() {
    let corpus = corpus();
    let opts: UpdateOrderOpts = serde_json::from_value(corpus["extraInput"].clone()).unwrap();
    let logs = RefCell::new(Vec::new());
    let result = run(update_order(
        &opts,
        |_| std::future::ready(Ok(())),
        |_| std::future::ready(Ok(vec!["extra-id".into()])),
        |text, _| {
            logs.borrow_mut().push(text.to_owned());
            Ok(())
        },
        |_| Ok(()),
    ))
    .unwrap();
    assert_eq!(result, ["extra-id"], "UPDATE extra-key native result");
    assert_eq!(
        logs.borrow()[0],
        "Updating order extra (fieldCount=1)",
        "UPDATE declared typed key count"
    );
    assert_eq!(
        corpus["extraFirstLog"], "Updating order extra (fieldCount=2)",
        "UPDATE measured source extra key count"
    );
}

#[test]
fn listen_ohlcv_native_callback_and_cleanup() {
    let corpus = corpus();
    let rows: Vec<_> = corpus["listenRows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["method"] == "listenOHLCV")
        .collect();
    assert_eq!(rows.len(), 2, "LISTEN candle independent floor");
    for (index, row) in rows.iter().enumerate() {
        let opts: OHLCVOptions = serde_json::from_value(row["input"].clone()).unwrap();
        let values = RefCell::new(Vec::new());
        let delegated = RefCell::new(false);
        let cleanup = RefCell::new(false);
        let result = listen_ohlcv(
            &opts,
            |candle| values.borrow_mut().push(candle),
            |opts, mut callback| {
                *delegated.borrow_mut() = true;
                assert_eq!(opts.symbol, "BTC_USDT_PERP", "LISTEN candle native symbol");
                assert_eq!(
                    opts.symbol,
                    row["effects"][0][1]["symbol"].as_str().unwrap(),
                    "LISTEN candle source symbol"
                );
                assert_eq!(
                    opts.interval,
                    safe_cex_types::AnyTimeframe::Variant1(Timeframe::M5),
                    "LISTEN candle native interval"
                );
                assert_eq!(
                    (opts.from, opts.to, opts.limit),
                    (Some(11.0), Some(23.0), Some(17.0)),
                    "LISTEN candle native options"
                );
                let source = &row["effects"][0][1];
                assert_eq!(source["interval"], "5m", "LISTEN candle source interval");
                assert_eq!(
                    (opts.from, opts.to, opts.limit),
                    (
                        source["from"].as_f64(),
                        source["to"].as_f64(),
                        source["limit"].as_f64()
                    ),
                    "LISTEN candle source options"
                );
                if index == 1 {
                    return Err(ExchangeError::new("listen-fail"));
                }
                callback(Candle {
                    timestamp: 701.0,
                    open: 2.0,
                    high: 5.0,
                    low: 1.0,
                    close: 4.0,
                    volume: 13.0,
                    last_side: None,
                });
                Ok(|| {
                    *cleanup.borrow_mut() = true;
                    "released"
                })
            },
        );
        assert!(*delegated.borrow(), "LISTEN candle eager delegate");
        match result {
            Ok(release) => {
                let native = values.borrow();
                assert_eq!(native.len(), 1, "LISTEN candle native event count");
                let event = &native[0];
                assert_eq!(
                    (
                        event.timestamp,
                        event.open,
                        event.high,
                        event.low,
                        event.close,
                        event.volume
                    ),
                    (701.0, 2.0, 5.0, 1.0, 4.0, 13.0),
                    "LISTEN candle native event"
                );
                let source = &row["effects"][1][1];
                assert_eq!(
                    (
                        event.timestamp,
                        event.open,
                        event.high,
                        event.low,
                        event.close,
                        event.volume
                    ),
                    (
                        source["timestamp"].as_f64().unwrap(),
                        source["open"].as_f64().unwrap(),
                        source["high"].as_f64().unwrap(),
                        source["low"].as_f64().unwrap(),
                        source["close"].as_f64().unwrap(),
                        source["volume"].as_f64().unwrap()
                    ),
                    "LISTEN candle native source event"
                );
                assert_eq!(
                    row["effects"].as_array().unwrap().len(),
                    3,
                    "LISTEN candle independent source effects"
                );
                assert_eq!(
                    release(),
                    row["result"].as_str().unwrap(),
                    "LISTEN candle native cleanup result"
                );
                assert!(*cleanup.borrow(), "LISTEN candle cleanup invocation");
            }
            Err(error) => {
                assert_error(Some(&error), &row["error"], "LISTEN candle");
                assert_eq!(
                    values.borrow().len(),
                    0,
                    "LISTEN candle failed delegate has no callback"
                );
            }
        }
    }
}
#[test]
fn listen_book_native_callback_and_cleanup() {
    let corpus = corpus();
    let rows: Vec<_> = corpus["listenRows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["method"] == "listenOrderBook")
        .collect();
    assert_eq!(rows.len(), 2, "LISTEN book independent floor");
    for (index, row) in rows.iter().enumerate() {
        let values = RefCell::new(Vec::new());
        let cleanup = RefCell::new(false);
        let result = listen_order_book(
            "BTC_USDT_PERP",
            |book| values.borrow_mut().push(book),
            |symbol, mut callback| {
                assert_eq!(symbol, "BTC_USDT_PERP", "LISTEN book native symbol");
                assert_eq!(
                    symbol,
                    row["effects"][0][1].as_str().unwrap(),
                    "LISTEN book source symbol"
                );
                if index == 1 {
                    return Err(ExchangeError::new("listen-fail"));
                }
                callback(OrderBook {
                    bids: vec![OrderBookOrders {
                        price: 7.0,
                        amount: 2.0,
                        total: 2.0,
                    }],
                    asks: vec![OrderBookOrders {
                        price: 11.0,
                        amount: 3.0,
                        total: 3.0,
                    }],
                });
                Ok(|| {
                    *cleanup.borrow_mut() = true;
                    "released"
                })
            },
        );
        match result {
            Ok(release) => {
                let native = values.borrow();
                assert_eq!(native.len(), 1, "LISTEN book independent event count");
                let book = &native[0];
                assert_eq!(
                    (book.bids.len(), book.asks.len()),
                    (1, 1),
                    "LISTEN book native side count"
                );
                assert_eq!(
                    (book.bids[0].price, book.bids[0].amount, book.bids[0].total),
                    (7.0, 2.0, 2.0),
                    "LISTEN book native bid"
                );
                assert_eq!(
                    (book.asks[0].price, book.asks[0].amount, book.asks[0].total),
                    (11.0, 3.0, 3.0),
                    "LISTEN book native ask"
                );
                for (levels, key) in [(&book.bids, "bids"), (&book.asks, "asks")] {
                    let source = &row["effects"][1][1][key][0];
                    assert_eq!(
                        (levels[0].price, levels[0].amount, levels[0].total),
                        (
                            source["price"].as_f64().unwrap(),
                            source["amount"].as_f64().unwrap(),
                            source["total"].as_f64().unwrap()
                        ),
                        "LISTEN book native source level"
                    );
                }
                assert_eq!(
                    row["effects"].as_array().unwrap().len(),
                    3,
                    "LISTEN book independent source effects"
                );
                assert_eq!(
                    release(),
                    row["result"].as_str().unwrap(),
                    "LISTEN book native cleanup result"
                );
                assert!(*cleanup.borrow(), "LISTEN book cleanup invocation");
            }
            Err(error) => {
                assert_error(Some(&error), &row["error"], "LISTEN book");
                assert_eq!(
                    values.borrow().len(),
                    0,
                    "LISTEN book failed delegate has no callback"
                );
            }
        }
    }
}
