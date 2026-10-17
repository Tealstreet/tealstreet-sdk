use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_value_ref, js_value_as_f64};
use safe_cex_store::{StoreUpdate, store::DefaultStore};
use safe_cex_types::{Balance, Order, Position};
use safe_cex_venue_breakoutprop::portfolio_reads::*;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, sync::LazyLock};
const SOURCE_CENSUS: &str = include_str!("support/portfolio_reads-census.json");
include!("support/cardinality.rs");
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/portfolio_reads_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .expect("execute actual venue TypeScript");
    assert!(
        out.status.success(),
        "portfolio oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_source_census(&corpus, SOURCE_CENSUS);
    corpus
});
fn number(value: f64) -> Value {
    json!({"number":format!("{:016x}",if value.is_nan() {0x7ff8000000000000} else {value.to_bits()})})
}
fn observe(value: &Value) -> Value {
    match value {
        Value::Number(_) => number(js_value_as_f64(value).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        v => v.clone(),
    }
}
fn optional(value: &mut Value, name: &str, field: Option<Value>) {
    if let Some(field) = field {
        value[name] = field;
    }
}
fn position(p: &Position) -> Value {
    let mut v = json!({"id":p.id,"symbol":p.symbol,"side":p.side,"entryPrice":number(p.entry_price),"notional":number(p.notional),"leverage":number(p.leverage),"unrealizedPnl":number(p.unrealized_pnl),"contracts":number(p.contracts),"liquidationPrice":number(p.liquidation_price)});
    optional(&mut v, "marginType", p.margin_type.map(|x| json!(x)));
    optional(&mut v, "positionMode", p.position_mode.map(|x| json!(x)));
    optional(&mut v, "marginSize", p.margin_size.map(number));
    optional(&mut v, "lastUpdate", p.last_update.map(number));
    optional(&mut v, "info", p.info.as_ref().map(observe));
    v
}
fn balance(b: &Balance) -> Value {
    let mut v = json!({"used":number(b.used),"free":number(b.free),"total":number(b.total),"upnl":number(b.upnl)});
    optional(&mut v, "info", b.info.as_ref().map(observe));
    v
}
fn order(o: &Order) -> Value {
    let mut v = json!({"id":o.id,"status":o.status,"symbol":o.symbol,"type":o.r#type,"side":o.side,"price":number(o.price.to_number()),"amount":number(o.amount),"filled":number(o.filled),"remaining":number(o.remaining),"reduceOnly":o.reduce_only,"lastUpdate":number(o.last_update)});
    optional(&mut v, "info", o.info.as_ref().map(observe));
    optional(&mut v, "timeInForce", o.time_in_force.map(|x| json!(x)));
    v
}
fn result<T>(
    value: Result<T, String>,
    observe: impl FnOnce(T) -> Value,
    errors: &RefCell<Vec<String>>,
) -> Value {
    match value {
        Ok(v) => json!({"ok":observe(v),"errors":errors.borrow().clone(),"trace":["raw"]}),
        Err(e) => {
            json!({"error":e,"errors":errors.borrow().clone(),"trace":["raw"],"sameError":e=="raw-rejection"})
        }
    }
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("ready fixture unexpectedly pending"),
    }
}
#[test]
fn actual_portfolio_reads() {
    let mut comparisons = 0;
    eprintln!("read oracle runtime: {}", ORACLE["runtime"]);
    for case in ORACLE["cases"].as_array().unwrap() {
        let s: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        let mut tickers: Vec<safe_cex_types::Ticker> = js_from_value_ref(&s["tickers"]).unwrap();
        if s["nanMark"] == true {
            tickers[0].mark = f64::NAN;
        }
        safe_cex_store::Store::update(
            &store,
            StoreUpdate {
                balance: Some(js_from_value_ref(&s["balance"]).unwrap()),
                orders: Some(js_from_value_ref(&s["orders"]).unwrap()),
                markets: Some(js_from_value_ref(&s["markets"]).unwrap()),
                tickers: Some(tickers),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("fixture Store update");
        let symbols: Option<Vec<String>> = serde_json::from_value(case["symbols"].clone()).unwrap();
        let errors = RefCell::new(Vec::new());
        let log = |s: &str| {
            errors.borrow_mut().push(s.to_owned());
            Ok(())
        };
        for method in [
            "fetchOpenPositions",
            "fetchPositions",
            "fetchBalance",
            "fetchOrders",
        ] {
            errors.borrow_mut().clear();
            comparisons += 1;
            let calls = std::cell::Cell::new(0);
            let fetch = || {
                calls.set(calls.get() + 1);
                std::future::ready(if case["name"] == "reject" {
                    Err("raw-rejection".to_owned())
                } else if case["name"] == "null" {
                    Ok(None)
                } else if case["name"]
                    .as_str()
                    .unwrap()
                    .starts_with("composed-falsy-")
                {
                    let input =
                        serde_json::from_str::<JsNumberInput>(case["raw"].as_str().unwrap())
                            .unwrap();
                    run(safe_cex_venue_breakoutprop::raw_reads::fetch_portfolio_raw(
                        || std::future::ready(Ok("token".into())),
                        || std::future::ready(Ok("account-A".into())),
                        |_| std::future::ready(Ok(input)),
                        &|_| panic!("composed falsy response must not log"),
                    ))
                    .inspect(|value| {
                        assert!(
                            value.is_some(),
                            "COMPOSED-RAW successful falsy body remains Some"
                        );
                    })
                    .map_err(|e| e.message)
                } else {
                    Ok(Some(
                        serde_json::from_str::<JsNumberInput>(case["raw"].as_str().unwrap())
                            .unwrap(),
                    ))
                })
            };
            let actual = match method {
                "fetchBalance" => result(
                    run(fetch_balance(fetch, &store, &|ms| ms, &log, |e| e.message)),
                    |v| balance(&v),
                    &errors,
                ),
                "fetchPositions" => result(
                    run(fetch_positions(
                        fetch,
                        &store,
                        &|ms| ms,
                        symbols.as_deref(),
                        &log,
                        |e| e.message,
                    )),
                    |v| Value::Array(v.iter().map(position).collect()),
                    &errors,
                ),
                "fetchOpenPositions" => result(
                    run(fetch_open_positions(fetch, &store, &|ms| ms, &log, |e| {
                        e.message
                    })),
                    |v| {
                        v.map(|p| Value::Array(p.iter().map(position).collect()))
                            .unwrap_or(json!({"$undefined":true}))
                    },
                    &errors,
                ),
                "fetchOrders" => result(
                    run(fetch_orders(fetch, &store, &|ms| ms, &log, |e| e.message)),
                    |v| {
                        v.map(|o| Value::Array(o.iter().map(order).collect()))
                            .unwrap_or(json!({"$undefined":true}))
                    },
                    &errors,
                ),
                _ => unreachable!(),
            };
            assert_eq!(calls.get(), 1, "one_raw_request_{method}");
            assert_native_cardinality(
                &actual,
                &case["expected"][method],
                &format!("portfolio_read_{method}: {}", case["name"]),
            );
            assert_eq!(
                actual, case["expected"][method],
                "portfolio_read_{method}: {}",
                case["name"]
            );
        }
    }
    assert_eq!(comparisons, 120, "COMPARISON-COUNT portfolio_reads");
}

#[test]
fn original_rejection_identity_and_eager_call() {
    use std::{cell::Cell, rc::Rc};
    let store = DefaultStore::new();
    let original = Rc::new("original rejection metadata");
    for method in [
        "fetchBalance",
        "fetchPositions",
        "fetchOpenPositions",
        "fetchOrders",
    ] {
        let calls = Cell::new(0);
        let fetch = || {
            calls.set(calls.get() + 1);
            std::future::ready(Err::<Option<JsNumberInput>, _>(original.clone()))
        };
        let identity =
            |_: ExchangeError| -> Rc<&str> { panic!("raw rejection must bypass format_error") };
        type RejectionFuture<'a> =
            std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Rc<&'static str>>> + 'a>>;
        let future: RejectionFuture<'_> = match method {
            "fetchBalance" => {
                let f = fetch_balance(fetch, &store, &|ms| ms, &|_| Ok(()), identity);
                Box::pin(async move { f.await.map(|_| ()) })
            }
            "fetchPositions" => {
                let f = fetch_positions(fetch, &store, &|ms| ms, None, &|_| Ok(()), identity);
                Box::pin(async move { f.await.map(|_| ()) })
            }
            "fetchOpenPositions" => {
                let f = fetch_open_positions(fetch, &store, &|ms| ms, &|_| Ok(()), identity);
                Box::pin(async move { f.await.map(|_| ()) })
            }
            "fetchOrders" => {
                let f = fetch_orders(fetch, &store, &|ms| ms, &|_| Ok(()), identity);
                Box::pin(async move { f.await.map(|_| ()) })
            }
            _ => unreachable!(),
        };
        assert_eq!(calls.get(), 1, "eager_raw_{method}");
        assert!(
            Rc::ptr_eq(&original, &run(future).unwrap_err()),
            "original_rejection_{method}"
        );
    }
}

#[test]
fn deferred_fallback_reads_latest_store() {
    use std::{future::poll_fn, task::Poll};
    for case in ORACLE["deferred"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        let store = DefaultStore::new();
        let settled = std::cell::Cell::new(false);
        let fetch = || {
            poll_fn(|_| {
                if settled.get() {
                    Poll::Ready(Ok::<_, String>(None))
                } else {
                    Poll::Pending
                }
            })
        };
        let mut future: std::pin::Pin<Box<dyn std::future::Future<Output = Value> + '_>> =
            match method {
                "fetchBalance" => {
                    let f = fetch_balance(fetch, &store, &|ms| ms, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { json!(f.await.unwrap().free) })
                }
                "fetchPositions" => {
                    let f =
                        fetch_positions(fetch, &store, &|ms| ms, None, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { json!(f.await.unwrap()[0].symbol) })
                }
                "fetchOpenPositions" => {
                    let f =
                        fetch_open_positions(fetch, &store, &|ms| ms, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { json!(f.await.unwrap().unwrap()[0].symbol) })
                }
                "fetchOrders" => {
                    let f = fetch_orders(fetch, &store, &|ms| ms, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { json!(f.await.unwrap().unwrap()[0].id) })
                }
                _ => unreachable!(),
            };
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            future.as_mut().poll(&mut cx).is_pending(),
            "await_raw_{method}"
        );
        let position:Position=js_from_value_ref(&json!({"id":"after-id","symbol":"AFTER","side":"long","entryPrice":13,"notional":17,"leverage":19,"unrealizedPnl":23,"contracts":29,"liquidationPrice":31})).unwrap();
        let order:Order=js_from_value_ref(&json!({"id":"AFTER","symbol":"after-symbol","type":"limit","side":"buy","price":37,"amount":41,"filled":43,"remaining":47,"reduceOnly":false,"status":"open","lastUpdate":53})).unwrap();
        safe_cex_store::Store::update(
            &store,
            StoreUpdate {
                balance: Some(Balance {
                    free: 673.0,
                    total: 701.0,
                    used: 28.0,
                    upnl: 59.0,
                    info: None,
                    currencies: None,
                    coins: None,
                }),
                positions: Some(vec![position]),
                orders: Some(vec![order]),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("fixture Store update");
        settled.set(true);
        assert_eq!(
            observe(&run(future)),
            observe(&case["value"]),
            "deferred_fallback_{method}"
        );
    }
}

#[test]
fn inherited_and_utf8_read_boundaries() {
    let store = DefaultStore::new();
    assert_eq!(ORACLE["inherited"].as_array().unwrap().len(), 18);
    for case in ORACLE["inherited"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let fetch = || std::future::ready(Ok::<_, String>(Some(input.clone())));
        if method == "fetchBalance" {
            let balance = run(fetch_balance(fetch, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap();
            assert_eq!(
                (balance.total, balance.used, balance.upnl, balance.free),
                (131.25, 169.0, 0.0, -37.75)
            );
            continue;
        }
        let error = match method {
            "fetchBalance" => run(fetch_balance(fetch, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap_err(),
            "fetchPositions" => run(fetch_positions(
                fetch,
                &store,
                &|ms| ms,
                Some(&["BTCUSD".into()]),
                &|_| Ok(()),
                |e| e.message,
            ))
            .unwrap_err(),
            "fetchOpenPositions" => run(fetch_open_positions(
                fetch,
                &store,
                &|ms| ms,
                &|_| Ok(()),
                |e| e.message,
            ))
            .unwrap_err(),
            "fetchOrders" => run(fetch_orders(fetch, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap_err(),
            _ => unreachable!(),
        };
        assert_eq!(
            case["beforeSerialization"]["type"],
            if case["key"] == "__proto__" {
                "object"
            } else {
                "function"
            },
            "read_source_typeof_{method}"
        );
        assert_eq!(
            case["beforeSerialization"]["truthy"], true,
            "read_source_truthy_{method}"
        );
        let field = if case["field"] == "position-side" {
            "position side".to_owned()
        } else {
            format!("order {}", case["field"].as_str().unwrap())
        };
        assert_eq!(
            error,
            format!("Unrepresentable {field}: {}", case["key"].as_str().unwrap()),
            "read_closed_boundary_{method}"
        );
    }
    for case in ORACLE["utf8"].as_array().unwrap() {
        assert_eq!(case["codeUnit"], 55296, "read_source_surrogate");
        assert!(
            serde_json::from_str::<JsNumberInput>(case["raw"].as_str().unwrap()).is_err(),
            "read_utf8_boundary"
        );
    }
}

#[test]
fn owned_read_info_and_fallbacks() {
    let case = &ORACLE["cases"][0];
    for alias in ORACLE["aliases"].as_array().unwrap() {
        let method = alias["method"].as_str().unwrap();
        let store = DefaultStore::new();
        let raw: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
        safe_cex_store::Store::update(
            &store,
            StoreUpdate {
                balance: Some(js_from_value_ref(&raw["balance"]).unwrap()),
                orders: Some(js_from_value_ref(&raw["orders"]).unwrap()),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("fixture Store update");
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let fetch = || std::future::ready(Ok::<_, String>(Some(input.clone())));
        let mut info = match method {
            "fetchBalance" => run(fetch_balance(fetch, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap()
            .info
            .unwrap(),
            "fetchPositions" => run(fetch_positions(
                fetch,
                &store,
                &|ms| ms,
                Some(&["BTCUSD".into()]),
                &|_| Ok(()),
                |e| e.message,
            ))
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .info
            .unwrap(),
            "fetchOpenPositions" => run(fetch_open_positions(
                fetch,
                &store,
                &|ms| ms,
                &|_| Ok(()),
                |e| e.message,
            ))
            .unwrap()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .info
            .unwrap(),
            "fetchOrders" => run(fetch_orders(fetch, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .info
            .unwrap(),
            _ => unreachable!(),
        };
        if method == "fetchBalance" {
            info["portfolios"][0]["positions"][0]["symbol"] = json!("changed");
        } else if method == "fetchOrders" {
            info["instrument"] = json!("changed");
        } else {
            info["symbol"] = json!("changed");
        }
        let decoded: safe_cex_venue_breakoutprop::types::PortfolioResponse =
            input.decode().unwrap();
        let native = if method == "fetchOrders" {
            &decoded.portfolios[0].orders[0].instrument
        } else {
            &decoded.portfolios[0].positions[0].symbol
        };
        assert_eq!(
            alias["observed"], "changed",
            "source_read_info_alias_{method}"
        );
        assert_eq!(native, "BTCUSD", "owned_read_info_{method}");
        let null = || std::future::ready(Ok::<_, String>(None));
        if method == "fetchBalance" {
            let mut fallback = run(fetch_balance(null, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap();
            fallback.free = 881.0;
            assert_eq!(fallback.free, 881.0);
            assert_eq!(alias["fallbackObserved"], 881);
            assert_eq!(
                safe_cex_store::Store::balance(&store).free,
                893.0,
                "owned_read_fallback_balance"
            );
        } else if method == "fetchOrders" {
            let mut fallback = run(fetch_orders(null, &store, &|ms| ms, &|_| Ok(()), |e| {
                e.message
            }))
            .unwrap()
            .unwrap();
            fallback.clear();
            assert!(fallback.is_empty());
            assert_eq!(alias["fallbackObserved"], 2);
            assert_eq!(
                safe_cex_store::Store::orders(&store).len(),
                1,
                "owned_read_fallback_orders"
            );
        } else {
            let mut fallback = if method == "fetchPositions" {
                run(fetch_positions(
                    null,
                    &store,
                    &|ms| ms,
                    None,
                    &|_| Ok(()),
                    |e| e.message,
                ))
                .unwrap()
            } else {
                run(fetch_open_positions(
                    null,
                    &store,
                    &|ms| ms,
                    &|_| Ok(()),
                    |e| e.message,
                ))
                .unwrap()
                .unwrap()
            };
            let pos = run(fetch_open_positions(
                || {
                    std::future::ready(Ok::<_, String>(Some(
                        serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
                    )))
                },
                &store,
                &|ms| ms,
                &|_| Ok(()),
                |e| e.message,
            ))
            .unwrap()
            .unwrap()
            .remove(0);
            fallback.push(pos);
            assert_eq!(fallback.len(), 1);
            assert_eq!(alias["fallbackObserved"], 1);
            assert!(
                safe_cex_store::Store::positions(&store).is_empty(),
                "owned_read_fallback_{method}"
            );
        }
    }
}

#[test]
fn abandoned_promise_vs_dropped_future() {
    use std::{cell::Cell, rc::Rc, task::Poll};
    struct Release(Rc<Cell<usize>>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    for case in ORACLE["abandoned"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        assert_eq!(
            case["before"],
            json!({"rawCalls":1,"formatted":0}),
            "source_abandoned_prefix_{method}"
        );
        assert_eq!(
            case["after"],
            json!({"rawCalls":1,"formatted":1}),
            "source_abandoned_continues_{method}"
        );
        for mode in ["unpolled", "pending", "complete"] {
            let store = DefaultStore::new();
            let dropped = Rc::new(Cell::new(0));
            let settled = Cell::new(false);
            let polled = Cell::new(0);
            let factories = Cell::new(0);
            let formatted = Cell::new(0);
            let local = |ms| {
                formatted.set(formatted.get() + 1);
                ms
            };
            let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
            let guard = Release(dropped.clone());
            let fetch = || {
                factories.set(factories.get() + 1);
                let ready = &settled;
                let polls = &polled;
                async move {
                    let _guard = guard;
                    std::future::poll_fn(|_| {
                        polls.set(polls.get() + 1);
                        if ready.get() {
                            Poll::Ready(())
                        } else {
                            Poll::Pending
                        }
                    })
                    .await;
                    Ok::<_, String>(Some(input))
                }
            };
            let mut future: std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + '_>,
            > = match method {
                "fetchBalance" => {
                    let f = fetch_balance(fetch, &store, &local, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { f.await.map(|_| ()) })
                }
                "fetchPositions" => {
                    let f =
                        fetch_positions(fetch, &store, &local, None, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { f.await.map(|_| ()) })
                }
                "fetchOpenPositions" => {
                    let f = fetch_open_positions(fetch, &store, &local, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { f.await.map(|_| ()) })
                }
                "fetchOrders" => {
                    let f = fetch_orders(fetch, &store, &local, &|_| Ok(()), |e| e.message);
                    Box::pin(async move { f.await.map(|_| ()) })
                }
                _ => unreachable!(),
            };
            assert_eq!(factories.get(), 1, "eager_factory_before_poll_{method}");
            assert_eq!(polled.get(), 0, "lazy_raw_work_before_poll_{method}");
            if mode != "unpolled" {
                let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
                assert!(future.as_mut().poll(&mut cx).is_pending());
                assert_eq!(polled.get(), 1);
            }
            if mode == "complete" {
                settled.set(true);
                run(future).unwrap();
                assert!(formatted.get() > 0, "completed_future_formats_{method}");
            } else {
                drop(future);
                settled.set(true);
                assert_eq!(formatted.get(), 0, "dropped_future_stops_{method}_{mode}");
                assert_eq!(
                    polled.get(),
                    usize::from(mode == "pending"),
                    "dropped_future_not_polled_again_{method}_{mode}"
                );
            }
            assert_eq!(dropped.get(), 1, "raw_future_released_{method}_{mode}");
        }
    }
}

include!("support/logger.rs");
#[test]
fn read_logger_failure_preserves_metadata() {
    assert_eq!(ORACLE["loggerFailures"].as_array().unwrap().len(), 2);
    for case in ORACLE["loggerFailures"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        assert_eq!(case["calls"], 1, "LOGGER source calls {method}");
        assert_eq!(case["sameError"], true, "LOGGER source identity {method}");
        let original = logger_error_fixture();
        assert_eq!(
            logger_error_observe(&original),
            case["error"],
            "LOGGER source metadata {method}"
        );
        let calls = std::cell::Cell::new(0);
        let report = |_: &str| {
            calls.set(calls.get() + 1);
            Err(original.clone())
        };
        let input = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let fetch = || std::future::ready(Ok(Some(input)));
        let store = DefaultStore::new();
        let result = match method {
            "fetchBalance" => {
                run(fetch_balance(fetch, &store, &|ms| ms, &report, |e| e)).map(|_| ())
            }
            "fetchOrders" => run(fetch_orders(fetch, &store, &|ms| ms, &report, |e| e)).map(|_| ()),
            _ => unreachable!(),
        };
        assert_eq!(result, Err(original), "LOGGER-PROPAGATION {method}");
        assert_eq!(calls.get(), 1, "LOGGER native calls {method}");
    }
}
