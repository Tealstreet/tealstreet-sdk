//! Classic orders-algo frames through the actual private socket, mapper and Store.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;

fn market() -> Value {
    json!({"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"100","minLever":"1","maxLever":"125"})
}
fn position(qty: &str) -> Value {
    json!({"symbol":"BTCUSDT","holdSide":"long","total":qty,"openPriceAvg":"50000","markPrice":"50100","marginMode":"crossed","posMode":"one_way_mode","leverage":"10","unrealizedPL":"2.5","liquidationPrice":"40000","achievedProfits":"0","totalFee":"0","deductedFee":"0","marginSize":"50","uTime":if qty=="0.01" {"1700000000000"} else {"1700000000010"}})
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
        Self { h, socket }
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
fn algo(id: &str, status: &str, plan: &str) -> Value {
    json!({"orderId":id,"clientOid":format!("client-{id}"),"symbol":"BTCUSDT","side":"buy","planType":plan,"triggerPrice":"60000","triggerType":"mark_price","size":"1","status":status,"tradeSide":"open","posSide":"long","posMode":"one_way_mode","cTime":"1700000000000"})
}
#[test]
fn real_algo_frame_skips_unmapped_status_and_keeps_later_rows() {
    let s = Scene::connect();
    let before =
        s.h.http
            .calls("/api/v2/mix/order/orders-plan-pending")
            .len();
    s.send(
        "orders-algo",
        json!([
            algo("algo-unmapped", "executing", "normal_plan"),
            algo("algo-live", "live", "profit_plan")
        ]),
    );
    let orders = s.h.store.orders();
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["algo-live"],
        "unmapped algo status must not abort the frame"
    );
    drop(orders);
    s.h.runtime.advance(10_000.0);
    assert!(
        s.h.http
            .calls("/api/v2/mix/order/orders-plan-pending")
            .len()
            > before,
        "algo frame must still schedule the order reconcile"
    );
    s.h.venue().dispose().unwrap();
}
