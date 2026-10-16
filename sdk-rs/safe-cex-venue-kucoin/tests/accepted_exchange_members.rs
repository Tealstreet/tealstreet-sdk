#[allow(dead_code)]
mod fx {
    use safe_cex_venue_kucoin as venue_crate;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/kucoin-io-fixture.rs"
    ));
}
use safe_cex_exchange::{exchange::Exchange, socket_lifecycle::SocketSlot};
use safe_cex_types::{ExchangeName, OrderBook};
use safe_cex_venue_kucoin::kucoin_exchange::{KucoinExchange, KucoinSpotExchange};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[test]
fn futures_native_book_callback_and_cleanup() {
    for (symbol, api, price, amount) in [
        ("BTCUSDT", "XBTUSDTM", 17.25_f64, 3.5_f64),
        ("ETHUSDT", "ETHUSDTM", 29.5_f64, 7.25_f64),
    ] {
        let (h, parts, deps, transport) = fx::io::connected_parts(ExchangeName::Kucoin);
        let venue = KucoinExchange::new(parts, deps).unwrap();
        fx::seed_market(h.store.as_ref(), fx::market(api, symbol, Some(0.5)));
        h.wire.script("POST","/api/v1/bullet-public",json!({"code":"200000","data":{"token":"native-token","instanceServers":[{"endpoint":"wss://native.invalid","pingInterval":18000}]}}));
        h.wire.script("GET","/api/v1/level2/snapshot",json!({"code":"200000","data":{"bids":[[price,amount]],"asks":[[price+1.0,amount+1.0]],"sequence":1}}));
        let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert!(
            socket.is_connected(),
            "actual scripted native socket connected: {:?}",
            h.runtime.errors.borrow()
        );
        let books = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
        let received = books.clone();
        let cleanup = <KucoinExchange as Exchange>::listen_order_book(
            venue.as_ref(),
            symbol.into(),
            Rc::new(move |book| received.borrow_mut().push(book)),
        )
        .unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert_eq!(
            books.borrow().len(),
            1,
            "actual native callback: {:?}",
            h.runtime.errors.borrow()
        );
        let book = books.borrow()[0].clone();
        assert_eq!(book.bids[0].price.to_bits(), price.to_bits());
        assert_eq!(book.bids[0].amount.to_bits(), amount.to_bits());
        cleanup().unwrap();
        socket.on_message(&safe_cex_transport::socket::SocketFrame::Text(json!({"type":"message","topic":format!("/contractMarket/level2:{api}"),"data":{"sequence":2}}).to_string())).unwrap();
        assert_eq!(books.borrow().len(), 1, "cleanup removes actual callback");
        assert_eq!(transport.sockets.borrow().len(), 1);
        venue.dispose().unwrap();
    }
}

#[test]
fn spot_native_book_callback_and_cleanup() {
    for (symbol, price, amount) in [
        ("BTC-USDT", 17.25_f64, 3.5_f64),
        ("ETH-USDT", 29.5_f64, 7.25_f64),
    ] {
        let (h, parts, deps, _transport) = fx::io::connected_parts(ExchangeName::Kucoinspot);
        let venue = KucoinSpotExchange::new(parts, deps).unwrap();
        fx::seed_market(h.store.as_ref(), fx::market(symbol, symbol, Some(1.0)));
        h.wire.script("POST","/api/v1/bullet-public",json!({"code":"200000","data":{"token":"native-token","instanceServers":[{"endpoint":"wss://native.invalid","pingInterval":18000}]}}));
        let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert!(socket.is_connected());
        let books = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
        let received = books.clone();
        let cleanup = <KucoinSpotExchange as Exchange>::listen_order_book(
            venue.as_ref(),
            symbol.into(),
            Rc::new(move |book| received.borrow_mut().push(book)),
        )
        .unwrap();
        let frame=safe_cex_transport::socket::SocketFrame::Text(json!({"type":"message","topic":format!("/spotMarket/level2Depth50:{symbol}"),"data":{"bids":[[price,amount]],"asks":[[price+1.0,amount+1.0]]}}).to_string());
        socket.on_message(&frame).unwrap();
        assert_eq!(
            books.borrow().len(),
            1,
            "native callback: {:?}",
            h.runtime.errors.borrow()
        );
        let book = books.borrow()[0].clone();
        assert_eq!(book.bids[0].price.to_bits(), price.to_bits());
        assert_eq!(book.bids[0].amount.to_bits(), amount.to_bits());
        cleanup().unwrap();
        socket.on_message(&frame).unwrap();
        assert_eq!(books.borrow().len(), 1);
        venue.dispose().unwrap();
    }
}
