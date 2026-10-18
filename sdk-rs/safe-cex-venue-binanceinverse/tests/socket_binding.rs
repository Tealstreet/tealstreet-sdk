//! Actual public venue constructor must install both maintained socket classes.
mod support;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
#[test]
fn public_socket_is_assembled_by_real_constructor() {
    let h = support::fixture(true, false);
    assert!(
        h.exchange.base().get_socket(SocketSlot::Public).is_some(),
        "real inverse constructor must install maintained public socket"
    );
}
#[test]
fn private_socket_is_assembled_by_real_constructor() {
    let h = support::fixture(false, true);
    assert!(
        h.exchange.base().get_socket(SocketSlot::Private).is_some(),
        "real inverse constructor must install maintained private socket"
    );
}

use safe_cex_exchange::{
    js_runtime::value::{Js, from_json},
    websocket::BaseWebSocket,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
const SYMBOL: &str = "BTCUSD_PERP";
fn seed(h: &support::Harness) {
    h.store.update(StoreUpdate{
 markets:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"base":"BTC","quote":"USD","active":true,"isInverse":true,"contractSize":100,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":125}}})).unwrap()]),
 tickers:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"bid":49999,"ask":50001,"last":50000,"mark":50000,"index":50000,"percentage":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap()]),
 ..StoreUpdate::default()},false).unwrap();
}
fn connect(h: &support::Harness, slot: SocketSlot) -> Rc<BaseWebSocket> {
    let socket = h
        .exchange
        .base()
        .get_socket(slot)
        .expect("real installed socket");
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    assert!(
        socket.is_connected(),
        "real source socket opens through runtime"
    );
    socket
}
fn frame(socket: &Rc<BaseWebSocket>, value: serde_json::Value) {
    socket
        .on_message(&SocketFrame::Text(value.to_string()))
        .unwrap();
}
fn invoke(h: &support::Harness, name: &str, args: Vec<Js>) -> Js {
    let parent = h.exchange.receiver();
    parent.get(name).unwrap().call(parent, args).unwrap()
}
fn messages(h: &support::Harness) -> Vec<serde_json::Value> {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .flat_map(|(_, s)| s.sent.borrow().clone())
        .map(|s| serde_json::from_str(&s).unwrap())
        .collect()
}
fn key(h: &support::Harness, text: &str) {
    h.http
        .replies
        .borrow_mut()
        .push_back(Ok(from_json(&json!({"listenKey":text}))));
}

#[test]
fn public_wire_topics_and_all_three_ticker_streams_reach_real_store() {
    for (public_only, testnet, host) in [
        (true, false, "wss://dstream.binance.com/ws"),
        (false, true, "wss://dstream.binancefuture.com/ws"),
    ] {
        let h = support::fixture(public_only, testnet);
        seed(&h);
        let socket = connect(&h, SocketSlot::Public);
        assert_eq!(h.runtime.sockets.borrow()[0].0, host);
        let topics = if public_only {
            json!(["!ticker@arr", "!markPrice@arr@1s"])
        } else {
            json!(["!ticker@arr", "!markPrice@arr@1s", "!bookTicker"])
        };
        assert!(messages(&h).contains(&json!({"id":1,"method":"SUBSCRIBE","params":topics})));
        frame(
            &socket,
            json!({"e":"24hrTicker","s":SYMBOL,"c":"50020.5","P":"-1.25","v":"9","q":"","Q":"450184.5"}),
        );
        frame(
            &socket,
            json!({"e":"markPriceUpdate","s":SYMBOL,"p":"50010","i":"50011","r":"0.0001","T":1700000000123u64}),
        );
        frame(
            &socket,
            json!({"e":"bookTicker","s":SYMBOL,"b":"50009","a":"50012"}),
        );
        let tickers = h.store.tickers();
        let t = &tickers[0];
        assert_eq!(t.last, 50020.5);
        assert_eq!(t.percentage, -1.25);
        assert_eq!(t.volume, 9.);
        assert_eq!(t.quote_volume, 450184.5);
        assert_eq!(t.mark, 50010.);
        assert_eq!(t.index, 50011.);
        assert_eq!(t.bid, 50009.);
        assert_eq!(t.ask, 50012.);
    }
}
#[test]
fn candle_callbacks_refcount_and_cleanup_use_actual_socket() {
    let h = support::fixture(true, false);
    seed(&h);
    let socket = connect(&h, SocketSlot::Public);
    let calls = Rc::new(RefCell::new(vec![]));
    let sink = calls.clone();
    let cleanup = invoke(
        &h,
        "listenOHLCV",
        vec![
            from_json(&json!({"symbol":SYMBOL,"interval":"1m"})),
            Js::function("capture", move |_, a| {
                sink.borrow_mut().push(a[0].clone());
                Ok(Js::Undefined)
            }),
        ],
    );
    let other = invoke(
        &h,
        "listenOHLCV",
        vec![
            from_json(&json!({"symbol":SYMBOL,"interval":"1m"})),
            Js::function("other", |_, _| Ok(Js::Undefined)),
        ],
    );
    frame(
        &socket,
        json!({"e":"kline","s":SYMBOL,"k":{"s":SYMBOL,"i":"1m","t":1700000000000u64,"o":"1","h":"2","l":"0.5","c":"1.5","v":"7"}}),
    );
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        calls.borrow()[0]
            .get("timestamp")
            .unwrap()
            .to_number()
            .unwrap(),
        1700000000.
    );
    assert_eq!(
        calls.borrow()[0]
            .get("volume")
            .unwrap()
            .to_number()
            .unwrap(),
        7.
    );
    cleanup.call(Js::Undefined, vec![]).unwrap();
    cleanup.call(Js::Undefined, vec![]).unwrap();
    assert!(!messages(&h).iter().any(|m| m["method"] == "UNSUBSCRIBE"));
    other.call(Js::Undefined, vec![]).unwrap();
    assert_eq!(
        messages(&h)
            .iter()
            .filter(
                |m| m["method"] == "UNSUBSCRIBE" && m["params"] == json!(["btcusd_perp@kline_1m"])
            )
            .count(),
        1
    );
    frame(
        &socket,
        json!({"e":"kline","s":SYMBOL,"k":{"s":SYMBOL,"i":"1m","t":1,"o":"1","h":"2","l":"0","c":"1","v":"1"}}),
    );
    assert_eq!(calls.borrow().len(), 1);
}
#[test]
fn depth_buffers_snapshot_requires_previous_id_and_resyncs_gap() {
    let h = support::fixture(true, false);
    seed(&h);
    let socket = connect(&h, SocketSlot::Public);
    h.http.replies.borrow_mut().push_back(Ok(from_json(
        &json!({"lastUpdateId":100,"bids":[["100","2"]],"asks":[["101","3"]]}),
    )));
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let cleanup = invoke(
        &h,
        "listenOrderBook",
        vec![
            Js::str(SYMBOL),
            Js::function("capture", move |_, a| {
                sink.borrow_mut()
                    .push(safe_cex_exchange::js_runtime::value::to_json(&a[0])?);
                Ok(Js::Undefined)
            }),
        ],
    );
    frame(
        &socket,
        json!({"e":"depthUpdate","s":SYMBOL,"U":101,"u":101,"pu":100,"b":[["100","4"]],"a":[]}),
    );
    h.runtime.pump();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0]["bids"][0]["amount"], 4.);
    assert_eq!(seen.borrow()[0]["bids"][0]["total"], 4.);
    frame(
        &socket,
        json!({"e":"depthUpdate","s":SYMBOL,"U":100,"u":100,"pu":99,"b":[["100","99"]],"a":[]}),
    );
    assert_eq!(seen.borrow().len(), 1, "old final id ignored");
    h.http.replies.borrow_mut().push_back(Ok(from_json(
        &json!({"lastUpdateId":102,"bids":[["100","5"]],"asks":[]}),
    )));
    frame(
        &socket,
        json!({"e":"depthUpdate","s":SYMBOL,"U":103,"u":103,"pu":999,"b":[["100","6"]],"a":[]}),
    );
    h.runtime.pump();
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[1]["bids"][0]["amount"], 6.);
    let requests = h.http.requests.borrow();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request.get("url").unwrap().to_display(), "/dapi/v1/depth");
        assert_eq!(
            request
                .get("params")
                .unwrap()
                .get("limit")
                .unwrap()
                .to_number()
                .unwrap(),
            1000.
        );
    }
    drop(requests);
    cleanup.call(Js::Undefined, vec![]).unwrap();
    assert!(
        messages(&h)
            .iter()
            .any(|m| m["method"] == "UNSUBSCRIBE" && m["params"] == json!(["btcusd_perp@depth"]))
    );
}
#[test]
fn private_stream_preserves_big_ids_raw_fill_units_and_terminal_reason() {
    let h = support::fixture(false, false);
    seed(&h);
    key(&h, "key /+λ");
    let socket = connect(&h, SocketSlot::Private);
    assert_eq!(
        h.runtime.sockets.borrow()[0].0,
        "wss://dstream.binance.com/ws/key%20%2F%2B%CE%BB"
    );
    assert!(messages(&h).contains(&json!({"id":42,"method":"LIST_SUBSCRIPTIONS"})));
    let fills = Rc::new(RefCell::new(vec![]));
    let sink = fills.clone();
    h.exchange.base().on(
        "fill",
        Rc::new(move |_, v| {
            sink.borrow_mut().push(v.clone());
            Ok(())
        }),
    );
    let event = |status: &str, filled: &str, reject: &str| json!({"e":"ORDER_TRADE_UPDATE","o":{"i":12345678901234567u64,"s":SYMBOL,"S":"BUY","ot":"STOP_MARKET","q":"10","z":filled,"l":"2","ap":"50000","p":"0","sp":"49000","R":true,"cp":false,"wt":"MARK_PRICE","X":status,"r":reject}});
    frame(&socket, event("NEW", "0", ""));
    {
        let orders = h.store.orders();
        let o = &orders[0];
        assert_eq!(o.id, "12345678901234567");
        assert_eq!(o.amount, 10.);
        assert_eq!(o.price.value(), Some(&49000.));
        assert_eq!(o.reduce_only.value(), Some(&true));
        assert_eq!(
            o.sl_trigger_type.value(),
            Some(&safe_cex_types::OrderStopTriggerType::MarkPrice)
        );
    }
    frame(&socket, event("PARTIALLY_FILLED", "3", ""));
    assert_eq!(fills.borrow().len(), 1);
    assert_eq!(
        fills.borrow()[0]
            .get("amount")
            .unwrap()
            .to_number()
            .unwrap(),
        2.,
        "source emits native contracts, not converted base"
    );
    assert_eq!(h.store.orders()[0].filled, 3.);
    assert_eq!(h.store.orders()[0].remaining, 7.);
    frame(&socket, event("CANCELLED", "3", "GTX would cross"));
    let orders = h.store.orders();
    assert_eq!(
        orders[0].status.value(),
        Some(&safe_cex_types::OrderStatus::Canceled)
    );
    assert_eq!(
        orders[0].extra.value().unwrap().reject_reason,
        Some(safe_cex_types::OrderRejectReason::PostOnly)
    );
}
#[test]
fn account_frame_converts_inverse_contracts_and_preserves_existing_fields() {
    let h = support::fixture(false, false);
    seed(&h);
    key(&h, "key");
    let socket = connect(&h, SocketSlot::Private);
    h.store.update(StoreUpdate{positions:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"side":"long","entryPrice":40000,"contracts":0.1,"notional":1000,"unrealizedPnl":0,"leverage":17,"liquidationPrice":20000,"marginType":"isolated","positionMode":"oneway","info":{"old":true}})).unwrap()]),..StoreUpdate::default()},false).unwrap();
    h.exchange
        .receiver()
        .set(
            "backgroundReconciliationPausedUntil",
            Js::Number(support::START_MS + 30000.),
        )
        .unwrap();
    frame(
        &socket,
        json!({"e":"ACCOUNT_UPDATE","a":{"P":[{"s":SYMBOL,"pa":"-2","ps":"BOTH","ep":"45000","up":"-0.02"}]}}),
    );
    let positions = h.store.positions();
    let p = &positions[0];
    assert_eq!(p.contracts, 0.004);
    assert_eq!(p.notional, 200.);
    assert_eq!(p.side.value(), Some(&safe_cex_types::PositionSide::Short));
    assert_eq!(p.leverage, 17.);
    assert_eq!(p.liquidation_price, 20000.);
    assert_eq!(p.info.value().unwrap()["contractsNative"], 2.);
    assert_eq!(p.info.value().unwrap()["old"], true);
    assert_eq!(
        h.http.requests.borrow().len(),
        1,
        "paused background repair does not send positionRisk"
    );
}
