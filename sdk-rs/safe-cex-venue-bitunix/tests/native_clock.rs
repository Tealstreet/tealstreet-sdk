//! Exercise concrete REST/WS clocks and both real resync hooks, not injected signing callbacks.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("clock witness did not settle");
}
fn skew() {
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
}
fn between(value: f64, before: f64, after: f64) {
    assert!(
        value >= before && value <= after,
        "synced timestamp {value} outside {before}..={after}"
    );
}

#[test]
fn concrete_signing_and_login_retry_use_virtual_clock() {
    let (venue, rt, _, wire) = support::fixture_with(false);
    rt.now.set(123.0);
    *rt.id.borrow_mut() = Some("fixture-_nonce".into());
    skew();
    let before = VIRTUAL_CLOCK.get_current_time();
    drive(&rt, venue.validate_account()).unwrap();
    let after = VIRTUAL_CLOCK.get_current_time();
    let requests = wire.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|r| r.url == "/api/v1/futures/account")
        .unwrap();
    between(request.headers["timestamp"].parse().unwrap(), before, after);
    drop(requests);
    for (path, data) in [
        ("/api/v1/futures/market/trading_pairs", json!([])),
        ("/api/v1/futures/market/tickers", json!([])),
        ("/api/v1/futures/position/get_pending_positions", json!([])),
        (
            "/api/v1/futures/trade/get_pending_orders",
            json!({"orderList":[]}),
        ),
        ("/api/v1/futures/tpsl/get_pending_orders", json!([])),
    ] {
        wire.bodies
            .lock()
            .unwrap()
            .insert(path.into(), json!({"code":0,"data":data}));
    }
    rt.dial.set(true);
    let before = VIRTUAL_CLOCK.get_current_time();
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
        rt.now.set(rt.now.get() + 1000.0);
    }
    let socket = rt
        .dialed
        .borrow()
        .iter()
        .find(|(u, _)| u.ends_with("/private/"))
        .unwrap()
        .1
        .clone();
    let frames: Vec<Value> = socket
        .sent
        .borrow()
        .iter()
        .map(|f| serde_json::from_str(f).unwrap())
        .collect();
    let login = frames
        .iter()
        .find(|v| v["op"] == "login")
        .expect("login frame");
    assert_eq!(
        login["args"][0]["nonce"], "fixturenonce",
        "WS nonce uses source utils/uuid"
    );
    // TS captures timestamp BEFORE sync, which on native clears the server offset.
    between(
        login["args"][0]["timestamp"].as_f64().unwrap(),
        before,
        (VIRTUAL_CLOCK.now)() + 40_000.0,
    );
    assert!(
        (VIRTUAL_CLOCK.get_current_time_ms() - (VIRTUAL_CLOCK.now)()).abs() < 1000.0,
        "sendLogin must await real native sync"
    );
    skew();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                r#"{"op":"login","data":{"result":false}}"#.into(),
            ),
        ));
    for _ in 0..4 {
        rt.pump();
    }
    assert!(
        (VIRTUAL_CLOCK.get_current_time_ms() - (VIRTUAL_CLOCK.now)()).abs() < 1000.0,
        "login failure must resync before its retry delay"
    );
    venue.dispose().unwrap();
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
}
