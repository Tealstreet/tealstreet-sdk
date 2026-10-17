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

fn single_candle(name: ExchangeName) {
    let spot = name == ExchangeName::Bybitspot;
    let (h, socket, symbol) = setup(name);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":symbol,"interval":"1m"})).unwrap(),
            Rc::new(move |candle| sink.borrow_mut().push(candle)),
        )
        .unwrap();
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"100","high":"105","low":"99","close":"103","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(
        seen.borrow().len(),
        1,
        "single candle listener must reach real public frame"
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("publicTrade.{symbol}"),"data":[{"T":1700000045000u64,"p":"104","v":"208","S":"Sell"}]}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[1].close, 104.0);
    assert_eq!(seen.borrow()[1].volume, 9.0);
    assert_eq!(
        seen.borrow()[1].last_side,
        if spot {
            None
        } else {
            Some(safe_cex_types::OrderSide::Sell)
        }
    );
    cleanup().unwrap();
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"100","high":"105","low":"99","close":"103","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 2);
    h.venue().dispose().unwrap();
}
#[test]
fn primary_single_candle_reaches_real_socket() {
    single_candle(ExchangeName::Bybit);
}
#[test]
fn demo_single_candle_reaches_real_socket() {
    single_candle(ExchangeName::Bybitdemo);
}
#[test]
fn spot_single_candle_reaches_real_socket() {
    single_candle(ExchangeName::Bybitspot);
}
#[test]
fn inverse_single_candle_reaches_real_socket() {
    single_candle(ExchangeName::Bybitinverse);
}
#[test]
fn primary_single_candle_cleanup_cancels_pending_subscription_retry() {
    let h = support::harness(support::options(), |_| {});
    let socket = h.venue().base().get_socket(SocketSlot::Public).unwrap();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(|_| panic!("cleaned candle callback")),
        )
        .unwrap();
    cleanup().unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    h.runtime.now.set(h.runtime.now.get() + 100.);
    h.runtime.pump();
    assert_eq!(sent_topics(&h, "subscribe", "kline.1.BTCUSDT"), 0);
    assert_eq!(sent_topics(&h, "subscribe", "publicTrade.BTCUSDT"), 0);
    h.venue().dispose().unwrap();
}

#[test]
fn dedicated_single_candle_cleanup_retains_source_pending_retry() {
    for (name, slot, symbol) in [
        (ExchangeName::Bybitspot, SocketSlot::Public, "BTCUSDT"),
        (
            ExchangeName::Bybitinverse,
            SocketSlot::PublicSecondary,
            "BTCUSD",
        ),
    ] {
        let h = support::harness_as(name, support::options(), |_| {});
        let socket = h.venue().base().get_socket(slot).unwrap();
        let cleanup = h
            .venue()
            .listen_ohlcv(
                serde_json::from_value(json!({"symbol":symbol,"interval":"1m"})).unwrap(),
                Rc::new(|_| {}),
            )
            .unwrap();
        cleanup().unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        assert_eq!(
            sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
            0
        );
        h.runtime.now.set(h.runtime.now.get() + 100.);
        h.runtime.pump();
        assert_eq!(
            sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
            1
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn primary_single_candle_silence_recovers_and_cleanup_stops_watchdog() {
    let (h, _socket, symbol) = setup(ExchangeName::Bybit);
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":symbol,"interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        )
        .unwrap();
    let topic = format!("kline.1.{symbol}");
    assert_eq!(sent_topics(&h, "subscribe", &topic), 1);
    h.runtime.now.set(h.runtime.now.get() + 24_999.);
    h.runtime.pump();
    assert_eq!(sent_topics(&h, "subscribe", &topic), 1);
    h.runtime.now.set(h.runtime.now.get() + 1.);
    h.runtime.pump();
    assert_eq!(sent_topics(&h, "subscribe", &topic), 2);
    assert_eq!(sent_topics(&h, "unsubscribe", &topic), 1);
    cleanup().unwrap();
    let subscriptions = sent_topics(&h, "subscribe", &topic);
    h.runtime.now.set(h.runtime.now.get() + 25_000.);
    h.runtime.pump();
    assert_eq!(sent_topics(&h, "subscribe", &topic), subscriptions);
    h.venue().dispose().unwrap();
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn single_candle_sequences_match_maintained_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/single-candle-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 12);
    for (index, case) in cases.iter().enumerate() {
        let name = match case["variant"].as_str().unwrap() {
            "bybitspot" => ExchangeName::Bybitspot,
            "bybitinverse" => ExchangeName::Bybitinverse,
            _ => ExchangeName::Bybit,
        };
        let (h, socket, symbol) = setup(name);
        let seen = Rc::new(RefCell::new(vec![]));
        let sink = seen.clone();
        let cleanup = h
            .venue()
            .listen_ohlcv(
                serde_json::from_value(json!({"symbol":symbol,"interval":case["interval"]}))
                    .unwrap(),
                Rc::new(move |candle| sink.borrow_mut().push(candle)),
            )
            .unwrap();
        for (step, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let before = seen.borrow().len();
            let mut frame = row["frame"].clone();
            let wire = if case["interval"] == "1w" { "W" } else { "1" };
            frame["topic"] = json!(if row["kind"] == "kline" {
                format!("kline.{wire}.{symbol}")
            } else {
                format!("publicTrade.{symbol}")
            });
            socket
                .on_message(&SocketFrame::Text(frame.to_string()))
                .unwrap();
            let expected = &case["outcomes"][step];
            if expected.is_null() {
                assert_eq!(
                    seen.borrow().len(),
                    before,
                    "source sequence {index}/{step}"
                );
                continue;
            }
            assert_eq!(
                seen.borrow().len(),
                before + 1,
                "source sequence {index}/{step}"
            );
            let seen = seen.borrow();
            let candle = seen.last().unwrap();
            let mut actual = serde_json::to_value(candle).unwrap();
            let mut expected = expected.clone();
            for (field, value) in [
                ("timestamp", candle.timestamp),
                ("open", candle.open),
                ("high", candle.high),
                ("low", candle.low),
                ("close", candle.close),
                ("volume", candle.volume),
            ] {
                actual[field] = if value.is_nan() {
                    json!("NaN")
                } else if value == f64::INFINITY {
                    json!("Infinity")
                } else if value == f64::NEG_INFINITY {
                    json!("-Infinity")
                } else if value == 0. && value.is_sign_negative() {
                    json!("-0")
                } else {
                    json!(value)
                };
                if let Some(number) = expected[field].as_f64() {
                    expected[field] = json!(number);
                }
            }
            assert_eq!(actual, expected, "source sequence {index}/{step}");
        }
        cleanup().unwrap();
        h.venue().dispose().unwrap();
    }
}

fn bulk_candles(name: ExchangeName) {
    let (h, socket, symbol) = setup(name);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
        Rc::new(move |candle| sink.borrow_mut().push(candle)),
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"100","high":"105","low":"99","close":"103","volume":"7"}]}).to_string())).unwrap();
    let seen = seen.borrow();
    assert_eq!(
        seen.len(),
        1,
        "bulk candle listener must reach real public frame"
    );
    assert_eq!(seen[0].candle.timestamp, 1700000040.0);
    assert_eq!(seen[0].candle.close, 103.0);
    assert_eq!(seen[0].candle.volume, 7.0);
    assert_eq!(seen[0].symbol, symbol);
    assert_eq!(serde_json::to_value(&seen[0].interval).unwrap(), "1m");
    assert_eq!(
        sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
        1
    );
    h.venue().dispose().unwrap();
}
#[test]
fn primary_bulk_candles_reach_real_socket() {
    bulk_candles(ExchangeName::Bybit);
}
#[test]
fn demo_bulk_candles_reach_real_socket() {
    bulk_candles(ExchangeName::Bybitdemo);
}
#[test]
fn spot_bulk_candles_preserve_inherited_noop() {
    let (h, socket, symbol) = setup(ExchangeName::Bybitspot);
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
        Rc::new(|_| panic!("Spot inherits BaseExchange.listenOHLCVs")),
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"1","high":"2","low":"0","close":"1","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(
        sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
        0
    );
    h.venue().dispose().unwrap();
}
#[test]
fn inverse_bulk_candles_reach_real_socket() {
    bulk_candles(ExchangeName::Bybitinverse);
}

#[test]
fn bulk_candle_subscription_preserves_source_family_chunk_sizes() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let (h, _, _) = setup(name);
        let symbols: Vec<_> = (0..51).map(|i| format!("ASSET{i}USDT")).collect();
        h.venue().listen_ohlcvs(
            serde_json::from_value(json!({"symbols":symbols,"interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        );
        let batches: Vec<serde_json::Value> = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .flat_map(|(_, socket)| socket.sent.borrow().clone())
            .map(|raw| serde_json::from_str::<serde_json::Value>(&raw).unwrap())
            .filter(|v| {
                v["op"] == "subscribe"
                    && v["args"].as_array().is_some_and(|a| {
                        a.first()
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s.starts_with("kline."))
                    })
            })
            .collect();
        let lengths: Vec<_> = batches
            .iter()
            .map(|v| v["args"].as_array().unwrap().len())
            .collect();
        assert_eq!(lengths, vec![50, 1]);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn replacing_bulk_candle_listener_keeps_topics_and_replaces_prefix_handler() {
    let (h, socket, symbol) = setup(ExchangeName::Bybit);
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
        Rc::new(|_| panic!("replaced bulk listener")),
    );
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":["ETHUSDT"],"interval":"15m"})).unwrap(),
        Rc::new(move |candle| sink.borrow_mut().push(candle)),
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"1","high":"2","low":"0","close":"1","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].symbol, symbol);
    assert_eq!(
        serde_json::to_value(&seen.borrow()[0].interval).unwrap(),
        "1m"
    );
    h.drive(Box::pin(socket.on_open())).unwrap();
    h.runtime.pump();
    assert_eq!(
        sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
        2
    );
    assert_eq!(sent_topics(&h, "subscribe", "kline.15.ETHUSDT"), 2);
    h.venue().dispose().unwrap();
}

#[test]
fn bulk_candle_listener_waits_for_the_source_connection_retry() {
    let h = support::harness(support::options(), |_| {});
    let socket = h.venue().base().get_socket(SocketSlot::Public).unwrap();
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":["BTCUSDT"],"interval":"1m"})).unwrap(),
        Rc::new(|_| {}),
    );
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    let subscriptions = sent_topics(&h, "subscribe", "kline.1.BTCUSDT");
    h.runtime.now.set(h.runtime.now.get() + 99.0);
    h.runtime.pump();
    assert_eq!(
        sent_topics(&h, "subscribe", "kline.1.BTCUSDT"),
        subscriptions
    );
    h.runtime.now.set(h.runtime.now.get() + 1.0);
    h.runtime.pump();
    assert_eq!(
        sent_topics(&h, "subscribe", "kline.1.BTCUSDT"),
        subscriptions + 1
    );
    h.venue().dispose().unwrap();
}

fn bulk_candle_interval(wire: &str, expected: Option<&str>) {
    let (h, socket, symbol) = setup(ExchangeName::Bybit);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
        Rc::new(move |candle| sink.borrow_mut().push(candle)),
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.{wire}.{symbol}"),"data":[{"start":1700000040000u64,"open":"1","high":"2","low":"0","close":"1","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 1);
    let native = safe_cex_exchange::js_runtime::value::from_typed(&seen.borrow()[0]).unwrap();
    let interval_key: Vec<u16> = "interval".encode_utf16().collect();
    assert!(
        native
            .own_keys()
            .iter()
            .any(|key| key.as_ref() == interval_key.as_slice())
    );
    let serialized = serde_json::to_value(&seen.borrow()[0]).unwrap();
    if let Some(expected) = expected {
        assert_eq!(serialized["interval"], expected);
    } else {
        assert!(serialized.get("interval").is_none());
        assert!(native.get("interval").unwrap().is_undefined());
    }
    h.venue().dispose().unwrap();
}
#[test]
fn bulk_candle_month_interval_reaches_native_callback() {
    bulk_candle_interval("M", Some("1M"));
}
#[test]
fn bulk_candle_unknown_interval_reaches_callback_as_own_undefined() {
    bulk_candle_interval("999", None);
}

#[test]
fn primary_bulk_candles_route_inverse_symbols_to_secondary_socket() {
    let (h, socket, symbol) = setup_for(ExchangeName::Bybit, true);
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    h.venue().listen_ohlcvs(
        serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
        Rc::new(move |candle| sink.borrow_mut().push(candle)),
    );
    socket.on_message(&SocketFrame::Text(json!({"topic":format!("kline.1.{symbol}"),"data":[{"start":1700000040000u64,"open":"100","high":"105","low":"99","close":"103","volume":"7"}]}).to_string())).unwrap();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].symbol, symbol);
    assert_eq!(
        sent_topics(&h, "subscribe", &format!("kline.1.{symbol}")),
        1
    );
    assert!(
        h.venue()
            .base()
            .get_socket(SocketSlot::Public)
            .unwrap()
            .message_handlers
            .borrow()
            .get("kline")
            .is_none()
    );
    h.venue().dispose().unwrap();
}

#[test]
fn bulk_candle_malformed_data_preserves_source_type_errors() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let (h, socket, symbol) = setup(name);
        h.venue().listen_ohlcvs(
            serde_json::from_value(json!({"symbols":[symbol],"interval":"1m"})).unwrap(),
            Rc::new(|_| panic!("malformed candle must not emit")),
        );
        for (data, message) in [
            (
                json!({}),
                "object is not iterable (cannot read property Symbol(Symbol.iterator))",
            ),
            (
                json!(false),
                "boolean false is not iterable (cannot read property Symbol(Symbol.iterator))",
            ),
            (
                json!(42),
                "number 42 is not iterable (cannot read property Symbol(Symbol.iterator))",
            ),
            (
                json!(null),
                "Cannot destructure property 'Symbol(Symbol.iterator)' of 'object null' as it is null.",
            ),
        ] {
            let error = socket
                .on_message(&SocketFrame::Text(
                    json!({"topic":format!("kline.1.{symbol}"),"data":data}).to_string(),
                ))
                .unwrap_err();
            assert_eq!(error.name, "TypeError");
            assert_eq!(error.message, message);
        }
        h.venue().dispose().unwrap();
    }
}

#[test]
fn disconnected_candle_watchdog_uses_existing_transport_reconnect() {
    let (h, socket, symbol) = setup(ExchangeName::Bybit);
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":symbol,"interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        )
        .unwrap();
    let wire = h.runtime.sockets.borrow()[0].1.clone();
    safe_cex_exchange::websocket::WsSocket::close(wire.as_ref());
    assert!(!socket.is_connected());
    let dials = h.runtime.sockets.borrow().len();
    h.runtime.now.set(h.runtime.now.get() + 25_000.);
    h.runtime.pump();
    assert_eq!(
        h.runtime.sockets.borrow().len(),
        dials,
        "source reconnect operates on the existing closed native transport"
    );
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}
