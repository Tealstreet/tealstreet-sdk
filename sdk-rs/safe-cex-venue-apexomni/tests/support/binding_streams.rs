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
