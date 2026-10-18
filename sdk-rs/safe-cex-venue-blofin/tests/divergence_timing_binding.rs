//! Scheduling observations through the actual public native exchange and Store.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::PositionMode;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<T>(runtime: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("bounded native mode operation did not settle")
}

#[test]
fn mode_change_logs_run_in_the_call_prefix_before_the_caller_can_poll() {
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/divergence_timing_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["prefix"].as_array().unwrap().len(), 3);
    assert_eq!(source["settled"].as_array().unwrap().len(), 4);
    let (venue, runtime, store, wire) = support::fixture();
    Store::update(store.as_ref(), StoreUpdate {
        fetched_leverage: Some(serde_json::from_value(json!({"BTCUSDT":{"buyLeverage":3,"sellLeverage":5,"marginType":"cross","positionMode":"oneway"}})).unwrap()),
        ..Default::default()
    }, false).unwrap();
    wire.bodies.borrow_mut().insert(
        "/api/v1/account/set-position-mode".into(),
        json!({"code":"0","msg":"success","data":{"positionMode":"long_short_mode"}}),
    );
    let logs = Rc::new(RefCell::new(Vec::<Value>::new()));
    let seen = logs.clone();
    venue.base().on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(json!(
                args.iter()
                    .map(|arg| String::from_utf16(&arg.to_js_string().unwrap()).unwrap())
                    .collect::<Vec<_>>()
            ));
            Ok(())
        }),
    );
    let pending = venue.change_position_mode(true, None);
    assert_eq!(
        json!(&*logs.borrow()),
        source["prefix"],
        "MODE-PREFIX actual public call must emit the three maintained diagnostics before returning its future"
    );
    drive(&runtime, pending).unwrap();
    assert_eq!(json!(&*logs.borrow()), source["settled"]);
    assert_eq!(
        store.fetched_leverage().unwrap()["BTCUSDT"].position_mode,
        Some(PositionMode::Hedged)
    );
    assert_eq!(wire.requests.borrow().len(), 1);
    venue.dispose().unwrap();
}

#[test]
fn rejected_order_family_does_not_cancel_a_sibling_pagination() {
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/divergence_retry_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["first"].as_array().unwrap().len(), 3);
    let (venue, runtime, _store, wire) = support::fixture();
    wire.bodies.borrow_mut().insert(
        "/api/v1/trade/orders-pending".into(),
        json!({"code":"0","msg":"normal family refused","data":null}),
    );
    wire.pages.borrow_mut().insert(
        "/api/v1/trade/orders-tpsl-pending".into(),
        [
            json!({"code":"0","data":[{"tpslId":"page-a"}]}),
            json!({"code":"0","data":[{"tpslId":"page-b"}]}),
            json!({"code":"0","data":[]}),
        ]
        .into(),
    );
    *wire.paused_cursor.borrow_mut() = Some("page-b".into());
    let logs = Rc::new(RefCell::new(Vec::new()));
    let seen = logs.clone();
    venue.base().on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut()
                .push(String::from_utf16(&args[0].to_js_string().unwrap()).unwrap());
            Ok(())
        }),
    );
    let pending = venue.fetch_orders(None);
    drop(pending); // The public waiter owns neither the operation nor its siblings.
    runtime.pump();
    let after: Vec<Value> = wire
        .requests
        .borrow()
        .iter()
        .filter(|r| support::path(&r.url) == "/api/v1/trade/orders-tpsl-pending")
        .map(|r| {
            r.params
                .as_ref()
                .and_then(|p| p.get("after"))
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect();
    assert_eq!(
        json!(after),
        json!([null, "page-a", "page-b"]),
        "ORDER-SIBLING public fetchOrders must finish the successful sibling's pagination after another family rejects"
    );
    assert_eq!(
        wire.requests
            .borrow()
            .iter()
            .filter(|r| support::path(&r.url) == "/api/v1/trade/orders-pending")
            .count(),
        1,
        "the failed join enters backoff without waiting for the successful sibling or starting another attempt"
    );
    assert_eq!(
        runtime.now.get(),
        1_710_000_000_000.0,
        "no retry timer advanced"
    );
    assert_eq!(source["backoff"], true);
    assert!(
        logs.borrow()
            .iter()
            .any(|s| s == "[blofin] fetchOrders attempt 1 failed, retrying..."),
        "ORDER-FAIL-FAST backoff begins while the successful sibling's last HTTP response is still held"
    );
    assert!(
        wire.paused_waker.borrow().is_some(),
        "actual sibling response is held"
    );
    wire.paused_cursor.borrow_mut().take();
    wire.paused_waker.borrow_mut().take().unwrap().wake();
    runtime.pump();
    venue.dispose().unwrap();
}

fn cached_mode_race(kind: &str) {
    for dropped in [false, true] {
        cached_mode_race_lifetime(kind, dropped);
    }
}
fn cached_mode_race_lifetime(kind: &str, dropped: bool) {
    use safe_cex_exchange::worker::CallableMethodsInWorker;
    use safe_cex_types::PlaceOrderOpts;
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/remaining_fix_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(profiles.len(), 4, "independent source case floor");
    let source = profiles.iter().find(|row| row["kind"] == kind).unwrap();
    let (venue, runtime, store, wire) = support::fixture();
    let market = json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}},"contractSize":0.001,"info":{"lotSize":"1","minSize":"1","contractValue":0.001}});
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![serde_json::from_value(market).unwrap()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    for (path, data) in [
        (
            "/api/v1/trade/batch-orders",
            json!([{"orderId":"n1","code":"0","msg":"success"}]),
        ),
        (
            "/api/v1/trade/order-tpsl",
            json!({"tpslId":"a1","code":"0","msg":"success"}),
        ),
        (
            "/api/v1/trade/order-algo",
            json!({"algoId":"t1","code":"0"}),
        ),
    ] {
        wire.bodies
            .borrow_mut()
            .insert(path.into(), json!({"code":"0","msg":"success","data":data}));
    }
    venue
        .after_method_called_in_worker(
            &Value::Null,
            CallableMethodsInWorker::ChangePositionMode,
            &[json!(false)],
        )
        .unwrap();
    venue
        .after_method_called_in_worker(
            &Value::Null,
            CallableMethodsInWorker::ChangeMarginType,
            &[json!(false)],
        )
        .unwrap();
    let make_opts = |kind: &str| -> PlaceOrderOpts {
        let kind_type = match kind {
            "normal" => "limit",
            "algo" => "stop_market",
            "trigger" => "conditional",
            _ => unreachable!(),
        };
        let mut opts =
            json!({"symbol":"BTCUSDT","type":kind_type,"side":"buy","amount":2,"price":100});
        if kind == "trigger" {
            opts["triggerPrice"] = json!(105);
        }
        serde_json::from_value(opts).unwrap()
    };
    let pending = async {
        if kind == "batch" {
            venue
                .base()
                .place_orders(
                    venue.as_ref(),
                    ["normal", "algo", "trigger"]
                        .iter()
                        .map(|kind| make_opts(kind))
                        .collect(),
                )
                .await
        } else {
            venue
                .base()
                .place_order(venue.as_ref(), make_opts(kind))
                .await
        }
    };
    let mut pending = Box::pin(pending);
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(wire.requests.borrow().len(), 0, "same source call prefix");
    venue
        .after_method_called_in_worker(
            &Value::Null,
            if kind == "algo" {
                CallableMethodsInWorker::ChangeMarginType
            } else {
                CallableMethodsInWorker::ChangePositionMode
            },
            &[json!(true)],
        )
        .unwrap();
    if kind == "batch" {
        venue
            .after_method_called_in_worker(
                &Value::Null,
                CallableMethodsInWorker::ChangeMarginType,
                &[json!(true)],
            )
            .unwrap();
    }
    if dropped {
        drop(pending);
        runtime.pump();
    } else {
        let ids = drive(&runtime, pending).unwrap();
        assert_eq!(json!(ids), source["ids"]);
    }
    assert_eq!(
        wire.requests.borrow().len(),
        if kind == "batch" { 3 } else { 1 }
    );
    let requests: Vec<Value> = wire.requests.borrow().iter().map(|r| {
        let body = r.body.as_ref().unwrap().untransformed().unwrap().clone().into_value().unwrap();
        let row = body.as_array().map(|a| &a[0]).unwrap_or(&body);
        json!({"path":r.url,"marginMode":row["marginMode"],"positionSide":row["positionSide"],"side":row["side"],"size":row["size"],"type":row["orderType"],"trigger":row["triggerPrice"]})
    }).collect();
    assert_eq!(
        json!(requests),
        source["requests"],
        "CACHED-AWAIT {kind}: a fulfilled cached reader still resumes in a later reaction before observing the next mode"
    );
    venue.dispose().unwrap();
}
#[test]
fn cached_margin_read_in_normal_formatter_defers_the_position_mode_observation() {
    cached_mode_race("normal");
}
#[test]
fn cached_position_side_in_algo_formatter_defers_the_margin_observation() {
    cached_mode_race("algo");
}
#[test]
fn cached_margin_read_in_trigger_formatter_defers_the_position_mode_observation() {
    cached_mode_race("trigger");
}

#[test]
fn cached_pair_in_batch_placement_defers_all_formatter_observations() {
    cached_mode_race("batch");
}

/// Real SDK paths for the remaining startup/cancellation findings. These observations
/// price the assigned scheduling claim, not unrelated typed/alias boundaries.
#[test]
fn remaining_real_sdk_paths_match_source_and_survive_a_dropped_waiter() {
    use safe_cex_exchange::{
        journal::HistoricalPositionOptions, runtime::ExchangeFuture,
        worker::CallableMethodsInWorker,
    };
    use safe_cex_types::Order;
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/remaining_binding_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "independent complete-class source floor");
    for case in cases {
        let kind = case["kind"].as_str().unwrap();
        for dropped in [false, true] {
            let (venue, runtime, store, wire) = support::fixture();
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    markets: Some(vec![
                        serde_json::from_value(source["market"].clone()).unwrap(),
                    ]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            for method in [
                CallableMethodsInWorker::ChangePositionMode,
                CallableMethodsInWorker::ChangeMarginType,
            ] {
                venue
                    .after_method_called_in_worker(
                        &Value::Null,
                        method,
                        &[json!(kind == "set-leverage")],
                    )
                    .unwrap();
            }
            let order_kind = kind.strip_prefix("cancel-");
            let orders: Vec<Order> = order_kind.map(|family| {
                let id = match family { "algo" => "a__stop_loss", "trigger" => "t", _ => "n" };
                let typ = match family { "algo" => "stop_market", "trigger" => "conditional", _ => "limit" };
                let mut value = json!({"id":id,"symbol":"BTCUSDT","status":"open","type":typ,"side":"buy","amount":2,"filled":0,"remaining":2,"price":100,"lastUpdate":1710000000000.0,"reduceOnly":false});
                if family == "trigger" { value["info"] = json!({"instId":"BTC-USDT","orderType":"trigger","algoId":"t"}); }
                vec![serde_json::from_value(value).unwrap()]
            }).unwrap_or_default();
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    orders: Some(orders.clone()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            for (path, code, data) in [
                (
                    "/api/v1/trade/orders-pending",
                    "0",
                    json!([{ "orderId":"npage" }]),
                ),
                (
                    "/api/v1/trade/orders-tpsl-pending",
                    "0",
                    json!([{ "tpslId":"apage" }]),
                ),
                (
                    "/api/v1/trade/orders-algo-pending",
                    "0",
                    json!([{ "algoId":"tpage" }]),
                ),
                (
                    "/api/v1/market/tickers",
                    "0",
                    json!([{"instId":"BTC-USDT","last":"110","open24h":"100","bidPrice":"109","askPrice":"111","volCurrency24h":"12","vol24h":"1320"}]),
                ),
                ("/api/v1/trade/orders-history", "0", json!([])),
                (
                    "/api/v1/account/batch-leverage-info",
                    "0",
                    json!([{"instId":"BTC-USDT","positionSide":"net","marginMode":"cross","leverage":"7"}]),
                ),
                ("/api/v1/account/set-leverage", "0", json!([])),
                (
                    "/api/v1/account/set-margin-mode",
                    "0",
                    json!({"marginMode":"isolated"}),
                ),
                (
                    "/api/v1/account/margin-mode",
                    "0",
                    json!({"marginMode":"isolated"}),
                ),
                (
                    "/api/v1/trade/cancel-batch-orders",
                    "1",
                    json!([{"orderId":"n","code":"1000","msg":"does not exist"}]),
                ),
                (
                    "/api/v1/trade/cancel-tpsl",
                    "1",
                    json!([{"tpslId":"a","code":"1000","msg":"does not exist"}]),
                ),
                (
                    "/api/v1/trade/cancel-algo",
                    "0",
                    json!({"algoId":"t","code":"1000","msg":"does not exist"}),
                ),
                (
                    "/api/v1/account/positions-history",
                    "0",
                    json!([{"instId":"BTC-USDT","historyId":"h","positionId":"p","positionSide":"long","closePositions":"2","maxPositions":"3","fee":"-0.5","realizedPnl":"9.5","createTime":"1700000000000","updateTime":"1700000001000","openAveragePrice":"100","closeAveragePrice":"105","leverage":"4"}]),
                ),
            ] {
                wire.bodies.borrow_mut().insert(
                    path.into(),
                    json!({"code":code,"data":data,"msg":"success"}),
                );
            }
            let mut pending: ExchangeFuture<'_, Value> = match kind {
                "active-orders" => {
                    let f = venue.fetch_orders(None);
                    Box::pin(async move { f.await.map(|v| json!(v)) })
                }
                "tickers" => {
                    let f = venue.fetch_tickers();
                    Box::pin(async move {
                        let rows = f.await?;
                        assert_eq!(rows.len(), 1);
                        assert_eq!(rows[0].last.to_bits(), 110f64.to_bits());
                        Ok(json!(rows))
                    })
                }
                "history" => {
                    let f = venue.fetch_historical_orders(Some(
                        serde_json::from_value(
                            json!({"symbol":"BTCUSDT","from":100,"to":200,"limit":7}),
                        )
                        .unwrap(),
                    ));
                    Box::pin(async move { f.await.map(|v| json!(v)) })
                }
                "leverage" => {
                    let f = venue.fetch_leverage_for_symbol("BTCUSDT".into());
                    Box::pin(async move { f.await.map(|v| json!(v)) })
                }
                "set-leverage" => {
                    let f = venue.set_leverage("BTCUSDT".into(), 300., None);
                    Box::pin(async move { f.await.map(|_| Value::Null) })
                }
                "margin" => {
                    let f = venue.change_margin_type(true, None);
                    Box::pin(async move { f.await.map(|_| Value::Null) })
                }
                "journal" => {
                    let f = venue.fetch_historical_positions(HistoricalPositionOptions {
                        account: "account".into(),
                        from: None,
                        to: None,
                    });
                    Box::pin(async move { f.await.map(|v| json!(v)) })
                }
                _ if order_kind.is_some() => {
                    let f = venue.cancel_orders(orders);
                    Box::pin(async move { f.await.map(|_| Value::Null) })
                }
                _ => unreachable!(),
            };
            assert!(
                wire.requests.borrow().is_empty(),
                "source Axios terminal prefix {kind}"
            );
            let delay = match order_kind {
                Some("algo") => 1_000.,
                Some("trigger") => 350.,
                _ => 0.,
            };
            if dropped {
                drop(pending);
                runtime.pump();
                runtime.now.set(runtime.now.get() + delay);
                runtime.pump();
            } else {
                // Only source's real cancellation timers advance time; Promise turns do not.
                if delay > 0. {
                    runtime.pump();
                    assert!(
                        pending
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop()))
                            .is_pending(),
                        "real cancellation timer {kind}"
                    );
                    runtime.now.set(runtime.now.get() + delay);
                }
                let result = drive(&runtime, pending).unwrap();
                if kind == "tickers" {
                    assert_eq!(
                        result[0]["last"].as_f64().unwrap().to_bits(),
                        case["result"][0]["last"].as_f64().unwrap().to_bits()
                    );
                } else {
                    assert_eq!(
                        finite_json(result),
                        finite_json(case["result"].clone()),
                        "actual returned value {kind}"
                    );
                }
            }
            let requests: Vec<Value> = wire.requests.borrow().iter().map(|r| json!({"method":r.method.to_uppercase(),"path":r.url,"params":r.params,"body":r.body.as_ref().map(|b|b.untransformed().unwrap().clone().into_value().unwrap())})).collect();
            assert_eq!(
                finite_json(json!(requests)),
                finite_json(case["requests"].clone()),
                "real signed HTTP {kind} dropped={dropped}"
            );
            for (symbol, row) in store.fetched_leverage().unwrap().iter() {
                for (native, key) in [
                    (row.buy_leverage, "buyLeverage"),
                    (row.sell_leverage, "sellLeverage"),
                ] {
                    assert_eq!(
                        native.to_bits(),
                        case["leverage"][symbol][key].as_f64().unwrap().to_bits(),
                        "native leverage bits {kind}"
                    );
                }
            }
            assert_eq!(
                finite_json(json!(store.fetched_leverage().unwrap())),
                finite_json(case["leverage"].clone()),
                "native Store after completion {kind} dropped={dropped}"
            );
            assert_eq!(
                json!(
                    store
                        .orders()
                        .unwrap()
                        .iter()
                        .map(|o| o.id.clone())
                        .collect::<Vec<_>>()
                ),
                case["orders"],
                "native reconciliation {kind} dropped={dropped}"
            );
            venue.dispose().unwrap();
            runtime.pump();
        }
    }
}

// This corpus is finite JSON. Numeric JSON spelling (7 vs 7.0) is not a JS
// value distinction; native scalar bits are checked before this report view.
fn finite_json(value: Value) -> Value {
    match value {
        Value::Number(n) => serde_json::Number::from_f64(n.as_f64().unwrap())
            .unwrap()
            .into(),
        Value::Array(rows) => Value::Array(rows.into_iter().map(finite_json).collect()),
        Value::Object(rows) => {
            Value::Object(rows.into_iter().map(|(k, v)| (k, finite_json(v))).collect())
        }
        v => v,
    }
}
