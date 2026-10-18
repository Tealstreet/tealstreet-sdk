#[path = "support/data_trace.rs"]
mod data_trace;
#[path = "support/error_post.rs"]
mod error_post_support;
use serde_json::Value;

#[test]
fn source_store_notification_failure_commits_then_reaches_worker_boundary() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/store_notification_oracle.mjs"
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
    assert_eq!(rows.len(), 36, "NOTIFICATION worker path inventory");
    for path in [
        "positions-full",
        "positions-partial",
        "positions-dispatch-full",
        "positions-dispatch-partial",
        "leverage",
        "remove",
        "bbo",
    ] {
        let paths: Vec<_> = rows.iter().filter(|r| r["path"] == path).collect();
        assert_eq!(
            paths.len(),
            if path.starts_with("positions") { 6 } else { 4 },
            "NOTIFICATION positive and failure {path}"
        );
        assert_eq!(paths.iter().filter(|r| r["fail"] == true).count(), 2);
        for kind in ["Error", "TypeError"] {
            let kinds: Vec<_> = paths.iter().filter(|r| r["kind"] == kind).collect();
            assert_eq!(
                kinds.len(),
                if path.starts_with("positions") { 3 } else { 2 },
                "NOTIFICATION error kind inventory {path} {kind}"
            );
            assert_eq!(kinds.iter().filter(|r| r["fail"] == true).count(), 1);
        }
    }
    let symbols = report["leverageSymbols"].as_array().unwrap();
    assert_eq!(
        symbols.len(),
        4,
        "LEVERAGE independent symbol witness floor"
    );
    for (row, expected_units) in symbols.iter().zip([
        vec![0xd800_u16],
        vec![0xfffd],
        vec![0xd83d, 0xde80],
        vec![92, 117, 100, 56, 48, 48],
    ]) {
        assert_eq!(row["units"], json!(expected_units));
        assert_eq!(row["committedLeverage"], 8);
        assert_eq!(row["observed"].as_array().unwrap().len(), 1);
        assert_eq!(row["observed"][0]["units"], json!(expected_units));
        let symbol = serde_json::from_str::<String>(row["symbolJson"].as_str().unwrap());
        let position = serde_json::from_str::<Position>(row["positionJson"].as_str().unwrap());
        if row["label"] == "lone" {
            assert!(
                symbol.is_err(),
                "LEVERAGE native String refuses materialized D800"
            );
            assert!(
                position.is_err(),
                "LEVERAGE native whole Position refuses materialized D800 symbol"
            );
            continue;
        }
        let symbol = symbol.unwrap();
        assert_eq!(symbol.encode_utf16().collect::<Vec<_>>(), expected_units);
        let runtime = Rc::new(runtime::Runtime::default());
        let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
        store
            .update(
                StoreUpdate {
                    positions: Some(vec![position.unwrap()]),
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        let venue = Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                store.clone(),
                runtime,
                Default::default(),
            ),
            capabilities: Default::default(),
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let unsubscribe = store.subscribe(Box::new(move |data, _| {
            sink.borrow_mut().push((
                data.positions[0].leverage,
                data.positions[0].symbol.encode_utf16().collect::<Vec<_>>(),
            ));
            Ok(())
        }));
        let leverage = safe_cex_types::FetchedLeverage {
            buy_leverage: 8.0,
            sell_leverage: 9.0,
            margin_type: None,
            position_mode: None,
            is_fetching: None,
        };
        update_positions_with_leverage(&venue, &symbol, &leverage).unwrap();
        assert_eq!(
            store.positions()[0].leverage,
            8.0,
            "LEVERAGE native positive symbol commit"
        );
        assert_eq!(
            calls.borrow().as_slice(),
            &[(8.0, expected_units)],
            "LEVERAGE native positive subscriber units"
        );
        unsubscribe();
    }
    // BBO still has an infallible core callback; its source throw is separate.
}

#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, Position, StoreData};
use safe_cex_webworker::{
    call_method::CallMethodHandler, positions::UpdatePositionsHandler,
    ui_state::update_positions_with_leverage,
};
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async { Ok(vec![position(8.0)]) })
    }
}
fn position(leverage: f64) -> Position {
    serde_json::from_value(json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"contracts":2,"notional":200,"leverage":leverage,"unrealizedPnl":10,"liquidationPrice":0})).unwrap()
}
fn view(data: &StoreData) -> Value {
    json!({"leverage":data.positions.first().map(|p| p.leverage),"orders":data.orders.iter().map(|o|&o.id).collect::<Vec<_>>()})
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("NOTIFICATION expected completed worker call"),
    }
}
#[test]
fn native_store_notification_reaches_worker_after_committed_mutation() {
    for path in [
        "positions-full",
        "positions-partial",
        "positions-dispatch-full",
        "positions-dispatch-partial",
        "leverage",
        "remove",
    ] {
        for (fail, dispose) in [(false, false), (true, false), (false, true)] {
            if dispose && !path.starts_with("positions") {
                continue;
            }
            for name in ["Error", "TypeError"] {
                let runtime = Rc::new(runtime::Runtime::default());
                runtime.advance(10_000);
                let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
                let remove_warmup = store.subscribe(Box::new(|_, _| Ok(())));
                remove_warmup();
                store.update(StoreUpdate {
                    positions: Some(vec![position(5.0)]),
                    orders: Some(vec![serde_json::from_value(json!({"id":"o","symbol":"BTC","status":"open","type":"limit","side":"buy","price":100,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0})).unwrap()]),
                    ..Default::default()
                }, true).unwrap();
                let venue = Rc::new(Venue {
                    base: BaseExchange::new(
                        ExchangeName::Bybit,
                        serde_json::from_value(
                            json!({"key":"notification-key","secret":"notification-secret"}),
                        )
                        .unwrap(),
                        store.clone(),
                        runtime.clone(),
                        Default::default(),
                    ),
                    capabilities: Default::default(),
                });
                let mut error = ExchangeError::new("subscriber failed");
                error.name = name.into();
                error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(197)));
                error.stack = Some("native-stack".into());
                error.context = Some(json!({"origin":"first listener"}));
                error.store_subscriber_ids = vec![401];
                let events = Rc::new(RefCell::new(Vec::new()));
                let first = events.clone();
                let original = error.clone();
                let disposal_target = venue.clone();
                let unsubscribe_first = store.subscribe(Box::new(move |data, _| {
                    first.borrow_mut().push(("first", view(data)));
                    if fail {
                        Err(original.clone().into())
                    } else {
                        if dispose && !disposal_target.base.is_disposed() {
                            disposal_target.base.dispose().unwrap();
                        }
                        Ok(())
                    }
                }));
                let second = events.clone();
                let unsubscribe_second = store.subscribe(Box::new(move |data, _| {
                    second.borrow_mut().push(("second", view(data)));
                    Ok(())
                }));
                let posts = Rc::new(RefCell::new(Vec::new()));
                let sink = posts.clone();
                let post = Rc::new(move |payload| sink.borrow_mut().push(payload));
                match path {
                    "positions-full"
                    | "positions-partial"
                    | "positions-dispatch-full"
                    | "positions-dispatch-partial" => {
                        let handler = UpdatePositionsHandler::new(venue.clone(), post);
                        let symbols = path.ends_with("partial").then(|| vec!["BTC".to_owned()]);
                        if path.contains("dispatch") {
                            handler.handle(safe_cex_webworker::payload::StoreWorkerPayload {
                                op: safe_cex_webworker::payload::StoreWorkerOp::UpdatePositions,
                                instance_id: None,
                                data: Some(json!({"requestId":"r","symbols":symbols})),
                            });
                            assert_eq!(
                                runtime.tasks.borrow().len(),
                                1,
                                "NOTIFICATION native dispatch schedules once"
                            );
                            runtime.advance(10_000);
                            assert!(
                                runtime.tasks.borrow().is_empty(),
                                "NOTIFICATION native dispatch settles once"
                            );
                        } else {
                            ready(handler.handle_update_positions("r".into(), symbols));
                        }
                    }
                    "remove" => {
                        let handler = CallMethodHandler::new(
                            venue.clone(),
                            data_trace::post(post),
                            error_post_support::unexpected(),
                        );
                        ready(handler.handle_call_method(
                            "r".into(),
                            "removeOrders".into(),
                            vec![json!([{"id":"o"}])],
                        ));
                    }
                    "leverage" => {
                        let leverage =
                            serde_json::from_value(json!({"buyLeverage":8,"sellLeverage":9}))
                                .unwrap();
                        let result =
                            update_positions_with_leverage(venue.as_ref(), "BTC", &leverage);
                        if fail {
                            assert!(
                                result.is_err(),
                                "NOTIFICATION native leverage propagates subscriber failure"
                            );
                            let mutation = result.unwrap_err();
                            let safe_cex_store::StoreMutationError::Notification(notification) =
                                &mutation
                            else {
                                panic!("NOTIFICATION native failure retains notification origin");
                            };
                            let subscriber_id = notification.subscriber_id();
                            assert_ne!(subscriber_id, 0, "NOTIFICATION native nonzero subscriber");
                            let recovered = ExchangeError::from(mutation);
                            assert_eq!(
                                recovered.store_subscriber_ids,
                                vec![401, subscriber_id],
                                "NOTIFICATION native inner-to-outer subscriber identity"
                            );
                            assert_eq!(
                                recovered.name, error.name,
                                "NOTIFICATION native error name"
                            );
                            assert_eq!(
                                recovered.metadata, error.metadata,
                                "NOTIFICATION native error code"
                            );
                            assert_eq!(
                                recovered.stack, error.stack,
                                "NOTIFICATION native error stack"
                            );
                            assert_eq!(
                                recovered.context, error.context,
                                "NOTIFICATION native error context"
                            );
                        } else {
                            result.unwrap();
                        }
                    }
                    _ => unreachable!(),
                }
                if dispose {
                    assert_eq!(
                        events.borrow()[0].1["leverage"],
                        8.0,
                        "NOTIFICATION native write precedes disposal"
                    );
                    assert!(
                        venue.base.is_disposed(),
                        "NOTIFICATION native subscriber disposed exchange"
                    );
                    assert_eq!(
                        posts.borrow().len(),
                        1,
                        "NOTIFICATION native disposal preserves success reply"
                    );
                    assert_eq!(
                        posts.borrow()[0].data.as_ref().unwrap()["success"],
                        true,
                        "NOTIFICATION native disposal reply succeeds"
                    );
                    unsubscribe_first();
                    unsubscribe_second();
                    continue;
                }
                let state = store.serialize();
                assert_eq!(
                    venue.base.is_disposed(),
                    dispose,
                    "NOTIFICATION native disposal during subscriber"
                );
                assert_eq!(
                    events.borrow().len(),
                    if fail { 1 } else { 2 },
                    "NOTIFICATION native first failure stops fanout {path}/{name}"
                );
                assert_eq!(
                    events.borrow()[0],
                    ("first", view(&state)),
                    "NOTIFICATION native mutation precedes notification"
                );
                if path == "remove" {
                    assert!(
                        state.orders.is_empty(),
                        "NOTIFICATION native removal committed"
                    );
                } else {
                    assert_eq!(
                        state.positions[0].leverage, 8.0,
                        "NOTIFICATION native leverage committed"
                    );
                }
                if path != "leverage" {
                    let posts = posts.borrow();
                    assert_eq!(posts.len(), 1, "NOTIFICATION native reply count");
                    let data = posts[0].data.as_ref().unwrap();
                    assert_eq!(data["success"], !fail, "NOTIFICATION native failure reply");
                    if fail && path == "remove" {
                        assert_eq!(
                            data["result"]["name"], name,
                            "NOTIFICATION native reply error name"
                        );
                        assert_eq!(
                            data["result"]["code"], 197,
                            "NOTIFICATION native reply error code"
                        );
                    } else if fail {
                        assert_eq!(
                            data["reason"],
                            format!("{name}: subscriber failed"),
                            "NOTIFICATION native refresh reason"
                        );
                    }
                }
                unsubscribe_first();
                unsubscribe_second();
            }
        }
    }
}
