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
        store.fetched_leverage()["BTCUSDT"].position_mode,
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
    assert_eq!(profiles.len(), 3, "independent source case floor");
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
    let kind_type = match kind {
        "normal" => "limit",
        "algo" => "stop_market",
        "trigger" => "conditional",
        _ => unreachable!(),
    };
    let mut opts = json!({"symbol":"BTCUSDT","type":kind_type,"side":"buy","amount":2,"price":100});
    if kind == "trigger" {
        opts["triggerPrice"] = json!(105);
    }
    let opts: PlaceOrderOpts = serde_json::from_value(opts).unwrap();
    let pending = venue.base().place_order(venue.as_ref(), opts);
    let mut pending = std::pin::pin!(pending);
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
    let ids = drive(&runtime, pending).unwrap();
    assert_eq!(json!(ids), source["ids"]);
    assert_eq!(wire.requests.borrow().len(), 1);
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
