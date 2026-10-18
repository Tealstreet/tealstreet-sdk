use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{
    Store, StoreMutationError, StoreUpdate, model_patch::ModelPatch, store::DefaultStore,
};
use safe_cex_types::{Order, OrderStatus, OrderType, StoreSubscriberError};
use safe_cex_venue_binance::cancel_symbol_orders::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
fn initial() -> Vec<Order> {
    [("r","BTCUSDT","limit"),("a","BTCUSDT","stop_market"),("tp","BTCUSDT","take_profit_market"),("trail","BTCUSDT","trailing_stop_market"),("conditional","BTCUSDT","conditional"),("market","BTCUSDT","market"),("other","ETHUSDT","stop_market")].into_iter().map(|(id,symbol,kind)|serde_json::from_value(json!({"id":id,"symbol":symbol,"type":kind,"status":"open","side":"buy","price":12.3,"amount":3.6,"filled":0,"remaining":3.6,"reduceOnly":false,"lastUpdate":100})).unwrap()).collect()
}
fn fixture() -> Rc<DefaultStore> {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(initial()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        store.update_ids().orders,
        1.0,
        "independent seeded order version"
    );
    store
}
fn oracle() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cancel-symbol-orders.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 95, "fixed authored source count 43+26+26");
    cases
}
fn sentinel(role: &str, msg: Option<&Value>) -> Rc<ExchangeError> {
    let mut error = ExchangeError::new(format!("fixture {role}"));
    error.name = "RangeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        format!("{role}-code")
    )));
    error = error.with_retained_response(
        (json!({"status":503,"data":msg.map(|value|json!({"msg":value})).unwrap_or(json!({}))}))
            .into(),
    );
    Rc::new(error)
}
fn emission(role: &str, msg: Option<&Value>) -> Value {
    match msg {
        None | Some(Value::Null) | Some(Value::Bool(false)) => json!(format!("fixture {role}")),
        Some(Value::String(s)) if s.is_empty() => json!(format!("fixture {role}")),
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => json!(format!("fixture {role}")),
        Some(value) => value.clone(),
    }
}
fn patch(store: &DefaultStore, handles: &[CancelOrderHandle], id: &str, changes: Value) {
    let patch = ModelPatch::from(changes.as_object().unwrap().clone());
    for handle in handles {
        let mut row = handle.borrow_mut();
        if row.id == id {
            if let Some(value) = changes.get("id") {
                row.id = value.as_str().unwrap().into();
            }
            if let Some(value) = changes.get("symbol") {
                row.symbol = value.as_str().unwrap().into();
            }
            if changes.get("type").is_some() {
                row.r#type = safe_cex_types::ModelField::Value(OrderType::StopLoss);
            }
        }
    }
    Store::update_order(store, id, &patch).unwrap();
}
fn apply_mutation(store: &DefaultStore, handles: &[CancelOrderHandle], mutation: &str) {
    match mutation {
        "replace-before-resolve" => {
            let mut new = initial().remove(0);
            new.id = "new".into();
            Store::update(
                store,
                StoreUpdate {
                    orders: Some(vec![new, initial().pop().unwrap()]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        "rename-before-resolve" => patch(store, handles, "r", json!({"id":"r-renamed"})),
        "change-before-resolve" => patch(
            store,
            handles,
            "r",
            json!({"symbol":"ETHUSDT","type":"stop_market"}),
        ),
        _ => {}
    }
}
// Authored expectation independent of the source outcome and Rust artifact.
fn expected(
    case: &Value,
    pm_spot: bool,
) -> (
    Vec<Order>,
    Vec<Vec<String>>,
    usize,
    usize,
    Option<&'static str>,
) {
    let symbol = case["symbol"].as_str().unwrap();
    let mutation = case["mutation"].as_str().unwrap();
    let mut rows = initial();
    let synchronous =
        case["regular"] == "sync-reject" || (!pm_spot && case["algo"] == "sync-reject");
    let mut request_notifications = 0;
    if !synchronous {
        match mutation {
            "replace-before-resolve" => {
                let mut new = rows.remove(0);
                new.id = "new".into();
                rows = vec![new, initial().pop().unwrap()];
                request_notifications = 1;
            }
            "rename-before-resolve" => {
                rows[0].id = "r-renamed".into();
                request_notifications = 1;
            }
            "change-before-resolve" => {
                rows[0].symbol = "ETHUSDT".into();
                rows[0].r#type = safe_cex_types::ModelField::Value(OrderType::StopLoss);
                request_notifications = 1;
            }
            _ => {}
        }
    }
    let mut groups = vec![];
    if !synchronous {
        if pm_spot {
            if case["regular"] == "ok" {
                groups.push(
                    rows.iter()
                        .filter(|r| r.symbol == symbol)
                        .map(|r| r.id.clone())
                        .collect::<Vec<_>>(),
                );
            }
        } else {
            // Snapshot membership is authored from original objects, including
            // Conditional and Market in the regular group, not the algo group.
            if case["regular"] == "ok" {
                groups.push(if symbol == "BTCUSDT" {
                    vec![
                        if mutation == "rename-before-resolve" {
                            "r-renamed"
                        } else {
                            "r"
                        }
                        .into(),
                        "conditional".into(),
                        "market".into(),
                    ]
                } else {
                    vec![]
                });
            }
            if case["algo"] == "ok" {
                groups.push(if symbol == "BTCUSDT" {
                    vec!["a".into(), "tp".into(), "trail".into()]
                } else if symbol == "ETHUSDT" {
                    vec!["other".into()]
                } else {
                    vec![]
                });
            }
        }
    }
    let mut calls = vec![];
    let mut status_notifications = 0;
    let mut nested_notifications = 0;
    let mut listener_failed = false;
    for mut ids in groups {
        if mutation == "rename-algo-in-first-status" && status_notifications > 0 {
            for id in &mut ids {
                if id == "a" {
                    *id = "a-after-notify".into();
                }
            }
        }
        calls.push(ids.clone());
        let mut changed = false;
        for row in &mut rows {
            if ids.contains(&row.id) {
                row.status = safe_cex_types::ModelField::Value(OrderStatus::Canceled);
                changed = true;
            }
        }
        if changed {
            status_notifications += 1;
            if case["listenerFail"].as_u64().unwrap() as usize == status_notifications {
                listener_failed = true;
                break;
            }
            if mutation == "rename-algo-in-first-status" && status_notifications == 1 {
                rows[1].id = "a-after-notify".into();
                nested_notifications = 1;
            }
        }
    }
    let origin = if listener_failed {
        Some("listener")
    } else if case["regular"] != "ok" {
        Some("regular")
    } else if !pm_spot && case["algo"] != "ok" {
        Some("algo")
    } else {
        None
    };
    let total = request_notifications + status_notifications + nested_notifications;
    (
        rows,
        calls,
        total,
        total - usize::from(listener_failed),
        origin,
    )
}
fn compare(variant: &str, count: usize) {
    let mut completed = 0;
    for case in oracle().into_iter().filter(|c| c["variant"] == variant) {
        let index = case["index"].as_u64().unwrap();
        let pm_spot = variant != "binance";
        let store = fixture();
        let handles: Vec<CancelOrderHandle> = initial()
            .into_iter()
            .map(|r| Rc::new(RefCell::new(r)))
            .collect();
        let regular = sentinel("regular", case.get("msg"));
        let algo = sentinel("algo", case.get("msg"));
        let listener = sentinel("listener", case.get("msg"));
        let emitter = sentinel("emitter", case.get("msg"));
        let stage = Rc::new(Cell::new("during-request"));
        let notifications = Rc::new(RefCell::new(vec![]));
        let later = Rc::new(RefCell::new(vec![]));
        let s = store.clone();
        let st = stage.clone();
        let ns = notifications.clone();
        let ls = listener.clone();
        let hs = handles.clone();
        let fail = case["listenerFail"].as_u64().unwrap() as usize;
        let rename = case["mutation"] == "rename-algo-in-first-status";
        let unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |data, _| {
                ns.borrow_mut().push((
                    st.get(),
                    data.orders
                        .iter()
                        .map(|o| (o.id.clone(), o.status))
                        .collect::<Vec<_>>(),
                ));
                if st.get() == "status" {
                    let ordinal = ns
                        .borrow()
                        .iter()
                        .filter(|(stage, _)| *stage == "status")
                        .count();
                    if fail == ordinal {
                        return Err(StoreSubscriberError::with_response(
                            ls.message.clone(),
                            ls.response_json_view().clone().unwrap(),
                        )
                        .with_cause(ls.clone()));
                    }
                    if rename && ordinal == 1 {
                        patch(s.as_ref(), &hs, "a", json!({"id":"a-after-notify"}));
                    }
                }
                Ok(())
            }),
        );
        let st = stage.clone();
        let logs = later.clone();
        let unsubscribe_later = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                logs.borrow_mut().push(st.get());
                Ok(())
            }),
        );
        let requests = RefCell::new(vec![]);
        let updates = RefCell::new(vec![]);
        let emissions = RefCell::new(vec![]);
        let mutated = Cell::new(false);
        let view_reads = Cell::new(0);
        let mutation = case["mutation"].as_str().unwrap();
        let delete = |path: &'static str, request: CancelSymbolRequest| {
            let role = if path.contains("algoOpenOrders") {
                "algo"
            } else {
                "regular"
            };
            let mode = case[role].as_str().unwrap();
            let error = if role == "algo" {
                algo.clone()
            } else {
                regular.clone()
            };
            requests.borrow_mut().push((path, request, role));
            if mode == "sync-reject" {
                return Err(error);
            }
            let store = store.clone();
            let handles = handles.clone();
            let mutated = &mutated;
            Ok(std::future::poll_fn(move |_| {
                if !mutated.replace(true) {
                    apply_mutation(store.as_ref(), &handles, mutation);
                }
                Poll::Ready(if mode == "reject" {
                    Err(error.clone())
                } else {
                    Ok(())
                })
            }))
        };
        let emit = |message: safe_cex_exchange::js_runtime::value::Js| {
            let message = safe_cex_exchange::js_runtime::value::to_json(&message)
                .expect("finite source message fixture");
            emissions.borrow_mut().push(message);
            if case["emitReject"] == true {
                Err(emitter.clone())
            } else {
                Ok(())
            }
        };
        let symbol = case["symbol"].as_str().unwrap();
        let result = if variant == "binance" {
            run(cancel_binance_symbol_orders(
                || {
                    view_reads.set(view_reads.get() + 1);
                    handles.clone()
                },
                symbol,
                delete,
                |group, status| {
                    let ids: Vec<String> = group.iter().map(|h| h.borrow().id.clone()).collect();
                    updates.borrow_mut().push((ids.clone(), status));
                    stage.set("status");
                    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
                    Store::update_orders_status(store.as_ref(), &refs, status)
                },
                emit,
            ))
        } else {
            // Production PM/Spot code writes the Store directly. The request
            // observer changes stage just before successful await completes.
            let delete = |path, request| {
                delete(path, request).map(|future| async {
                    let result = future.await;
                    if result.is_ok() {
                        stage.set("status");
                    }
                    result
                })
            };
            if variant == "binancepm" {
                run(cancel_binance_pm_symbol_orders(
                    store.as_ref(),
                    symbol,
                    delete,
                    emit,
                ))
            } else {
                run(cancel_binance_spot_symbol_orders(
                    store.as_ref(),
                    symbol,
                    delete,
                    emit,
                ))
            }
        };
        let (expected_rows, expected_updates, expected_notifications, expected_later, origin) =
            expected(&case, pm_spot);
        // Assert the decoded native Store and errors before any source trace or
        // request wire-option comparison. No serialization/roundtrip is used.
        let actual = store.orders();
        assert_eq!(
            actual.len(),
            expected_rows.len(),
            "CANCEL {variant} case {index} native authored order count"
        );
        for (row, (actual, expected)) in actual.iter().zip(&expected_rows).enumerate() {
            assert_eq!(
                actual.id, expected.id,
                "CANCEL {variant} case {index} row {row} native ID"
            );
            assert_eq!(
                actual.symbol, expected.symbol,
                "CANCEL {variant} case {index} row {row} native symbol"
            );
            assert_eq!(
                actual.r#type, expected.r#type,
                "CANCEL {variant} case {index} row {row} native type"
            );
            assert_eq!(
                actual.status, expected.status,
                "CANCEL {variant} case {index} row {row} native status"
            );
            assert_eq!(
                actual.price.to_number().to_bits(),
                12.3f64.to_bits(),
                "CANCEL native unchanged price"
            );
            assert_eq!(
                actual.amount.to_bits(),
                3.6f64.to_bits(),
                "CANCEL native unchanged quantity"
            );
        }
        drop(actual);
        assert_eq!(
            notifications.borrow().len(),
            expected_notifications,
            "CANCEL {variant} case {index} native notification count"
        );
        assert_eq!(
            later.borrow().len(),
            expected_later,
            "CANCEL {variant} case {index} native later subscriber count"
        );
        assert_eq!(
            store.update_ids().orders,
            (1 + expected_notifications) as f64,
            "CANCEL {variant} case {index} native authored version delta"
        );
        let expected_emissions = origin
            .map(|role| vec![emission(role, case.get("msg"))])
            .unwrap_or_default();
        assert_eq!(
            *emissions.borrow(),
            expected_emissions,
            "CANCEL {variant} case {index} native emitted message"
        );
        let expected_origin = if origin.is_some() && case["emitReject"] == true {
            Some("emitter")
        } else {
            origin
        };
        match (&result, expected_origin) {
            (Ok(()), None) => {}
            (Err(CancelSymbolError::Request(error)), Some(role @ ("regular" | "algo"))) => {
                assert!(
                    Rc::ptr_eq(error, if role == "algo" { &algo } else { &regular }),
                    "CANCEL native original request identity"
                );
            }
            (Err(CancelSymbolError::Emission(error)), Some("emitter")) => {
                assert!(
                    Rc::ptr_eq(error, &emitter),
                    "CANCEL native emitter precedence/identity"
                );
            }
            (Err(CancelSymbolError::Store(error)), Some("listener")) => {
                let StoreMutationError::Notification(notification) = error.as_ref() else {
                    panic!("expected original notification {error:?}")
                };
                assert_eq!(
                    notification.subscriber_id(),
                    0,
                    "CANCEL native first subscriber ID"
                );
                let original = notification
                    .cause()
                    .source()
                    .unwrap()
                    .downcast_ref::<ExchangeError>()
                    .unwrap();
                assert!(
                    std::ptr::eq(original, listener.as_ref()),
                    "CANCEL native listener opaque identity"
                );
                assert_eq!(
                    original,
                    listener.as_ref(),
                    "CANCEL native complete listener metadata"
                );
            }
            other => panic!("CANCEL {variant} case {index} native error role mismatch: {other:?}"),
        }
        let expected_requests = if !pm_spot && case["regular"] != "sync-reject" {
            2
        } else {
            1
        };
        assert_eq!(
            requests.borrow().len(),
            expected_requests,
            "CANCEL {variant} case {index} native DELETE count"
        );
        assert_eq!(
            view_reads.get(),
            usize::from(!pm_spot),
            "CANCEL native futures snapshot read count"
        );
        for (request_index, (path, request, _)) in requests.borrow().iter().enumerate() {
            let expected_path = match (variant, request_index) {
                ("binance", 0) => "/fapi/v1/allOpenOrders",
                ("binance", 1) => "/fapi/v1/algoOpenOrders",
                ("binancepm", _) => "/papi/v1/um/allOpenOrders",
                _ => "/api/v3/openOrders",
            };
            assert_eq!(*path, expected_path, "CANCEL {variant} native endpoint");
            assert_eq!(request.symbol, symbol, "CANCEL native request symbol");
        }
        if !pm_spot {
            assert_eq!(
                updates
                    .borrow()
                    .iter()
                    .map(|(ids, _)| ids.clone())
                    .collect::<Vec<_>>(),
                expected_updates,
                "CANCEL futures native current-ID groups"
            );
            for (_, status) in updates.borrow().iter() {
                assert_eq!(
                    *status,
                    OrderStatus::Canceled,
                    "CANCEL futures native status argument"
                );
            }
        }
        for (_, rows) in notifications.borrow().iter() {
            assert_eq!(
                rows.len(),
                expected_rows.len(),
                "native notification authored row count"
            );
        }
        // All native assertions above; decoded source state/wire views below.
        assert_eq!(
            case["error"].get("identity").and_then(Value::as_str),
            expected_origin,
            "CANCEL source error identity role"
        );
        if expected_origin.is_none() {
            assert!(case["error"].is_null());
            assert_eq!(case["returnedUndefined"], true);
        }
        assert_eq!(
            case["orders"].as_array().unwrap().len(),
            expected_rows.len()
        );
        assert_eq!(
            case["requests"].as_array().unwrap().len(),
            expected_requests
        );
        assert_eq!(
            case["notifications"].as_array().unwrap().len(),
            expected_notifications
        );
        assert_eq!(case["later"].as_array().unwrap().len(), expected_later);
        for (stage, source) in later.borrow().iter().zip(case["later"].as_array().unwrap()) {
            assert_eq!(*stage, source.as_str().unwrap());
        }
        let source_updates = case["updates"].as_array().unwrap();
        assert_eq!(source_updates.len(), expected_updates.len());
        for (update, ids) in source_updates.iter().zip(&expected_updates) {
            assert_eq!(update["ids"].as_array().unwrap().len(), ids.len());
            for (source, id) in update["ids"].as_array().unwrap().iter().zip(ids) {
                assert_eq!(source.as_str().unwrap(), id);
            }
            assert_eq!(
                serde_json::from_value::<OrderStatus>(update["status"].clone()).unwrap(),
                OrderStatus::Canceled
            );
        }
        for (actual, source) in store
            .orders()
            .iter()
            .zip(case["orders"].as_array().unwrap())
        {
            assert_eq!(actual.id, source["id"].as_str().unwrap());
            assert_eq!(
                actual.status,
                serde_json::from_value::<OrderStatus>(source["status"].clone()).unwrap()
            );
        }
        assert_eq!(
            store.update_ids().orders,
            case["updateId"].as_f64().unwrap()
        );
        assert_eq!(
            emissions.borrow().as_slice(),
            case["emissions"].as_array().unwrap()
        );
        for (i, (path, request, _)) in requests.borrow().iter().enumerate() {
            assert_eq!(*path, case["requests"][i]["endpoint"].as_str().unwrap());
            assert_eq!(
                request.symbol,
                case["requests"][i]["options"]["params"]["symbol"]
                    .as_str()
                    .unwrap()
            );
        }
        for (i, (stage, rows)) in notifications.borrow().iter().enumerate() {
            assert_eq!(*stage, case["notifications"][i]["stage"].as_str().unwrap());
            assert_eq!(
                case["notifications"][i]["orders"].as_array().unwrap().len(),
                expected_rows.len()
            );
            for ((id, status), source) in rows
                .iter()
                .zip(case["notifications"][i]["orders"].as_array().unwrap())
            {
                assert_eq!(id, source["id"].as_str().unwrap());
                assert_eq!(
                    *status,
                    serde_json::from_value::<OrderStatus>(source["status"].clone()).unwrap()
                );
            }
        }
        unsubscribe();
        unsubscribe_later();
        completed += 1;
    }
    assert_eq!(completed, count, "fixed completed scenario count {variant}");
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn futures_cancel_symbol_matches_source() {
    compare("binance", 43);
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn pm_cancel_symbol_matches_source() {
    compare("binancepm", 26);
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn spot_cancel_symbol_matches_source() {
    compare("binancespot", 26);
}

#[test]
fn futures_waits_for_both_settlements_without_repolling_ready_future() {
    for first in [0, 1] {
        for regular_reject in [false, true] {
            for algo_reject in [false, true] {
                let store = fixture();
                let handles: Vec<_> = initial()
                    .into_iter()
                    .map(|row| Rc::new(RefCell::new(row)))
                    .collect();
                let ready = [Cell::new(false), Cell::new(false)];
                let polls = [Cell::new(0), Cell::new(0)];
                let calls = Cell::new(0);
                let emits = Cell::new(0);
                let regular = sentinel("regular", None);
                let algo = sentinel("algo", None);
                let delete = |_: &'static str, _: CancelSymbolRequest| {
                    let index = calls.get();
                    calls.set(index + 1);
                    let error = if index == 0 {
                        regular.clone()
                    } else {
                        algo.clone()
                    };
                    let reject = if index == 0 {
                        regular_reject
                    } else {
                        algo_reject
                    };
                    let ready = &ready;
                    let polls = &polls;
                    Ok(std::future::poll_fn(move |_| {
                        polls[index].set(polls[index].get() + 1);
                        if ready[index].get() {
                            Poll::Ready(if reject { Err(error.clone()) } else { Ok(()) })
                        } else {
                            Poll::Pending
                        }
                    }))
                };
                let mut future = std::pin::pin!(cancel_binance_symbol_orders(
                    || handles.clone(),
                    "BTCUSDT",
                    delete,
                    |group, status| {
                        let ids: Vec<_> = group.iter().map(|h| h.borrow().id.clone()).collect();
                        let refs: Vec<_> = ids.iter().map(String::as_str).collect();
                        Store::update_orders_status(store.as_ref(), &refs, status)
                    },
                    |_| {
                        emits.set(emits.get() + 1);
                        Ok(())
                    }
                ));
                let mut context = Context::from_waker(Waker::noop());
                assert!(future.as_mut().poll(&mut context).is_pending());
                assert_eq!(calls.get(), 2, "CANCEL native both DELETEs started");
                ready[first].set(true);
                assert!(future.as_mut().poll(&mut context).is_pending());
                let finished_polls = polls[first].get();
                assert!(future.as_mut().poll(&mut context).is_pending());
                assert_eq!(
                    polls[first].get(),
                    finished_polls,
                    "CANCEL native completed future not repolled"
                );
                assert!(
                    store
                        .orders()
                        .iter()
                        .all(|order| order.status == OrderStatus::Open),
                    "CANCEL native no early mutation after one settlement"
                );
                assert_eq!(emits.get(), 0, "CANCEL native no early rejection emission");
                ready[1 - first].set(true);
                let Poll::Ready(result) = future.as_mut().poll(&mut context) else {
                    panic!("both settled must finish")
                };
                match result {
                    Ok(()) => assert!(!regular_reject && !algo_reject),
                    Err(CancelSymbolError::Request(error)) => assert!(Rc::ptr_eq(
                        &error,
                        if regular_reject { &regular } else { &algo }
                    )),
                    other => panic!("unexpected {other:?}"),
                };
                assert_eq!(emits.get(), usize::from(regular_reject || algo_reject));
            }
        }
    }
}
