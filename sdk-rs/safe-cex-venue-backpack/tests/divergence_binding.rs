//! Real owned adapter, installed SDK entrypoints and runtime; no helper substitutions.
#[path = "support/private_socket.rs"]
mod socket;
#[path = "support/adapter_runtime.rs"]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    websocket::{WsEvent, WsSocket},
};
use safe_cex_transport::RestResponseBody;
use serde_json::json;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
fn put(wire: &support::Wire, path: &str, value: serde_json::Value) {
    wire.bodies.lock().unwrap().insert(
        path.into(),
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
        ),
    );
}
fn dropped_read(balance: bool) {
    let (owner, runtime, _, wire) = support::fixture();
    let gate = Arc::new(AtomicBool::new(false));
    *wire.response_gate.lock().unwrap() = Some(gate.clone());
    let logs = Rc::new(RefCell::new(Vec::new()));
    let seen = logs.clone();
    owner.base.on(
        "log",
        Rc::new(move |_, value| {
            seen.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let path = if balance {
        "/api/v1/capital/collateral"
    } else {
        "/api/v1/account"
    };
    put(&wire, path, json!(null));
    if balance {
        drop(owner.fetch_balance());
    } else {
        drop(Exchange::get_account(owner.as_ref()));
    }
    runtime.pump();
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        1,
        "real request starts without waiter polling"
    );
    assert!(logs.borrow().is_empty(), "response must remain pending");
    gate.store(true, Ordering::SeqCst);
    runtime.pump();
    assert_eq!(
        logs.borrow().len(),
        1,
        "source catch runs after dropped waiter"
    );
    assert_eq!(
        runtime.errors.borrow().len(),
        1,
        "dropped rejected Promise remains an unhandled rejection: {:?}",
        runtime.errors.borrow()
    );
}
#[test]
fn real_account_catch_survives_dropped_sdk_waiter() {
    dropped_read(false);
}
#[test]
fn real_balance_catch_survives_dropped_waiter() {
    dropped_read(true);
}
#[test]
fn real_orders_reconciliation_silently_catches_failure() {
    let (owner, runtime, store, wire) = support::fixture();
    put(&wire, "/api/v1/orders", json!(null));
    let private = owner.private_websocket.get().unwrap();
    assert!(
        support::ready(&runtime, private.fetch_orders_throttled()).is_ok(),
        "source silently catches real orders read failure"
    );
    assert!(!store.loaded().unwrap().orders);
    assert!(private.state.orders_timeout.borrow().is_none());
    put(&wire, "/api/v1/orders", json!([]));
    support::ready(&runtime, private.fetch_orders_throttled()).unwrap();
    assert!(store.loaded().unwrap().orders);
    assert_eq!(wire.requests.lock().unwrap().len(), 2);
    assert!(runtime.errors.borrow().is_empty());
}
#[test]
fn real_balance_reconciliation_silently_catches_failure() {
    let (owner, runtime, store, wire) = support::fixture();
    put(&wire, "/api/v1/capital/collateral", json!(null));
    let private = owner.private_websocket.get().unwrap();
    runtime.now.set(1234.0);
    private.state.balance.pending.set(true);
    assert!(
        support::ready(&runtime, private.execute_balance_fetch()).is_ok(),
        "source silently catches real balance read failure"
    );
    assert!(!private.state.balance.pending.get());
    assert_eq!(private.state.balance.last.get(), 1234.0);
    put(
        &wire,
        "/api/v1/capital/collateral",
        json!({"netEquity":"25","netEquityAvailable":"18","netEquityLocked":"7","pnlUnrealized":"-0"}),
    );
    support::ready(&runtime, private.execute_balance_fetch()).unwrap();
    assert_eq!(store.balance().unwrap().total, 25.0);
    assert_eq!(store.balance().unwrap().upnl.to_bits(), (-0.0f64).to_bits());
    assert_eq!(wire.requests.lock().unwrap().len(), 2);
    assert!(runtime.errors.borrow().is_empty());
}
#[test]
fn real_reconciliation_continues_after_dropped_waiters() {
    let (owner, runtime, store, wire) = support::fixture();
    put(&wire, "/api/v1/orders", json!([]));
    put(
        &wire,
        "/api/v1/capital/collateral",
        json!({"netEquity":"31"}),
    );
    let private = owner.private_websocket.get().unwrap();
    drop(private.fetch_orders_throttled());
    drop(private.execute_balance_fetch());
    runtime.pump();
    assert!(
        store.loaded().unwrap().orders,
        "owned order reconciliation continuation"
    );
    assert_eq!(
        store.balance().unwrap().total,
        31.0,
        "owned balance reconciliation continuation"
    );
    assert!(runtime.errors.borrow().is_empty());
}
#[test]
fn real_public_receiver_stays_live_while_exchange_is_held() {
    use safe_cex_venue_backpack::{
        native_value::{NativeValue, Value},
        public_control::JsString,
    };
    let (owner, runtime, store, _) = support::fixture();
    let public = owner.public_websocket.get().unwrap().clone();
    let weak = Rc::downgrade(&public);
    let transport = Rc::new(socket::Socket::default());
    transport.events.borrow_mut().push_back(WsEvent::Open);
    runtime.dial_sockets.borrow_mut().push(transport);
    support::ready(
        &runtime,
        public
            .base
            .create_websocket("ws://held-fixture".into(), vec![], None),
    )
    .unwrap();
    runtime.pump();
    let data = |last| Value::from_prepared(json!({"s":"BTC_USDC","c":last}).into());
    public.ticker_handler.call(data("10")).unwrap();
    runtime.now.set(100.0);
    public.ticker_handler.call(data("20")).unwrap();
    let hits = Rc::new(RefCell::new(Vec::new()));
    let seen = hits.clone();
    let stop = public
        .listen_ohlcv(
            "BTC_USDC".encode_utf16().collect::<JsString>(),
            "1m".encode_utf16().collect(),
            Rc::new(move |c| {
                seen.borrow_mut().push(c);
                Ok(())
            }),
        )
        .unwrap();
    drop(public);
    runtime.now.set(1000.0);
    runtime.pump();
    assert_eq!(
        store.tickers().unwrap()[0].last,
        20.0,
        "held exchange retains trailing throttle receiver"
    );
    let socket = weak.upgrade().unwrap();
    socket.on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({"stream":"kline.1m.BTC_USDC","data":{"e":"kline","s":"BTC_USDC","t":"2024-01-01T00:00:00","o":"1","h":"3","l":"0","c":"2","v":"4"}}).to_string())).unwrap();
    assert_eq!(
        hits.borrow().len(),
        1,
        "held exchange retains stored listener receiver"
    );
    stop().unwrap();
    assert!(Rc::ptr_eq(&socket, owner.public_websocket.get().unwrap()));
    assert!(runtime.errors.borrow().is_empty());
}
#[test]
fn real_final_drop_fully_tears_down_public_socket() {
    let (owner, runtime, _, _) = support::fixture();
    let public = owner.public_websocket.get().unwrap().clone();
    let transport = Rc::new(socket::Socket::default());
    transport.events.borrow_mut().push_back(WsEvent::Open);
    runtime.dial_sockets.borrow_mut().push(transport.clone());
    support::ready(
        &runtime,
        public
            .base
            .create_websocket("ws://drop-fixture".into(), vec![], None),
    )
    .unwrap();
    runtime.pump();
    let base = owner.base.clone();
    let operation = base.start_owned_operation(std::future::pending());
    let canceled = Rc::new(std::cell::Cell::new(false));
    *owner
        .private_websocket
        .get()
        .unwrap()
        .state
        .orders_timeout
        .borrow_mut() = Some(canceled.clone());
    let weak = Rc::downgrade(&owner);
    drop(owner);
    assert!(
        weak.upgrade().is_none(),
        "no final external exchange handle"
    );
    assert!(
        base.is_disposed(),
        "final drop must set the shared disposed flag"
    );
    assert!(
        public.base.is_force_closed(),
        "final drop must close socket health/reconnect work"
    );
    assert_eq!(transport.ready_state(), 3);
    assert!(
        !operation.is_active(),
        "final drop cancels shared owned work"
    );
    assert!(
        canceled.get(),
        "final drop clears private reconciliation timer too"
    );
    runtime.now.set(60000.0);
    runtime.pump();
    assert!(
        runtime.errors.borrow().is_empty(),
        "no half-live weak receiver callbacks after drop: {:?}",
        runtime.errors.borrow()
    );
}

#[test]
fn maintained_source_pins_dropped_waiters_and_silent_catches() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["reads"].as_array().unwrap().len(), 2);
    assert_eq!(source["catches"].as_array().unwrap().len(), 6);
    for row in source["reads"].as_array().unwrap() {
        assert_eq!(row["calls"].as_array().unwrap().len(), 1);
        assert_eq!(row["logs"].as_array().unwrap().len(), 1);
    }
    for row in source["catches"].as_array().unwrap() {
        assert_eq!(row["fulfilled"], true);
    }
}

#[test]
fn real_private_store_failures_remain_silent_after_commit() {
    for orders in [false, true] {
        let (owner, runtime, store, wire) = support::fixture();
        put(&wire, "/api/v1/orders", json!([]));
        put(
            &wire,
            "/api/v1/capital/collateral",
            json!({"netEquity":"37"}),
        );
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        let unsubscribe = store.subscribe(Arc::new(move |_, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            Err(safe_cex_types::StoreSubscriberError::new(
                "native notification failure",
            ))
        }));
        let private = owner.private_websocket.get().unwrap();
        let result = if orders {
            support::ready(&runtime, private.fetch_orders_throttled())
        } else {
            support::ready(&runtime, private.execute_balance_fetch())
        };
        assert!(
            result.is_ok(),
            "source catch includes original Store failure"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "real Store notification executed"
        );
        if orders {
            assert!(store.loaded().unwrap().orders);
        } else {
            assert_eq!(store.balance().unwrap().total, 37.0);
        }
        unsubscribe();
        assert!(runtime.errors.borrow().is_empty());
    }
}

#[test]
fn real_private_post_await_disposal_policies_stay_distinct() {
    for orders in [false, true] {
        let (owner, runtime, store, wire) = support::fixture();
        put(&wire, "/api/v1/orders", json!([]));
        put(
            &wire,
            "/api/v1/capital/collateral",
            json!({"netEquity":"43"}),
        );
        let gate = Arc::new(AtomicBool::new(false));
        *wire.response_gate.lock().unwrap() = Some(gate.clone());
        let private = owner.private_websocket.get().unwrap();
        if orders {
            drop(private.fetch_orders_throttled());
        } else {
            drop(private.execute_balance_fetch());
        }
        runtime.pump();
        assert_eq!(wire.requests.lock().unwrap().len(), 1);
        owner.dispose().unwrap();
        gate.store(true, Ordering::SeqCst);
        runtime.pump();
        // Source orders has only a pre-fetch guard; balance also checks after await.
        assert_eq!(store.loaded().unwrap().orders, orders);
        assert_eq!(store.balance().unwrap().total, 0.0);
        assert!(runtime.errors.borrow().is_empty());
    }
}

#[test]
fn real_pending_market_timer_does_not_retain_final_exchange() {
    let (owner, runtime, store, _) = support::fixture();
    owner
        .subscribe_to_all_markets(store.markets().unwrap().to_vec())
        .unwrap();
    let base = owner.base.clone();
    let weak = Rc::downgrade(&owner);
    drop(owner);
    assert!(
        weak.upgrade().is_none(),
        "pending source subscription timer must allow native final teardown"
    );
    assert!(base.is_disposed());
    runtime.now.set(2000.0);
    runtime.pump();
    assert!(runtime.errors.borrow().is_empty());
}

#[test]
fn real_pending_poll_loop_does_not_retain_final_exchange() {
    let secret = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [1; 32]);
    let (owner, runtime, _, wire) = support::fixture_options(
        serde_json::from_value(json!({"key":"native-key","secret":secret})).unwrap(),
    );
    put(&wire, "/api/v1/account", json!({"leverageLimit":"2"}));
    put(&wire, "/api/v1/position", json!([]));
    put(
        &wire,
        "/api/v1/capital/collateral",
        json!({"netEquity":"41"}),
    );
    put(&wire, "/api/v1/orders", json!([]));
    support::ready(&runtime, owner.tick()).unwrap();
    assert_eq!(
        wire.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.url.as_str())
            .collect::<Vec<_>>(),
        [
            "/api/v1/account",
            "/api/v1/position",
            "/api/v1/capital/collateral",
            "/api/v1/orders"
        ]
    );
    runtime.now.set(10000.0);
    runtime.pump();
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        8,
        "held original Exchange keeps native polling alive"
    );
    let weak = Rc::downgrade(&owner);
    let base = owner.base.clone();
    drop(owner);
    assert!(
        weak.upgrade().is_none(),
        "pending repeat-tick timer must allow native final teardown"
    );
    assert!(base.is_disposed());
    runtime.now.set(20000.0);
    runtime.pump();
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        8,
        "no orphan poll after final drop"
    );
    assert!(runtime.errors.borrow().is_empty());
}

#[test]
fn real_delayed_market_subscription_keeps_held_receiver() {
    let (owner, runtime, store, _) = support::fixture();
    owner
        .subscribe_to_all_markets(store.markets().unwrap().to_vec())
        .unwrap();
    assert_eq!(
        owner
            .public_websocket
            .get()
            .unwrap()
            .subscriptions
            .borrow()
            .values()
            .len(),
        0
    );
    runtime.now.set(2000.0);
    runtime.pump();
    assert_eq!(
        owner
            .public_websocket
            .get()
            .unwrap()
            .subscriptions
            .borrow()
            .values()
            .len(),
        3,
        "held exchange runs original delayed subscription body"
    );
    assert!(runtime.errors.borrow().is_empty());
}
