//! Assigned lifecycle rows exercised on the real assembled owner and hosts.
#[path = "binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use safe_cex_exchange::{exchange::Exchange, js_runtime::value::Js};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::json;
const KEYS: &str = "/openApi/user/auth/userDataStream";
fn fixture() -> harness::Harness {
    harness::harness(ExchangeName::Bingx, harness::options(), |http| {
        http.route("POST", KEYS, 200.0, json!({"listenKey":"audit-key"}));
        http.route("PUT", KEYS, 200.0, json!({}));
        http.ok("GET", "/openApi/swap/v2/user/positions", json!([]));
        http.ok(
            "GET",
            "/openApi/swap/v2/trade/openOrders",
            json!({"orders":[]}),
        );
    })
}
fn open(h: &harness::Harness) {
    h.drive(
        h.exchange
            .inner
            .private_websocket
            .keys
            .connect_and_subscribe(),
    )
    .unwrap();
    h.runtime.pump();
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
}
#[test]
fn real_message_rebuild_detaches_socket_in_call_prefix() {
    let h = fixture();
    open(&h);
    let stale = h.runtime.sockets.borrow()[0].1.clone();
    let keys = &h.exchange.inner.private_websocket.keys;
    h.exchange
        .host
        .core(true)
        .unwrap()
        .on_message(&SocketFrame::Text(
            json!({"e":"listenKeyExpired"}).to_string(),
        ))
        .unwrap();
    assert!(
        stale.closed.get(),
        "real Promise bridge runs rebuild prefix immediately"
    );
    assert!(keys.rebuilding.get());
    assert!(keys.listen_key.borrow().is_none());
    assert!(keys.socket.get().is_none());
    assert!(h.exchange.host.private_listeners.borrow().is_empty());
    h.runtime.pump();
    assert!(!keys.rebuilding.get());
    assert_eq!(h.http.calls(KEYS).len(), 2);
    h.exchange.dispose().unwrap();
}
#[test]
fn real_open_refresh_and_disposal_cancel_live_host_timer() {
    let h = fixture();
    open(&h);
    let keys = &h.exchange.inner.private_websocket.keys;
    assert!(keys.refresh_timer.get().is_some());
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(socket.sent.borrow().iter().any(|s| s == "Ping"));
    let ping = h.exchange.host.private.get("pingAt").unwrap();
    assert!(matches!(ping, Js::Number(n) if n == h.runtime.now.get()));
    h.runtime.advance(30.0 * 60.0 * 1000.0 - 1.0);
    assert_eq!(
        h.http.calls(KEYS).len(),
        1,
        "no refresh before source deadline"
    );
    h.runtime.advance(1.0);
    assert_eq!(
        h.http.calls(KEYS).len(),
        2,
        "real refresh fires at 30 minutes"
    );
    keys.connecting.set(true);
    keys.rebuilding.set(true);
    keys.dispose().unwrap();
    assert!(keys.refresh_timer.get().is_none());
    assert!(keys.listen_key.borrow().is_none());
    assert!(
        keys.connecting.get() && keys.rebuilding.get(),
        "dispose retains source flags"
    );
    assert!(socket.closed.get());
    let calls = h.http.calls(KEYS).len();
    h.runtime.advance(30.0 * 60.0 * 1000.0);
    assert_eq!(
        h.http.calls(KEYS).len(),
        calls,
        "cancelled refresh sends no HTTP"
    );
    h.exchange.dispose().unwrap();
}
#[test]
fn real_recovery_timer_replaces_refresh_and_rebuilds_at_30_seconds() {
    let h = fixture();
    open(&h);
    let keys = &h.exchange.inner.private_websocket.keys;
    let prior = keys.refresh_timer.get().unwrap();
    keys.schedule_listen_key_recovery().unwrap();
    let recovery = keys.refresh_timer.get().unwrap();
    assert_ne!(prior, recovery);
    assert!(
        !h.exchange
            .host
            .tasks
            .borrow()
            .iter()
            .any(|(id, _)| *id == prior)
    );
    h.runtime.advance(29_999.0);
    assert_eq!(h.http.calls(KEYS).len(), 1);
    h.runtime.advance(1.0);
    assert_eq!(h.http.calls(KEYS).len(), 2);
    assert!(h.runtime.sockets.borrow()[0].1.closed.get());
    assert_eq!(h.runtime.sockets.borrow().len(), 2);
    assert!(
        keys.refresh_timer.get().is_some(),
        "replacement open installs the next refresh"
    );
    assert!(
        h.runtime.sockets.borrow()[1]
            .1
            .sent
            .borrow()
            .iter()
            .any(|s| s == "Ping")
    );
    h.runtime.advance(1_800_000.0);
    assert_eq!(
        h.http.calls(KEYS).len(),
        3,
        "held exchange keeps replacement refresh alive"
    );

    h.exchange.dispose().unwrap();
    assert!(h.runtime.reports.borrow().is_empty());
}
#[test]
fn real_tick_starts_selected_read_before_return() {
    let h = fixture();
    let pending = h.exchange.inner.history.tick(2.0);
    // Source has already invoked axios before this disposal; its pending request
    // still executes, while the post-await Store write is guarded.
    h.exchange.dispose().unwrap();
    h.drive(pending).unwrap();
    assert_eq!(
        h.http.calls("/openApi/swap/v2/trade/openOrders").len(),
        1,
        "disposal after entry cannot suppress the already-started read"
    );
    assert!(
        !h.store.loaded().orders,
        "post-await disposal guard prevents Store writes"
    );
}

fn dropped_timer(recovery: bool) {
    let h = fixture();
    open(&h);
    let weak = std::rc::Rc::downgrade(&h.exchange.inner.private_websocket.keys);
    if recovery {
        h.exchange
            .inner
            .private_websocket
            .keys
            .schedule_listen_key_recovery()
            .unwrap();
    }
    let runtime = h.runtime.clone();
    let http = h.http.clone();
    let socket = runtime.sockets.borrow()[0].1.clone();
    drop(h);
    assert!(
        socket.closed.get(),
        "final handle drop performs explicit socket teardown"
    );
    runtime.advance(1_800_000.0);
    assert_eq!(
        http.calls(KEYS).len(),
        1,
        "drop cancels refresh and recovery rather than leaving half-alive tasks"
    );
    assert!(weak.upgrade().is_none());
    assert!(
        runtime.reports.borrow().is_empty(),
        "cancelled callback cannot report owner released"
    );
}
#[test]
fn real_open_refresh_final_handle_drop_tears_down_everything() {
    dropped_timer(false);
}
#[test]
fn real_recovery_final_handle_drop_tears_down_everything() {
    dropped_timer(true);
}

#[test]
fn real_timer_recovery_closes_stale_socket_before_next_runtime_job() {
    use safe_cex_exchange::runtime::ExchangeRuntime;
    let h = fixture();
    open(&h);
    h.exchange
        .inner
        .private_websocket
        .keys
        .schedule_listen_key_recovery()
        .unwrap();
    let stale = h.runtime.sockets.borrow()[0].1.clone();
    h.runtime.now.set(h.runtime.now.get() + 30_000.0);
    let observed = std::rc::Rc::new(std::cell::Cell::new(false));
    let capture = observed.clone();
    h.runtime.spawn_local(Box::pin(async move {
        assert!(
            stale.closed.get(),
            "source recovery prefix runs inside the timer callback, before the next queued job"
        );
        capture.set(true);
    }));
    h.runtime.pump();
    assert!(observed.get());
    h.exchange.dispose().unwrap();
}

#[test]
fn real_final_drop_cancels_host_timers_even_if_store_reset_observer_rejects() {
    let h = fixture();
    open(&h);
    let callbacks = std::rc::Rc::new(std::cell::Cell::new(0));
    let capture = callbacks.clone();
    let _unsubscribe = safe_cex_store::Store::subscribe(h.store.as_ref(), Box::new(move |_, _| {
        capture.set(capture.get() + 1);
        Err(safe_cex_types::StoreSubscriberError::new("reset observer failed"))
    }));
    let host = h.exchange.host.clone();
    let runtime = h.runtime.clone();
    let http = h.http.clone();
    let socket = runtime.sockets.borrow()[0].1.clone();
    drop(h);
    assert!(callbacks.get() > 0, "actual teardown invoked the rejecting Store observer");
    assert!(socket.closed.get());
    assert!(host.tasks.borrow().is_empty(), "Store rejection cannot bypass timer cancellation");
    runtime.advance(1_800_000.0);
    assert_eq!(http.calls(KEYS).len(), 1);
    assert!(runtime.reports.borrow().is_empty(), "final teardown is silent");
}
