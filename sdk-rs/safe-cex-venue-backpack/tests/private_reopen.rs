#[path = "support/private_socket.rs"]
mod private_socket;
#[path = "support/adapter_runtime.rs"]
#[allow(dead_code)]
mod support;
use base64::Engine;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::RestResponseBody;
use safe_cex_types::{OrderSide, PositionSide};
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
fn drive<T>(runtime: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..128 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
        runtime.now.set(runtime.now.get() + 100.0);
    }
    panic!("private operation must settle")
}

struct PrivateFixture {
    owner: Rc<safe_cex_venue_backpack::adapter::BackpackExchange>,
    runtime: Rc<support::Runtime>,
    store: Rc<safe_cex_store::store::DefaultStore>,
    wire: Arc<support::Wire>,
    socket: Rc<private_socket::Socket>,
}
impl PrivateFixture {
    fn new() -> Self {
        let secret = base64::engine::general_purpose::STANDARD.encode([1; 32]);
        let (owner, runtime, store, wire) = support::fixture_options(
            serde_json::from_value(json!({"key":"private-fixture-key","secret":secret})).unwrap(),
        );
        runtime.now.set(1_000.0);
        wire.bodies.lock().unwrap().insert(
            "/api/v1/position".into(),
            RestResponseBody::Untransformed(json!([])),
        );
        wire.bodies.lock().unwrap().insert(
            "/api/v1/capital/collateral".into(),
            RestResponseBody::Untransformed(json!({"netEquity":"25","netEquityAvailable":"18","netEquityLocked":"7","pnlUnrealized":"3"})),
        );
        let socket = Rc::new(private_socket::Socket::default());
        socket.events.borrow_mut().push_back(WsEvent::Open);
        runtime.dial_sockets.borrow_mut().push(socket.clone());
        drive(
            &runtime,
            owner
                .private_websocket
                .get()
                .unwrap()
                .base
                .create_websocket("ws://private-fixture".into(), vec![], None),
        )
        .unwrap();
        pump(&runtime);
        assert!(socket.sent.borrow().iter().any(|s| s.contains("SUBSCRIBE")));
        Self {
            owner,
            runtime,
            store,
            wire,
            socket,
        }
    }
    fn frame(&self, stream: &str, data: serde_json::Value) {
        self.socket.events.borrow_mut().push_back(WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"stream":stream,"data":data}).to_string(),
            ),
        ));
        pump(&self.runtime);
        assert!(
            self.runtime.errors.borrow().is_empty(),
            "{:?}",
            self.runtime.errors.borrow()
        );
    }
    fn fill(&self) {
        self.frame("account.orderUpdate", json!({"e":"orderFill","s":"BTC_USDC","i":"fill-order","S":"Bid","o":"Limit","p":"10","q":"3","z":"1","X":"PartiallyFilled","E":1234000,"f":"GTC","l":"1","L":"11"}));
    }
}
impl Drop for PrivateFixture {
    fn drop(&mut self) {
        self.owner.dispose().unwrap();
    }
}

#[test]
fn private_order_frame_commits_the_order_to_store() {
    let f = PrivateFixture::new();
    f.frame("account.orderUpdate.BTC_USDC", json!({"e":"orderUpdate","s":"BTC_USDC","i":"new-order","S":"Ask","o":"Limit","p":"12","q":"3","z":"0","X":"New","E":1234000,"f":"GTC"}));
    let orders = f.store.orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "new-order");
    assert_eq!(orders[0].side, OrderSide::Sell);
    assert_eq!(orders[0].price, 12.0);
    assert_eq!(orders[0].remaining, 3.0);
}

#[test]
fn private_fill_frame_emits_fill_and_updates_order() {
    let f = PrivateFixture::new();
    let fills = Rc::new(RefCell::new(vec![]));
    let capture = fills.clone();
    f.owner.base.on(
        "fill",
        Rc::new(move |_, fill| {
            let fill = safe_cex_exchange::js_runtime::value::to_json(fill).unwrap();
            capture.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    f.fill();
    assert_eq!(
        *fills.borrow(),
        vec![
            json!({"orderId":"fill-order","symbol":"BTC_USDC","side":"buy","price":11.0,"amount":1.0,"timestamp":1234.0})
        ]
    );
    assert_eq!(f.store.orders()[0].filled, 1.0);
    assert_eq!(f.store.orders()[0].remaining, 2.0);
}

#[test]
fn private_position_frame_commits_the_position_to_store() {
    let f = PrivateFixture::new();
    f.frame(
        "account.positionUpdate",
        json!({"s":"BTC_USDC","q":"-2","b":"20","n":"40","f":"0.2","P":"3","l":"30","E":1234000}),
    );
    let positions = f.store.positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].id, "BTC_USDC:short");
    assert_eq!(positions[0].side, PositionSide::Short);
    assert_eq!(positions[0].contracts, 2.0);
    assert_eq!(positions[0].entry_price, 20.0);
    assert_eq!(positions[0].leverage, 5.0);
    assert_eq!(positions[0].unrealized_pnl, 3.0);
}

#[test]
fn private_fill_frame_refreshes_balance_through_real_rest_binding() {
    let f = PrivateFixture::new();
    assert_eq!(count(&f.wire, "/api/v1/capital/collateral"), 0);
    f.fill();
    assert_eq!(count(&f.wire, "/api/v1/capital/collateral"), 1);
    let balance = f.store.balance();
    assert_eq!(balance.total, 25.0);
    assert_eq!(balance.free, 18.0);
    assert_eq!(balance.used, 7.0);
    assert_eq!(balance.upnl, 3.0);
}
fn pump(runtime: &support::Runtime) {
    for _ in 0..64 {
        runtime.pump();
    }
}
fn count(wire: &support::Wire, path: &str) -> usize {
    wire.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.url == path)
        .count()
}
#[test]
fn private_reopen_refreshes_orders_without_starting_another_polling_loop() {
    let secret = base64::engine::general_purpose::STANDARD.encode([1; 32]);
    let (owner, runtime, _store, wire) = support::fixture_options(
        serde_json::from_value(json!({
            "key":"private-fixture-key", "secret":secret, "publicDataOnly":false
        }))
        .unwrap(),
    );
    for (path, body) in [
        ("/api/v1/account", json!({"leverageLimit":"5"})),
        ("/api/v1/position", json!([])),
        (
            "/api/v1/capital/collateral",
            json!({"netEquity":"10","availableMargin":"8","initialMargin":"2","unrealizedPnl":"0"}),
        ),
        ("/api/v1/orders", json!([])),
    ] {
        wire.bodies
            .lock()
            .unwrap()
            .insert(path.into(), RestResponseBody::Untransformed(body));
    }
    // Start the single source tick chain that normal private startup owns.
    drive(&runtime, owner.tick()).unwrap();
    assert_eq!(count(&wire, "/api/v1/account"), 1);
    let socket = Rc::new(private_socket::Socket::default());
    socket.events.borrow_mut().push_back(WsEvent::Open);
    runtime.dial_sockets.borrow_mut().push(socket.clone());
    let private = owner.private_websocket.get().unwrap();
    drive(
        &runtime,
        private
            .base
            .create_websocket("ws://private-fixture".into(), vec![], None),
    )
    .unwrap();
    pump(&runtime);
    assert!(
        socket.sent.borrow().iter().any(|s| s.contains("SUBSCRIBE")),
        "real private authentication must run"
    );
    let before_orders = count(&wire, "/api/v1/orders");
    socket.events.borrow_mut().push_back(WsEvent::Open);
    pump(&runtime);
    assert!(
        count(&wire, "/api/v1/orders") > before_orders,
        "the real reopen resync must refresh orders"
    );
    assert_eq!(
        count(&wire, "/api/v1/account"),
        1,
        "reopen must not start an account/position/balance tick"
    );
    runtime.now.set(runtime.now.get() + 10_000.0);
    pump(&runtime);
    assert_eq!(
        count(&wire, "/api/v1/account"),
        2,
        "only the original polling chain may run after 10s"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "{:?}",
        runtime.errors.borrow()
    );
}

#[test]
fn synced_clock_reaches_real_private_authentication() {
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    struct ResetClock;
    impl Drop for ResetClock {
        fn drop(&mut self) {
            VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
        }
    }
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    let f = PrivateFixture::new();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let frame: serde_json::Value = f
        .socket
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .find(|s| s["method"] == "SUBSCRIBE")
        .unwrap();
    let timestamp = frame["signature"][2]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    assert!(
        (before..=after).contains(&timestamp),
        "real private auth timestamp {timestamp} outside synced {before}..={after}"
    );
}
