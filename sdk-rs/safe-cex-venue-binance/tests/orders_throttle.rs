use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Order;
use safe_cex_venue_binance::orders_throttle::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn error(message: &str) -> Rc<ExchangeError> {
    let mut error = ExchangeError::new(message);
    error.name = "RangeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    Rc::new(error)
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("authored fixture exceeded three polls");
}
fn encode_timer(timer: &OrderTimer) -> Value {
    match timer {
        OrderTimer::Undefined => json!("undefined"),
        OrderTimer::Number(n) if n.is_nan() => json!("nan"),
        OrderTimer::Number(n) if *n == 0.0 && n.is_sign_negative() => json!("-zero"),
        OrderTimer::Number(n) => json!(n),
        OrderTimer::Json(value) => value.as_ref().clone(),
        OrderTimer::Handle(id) => json!({"handle":**id}),
    }
}
fn assert_timer(actual: &OrderTimer, expected: &OrderTimer, label: &str) {
    match (actual, expected) {
        (OrderTimer::Undefined, OrderTimer::Undefined) => (),
        (OrderTimer::Number(a), OrderTimer::Number(b)) => assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "{label} native numeric timer bits"
        ),
        (OrderTimer::Handle(a), OrderTimer::Handle(b)) => {
            assert!(Rc::ptr_eq(a, b), "{label} native timer handle identity")
        }
        (OrderTimer::Json(a), OrderTimer::Json(b)) => {
            assert!(Rc::ptr_eq(a, b), "{label} native JSON timer identity")
        }
        _ => panic!("{label} native timer variant"),
    }
}
fn source_value(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path} source decoded number"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} source array count");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                source_value(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path} source own keys"
            );
            for (key, value) in a {
                source_value(value, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path} source decoded value"),
    }
}
fn compare_source(variant: &str, method: &str, native: Vec<Value>) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/orders-throttle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 76, "fixed source floor 2*(18+20)");
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r["variant"] == variant && r["method"] == method)
        .collect();
    assert_eq!(rows.len(), if method == "throttle" { 18 } else { 20 });
    for (index, (actual, expected)) in rows.into_iter().zip(native).enumerate() {
        source_value(
            &actual,
            &expected,
            &format!("ORDERS {variant} {method} source case {index}"),
        );
    }
}
fn throttle(variant: &str) {
    let names = [
        "normal",
        "unset",
        "null",
        "false",
        "zero",
        "negative-zero",
        "empty-string",
        "nan",
        "negative",
        "object-zero",
        "clear-fail",
        "set-fail",
        "late-callback",
        "late-field-set-fail",
        "get-callback-fail",
        "disposed",
        "empty-array",
        "return-unset",
    ];
    let mut native = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let initial = match name {
            "unset" => OrderTimer::Undefined,
            "null" => OrderTimer::Json(Rc::new(Value::Null)),
            "false" => OrderTimer::Json(Rc::new(json!(false))),
            "zero" => OrderTimer::Number(0.0),
            "negative-zero" => OrderTimer::Number(-0.0),
            "empty-string" => OrderTimer::Json(Rc::new(json!(""))),
            "nan" => OrderTimer::Number(f64::NAN),
            "negative" => OrderTimer::Number(-1.0),
            "object-zero" => OrderTimer::Handle(Rc::new(0)),
            "empty-array" => OrderTimer::Json(Rc::new(json!([]))),
            _ => OrderTimer::Handle(Rc::new(11)),
        };
        let created = OrderTimer::Handle(Rc::new(42));
        let replacement = OrderTimer::Handle(Rc::new(33));
        let state = OrderThrottleState {
            timer: RefCell::new(initial.clone()),
            is_disposed: Cell::new(name == "disposed"),
        };
        let trace = RefCell::new(Vec::<Value>::new());
        let calls = Rc::new(RefCell::new(Vec::<u64>::new()));
        let cleared = RefCell::new(Vec::<OrderTimer>::new());
        let delays = RefCell::new(Vec::<f64>::new());
        let callbacks: Vec<FetchOrdersCallback> = (1..=3)
            .map(|id| {
                let calls = calls.clone();
                Rc::new(move || calls.borrow_mut().push(id)) as FetchOrdersCallback
            })
            .collect();
        let current = RefCell::new(callbacks[0].clone());
        let registered = RefCell::new(None::<FetchOrdersCallback>);
        let error = error("throttle fixture");
        let clear = |timer: OrderTimer| {
            assert_timer(&timer, &initial, "cancellation");
            cleared.borrow_mut().push(timer.clone());
            trace
                .borrow_mut()
                .push(json!(["clear", encode_timer(&timer)]));
            if name == "clear-fail" {
                return Err(error.clone());
            }
            if name == "late-callback" {
                *current.borrow_mut() = callbacks[1].clone();
            }
            if name == "late-field-set-fail" {
                *state.timer.borrow_mut() = replacement.clone();
            }
            Ok(())
        };
        let get_fetch = || {
            trace.borrow_mut().push(json!(["get-callback"]));
            if name == "get-callback-fail" {
                Err(error.clone())
            } else {
                Ok(current.borrow().clone())
            }
        };
        let set = |callback: FetchOrdersCallback, delay: f64| {
            trace.borrow_mut().push(json!(["set", delay]));
            delays.borrow_mut().push(delay);
            *registered.borrow_mut() = Some(callback);
            if name == "set-fail" || name == "late-field-set-fail" {
                Err(error.clone())
            } else {
                Ok(if name == "return-unset" {
                    OrderTimer::Undefined
                } else {
                    created.clone()
                })
            }
        };
        let result = if variant == "binance" {
            throttle_binance_orders(&state, clear, get_fetch, set)
        } else {
            throttle_binance_pm_orders(&state, clear, get_fetch, set)
        };
        let falsy = [
            "unset",
            "null",
            "false",
            "zero",
            "negative-zero",
            "empty-string",
            "nan",
        ]
        .contains(&name);
        assert_eq!(
            cleared.borrow().len(),
            usize::from(!falsy),
            "ORDERS {variant} throttle native cancellation count case {index}"
        );
        let set_reached = name != "clear-fail" && name != "get-callback-fail";
        assert_eq!(
            *delays.borrow(),
            if set_reached { vec![10_000.0] } else { vec![] },
            "ORDERS {variant} throttle native delay case {index}"
        );
        let failed = [
            "clear-fail",
            "set-fail",
            "late-field-set-fail",
            "get-callback-fail",
        ]
        .contains(&name);
        match &result {
            Ok(()) => assert!(!failed),
            Err(e) => {
                assert!(failed);
                assert_eq!(e.name, "RangeError", "native error name");
                assert_eq!(e.message, "throttle fixture", "native error message");
                assert_eq!(
                    observed_error_code(&e.code),
                    Some(json!("fixture-code")),
                    "native error metadata"
                );
                assert!(
                    Rc::ptr_eq(e, &error),
                    "native original scheduler error identity"
                );
            }
        }
        let expected = if failed {
            if name == "late-field-set-fail" {
                &replacement
            } else {
                &initial
            }
        } else if name == "return-unset" {
            &OrderTimer::Undefined
        } else {
            &created
        };
        assert_timer(
            &state.timer.borrow(),
            expected,
            &format!("ORDERS {variant} throttle case {index}"),
        );
        assert_eq!(state.is_disposed.get(), name == "disposed");
        let mut identity = Value::Null;
        if let Some(callback) = registered.borrow().as_ref() {
            assert!(
                Rc::ptr_eq(callback, &callbacks[usize::from(name == "late-callback")]),
                "ORDERS {variant} throttle native callback identity case {index}"
            );
            identity = json!(true);
            if !failed {
                *current.borrow_mut() = callbacks[2].clone();
                callback();
            }
        }
        assert_eq!(
            *calls.borrow(),
            if set_reached && !failed {
                vec![if name == "late-callback" { 2 } else { 1 }]
            } else {
                vec![]
            },
            "native registered callback retains function after replacement"
        );
        assert_eq!(
            trace.borrow().len(),
            usize::from(!falsy) + usize::from(name != "clear-fail") + usize::from(set_reached),
            "native authored scheduler effect count"
        );
        native.push(json!({"variant":variant,"method":"throttle","name":name,"trace":*trace.borrow(),"calls":*calls.borrow(),"delay":delays.borrow().first().copied(),"identity":identity,"timer":encode_timer(&state.timer.borrow()),"disposed":state.is_disposed.get(),"failure":if failed{json!("original")}else{Value::Null}}));
    }
    assert_eq!(
        native.len(),
        18,
        "authored native throttle floor stopped return-unset"
    );
    compare_source(variant, "throttle", native);
}
fn loaded(marker: Rc<Value>, secondary: bool, orders: bool, missing: bool) -> LoadedProperties {
    let mut props = vec![
        ("balance".into(), Rc::new(json!(!secondary))),
        ("orders".into(), Rc::new(json!(orders))),
        ("markets".into(), Rc::new(json!(secondary))),
        ("tickers".into(), Rc::new(json!(!secondary))),
        ("positions".into(), Rc::new(json!(secondary))),
        ("marker".into(), marker),
    ];
    if missing {
        props.retain(|(key, _)| key != "orders");
    }
    props
}
fn value<'a>(loaded: &'a LoadedProperties, key: &str) -> Option<&'a Rc<Value>> {
    loaded.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
fn orders() -> Rc<Vec<Order>> {
    Rc::new([("first",12.3,3.6),("second",14.2,2.1)].into_iter().map(|(id,price,amount)|serde_json::from_value(json!({"id":id,"symbol":"BTCUSDT","type":"limit","status":"open","side":"buy","price":price,"amount":amount,"filled":0,"remaining":amount,"reduceOnly":false,"lastUpdate":100})).unwrap()).collect())
}
fn fetch(variant: &str) {
    let names = [
        "normal",
        "disposed",
        "disposed-during-fetch",
        "stale",
        "stale-during-fetch",
        "stale-cleared-during-fetch",
        "fetch-sync-fail",
        "fetch-reject",
        "stale-getter-fail",
        "loaded-getter-fail",
        "update-fail",
        "late-loaded",
        "existing-orders-key",
        "missing-orders-key",
        "stale-getter-disposes",
        "loaded-getter-disposes",
        "fetch-replaces-timer",
        "update-replaces-timer",
        "update-getter-fail",
        "update-getter-changes-store",
    ];
    let mut native = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let state = OrderThrottleState {
            timer: RefCell::new(OrderTimer::Handle(Rc::new(11))),
            is_disposed: Cell::new(name == "disposed"),
        };
        let changed_timer = OrderTimer::Handle(Rc::new(if name == "update-replaces-timer" {
            44
        } else {
            33
        }));
        let trace = RefCell::new(Vec::<Value>::new());
        let polls = Cell::new(0);
        let stale = Cell::new(name == "stale" || name == "stale-cleared-during-fetch");
        let current_store = Cell::new(10);
        let marker = Rc::new(json!({"id":91}));
        let primary = RefCell::new(loaded(
            marker.clone(),
            false,
            name == "existing-orders-key",
            name == "missing-orders-key",
        ));
        let secondary = loaded(marker.clone(), true, false, false);
        let orders = orders();
        let captured = RefCell::new(None::<OrdersRefreshPatch<Rc<Vec<Order>>>>);
        let receiver = Cell::new(None::<u64>);
        let updates = Cell::new(0);
        let error = error("refresh fixture");
        let fetch_orders = || {
            trace.borrow_mut().push(json!(["fetch"]));
            assert_timer(
                &state.timer.borrow(),
                &OrderTimer::Undefined,
                "native timer cleared before fetch invocation",
            );
            if name == "fetch-sync-fail" {
                return Err(error.clone());
            }
            Ok(std::future::poll_fn(|_| {
                polls.set(polls.get() + 1);
                if polls.get() == 1 {
                    return Poll::Pending;
                }
                if name == "disposed-during-fetch" {
                    state.is_disposed.set(true);
                }
                if name == "stale-during-fetch" {
                    stale.set(true);
                }
                if name == "stale-cleared-during-fetch" {
                    stale.set(false);
                }
                if name == "late-loaded" {
                    let mut p = primary.borrow_mut();
                    for (key, v) in p.iter_mut() {
                        if key == "balance" {
                            *v = Rc::new(json!(false));
                        }
                        if key == "positions" {
                            *v = Rc::new(json!(true));
                        }
                    }
                }
                if name == "fetch-replaces-timer" {
                    *state.timer.borrow_mut() = changed_timer.clone();
                }
                Poll::Ready(if name == "fetch-reject" {
                    Err(error.clone())
                } else {
                    Ok(orders.clone())
                })
            }))
        };
        let snapshot_stale = || {
            trace.borrow_mut().push(json!(["stale"]));
            if name == "stale-getter-fail" {
                return Err(error.clone());
            }
            if name == "stale-getter-disposes" {
                state.is_disposed.set(true);
            }
            Ok(stale.get())
        };
        let get_update = || {
            let owner = current_store.get();
            trace.borrow_mut().push(json!(["get-update", owner]));
            if name == "update-getter-fail" {
                return Err(error.clone());
            }
            if name == "update-getter-changes-store" {
                current_store.set(20);
            }
            let trace = &trace;
            let captured = &captured;
            let receiver = &receiver;
            let updates = &updates;
            let state = &state;
            let changed_timer = &changed_timer;
            let error = &error;
            Ok(move |patch: OrdersRefreshPatch<Rc<Vec<Order>>>| {
                trace.borrow_mut().push(json!(["update", owner]));
                receiver.set(Some(owner));
                updates.set(updates.get() + 1);
                *captured.borrow_mut() = Some(patch);
                if name == "update-replaces-timer" {
                    *state.timer.borrow_mut() = changed_timer.clone();
                }
                if name == "update-fail" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            })
        };
        let get_loaded = || {
            let owner = current_store.get();
            trace.borrow_mut().push(json!(["get-loaded", owner]));
            if name == "loaded-getter-fail" {
                return Err(error.clone());
            }
            if name == "loaded-getter-disposes" {
                state.is_disposed.set(true);
            }
            Ok(if owner == 20 {
                secondary.clone()
            } else {
                primary.borrow().clone()
            })
        };
        if variant == "binance" {
            run(fetch_throttled_binance_orders(
                &state,
                fetch_orders,
                snapshot_stale,
                get_update,
                get_loaded,
            ));
        } else {
            run(fetch_throttled_binance_pm_orders(
                &state,
                fetch_orders,
                snapshot_stale,
                get_update,
                get_loaded,
            ));
        }
        let should_update = ![
            "disposed",
            "disposed-during-fetch",
            "stale",
            "stale-during-fetch",
            "fetch-sync-fail",
            "fetch-reject",
            "stale-getter-fail",
            "loaded-getter-fail",
            "update-getter-fail",
        ]
        .contains(&name);
        assert_eq!(
            updates.get(),
            u64::from(should_update),
            "ORDERS {variant} fetch native update count case {index}"
        );
        assert_eq!(
            receiver.get(),
            if should_update { Some(10) } else { None },
            "native captured Store receiver"
        );
        assert_eq!(
            current_store.get(),
            if name == "update-getter-changes-store" {
                20
            } else {
                10
            },
            "native current Store"
        );
        assert_eq!(
            polls.get(),
            if name == "disposed" || name == "fetch-sync-fail" {
                0
            } else {
                2
            },
            "native real await suspension count"
        );
        assert_eq!(
            state.is_disposed.get(),
            [
                "disposed",
                "disposed-during-fetch",
                "stale-getter-disposes",
                "loaded-getter-disposes"
            ]
            .contains(&name),
            "native final disposal state"
        );
        assert_timer(
            &state.timer.borrow(),
            if name == "fetch-replaces-timer" || name == "update-replaces-timer" {
                &changed_timer
            } else {
                &OrderTimer::Undefined
            },
            "native final refresh timer",
        );
        let expected_keys = if name == "missing-orders-key" {
            vec![
                "balance",
                "markets",
                "tickers",
                "positions",
                "marker",
                "orders",
            ]
        } else {
            vec![
                "balance",
                "orders",
                "markets",
                "tickers",
                "positions",
                "marker",
            ]
        };
        let mut wire_loaded = Value::Null;
        let mut wire_orders = Value::Null;
        let mut keys = Vec::<String>::new();
        if let Some(patch) = captured.borrow().as_ref() {
            assert!(should_update);
            assert!(
                Rc::ptr_eq(&patch.orders, &orders),
                "ORDERS {variant} fetch native orders identity case {index}"
            );
            assert_eq!(
                patch.orders.len(),
                2,
                "native independent decoded order count"
            );
            for (row, (id, price, amount)) in
                [("first", 12.3_f64, 3.6_f64), ("second", 14.2_f64, 2.1_f64)]
                    .into_iter()
                    .enumerate()
            {
                let order = &patch.orders[row];
                assert_eq!(order.id, id, "native order id");
                assert_eq!(
                    order.price.to_number().to_bits(),
                    price.to_bits(),
                    "native decoded order price"
                );
                assert_eq!(
                    order.amount.to_bits(),
                    amount.to_bits(),
                    "native decoded order amount"
                );
            }
            assert_eq!(
                patch.loaded.len(),
                6,
                "native independent loaded own-key count"
            );
            assert_eq!(
                patch
                    .loaded
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .collect::<Vec<_>>(),
                expected_keys,
                "native loaded property order"
            );
            assert_eq!(
                value(&patch.loaded, "orders").unwrap().as_bool(),
                Some(true),
                "ORDERS {variant} fetch native loaded orders case {index}"
            );
            let changed = name == "update-getter-changes-store";
            let late = name == "late-loaded";
            for (key, expected) in [
                ("balance", !changed && !late),
                ("markets", changed),
                ("tickers", !changed),
                ("positions", changed || late),
            ] {
                assert_eq!(
                    value(&patch.loaded, key).unwrap().as_bool(),
                    Some(expected),
                    "native preserved loaded flag {key}"
                );
            }
            assert!(
                Rc::ptr_eq(value(&patch.loaded, "marker").unwrap(), &marker),
                "native loaded extra object identity"
            );
            assert_eq!(
                value(&patch.loaded, "marker").unwrap()["id"].as_u64(),
                Some(91),
                "native decoded loaded extra value"
            );
            keys = patch.loaded.iter().map(|(key, _)| key.clone()).collect();
            wire_loaded = Value::Object(
                patch
                    .loaded
                    .iter()
                    .map(|(key, v)| (key.clone(), v.as_ref().clone()))
                    .collect(),
            );
            wire_orders = json!(
                patch
                    .orders
                    .iter()
                    .map(|o| json!({"id":o.id,"price":o.price,"amount":o.amount}))
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(!should_update);
        }
        let source_orders = value(&primary.borrow(), "orders").cloned();
        assert_eq!(
            source_orders.as_ref().and_then(|v| v.as_bool()),
            if name == "missing-orders-key" {
                None
            } else {
                Some(name == "existing-orders-key")
            },
            "native loaded input is not mutated"
        );
        let should_fetch = name != "disposed";
        let should_stale = ![
            "disposed",
            "disposed-during-fetch",
            "fetch-sync-fail",
            "fetch-reject",
        ]
        .contains(&name);
        let should_get_update =
            should_stale && !["stale", "stale-during-fetch", "stale-getter-fail"].contains(&name);
        let should_get_loaded = should_get_update && name != "update-getter-fail";
        assert_eq!(
            trace.borrow().len(),
            usize::from(should_fetch)
                + usize::from(should_stale)
                + usize::from(should_get_update)
                + usize::from(should_get_loaded)
                + usize::from(should_update),
            "native independent refresh effect count"
        );
        native.push(json!({"variant":variant,"method":"fetch","name":name,"trace":*trace.borrow(),"updates":updates.get(),"receiver":receiver.get(),"timer":encode_timer(&state.timer.borrow()),"disposed":state.is_disposed.get(),"loaded":wire_loaded,"keys":keys,"orders":wire_orders,"sourceLoadedOrders":source_orders.map(|v|v.as_ref().clone()).unwrap_or(json!("undefined")),"currentStore":current_store.get(),"identity":if should_update{json!(true)}else{Value::Null}}));
    }
    assert_eq!(
        native.len(),
        20,
        "authored native refresh floor stopped update-getter-changes-store"
    );
    compare_source(variant, "fetch", native);
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_throttle() {
    throttle("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_throttle() {
    throttle("binancepm");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_fetch() {
    fetch("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_fetch() {
    fetch("binancepm");
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
