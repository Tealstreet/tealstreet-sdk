#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::BaseWebSocket};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderBook};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

fn setup(name: ExchangeName) -> (support::Harness, Rc<BaseWebSocket>, &'static str) {
    let inverse = name == ExchangeName::Bybitinverse;
    let symbol = if inverse { "BTCUSD" } else { "BTCUSDT" };
    let h = support::harness_as(name, support::options(), |_| {});
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
    let wire = &h.runtime.sockets.borrow()[0].1;
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
