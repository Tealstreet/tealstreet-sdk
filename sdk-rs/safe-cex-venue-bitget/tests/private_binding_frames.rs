//! Classic private frames through the actual socket, mapper, Store and emitter.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderStatus, PositionSide};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn market() -> Value {
    json!({"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"100","minLever":"1","maxLever":"125"})
}
fn position(qty: &str) -> Value {
    json!({"symbol":"BTCUSDT","holdSide":"long","total":qty,"openPriceAvg":"50000","markPrice":"50100","marginMode":"crossed","posMode":"one_way_mode","leverage":"10","unrealizedPL":"2.5","liquidationPrice":"40000","achievedProfits":"0","totalFee":"0","deductedFee":"0","marginSize":"50","uTime":if qty=="0.01" {"1700000000000"} else {"1700000000010"}})
}
fn order(status: &str) -> Value {
    json!({"orderId":"private-native-order","clientOid":"private-native-client","symbol":"BTCUSDT","side":"buy","orderType":"limit","price":"100","priceAvg":"101.25","size":"2","baseVolume":if status=="filled" {"2"} else {"0"},"status":status,"tradeSide":"open","posSide":"long","reduceOnly":"NO","force":"gtc","posMode":"one_way_mode","cTime":"1700000000000"})
}
fn script(http: &Http) {
    http.route(
        "GET",
        "/api/v3/account/settings",
        400.0,
        json!({"code":"40084","msg":"Classic Account mode","requestTime":1,"data":null}),
    );
    http.ok(
        "GET",
        "/api/v2/spot/account/info",
        json!({"userId":"synthetic-user","parentId":null}),
    );
    http.ok("GET", "/api/v2/mix/market/contracts", json!([market()]));
    http.ok("GET","/api/v2/mix/market/tickers",json!([{"symbol":"BTCUSDT","lastPr":"50000","bidPr":"49999","askPr":"50001","indexPrice":"50000.5","changeUtc24h":"0.01","fundingRate":"0.0001","baseVolume":"10","quoteVolume":"500000"}]));
    http.ok("GET","/api/v2/mix/account/accounts",json!([{"marginCoin":"USDT","available":"100","accountEquity":"120","unrealizedPL":"2","locked":"5"}]));
    http.ok(
        "GET",
        "/api/v2/mix/position/all-position",
        json!([position("0.01")]),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-pending",
        json!({"entrustedList":[],"endId":null}),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-plan-pending",
        json!({"entrustedList":[],"endId":null}),
    );
}
struct Scene {
    h: Harness,
    socket: Rc<Socket>,
    fills: Rc<RefCell<Vec<Value>>>,
}
impl Scene {
    fn connect() -> Self {
        let h = harness(ExchangeName::Bitget, options(), script);
        h.drive(h.venue().start()).unwrap();
        let socket = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url == "wss://ws.bitget.com/v2/ws/private")
            .unwrap()
            .1
            .clone();
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                r#"{"event":"login","code":0}"#.into(),
            )));
        h.runtime.advance(10.0);
        assert!(
            socket.sent.borrow().iter().any(|m| m.contains("subscribe")),
            "real private connection must subscribe after auth"
        );
        let fills = Rc::new(RefCell::new(Vec::new()));
        let observed = fills.clone();
        h.venue().base().on(
            "fill",
            Rc::new(move |_, value| {
                observed.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        Self { h, socket, fills }
    }
    fn send(&self, channel: &str, data: Value) {
        let frame = json!({"action":if channel=="positions" {"snapshot"} else {"update"},"arg":{"instType":"USDT-FUTURES","channel":channel,"instId":"default"},"data":data});
        self.socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
        self.h.runtime.pump();
    }
}
#[test]
fn real_private_order_frame_populates_store_without_fill() {
    let s = Scene::connect();
    assert!(s.h.store.orders().is_empty());
    s.send("orders", json!([order("live")]));
    let orders = s.h.store.orders();
    assert_eq!(orders.len(), 1, "real orders frame must reach Store");
    assert_eq!(orders[0].id, "private-native-order");
    assert_eq!(orders[0].symbol, "BTCUSDT");
    assert_eq!(orders[0].status, OrderStatus::Open);
    assert_eq!(orders[0].amount, 2.0);
    assert_eq!(orders[0].remaining, 2.0);
    assert!(s.fills.borrow().is_empty());
    drop(orders);
    s.h.venue().dispose().unwrap();
}
#[test]
fn real_private_fill_closes_known_order_and_emits_exact_payload() {
    let s = Scene::connect();
    s.send("orders", json!([order("live")]));
    assert_eq!(
        s.h.store.orders().len(),
        1,
        "fill witness must start with known open order"
    );
    s.send("orders", json!([order("filled")]));
    let orders = s.h.store.orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(
        orders[0].status,
        OrderStatus::Closed,
        "real fill must patch existing order status"
    );
    // TS updateOrderStatus patches only status; quantities remain until reconciliation.
    assert_eq!(orders[0].filled, 0.0);
    assert_eq!(orders[0].remaining, 2.0);
    assert_eq!(
        s.fills.borrow().as_slice(),
        [
            json!({"id":"private-native-order","side":"buy","symbol":"BTCUSDT","price":"101.25","amount":2.0})
        ],
        "real fill must emit exact native contents"
    );
    drop(orders);
    s.h.venue().dispose().unwrap();
}
#[test]
fn real_private_position_snapshot_updates_native_position() {
    let s = Scene::connect();
    s.send("positions", json!([position("0.25")]));
    let positions = s.h.store.positions();
    assert_eq!(
        positions.len(),
        1,
        "real position snapshot must replace Store positions"
    );
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].side, PositionSide::Long);
    assert_eq!(
        positions[0].contracts, 0.25,
        "real position frame must preserve fresh WS contracts"
    );
    assert_eq!(positions[0].entry_price, 50000.0);
    assert_eq!(positions[0].leverage, 10.0);
    assert!(s.fills.borrow().is_empty());
    drop(positions);
    s.h.venue().dispose().unwrap();
}
#[test]
fn real_private_account_frame_updates_native_balance() {
    let s = Scene::connect();
    Store::update(
        s.h.store.as_ref(),
        StoreUpdate {
            positions: Some(vec![]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    s.send("account",json!([{"marginCoin":"USDT","unionTotalMargin":"130","unionAvailable":"90","unrealizedPL":"3.5"}]));
    let balance = s.h.store.balance();
    assert_eq!(
        balance.free, 90.0,
        "real account frame must reach Store balance"
    );
    assert_eq!(balance.used, 40.0);
    assert_eq!(balance.total, 126.5);
    assert_eq!(balance.upnl, 3.5);
    assert!(s.fills.borrow().is_empty());
    drop(balance);
    s.h.venue().dispose().unwrap();
}

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
        clock.sync_from_server_time((clock.now)());
    }
}

#[test]
fn synced_clock_keeps_fresh_private_position_during_rest_reconciliation() {
    let _reset = ResetClock;
    let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    clock.sync_from_server_time((clock.now)() + 40_000.0);
    let s = Scene::connect();
    let before = s.h.http.calls("/api/v2/mix/position/all-position").len();
    s.send("positions", json!([position("0.25")]));
    for _ in 0..40 {
        s.h.runtime.advance(50.0);
    }
    assert!(
        s.h.http.calls("/api/v2/mix/position/all-position").len() > before,
        "private frame must trigger actual REST position reconciliation"
    );
    assert_eq!(
        s.h.store.positions()[0].contracts, 0.25,
        "fresh private position must survive the older REST payload under the synced clock"
    );
    assert!(s.h.runtime.reports.borrow().is_empty());
    s.h.venue().dispose().unwrap();
}
