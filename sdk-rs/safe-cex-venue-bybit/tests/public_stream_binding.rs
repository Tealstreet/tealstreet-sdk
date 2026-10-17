#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::BaseWebSocket};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderBook};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

fn setup(name: ExchangeName) -> (support::Harness, Rc<BaseWebSocket>, &'static str) {
    setup_for(name, false)
}

fn setup_for(
    name: ExchangeName,
    secondary: bool,
) -> (support::Harness, Rc<BaseWebSocket>, &'static str) {
    let inverse = name == ExchangeName::Bybitinverse || secondary;
    let symbol = if inverse { "BTCUSD" } else { "BTCUSDT" };
    let mut options = support::options();
    if secondary {
        options.enable_inverse_markets = Some(true);
    }
    let h = support::harness_as(name, options, |_| {});
    Store::update(h.store.as_ref(), StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":if inverse {"USD"} else {"USDT"},"active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}}})).unwrap()]),
        ..Default::default()
    }, false).unwrap();
    let socket = h
        .venue()
        .base()
        .get_socket(if inverse {
            SocketSlot::PublicSecondary
        } else {
            SocketSlot::Public
        })
        .unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    assert!(socket.is_connected(), "real public socket must open");
    (h, socket, symbol)
}

fn book(name: ExchangeName) {
    let depth = if name == ExchangeName::Bybitspot {
        200
    } else {
        1000
    };
    let (h, socket, symbol) = setup(name);
    let seen = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            symbol.into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    let topic = format!("orderbook.{depth}.{symbol}");
    socket.on_message(&SocketFrame::Text(json!({"topic":topic,"type":"snapshot","data":{"u":1,"b":[["100","2"]],"a":[["101","3"]]}}).to_string())).unwrap();
    assert_eq!(
        seen.borrow().len(),
        1,
        "Exchange listener must receive the real public frame"
    );
    assert_eq!(seen.borrow()[0].bids[0].price, 100.0);
    assert_eq!(seen.borrow()[0].asks[0].total, 3.0);
    socket.on_message(&SocketFrame::Text(json!({"topic":topic,"type":"delta","data":{"u":2,"b":[["100","4"]],"a":[["101","0"]]}}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[1].bids[0].amount, 4.0);
    assert!(seen.borrow()[1].asks.is_empty());
    cleanup().unwrap();
    socket
        .on_message(&SocketFrame::Text(
            json!({"topic":topic,"type":"snapshot","data":{"u":3,"b":[],"a":[]}}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        seen.borrow().len(),
        2,
        "cleanup removes only the subscribed callback"
    );
    let wire = h.runtime.sockets.borrow()[0].1.clone();
    let sent: Vec<serde_json::Value> = wire
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(sent.contains(&json!({"op":"subscribe","args":[topic]})));
    assert!(sent.contains(&json!({"op":"unsubscribe","args":[topic]})));
    h.venue().dispose().unwrap();
}

#[test]
fn primary_book_listener_reaches_real_socket() {
    book(ExchangeName::Bybit);
}
#[test]
fn demo_book_listener_reaches_real_socket() {
    book(ExchangeName::Bybitdemo);
}
#[test]
fn spot_book_listener_reaches_real_socket() {
    book(ExchangeName::Bybitspot);
}
#[test]
fn inverse_book_listener_reaches_real_socket() {
    book(ExchangeName::Bybitinverse);
}

fn sent_topics(h: &support::Harness, op: &str, topic: &str) -> usize {
    h.runtime
        .sockets
        .borrow()
        .iter()
        .flat_map(|(_, socket)| socket.sent.borrow().clone())
        .filter(|s| {
            let frame: serde_json::Value = serde_json::from_str(s).unwrap();
            frame["op"] == op
                && frame["args"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|v| v == topic))
        })
        .count()
}

#[test]
fn primary_inverse_book_routes_to_the_secondary_socket() {
    let (h, socket, symbol) = setup_for(ExchangeName::Bybit, true);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            symbol.into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    let topic = format!("orderbook.1000.{symbol}");
    socket
        .on_message(&SocketFrame::Text(
            json!({"topic":topic,"type":"snapshot","data":{"u":1,"b":[["100","2"]],"a":[]}})
                .to_string(),
        ))
        .unwrap();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(sent_topics(&h, "subscribe", &topic), 1);
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}

#[test]
fn book_sequence_gap_retries_after_the_source_delay() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let depth = if name == ExchangeName::Bybitspot {
            200
        } else {
            1000
        };
        let (h, socket, symbol) = setup(name);
        let seen = Rc::new(RefCell::new(vec![]));
        let sink = seen.clone();
        let cleanup = h
            .venue()
            .listen_order_book(
                symbol.into(),
                Rc::new(move |book| sink.borrow_mut().push(book)),
            )
            .unwrap();
        let topic = format!("orderbook.{depth}.{symbol}");
        for (kind, u) in [("snapshot", 1), ("delta", 9)] {
            socket
                .on_message(&SocketFrame::Text(
                    json!({"topic":topic,"type":kind,"data":{"u":u,"b":[["100","2"]],"a":[]}})
                        .to_string(),
                ))
                .unwrap();
        }
        assert_eq!(
            seen.borrow().len(),
            1,
            "gap must suppress the incomplete book"
        );
        assert_eq!(sent_topics(&h, "unsubscribe", &topic), 1);
        h.runtime.now.set(h.runtime.now.get() + 99.0);
        h.runtime.pump();
        assert_eq!(sent_topics(&h, "subscribe", &topic), 1);
        h.runtime.now.set(h.runtime.now.get() + 1.0);
        h.runtime.pump();
        assert_eq!(sent_topics(&h, "subscribe", &topic), 2);
        cleanup().unwrap();
        h.venue().dispose().unwrap();
    }
}

#[test]
fn book_cleanup_cancels_subscription_waiting_for_connection() {
    let h = support::harness(support::options(), |_| {});
    let socket = h.venue().base().get_socket(SocketSlot::Public).unwrap();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(|_| panic!("unsubscribed listener")),
        )
        .unwrap();
    cleanup().unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    h.advance(200.0);
    assert_eq!(sent_topics(&h, "subscribe", "orderbook.1000.BTCUSDT"), 0);
    h.venue().dispose().unwrap();
}

#[test]
fn public_reopen_restores_active_book_topics_and_respects_cleanup() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let depth = if name == ExchangeName::Bybitspot {
            200
        } else {
            1000
        };
        let (h, socket, symbol) = setup(name);
        let cleanup = h
            .venue()
            .listen_order_book(symbol.into(), Rc::new(|_| {}))
            .unwrap();
        let topic = format!("orderbook.{depth}.{symbol}");
        h.drive(Box::pin(socket.on_open())).unwrap();
        h.runtime.pump();
        assert_eq!(sent_topics(&h, "subscribe", &topic), 2);
        cleanup().unwrap();
        h.drive(Box::pin(socket.on_open())).unwrap();
        h.runtime.pump();
        assert_eq!(sent_topics(&h, "subscribe", &topic), 2);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn book_frames_read_source_array_like_levels() {
    let (h, socket, symbol) = setup(ExchangeName::Bybit);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            symbol.into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("orderbook.1000.{symbol}"),"type":"snapshot","data":{"u":1,"b":{"0":["100","2"],"length":"1"},"a":false}}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].bids[0].price, 100.0);
    assert_eq!(seen.borrow()[0].bids[0].amount, 2.0);
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}

#[test]
fn malformed_book_levels_preserve_source_type_error() {
    let (h, socket, symbol) = setup(ExchangeName::Bybit);
    let cleanup = h
        .venue()
        .listen_order_book(symbol.into(), Rc::new(|_| panic!("malformed book")))
        .unwrap();
    let error = socket
        .on_message(&SocketFrame::Text(
            json!({"topic":format!("orderbook.1000.{symbol}"),"type":"snapshot","data":{"u":1,"a":[null]}}).to_string(),
        ))
        .unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(
        error.message,
        "Cannot read properties of null (reading '0')"
    );
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}
