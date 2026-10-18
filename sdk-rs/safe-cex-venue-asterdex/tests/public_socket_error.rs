//! Plain WebSocket errors stop at the base listener; close events reconnect.
#[path = "support/exchange_data.rs"]
mod exchange_data;
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::{exchange::Exchange, websocket::WsEvent};
use serde_json::json;
#[test]
fn public_transport_error_waits_for_close_before_reconnecting() {
    let (venue, rt, _, wire) =
        support::fixture(json!({"key":"fixture-key","secret":"fixture-secret"}));
    exchange_data::script(&wire);
    support::drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    let is_public = |url: &str| !url.ends_with("/ws/fixture-listen-key");
    let before = rt.urls.borrow().iter().filter(|url| is_public(url)).count();
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| is_public(url))
        .expect("production public socket")
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Error("transport error".into()));
    for _ in 0..35 {
        rt.now.set(rt.now.get() + 100.0);
        rt.pump();
    }
    assert_eq!(
        rt.urls.borrow().iter().filter(|url| is_public(url)).count(),
        before,
        "base stopImmediatePropagation suppresses the venue error listener"
    );
    socket.events.borrow_mut().push_back(WsEvent::Close {
        code: Some(1001),
        reason: "server close".into(),
    });
    for _ in 0..35 {
        rt.now.set(rt.now.get() + 100.0);
        rt.pump();
    }
    assert!(
        rt.urls.borrow().iter().filter(|url| is_public(url)).count() > before,
        "the actual close listener still reconnects"
    );
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    venue.dispose().unwrap();
}
