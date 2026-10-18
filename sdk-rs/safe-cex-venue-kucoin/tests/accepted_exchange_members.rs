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
        let update = |sequence: i64, quantity: f64| {
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"type":"message","topic":format!("/contractMarket/level2:{api}"),"data":{"sequence":sequence,"change":format!("{price},buy,{quantity}")}}).to_string(),
            )
        };
        socket.on_message(&update(2, amount + 2.0)).unwrap();
        assert_eq!(
            books.borrow().len(),
            2,
            "valid native update emits before cleanup"
        );
        assert_eq!(
            books.borrow()[1].bids[0].amount.to_bits(),
            (amount + 2.0).to_bits()
        );
        socket.on_message(&update(2, amount + 200.0)).unwrap();
        assert_eq!(books.borrow().len(), 2, "duplicate sequence is suppressed");
        h.wire.script(
            "GET",
            "/api/v1/level2/snapshot",
            json!({"code":"200000","data":{
                "bids":[[price,amount+3.0]],"asks":[[price+1.0,amount+1.0]],"sequence":3
            }}),
        );
        socket.on_message(&update(4, amount + 4.0)).unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert_eq!(
            books.borrow().len(),
            3,
            "sequence gap refetches then replays"
        );
        assert_eq!(
            books.borrow()[2].bids[0].amount.to_bits(),
            (amount + 4.0).to_bits()
        );
        cleanup().unwrap();
        socket.on_message(&update(5, amount + 5.0)).unwrap();
        assert_eq!(
            books.borrow().len(),
            3,
            "futures cleanup suppresses valid emitted update"
        );
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
        socket
            .on_message(&safe_cex_transport::socket::SocketFrame::Text(
                json!({
                    "type":"message","topic":format!("/spotMarket/level2Depth50:{symbol}"),
                    "data":{"bids":[[price+4.0,amount+7.0]],"asks":[],"checksum":-1}
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(books.borrow().len(), 2);
        assert_eq!(
            books.borrow()[1].bids.len(),
            1,
            "Spot replaces the snapshot"
        );
        assert_eq!(
            books.borrow()[1].bids[0].price.to_bits(),
            (price + 4.0).to_bits()
        );
        assert!(books.borrow()[1].asks.is_empty());
        cleanup().unwrap();
        socket.on_message(&frame).unwrap();
        assert_eq!(books.borrow().len(), 2);
        venue.dispose().unwrap();
    }
}

#[test]
fn public_candle_callbacks_keep_distinct_fields_for_both_families() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native_iteration_source.mjs"
        ))
        .arg("--candles")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 2);
    for spot in [false, true] {
        let (name, symbol, api, endpoint, topic) = if spot {
            (
                ExchangeName::Kucoinspot,
                "BTC-USDT",
                "BTC-USDT",
                "wss://spot.invalid",
                "/market/candles:BTC-USDT_1min",
            )
        } else {
            (
                ExchangeName::Kucoin,
                "BTCUSDT",
                "XBTUSDTM",
                "wss://futures.invalid",
                "/contractMarket/limitCandle:XBTUSDTM_1min",
            )
        };
        let (h, parts, deps, _) = fx::io::connected_parts(name);
        let venue: Rc<dyn Exchange> = if spot {
            KucoinSpotExchange::new(parts, deps).unwrap()
        } else {
            KucoinExchange::new(parts, deps).unwrap()
        };
        fx::seed_market(h.store.as_ref(), fx::market(api, symbol, Some(0.5)));
        h.wire.script("POST", "/api/v1/bullet-public", json!({"code":"200000","data":{
            "token":"native-token","instanceServers":[{"endpoint":endpoint,"pingInterval":18000}]
        }}));
        let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert!(socket.is_connected());
        let candles = Rc::new(RefCell::new(Vec::new()));
        let sink = candles.clone();
        let cleanup = venue
            .listen_ohlcv(
                serde_json::from_value(json!({"symbol":symbol,"interval":"1m"})).unwrap(),
                Rc::new(move |candle| sink.borrow_mut().push(candle)),
            )
            .unwrap();
        h.runtime.pump();
        let frame = safe_cex_transport::socket::SocketFrame::Text(json!({
            "type":"message","topic":topic,"data":{"candles":["1700000000","11.25","17.5","23.75","5.5","31.125"]}
        }).to_string());
        socket.on_message(&frame).unwrap();
        let seen = candles.borrow();
        assert_eq!(
            seen.len(),
            1,
            "spot={spot}, reports={:?}",
            h.runtime.errors.borrow()
        );
        let candle = &seen[0];
        let expected_source = &source
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["spot"] == spot)
            .unwrap()["fields"];
        assert_eq!(expected_source.as_object().unwrap().len(), 6);
        for (field, actual) in [
            ("timestamp", candle.timestamp),
            ("open", candle.open),
            ("close", candle.close),
            ("high", candle.high),
            ("low", candle.low),
            ("volume", candle.volume),
        ] {
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                expected_source[field].as_str().unwrap(),
                "source {field}, spot={spot}"
            );
        }
        for (actual, expected) in [
            (candle.timestamp, 1_700_000_000.0_f64),
            (candle.open, 11.25),
            (candle.close, 17.5),
            (candle.high, 23.75),
            (candle.low, 5.5),
            (candle.volume, 31.125),
        ] {
            assert_eq!(actual.to_bits(), expected.to_bits(), "spot={spot}");
        }
        drop(seen);
        cleanup().unwrap();
        socket.on_message(&frame).unwrap();
        assert_eq!(
            candles.borrow().len(),
            1,
            "cleanup removes the actual candle callback"
        );
        assert!(h.runtime.errors.borrow().is_empty());
        venue.dispose().unwrap();
    }
}
