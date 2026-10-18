//! Actual public venue constructor must install both maintained socket classes.
#[path = "support/inverse_runtime.rs"]
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
    Store::update(h.store.as_ref(), StoreUpdate{
 markets:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"base":"BTC","quote":"USD","active":true,"isInverse":true,"contractSize":100,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":125}}})).unwrap()]),
 tickers:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"bid":49999,"ask":50001,"last":50000,"mark":50000,"index":50000,"percentage":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0,"openInterest":0})).unwrap()]),
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
        "real source socket opens through runtime; reports={:?}, logs={:?}, HTTP={:?}",
        h.runtime.reports.borrow(),
        h.runtime.logs.borrow(),
        h.http
            .requests
            .borrow()
            .iter()
            .map(|r| (
                r.get("method").unwrap().to_display(),
                r.get("url").unwrap().to_display()
            ))
            .collect::<Vec<_>>()
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
    let referents = Rc::new(RefCell::new(vec![]));
    let saved = referents.clone();
    let cleanup = invoke(
        &h,
        "listenOrderBook",
        vec![
            Js::str(SYMBOL),
            Js::function("capture", move |_, a| {
                saved.borrow_mut().push(a[0].clone());
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
    assert_eq!(
        seen.borrow().len(),
        1,
        "depth callbacks; reports={:?}, logs={:?}, HTTP={:?}",
        h.runtime.reports.borrow(),
        h.runtime.logs.borrow(),
        h.http
            .requests
            .borrow()
            .iter()
            .map(|r| r.get("url").unwrap().to_display())
            .collect::<Vec<_>>()
    );
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
        json!({"e":"depthUpdate","s":SYMBOL,"U":102,"u":103,"pu":999,"b":[["100","6"]],"a":[]}),
    );
    h.runtime.pump();
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[1]["bids"][0]["amount"], 6.);
    assert!(
        !referents.borrow()[0].strict_equals(&referents.borrow()[1]),
        "native callback profile is an owned snapshot"
    );
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_json(&referents.borrow()[0]).unwrap()["bids"][0]["amount"],
        4.,
        "earlier native callback stays frozen; maintained source sees6"
    );
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
    Store::update(h.store.as_ref(), StoreUpdate{positions:Some(vec![serde_json::from_value(json!({"id":SYMBOL,"symbol":SYMBOL,"side":"long","entryPrice":40000,"contracts":0.1,"notional":1000,"unrealizedPnl":0,"leverage":17,"liquidationPrice":20000,"marginType":"isolated","positionMode":"oneway","info":{"old":true}})).unwrap()]),..StoreUpdate::default()},false).unwrap();
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
    let info = p.info.value().unwrap();
    assert_eq!(info.get("contractsNative").unwrap().as_f64(), Some(2.));
    assert_eq!(info.get("old").unwrap().as_bool(), Some(true));
    assert_eq!(
        h.http.requests.borrow().len(),
        1,
        "paused background repair does not send positionRisk"
    );
}

fn private_member(h: &support::Harness, method: &str) {
    let socket = h.exchange.receiver().get("privateWebsocket").unwrap();
    socket.get(method).unwrap().call(socket, vec![]).unwrap();
    h.runtime.pump();
}
fn advance(h: &support::Harness, milliseconds: f64) {
    h.runtime.now.set(h.runtime.now.get() + milliseconds);
    h.runtime.pump();
}
fn fail_http(h: &support::Harness, message: &str) {
    h.http.replies.borrow_mut().push_back(Err(
        safe_cex_exchange::js_runtime::value::Thrown::error("Error", message),
    ));
}
#[test]
fn listen_key_retry_backoff_caps_and_real_force_close_blocks_retry() {
    let h = support::fixture(false, false);
    fail_http(&h, "listen key unavailable");
    private_member(&h, "connectAndSubscribe");
    assert_eq!(h.http.requests.borrow().len(), 1);
    assert!(h.runtime.sockets.borrow().is_empty());
    for (index, delay) in [5000., 10000., 20000., 40000., 60000., 60000.]
        .into_iter()
        .enumerate()
    {
        fail_http(&h, "still unavailable");
        advance(&h, delay - 1.);
        assert_eq!(
            h.http.requests.borrow().len(),
            index + 1,
            "retry must wait full source delay"
        );
        advance(&h, 1.);
        assert_eq!(
            h.http.requests.borrow().len(),
            index + 2,
            "retry at source deadline"
        );
    }
    h.exchange
        .base()
        .get_socket(SocketSlot::Private)
        .unwrap()
        .force_close();
    advance(&h, 60000.);
    assert_eq!(
        h.http.requests.borrow().len(),
        7,
        "force-closed real core blocks scheduled retry"
    );
}
#[test]
fn listen_key_refresh_failure_retries_then_resets_and_dispose_cancels_timer() {
    let h = support::fixture(false, false);
    key(&h, "key");
    private_member(&h, "fetchListenKey");
    assert_eq!(h.http.requests.borrow().len(), 1);
    fail_http(&h, "keepalive unavailable");
    advance(&h, 1_800_000. - 1.);
    assert_eq!(h.http.requests.borrow().len(), 1);
    advance(&h, 1.);
    assert_eq!(h.http.requests.borrow().len(), 2);
    assert_eq!(
        h.http.requests.borrow()[1]
            .get("method")
            .unwrap()
            .to_display(),
        "put"
    );
    assert_eq!(
        h.http.requests.borrow()[1].get("url").unwrap().to_display(),
        "/dapi/v1/listenKey"
    );
    key(&h, "unused successful keepalive body");
    advance(&h, 59999.);
    assert_eq!(h.http.requests.borrow().len(), 2);
    advance(&h, 1.);
    assert_eq!(h.http.requests.borrow().len(), 3);
    advance(&h, 60000.);
    assert_eq!(
        h.http.requests.borrow().len(),
        3,
        "success restores thirty-minute interval"
    );
    private_member(&h, "dispose");
    advance(&h, 1_800_000.);
    assert_eq!(
        h.http.requests.borrow().len(),
        3,
        "dispose clears actual refresh timer"
    );
}
#[test]
#[ignore = "mandatory maintained TS classes: venue-oracle:binance includes ignored tests"]
fn maintained_inverse_socket_classes_match_native_case_expectations() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/inverse-socket-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websocket_version_prevents_late_rest_position_snapshot_overwrite() {
    let h = support::fixture(false, false);
    seed(&h);
    key(&h, "key");
    let socket = connect(&h, SocketSlot::Private);
    h.http.hold.set(true);
    h.http.replies.borrow_mut().push_back(Ok(Js::array(vec![])));
    private_member(&h, "executePositionFetch");
    assert_eq!(h.http.requests.borrow().len(), 2);
    assert_eq!(
        h.http.requests.borrow()[1].get("url").unwrap().to_display(),
        "/dapi/v1/positionRisk"
    );
    frame(
        &socket,
        json!({"e":"ACCOUNT_UPDATE","a":{"P":[{"s":SYMBOL,"pa":"2","ps":"BOTH","ep":"45000","up":"0.02"}]}}),
    );
    assert_eq!(h.store.positions()[0].contracts, 0.004);
    h.http.release();
    h.runtime.pump();
    assert_eq!(
        h.store.positions()[0].contracts,
        0.004,
        "newer websocket position survives completed old REST snapshot"
    );
    assert_eq!(
        h.http.requests.borrow().len(),
        2,
        "follow-up reconciliation stays throttled"
    );
    let private = h.exchange.receiver().get("privateWebsocket").unwrap();
    assert!(
        !private.get("positionFetchInFlight").unwrap().truthy(),
        "normal finally releases latch"
    );
}

#[test]
fn public_escaped_surrogate_frame_retains_documented_family_refusal() {
    let h = support::fixture(true, false);
    seed(&h);
    let socket = connect(&h, SocketSlot::Public);
    socket.on_message(&SocketFrame::Text(r#"{"e":"24hrTicker","s":"BTCUSD_PERP","c":"50020.5","P":"-1.25","v":"9","q":"1","extra":"\ud800"}"#.into())).unwrap();
    assert_eq!(
        h.store.tickers()[0].last,
        50000.,
        "canonical family serde parser refuses escaped lone surrogate; source accepts and publishes50020.5"
    );
    frame(
        &socket,
        json!({"e":"24hrTicker","s":SYMBOL,"c":"50020.5","P":"-1.25","v":"9","q":"1","extra":"plain"}),
    );
    assert_eq!(
        h.store.tickers()[0].last,
        50020.5,
        "same real binding accepts valid plain frame"
    );
}
#[test]
fn private_warning_prefix_preserves_native_surrogate_error_and_rejection() {
    for method in ["connectAndSubscribe", "updateListenKey"] {
        for structured in [false, true] {
            let h = support::fixture(false, false);
            let units = [0xd800];
            let error = if structured {
                let e = safe_cex_exchange::js_runtime::value::Thrown::error("Error", "");
                e.value.set("message", Js::utf16(&units)).unwrap();
                e
            } else {
                safe_cex_exchange::js_runtime::value::Thrown::new(Js::utf16(&units))
            };
            h.http.replies.borrow_mut().push_back(Err(error));
            private_member(&h, method);
            let prefix = if method == "connectAndSubscribe" {
                "Binance COIN-M user data stream unavailable: "
            } else {
                "Binance COIN-M user data stream keepalive failed: "
            };
            let expected: Vec<u16> = prefix.encode_utf16().chain(units).collect();
            assert!(
                h.runtime.raw_consoles.borrow().iter().any(|v| v
                    .as_str()
                    .is_some_and(|units| units.as_ref() == expected.as_slice())),
                "actual Base error console must retain warning units for {method}/Error={structured}; observed={:?}",
                h.runtime.raw_consoles.borrow()
            );
        }
    }
}

#[test]
fn disconnected_candle_listener_clone_retries_and_shares_cleanup_state() {
    let h = support::fixture(true, false);
    seed(&h);
    let calls = Rc::new(RefCell::new(vec![]));
    let sink = calls.clone();
    let cleanup = invoke(
        &h,
        "listenOHLCV",
        vec![
            from_json(&json!({"symbol":SYMBOL,"interval":"1m"})),
            Js::function("capture", move |_, args| {
                sink.borrow_mut().push(args[0].clone());
                Ok(Js::Undefined)
            }),
        ],
    );
    h.runtime.pump();
    assert!(
        messages(&h).is_empty(),
        "disconnected listener waits for its clone retry"
    );
    let socket = connect(&h, SocketSlot::Public);
    advance(&h, 100.0);
    assert_eq!(messages(&h).iter().filter(|m|
        m["method"] == "SUBSCRIBE" && m["params"] == json!(["btcusd_perp@kline_1m"])
    ).count(), 1, "cloned retry registers exactly the original candle parser");
    let candle = json!({"e":"kline","s":SYMBOL,"k":{"s":SYMBOL,"i":"1m","t":1000,"o":"1","h":"2","l":"0.5","c":"1.5","v":"7"}});
    frame(&socket, candle.clone());
    assert_eq!(
        calls.borrow().len(),
        1,
        "cloned listener retains the original parser callback"
    );
    assert_eq!(
        calls.borrow()[0].get("close").unwrap().to_number().unwrap(),
        1.5
    );
    cleanup.call(Js::Undefined, vec![]).unwrap();
    frame(&socket, candle);
    assert_eq!(
        calls.borrow().len(),
        1,
        "cleanup removes the parser installed by the clone"
    );
    assert_eq!(
        messages(&h)
            .iter()
            .filter(|m| m["method"] == "UNSUBSCRIBE")
            .count(),
        1
    );

    h.exchange.base().dispose().unwrap();
    let h = support::fixture(true, false);
    let cancelled = invoke(
        &h,
        "listenOHLCV",
        vec![
            from_json(&json!({"symbol":SYMBOL,"interval":"5m"})),
            Js::function("cancelled", |_, _| {
                panic!("cancelled retry must never register")
            }),
        ],
    );
    h.runtime.pump();
    cancelled.call(Js::Undefined, vec![]).unwrap();
    connect(&h, SocketSlot::Public);
    advance(&h, 100.0);
    assert!(
        !messages(&h)
            .iter()
            .any(|m| m["method"] == "SUBSCRIBE" && m["params"] == json!(["btcusd_perp@kline_5m"])),
        "cleanup before connection cancels the cloned listener retry"
    );
    h.exchange.base().dispose().unwrap();
    assert!(h.runtime.reports.borrow().is_empty());
}

#[test]
fn production_inverse_depth_overflow_ids_keep_native_sequence_rules() {
    let mut completed = 0;
    for buffered in [false, true] {
        for previous in ["1e400", "-1e400"] {
            let h = support::fixture(true, false);
            seed(&h);
            let socket = connect(&h, SocketSlot::Public);
            h.http.hold.set(buffered);
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
                    Js::function("capture", move |_, args| {
                        sink.borrow_mut()
                            .push(args[0].get("bids")?.get("0")?.get("amount")?.to_number()?);
                        Ok(Js::Undefined)
                    }),
                ],
            );
            frame(
                &socket,
                json!({"e":"depthUpdate","s":SYMBOL,"U":101,"u":101,"pu":100,"b":[["100","4"]],"a":[]}),
            );
            h.runtime.pump();
            assert_eq!(seen.borrow().len(), usize::from(!buffered));
            let raw = r#"{"e":"depthUpdate","s":"BTCUSD_PERP","U":102,"u":102,"pu":OVERFLOW,"b":[["100","6"]],"a":[]}"#.replace("OVERFLOW", previous);
            socket.on_message(&SocketFrame::Text(raw)).unwrap();
            h.runtime.pump();
            if buffered {
                assert!(seen.borrow().is_empty(), "real snapshot remains pending");
                h.http.release();
                h.runtime.pump();
            }
            assert_eq!(
                seen.borrow().last().copied(),
                Some(6.0),
                "source uses contiguous U when pu overflows, buffered={buffered}, previous={previous}"
            );
            assert_eq!(seen.borrow().len(), if buffered { 1 } else { 2 });
            assert_eq!(
                h.http.requests.borrow().len(),
                1,
                "overflow pu must not trigger a spurious resnapshot"
            );
            h.http.hold.set(true);
            h.http.replies.borrow_mut().push_back(Ok(from_json(
                &json!({"lastUpdateId":102,"bids":[],"asks":[]}),
            )));
            socket.on_message(&SocketFrame::Text(r#"{"e":"depthUpdate","s":"BTCUSD_PERP","U":103,"u":1e400,"pu":102,"b":[],"a":[]}"#.into())).unwrap();
            h.runtime.pump();
            assert_eq!(
                h.http.requests.borrow().len(),
                2,
                "nonfinite final ID must request a held resnapshot"
            );
            cleanup.call(Js::Undefined, vec![]).unwrap();
            assert!(h.runtime.reports.borrow().is_empty());
            completed += 1;
        }
    }
    assert_eq!(
        completed, 4,
        "all live/buffered signed overflow cases execute"
    );
}
