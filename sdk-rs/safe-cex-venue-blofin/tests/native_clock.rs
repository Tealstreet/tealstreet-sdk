//! Concrete REST and private websocket signing must use the synced singleton.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
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
fn concrete_rest_and_private_login_use_synced_time() {
    let (venue, rt, _, wire) = support::fixture();
    rt.now.set(123.0);
    skew();
    let before = VIRTUAL_CLOCK.get_current_time();
    drive(&rt, venue.validate_account()).unwrap();
    let after = VIRTUAL_CLOCK.get_current_time();
    let requests = wire.requests.borrow();
    let request = requests
        .iter()
        .find(|r| r.url == "/api/v1/asset/balances")
        .expect("signed REST request");
    between(
        request.headers["ACCESS-TIMESTAMP"].parse().unwrap(),
        before,
        after,
    );
    drop(requests);
    for (path, data) in [
        ("/api/v1/account/margin-mode", json!({"marginMode":"cross"})),
        (
            "/api/v1/account/position-mode",
            json!({"positionMode":"net_mode"}),
        ),
    ] {
        wire.bodies
            .borrow_mut()
            .insert(path.into(), json!({"code":"0","data":data}));
    }
    rt.open.set(true);
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let frames: Vec<Value> = rt
        .opened
        .borrow()
        .iter()
        .flat_map(|s| {
            s.sent
                .borrow()
                .iter()
                .filter_map(|f| serde_json::from_str(f).ok())
                .collect::<Vec<Value>>()
        })
        .collect();
    let login = frames
        .iter()
        .find(|v| v["op"] == "login")
        .expect("private login frame");
    between(
        login["args"][0]["timestamp"].as_f64().unwrap(),
        before,
        after,
    );
    venue.dispose().unwrap();
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
}
