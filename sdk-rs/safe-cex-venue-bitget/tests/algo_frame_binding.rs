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

#[test]
fn real_algo_frame_keeps_open_order_with_unmapped_plan_type() {
    let s = Scene::connect();
    s.send(
        "orders-algo",
        json!([
            algo("algo-unknown-plan", "live", "normal_plan"),
            algo("algo-known-plan", "live", "profit_plan")
        ]),
    );
    let orders = s.h.store.orders();
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["algo-unknown-plan", "algo-known-plan"],
        "unmapped plan type must remain in Store and not abort later rows"
    );
    assert!(
        orders[0].r#type.is_undefined(),
        "real mapper retains own undefined type"
    );
    let unknown = serde_json::to_value(&orders[0]).unwrap();
    assert!(
        unknown.get("type").is_none(),
        "source undefined type must omit on JSON projection"
    );
    assert_eq!(unknown["status"], "open");
    drop(orders);
    s.h.venue().dispose().unwrap();
}

fn rest_orders_keep_unmapped(name: ExchangeName) {
    let h = harness(name, options(), |http| {
        script(http);
        http.routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/api/v2/mix/order/orders-pending");
        http.ok(
            "GET",
            "/api/v2/mix/order/orders-pending",
            json!({"entrustedList":[
            algo("rest-unmapped", "executing", "normal_plan"),
            algo("rest-known", "live", "profit_plan")
        ], "endId":null}),
        );
    });
    let orders = h
        .drive(h.venue().fetch_orders(None))
        .expect("one unmapped row must not reject the REST page");
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["rest-unmapped", "rest-known"]
    );
    assert!(
        orders[0].status.is_undefined(),
        "real REST mapper retains own undefined status"
    );
    assert!(
        orders[0].r#type.is_undefined(),
        "real REST mapper retains own undefined type"
    );
    let unknown = serde_json::to_value(&orders[0]).unwrap();
    assert!(
        unknown.get("status").is_none(),
        "source undefined status must omit on JSON projection"
    );
    assert!(
        unknown.get("type").is_none(),
        "source undefined type must omit on JSON projection"
    );
    assert_eq!(
        serde_json::to_value(&orders[1]).unwrap()["type"],
        "take_profit_market"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn real_rest_orders_keep_unmapped_status_and_plan_type() {
    for name in [ExchangeName::Bitget, ExchangeName::Bitgetusdc] {
        rest_orders_keep_unmapped(name);
    }
}

#[test]
fn real_inverse_rest_orders_keep_unmapped_status_and_plan_type() {
    rest_orders_keep_unmapped(ExchangeName::Bitgetinverse);
}

#[test]
fn real_algo_unmapped_status_still_evaluates_mapper() {
    let s = Scene::connect();
    let mut malformed = algo("algo-invalid", "executing", "normal_plan");
    malformed["symbol"] = json!({});
    s.send(
        "orders-algo",
        json!([malformed, algo("algo-later", "live", "profit_plan")]),
    );
    assert!(
        s.h.store.orders().is_empty(),
        "source mapper error must still stop the frame before later rows"
    );
    s.h.venue().dispose().unwrap();
}

#[test]
fn real_algo_missing_side_retains_native_undefined_before_store_projection() {
    let s = Scene::connect();
    let mut absent = algo("algo-missing-side", "live", "profit_plan");
    absent.as_object_mut().unwrap().remove("side");
    s.send(
        "orders-algo",
        json!([absent, algo("algo-known-side", "live", "profit_plan")]),
    );
    let orders = s.h.store.orders();
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["algo-missing-side", "algo-known-side"]
    );
    assert!(
        orders[0].side.is_undefined(),
        "source assignment is own undefined in actual Store"
    );
    assert!(
        !orders[0].side.is_absent(),
        "own undefined is distinct from absent"
    );
    assert_eq!(
        orders[1].side.value(),
        Some(&safe_cex_types::OrderSide::Buy)
    );
    assert!(
        serde_json::to_value(&orders[0])
            .unwrap()
            .get("side")
            .is_none(),
        "JSON projection alone would hide the distinction"
    );
    drop(orders);
    s.h.venue().dispose().unwrap();
}

#[test]
fn real_rest_missing_side_retains_undefined_but_unknown_raw_side_refuses() {
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        for unknown_raw in [false, true] {
            let mut row = algo("rest-side", "live", "profit_plan");
            if unknown_raw {
                row["side"] = json!("unmapped-side");
            } else {
                row.as_object_mut().unwrap().remove("side");
            }
            let h = harness(name.clone(), options(), |http| {
                script(http);
                http.routes
                    .borrow_mut()
                    .retain(|(_, path, _)| path != "/api/v2/mix/order/orders-pending");
                http.ok(
                    "GET",
                    "/api/v2/mix/order/orders-pending",
                    json!({"entrustedList":[row],"endId":null}),
                );
            });
            let actual = h.drive(h.venue().fetch_orders(None));
            if unknown_raw {
                let error =
                    actual.expect_err("unknown raw side cannot enter the closed shared enum");
                assert_eq!(error.name, "RepresentationError");
                assert!(error.message.contains("side"), "{}", error.message);
            } else {
                let rows = actual.expect("missing raw side is representable own undefined");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].id, "rest-side");
                assert!(
                    rows[0].side.is_undefined(),
                    "native REST property before JSON"
                );
                assert!(!rows[0].side.is_absent());
                assert!(
                    serde_json::to_value(&rows[0])
                        .unwrap()
                        .get("side")
                        .is_none()
                );
            }
        }
    }
}
