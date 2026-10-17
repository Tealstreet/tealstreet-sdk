//! Ping timestamps must reach the production BaseWebSocket pong handler.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod support;
use safe_cex_exchange::{exchange::Exchange, websocket::WsEvent};
use serde_json::json;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(value) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return value;
        }
        rt.pump();
    }
    panic!("production exchange start did not settle");
}
#[test]
fn public_and_private_pongs_record_latency_from_the_sent_ping() {
    let (venue, rt, store, wire) = support::fixture();
    rt.now.set(1_000.0);
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
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    let sockets = rt.opened.borrow().clone();
    assert_eq!(sockets.len(), 2, "production public and private sockets");
    for (index, socket) in sockets.iter().enumerate() {
        assert!(
            socket.sent.borrow().iter().any(|frame| frame == "ping"),
            "socket {index} sent venue ping"
        );
        let elapsed = 25.0 + index as f64 * 22.0;
        rt.now.set(1_000.0 + elapsed);
        socket.events.borrow_mut().push_back(WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text("pong".into()),
        ));
        rt.pump();
        assert_eq!(
            store.latency(),
            (elapsed / 2.0).round(),
            "socket {index} pong records source half-roundtrip latency"
        );
    }
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    venue.dispose().unwrap();
}

struct Live {
    venue: std::rc::Rc<safe_cex_venue_blofin::native_exchange::BlofinNativeExchange>,
    rt: std::rc::Rc<support::Runtime>,
    store: std::rc::Rc<safe_cex_store::store::DefaultStore>,
    socket: std::rc::Rc<support::Socket>,
}
impl Live {
    fn new() -> Self {
        let (venue, rt, store, wire) = support::fixture();
        rt.now.set(1_000.0);
        for (path, data) in [
            ("/api/v1/account/margin-mode", json!({"marginMode":"cross"})),
            (
                "/api/v1/account/position-mode",
                json!({"positionMode":"net_mode"}),
            ),
            (
                "/api/v1/market/instruments",
                json!([{"instId":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","contractValue":"0.001","listTime":"1700000000000","expireTime":"0","maxLeverage":"150","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","instType":"SWAP","contractType":"linear","maxLimitSize":"100000","maxMarketSize":"10000","state":"live"}]),
            ),
        ] {
            wire.bodies
                .borrow_mut()
                .insert(path.into(), json!({"code":"0","data":data}));
        }
        rt.open.set(true);
        drive(&rt, venue.start()).unwrap();
        for _ in 0..8 {
            rt.pump();
        }
        let index = rt
            .dials
            .borrow()
            .iter()
            .position(|url| url.ends_with("/private"))
            .expect("production private socket dial");
        let socket = rt.opened.borrow()[index].clone();
        Self {
            venue,
            rt,
            store,
            socket,
        }
    }
    fn frame(&self, channel: &str, data: serde_json::Value) {
        let raw = json!({"arg":{"channel":channel},"data":data}).to_string();
        self.socket.events.borrow_mut().push_back(WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(raw),
        ));
        self.rt.pump();
        assert!(
            self.rt.errors.borrow().is_empty(),
            "{:?}",
            self.rt.errors.borrow()
        );
    }
}
impl Drop for Live {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
    }
}
fn order_row() -> serde_json::Value {
    json!({"orderId":"o1","clientOrderId":"client-1","instId":"BTC-USDT","marginMode":"cross","positionSide":"net","side":"buy","orderType":"limit","price":"100","size":"2","reduceOnly":"false","leverage":"10","state":"live","filledSize":"0","averagePrice":"0","fee":"0","pnl":"0","createTime":"1709990000000","updateTime":"1709990000000","orderCategory":"normal","tpTriggerPrice":null,"slTriggerPrice":null})
}
#[test]
fn private_orders_frame_updates_the_production_store() {
    let live = Live::new();
    live.frame("orders", json!([order_row()]));
    let rows = live.store.orders();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].id.as_str(), rows[0].symbol.as_str()),
        ("o1", "BTCUSDT")
    );
    assert_eq!(
        (
            rows[0].price.to_number(),
            rows[0].amount,
            rows[0].filled,
            rows[0].remaining
        ),
        (100.0, 0.002, 0.0, 0.002)
    );
}
#[test]
fn private_fill_frame_updates_order_and_emits_original_fill_fields() {
    let live = Live::new();
    let fills = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let saved = fills.clone();
    live.venue.base().on(
        "fill",
        std::rc::Rc::new(move |_, data| {
            let data = safe_cex_exchange::js_runtime::value::to_json(data).unwrap();
            saved.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    live.frame("orders", json!([order_row()]));
    let mut row = order_row();
    row["state"] = json!("partially_filled");
    row["filledSize"] = json!("0.5");
    row["averagePrice"] = json!("102");
    row["fillTime"] = json!("1710000000000");
    live.frame("orders", json!([row]));
    let rows = live.store.orders();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].filled, rows[0].remaining), (0.0005, 0.0015));
    assert_eq!(
        *fills.borrow(),
        [
            json!({"id":"o1","orderId":"o1","side":"buy","symbol":"BTCUSDT","price":102.0,"amount":0.0005,"timestamp":1710000000.0})
        ]
    );
}
#[test]
fn private_positions_frame_updates_positions_and_balance() {
    let live = Live::new();
    live.frame("positions", json!([{"positionId":"p1","instId":"BTC-USDT","instType":"SWAP","marginMode":"cross","positionSide":"net","positions":"2","availablePositions":"2","averagePrice":"100","markPrice":"102","liquidationPrice":"50","unrealizedPnl":"1.23","maintenanceMargin":"0.01","leverage":"10"}]));
    let rows = live.store.positions();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].symbol, "BTCUSDT");
    assert_eq!((rows[0].contracts, rows[0].entry_price), (0.002, 100.0));
    assert_eq!(
        (live.store.balance().used, live.store.balance().upnl),
        (0.01, 1.23)
    );
}
#[test]
fn private_account_frame_updates_balance() {
    let live = Live::new();
    live.frame("account", json!({"totalEquity":"123.45"}));
    let b = live.store.balance();
    assert_eq!(
        (b.total, b.free, b.used, b.upnl),
        (123.45, 123.45, 0.0, 0.0)
    );
}
