fn public_socket(h: &Harness) -> std::rc::Rc<support::Socket> {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.contains("private"))
        .unwrap()
        .1
        .clone()
}
fn frame(socket: &support::Socket, value: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
}
#[test]
fn bound_listen_ohlcv() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let received = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Candle>::new()));
    let sink = received.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDC","interval":"1m"})).unwrap(),
            std::rc::Rc::new(move |c| sink.borrow_mut().push(c)),
        )
        .unwrap();
    let socket = public_socket(&h);
    frame(
        &socket,
        json!({"topic":"candle.1.BTCUSDC","data":[{"start":1700000000000_f64,"open":"99","high":"103","low":"98","close":"101","volume":"7"}]}),
    );
    h.runtime.pump();
    assert_eq!(received.borrow().len(), 1);
    assert_eq!(received.borrow()[0].close, 101.);
    cleanup().unwrap();
    done(&h);
}
#[test]
fn bound_listen_order_book() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let received = std::rc::Rc::new(std::cell::RefCell::new(Vec::<OrderBook>::new()));
    let sink = received.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDC".into(),
            std::rc::Rc::new(move |b| sink.borrow_mut().push(b)),
        )
        .unwrap();
    let socket = public_socket(&h);
    frame(
        &socket,
        json!({"topic":"orderBook200.H.BTCUSDC","type":"snapshot","data":{"b":[["99","4"]],"a":[["101","6"]]}}),
    );
    h.runtime.pump();
    assert_eq!(received.borrow().len(), 1);
    assert_eq!(received.borrow()[0].bids[0].price, 99.);
    cleanup().unwrap();
    assert_eq!(
        received.borrow()[0].bids.len(),
        1,
        "owned callback snapshot survives cleanup"
    );
    done(&h);
}
#[test]
fn private_login_precedes_subscribe_and_ping_frames_are_answered() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let sockets = h.runtime.sockets.borrow().clone();
    let private = sockets
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    let sent = private.sent.borrow().clone();
    let login = sent
        .iter()
        .position(|v| v.contains("\"op\":\"login\""))
        .unwrap();
    let subscribe = sent
        .iter()
        .position(|v| v.contains("\"op\":\"subscribe\""))
        .unwrap();
    assert!(login < subscribe, "auth is awaited before subscription");
    frame(&private, json!({"op":"ping","args":["server-ping"]}));
    h.runtime.pump();
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|v| v.contains("\"op\":\"pong\"") && v.contains("server-ping"))
    );
    done(&h);
}

#[test]
fn unknown_market_private_position_aborts_the_entire_frame() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let mut positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    for position in &mut positions {
        position.id = safe_cex_types::model_utils::create_position_id(
            "BTC-USDC",
            position.side.into_value(),
            Some(PositionMode::Hedged),
        );
    }
    h.store.add_or_update_positions(positions).unwrap();
    let before = h.store.positions().unwrap().to_vec();
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    private.sent.borrow_mut().clear();
    let result = h.exchange.base().get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private).unwrap().on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({
        "topic":"ws_zk_accounts_v3", "op":"ping", "args":["same-frame"],
        "contents":{"positions":[
            {"symbol":"BTC-USDC","side":"LONG","size":"3","entryPrice":"123","customImr":"0.1"},
            {"symbol":"DELISTED-USDC","side":"LONG","size":"1"}
        ]}
    }).to_string()));
    result.expect("socket dispatch reports the listener exception to the host");
    assert_eq!(*h.runtime.report_names.borrow(), ["TypeError"]);
    assert_eq!(*h.runtime.reports.borrow(), ["this.log is not a function"]);
    assert_eq!(
        h.store.positions().unwrap().as_slice(),
        before.as_slice(),
        "all row mapping finishes before Store writes"
    );
    assert!(
        !private
            .sent
            .borrow()
            .iter()
            .any(|text| text.contains("same-frame")),
        "mapping error skips the same frame's pong"
    );
    done(&h);
}

#[test]
fn known_market_private_position_updates_store_and_answers_ping() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let mut positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    for position in &mut positions {
        position.id = safe_cex_types::model_utils::create_position_id(
            "BTC-USDC",
            position.side.into_value(),
            Some(PositionMode::Hedged),
        );
    }
    h.store.add_or_update_positions(positions).unwrap();
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    frame(
        &private,
        json!({"topic":"ws_zk_accounts_v3","op":"ping","args":["valid-frame"],"contents":{"positions":[{"symbol":"BTC-USDC","side":"LONG","size":"3","entryPrice":"123","customImr":"0.1"}]}}),
    );
    h.runtime.pump();
    assert_eq!(
        h.store
            .positions()
            .unwrap()
            .iter()
            .find(|p| p.id == "BTC-USDC:long")
            .unwrap()
            .contracts,
        3.0
    );
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|text| text.contains("valid-frame"))
    );
    assert!(h.runtime.reports.borrow().is_empty());
    done(&h);
}
#[test]
fn invalid_caller_id_fails_before_network() {
    let h = fixture();
    let mut opts = placed();
    opts.client_order_id = Some("invalid id".into());
    assert!(
        h.drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
            .is_err()
    );
    assert!(h.http.requests.borrow().is_empty());
    done(&h);
}

#[test]
fn recorded_public_pong_uses_registered_native_handlers() {
    use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
    use safe_cex_transport::socket::SocketFrame;
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let recorded: Value = serde_json::from_str(include_str!("../recorded-pong.json")).unwrap();
    let socket = public_socket(&h);
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            recorded["payload"].as_str().unwrap().into(),
        )));
    h.runtime.pump();
    let core = h.venue().base().get_socket(SocketSlot::Public).unwrap();
    let stats = core.message_handlers_stats.borrow()["pong"].clone();
    assert_eq!(
        stats.had_handlers,
        Some(true),
        "actual recorded pong has its registered native callback"
    );
    assert!(matches!(
        stats.last_callbacks_count,
        safe_cex_num::PrototypeScalar::Number(1.0)
    ));
    done(&h);
}
