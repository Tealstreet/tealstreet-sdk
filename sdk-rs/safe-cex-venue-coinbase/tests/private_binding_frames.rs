//! Actual Coinbase private socket frames, production mapping, Store and emitter.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::{Harness, Http, Socket, harness, options};
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn script(http: &Http) {
    http.ok("GET", "/api/v3/brokerage/portfolios", json!({"portfolios":[{"name":"Synthetic Spot", "uuid":"portfolio-synthetic", "type":"DEFAULT", "deleted":false}]}));
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-synthetic",
        json!({"breakdown":{"spot_positions":[{"asset":"USDC","available_to_trade_crypto":13.25,"total_balance_crypto":17.5}]}}),
    );
    http.route(
        "GET",
        "/api/v3/brokerage/cfm/balance_summary",
        403.0,
        json!({"message":"No CFM permissions"}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/products",
        json!({"products":[product()]}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/orders/historical/batch",
        json!({"orders":[],"has_next":false}),
    );
}
fn product() -> Value {
    json!({"product_id":"BTC-USDC","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100", "trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000"})
}

fn connected(name: ExchangeName) -> (Harness, Rc<Socket>, Rc<RefCell<Vec<Value>>>) {
    let h = harness(name, options(), script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://advanced-trade-ws-user.coinbase.com")
        .unwrap()
        .1
        .clone();
    let fills = Rc::new(RefCell::new(Vec::new()));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    (h, socket, fills)
}
fn order(status: &str) -> Value {
    json!({"order_id":"ws-native-order","client_order_id":"ws-native-client","product_id":"BTC-USDC",
        "order_type":"LIMIT","order_side":"BUY","status":status,"cumulative_quantity":if status=="FILLED" {"2.5"} else {"0"},
        "leaves_quantity":if status=="FILLED" {"0"} else {"2.5"},"limit_price":"123.45","avg_price":"123.55"})
}
fn send(h: &Harness, socket: &Socket, frame: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
}
fn snapshot(h: &Harness, socket: &Socket) {
    send(
        h,
        socket,
        json!({"channel":"user","events":[{"type":"snapshot","orders":[order("OPEN")]}]}),
    );
}
#[test]
fn real_private_order_snapshot_populates_store() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        assert!(h.store.orders().is_empty());
        snapshot(&h, &socket);
        let orders = h.store.orders();
        assert_eq!(orders.len(), 1, "real private snapshot must reach Store");
        assert_eq!(orders[0].id, "ws-native-order");
        assert_eq!(orders[0].parent_id.as_deref(), Some("ws-native-client"));
        assert_eq!(orders[0].symbol, "BTC-USDC");
        assert_eq!(orders[0].amount, 2.5);
        assert_eq!(orders[0].filled, 0.0);
        assert_eq!(orders[0].remaining, 2.5);
        assert!(h.store.loaded().orders);
        assert!(fills.borrow().is_empty(), "snapshot does not emit fill");
        drop(orders);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn real_private_filled_update_closes_order_and_emits_fill() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        snapshot(&h, &socket);
        assert_eq!(
            h.store.orders().len(),
            1,
            "fill witness must begin with an open order"
        );
        send(
            &h,
            &socket,
            json!({"channel":"user","events":[{"type":"update","orders":[order("FILLED")]}]}),
        );
        let orders = h.store.orders();
        assert_eq!(
            orders.len(),
            1,
            "filled order remains in source Store history"
        );
        assert_eq!(
            orders[0].status,
            OrderStatus::Closed,
            "real filled private update must close order"
        );
        assert_eq!(orders[0].filled, 2.5);
        assert_eq!(orders[0].remaining, 0.0);
        drop(orders);
        assert_eq!(
            fills.borrow().as_slice(),
            [
                json!({"id":"ws-native-order","side":"buy","symbol":"BTC-USDC","price":"123.55","amount":2.5})
            ],
            "real filled private update must emit exact fill payload"
        );
        h.venue().dispose().unwrap();
    }
}
fn ignored_topic(topic: &str) {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        snapshot(&h, &socket);
        let before_orders = h.store.orders().to_vec();
        let before_positions = h.store.positions().to_vec();
        let before_balance = h.store.balance().clone();
        send(
            &h,
            &socket,
            json!({"channel":topic,"events":[{"type":"update","positions":[{"product_id":"BTC-USDC","number_of_contracts":"999"}],"balances":[{"asset":"USDC","available":"999"}]}]}),
        );
        assert_eq!(
            h.store.orders().to_vec(),
            before_orders,
            "unsupported private topic must preserve orders"
        );
        assert_eq!(
            h.store.positions().to_vec(),
            before_positions,
            "source has no private position dispatcher"
        );
        assert_eq!(
            *h.store.balance(),
            before_balance,
            "source has no private balance dispatcher"
        );
        assert!(
            fills.borrow().is_empty(),
            "unsupported private topic must not invent fills"
        );
        h.venue().dispose().unwrap();
    }
}
#[test]
fn real_private_position_frame_preserves_source_noop() {
    ignored_topic("positions");
}
#[test]
fn real_private_balance_frame_preserves_source_noop() {
    ignored_topic("balances");
}
