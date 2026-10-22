//! Examples derived from Binance Spot and COIN-M market-data documentation.
use support::Options;
mod support;
use safe_cex_engine::{Command, Event};
use safe_cex_venue_binance::Variant;
use support::Harness;

const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.00001","minQty":"0.00001","maxQty":"9000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0","minQty":"0","maxQty":"10"}]}]}"#;
const SPOT_DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"2","volume":"10","quoteVolume":"1000"}]"#;
const INVERSE_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"120"}]},{"symbol":"BTCUSD_260925","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"CURRENT_QUARTER","contractSize":100},{"symbol":"ETHUSD_PERP","contractStatus":"PENDING_TRADING","baseAsset":"ETH","quoteAsset":"USD","marginAsset":"ETH","contractType":"PERPETUAL","contractSize":10}]}"#;

fn spot() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        public_data_only: true,
        ..Options::default()
    });
    assert!(h.paths()[0].contains("https://api.binance.com/api/v3/exchangeInfo"));
    h.respond("/api/v3/exchangeInfo", 200, SPOT_INFO);
    assert_eq!(h.requests.len(), 1);
    h.respond("/api/v3/ticker/24hr", 200, SPOT_DAILY);
    assert_eq!(h.sockets.len(), 2);
    for socket in h
        .sockets
        .iter()
        .map(|(socket, _)| *socket)
        .collect::<Vec<_>>()
    {
        h.event(Event::WsOpened { socket });
    }
    h
}

fn inverse() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        public_data_only: true,
        ..Options::default()
    });
    h.respond("/dapi/v1/exchangeInfo", 200, INVERSE_INFO);
    h.respond("/dapi/v1/ticker/24hr", 200, r#"[{"symbol":"BTCUSD_PERP","lastPrice":"10000","priceChangePercent":"2","volume":"100","baseVolume":"1"}]"#);
    h.respond(
        "/dapi/v1/ticker/bookTicker",
        200,
        r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
    );
    h.respond("/dapi/v1/premiumIndex", 200, r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0.0001","nextFundingTime":1000}]"#);
    assert_eq!(h.sockets.len(), 1);
    let socket = h.socket("dstream.binance.com/ws").unwrap();
    h.event(Event::WsOpened { socket });
    h
}

#[test]
fn spot_uses_filter_increments_and_daily_bid_ask_without_futures_requests() {
    let h = spot();
    let m = &h.engine.store().markets()[0];
    assert_eq!(m.id, "BTC/USDT");
    assert_eq!(m.is_spot, Some(true));
    assert_eq!(m.precision.amount, 0.00001);
    assert_eq!(m.precision.price, 0.01);
    assert_eq!(m.limits.amount.max, 9000.0);
    let t = &h.engine.store().tickers()[0];
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark, t.index),
        (99.0, 101.0, 100.0, 100.0, 100.0)
    );
    assert_eq!((t.funding_rate, t.funding_time), (0.0, 0.0));
    assert!(h.requests.is_empty());
    assert!(h.sent[0].1.contains("btcusdt@ticker"));
    assert!(!h.sent[0].1.contains("!ticker@arr"));
    assert!(!h.sent[0].1.contains("!bookTicker"));
    assert!(!h.sent[0].1.contains("markPrice"));
}

#[test]
fn spot_individual_ticker_updates_bid_ask_mark_and_index() {
    let mut h = spot();
    h.message(
        "stream.binance.com",
        r#"{"e":"24hrTicker","s":"BTCUSDT","c":"110","P":"3","v":"11","q":"1210","b":"109","a":"111"}"#,
    );
    let t = &h.engine.store().tickers()[0];
    assert_eq!(
        (t.last, t.mark, t.index, t.bid, t.ask),
        (110.0, 110.0, 110.0, 109.0, 111.0)
    );
}

#[test]
fn spot_depth_uses_update_ranges_without_previous_final_id() {
    let mut h = spot();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    spot_book_message(
        &mut h,
        r#"{"e":"depthUpdate","s":"BTCUSDT","U":157,"u":160,"b":[["99","2"]],"a":[]}"#,
    );
    h.respond(
        "/api/v3/depth",
        200,
        r#"{"lastUpdateId":159,"bids":[["98","1"]],"asks":[["101","3"]]}"#,
    );
    assert_eq!(h.best_bid(), Some(99.0));
    spot_book_message(
        &mut h,
        r#"{"e":"depthUpdate","s":"BTCUSDT","U":160,"u":162,"b":[["100","4"]],"a":[]}"#,
    );
    assert_eq!(h.best_bid(), Some(100.0));
    spot_book_message(
        &mut h,
        r#"{"e":"depthUpdate","s":"BTCUSDT","U":164,"u":165,"b":[["101","4"]],"a":[]}"#,
    );
    assert_eq!(h.best_bid(), None);
}

#[test]
fn coin_m_uses_contract_status_size_and_base_volume() {
    let mut h = inverse();
    assert_eq!(h.engine.store().markets().len(), 1);
    let m = &h.engine.store().markets()[0];
    assert_eq!(m.id, "BTC/USD:BTC");
    assert_eq!(m.contract_size, Some(100.0));
    assert_eq!(m.is_inverse, Some(true));
    assert_eq!(m.precision.amount, 1.0);
    assert_eq!(m.limits.amount.max, 120.0);
    let t = &h.engine.store().tickers()[0];
    assert_eq!((t.volume, t.quote_volume), (100.0, 1.0));
    h.message(
        "dstream.binance.com",
        r#"[{"e":"24hrTicker","s":"BTCUSD_PERP","c":"11000","P":"3","v":"200","Q":"2"}]"#,
    );
    let t = &h.engine.store().tickers()[0];
    assert_eq!((t.last, t.volume, t.quote_volume), (11000.0, 200.0, 2.0));
}

#[test]
fn coin_m_requires_futures_previous_final_id_and_preserves_native_book_sizes() {
    let mut h = inverse();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSD_PERP".into(),
    }));
    h.message("dstream.binance.com", r#"{"e":"depthUpdate","s":"BTCUSD_PERP","U":157,"u":160,"pu":156,"b":[["9999","2"]],"a":[]}"#);
    h.respond(
        "/dapi/v1/depth",
        200,
        r#"{"lastUpdateId":159,"bids":[["9998","1"]],"asks":[["10001","3"]]}"#,
    );
    assert_eq!(
        h.engine.book("BTCUSD_PERP").unwrap().best_bid(),
        Some(9999.0)
    );
    assert_eq!(
        h.engine.book("BTCUSD_PERP").unwrap().bids()[0],
        (9999.0, 2.0)
    );
    h.message("dstream.binance.com",r#"{"e":"depthUpdate","s":"BTCUSD_PERP","U":161,"u":162,"pu":158,"b":[["10000","4"]],"a":[]}"#);
    assert!(h.engine.book("BTCUSD_PERP").is_none());
}

#[test]
fn variant_public_testnet_urls_and_candles_use_their_family() {
    for (variant, host, endpoint) in [
        (Variant::Spot, "https://testnet.binance.vision", "/api/v3"),
        (Variant::CoinM, "https://demo-dapi.binance.com", "/dapi/v1"),
    ] {
        let mut h = Harness::started(Options {
            variant,
            testnet: true,
            public_data_only: true,
            ..Options::default()
        });
        assert!(
            h.requests[0]
                .url
                .starts_with(&format!("{host}{endpoint}/exchangeInfo"))
        );
        h.event(Event::Command(Command::FetchCandles {
            command: 99,
            symbol: "BTCUSD_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(100),
            from_ms: Some(1000.0),
            to_ms: Some(2000.0),
        }));
        let request = h.take("/klines");
        assert_eq!(
            request.url,
            format!(
                "{host}{endpoint}/klines?symbol=BTCUSD_PERP&interval=1m&startTime=-5998000&endTime=2000&limit=100"
            )
        );
        h.reply(
            request.id,
            200,
            "",
            r#"[[1000,"1","3","1","2","8",2000,"16",1,"0","0","0"]]"#,
        );
        assert!(
            h.result(99)
                .unwrap()
                .as_ref()
                .unwrap()
                .contains("\"volume\":8.0")
        );
    }
}

fn spot_book_message(h: &mut Harness, text: &str) {
    let socket = h.sockets[0].0;
    h.event(Event::WsMessage { socket, text });
}

#[test]
fn spot_shards_every_symbol_and_reconnects_only_the_closed_shard() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        public_data_only: true,
        ..Options::default()
    });
    let symbols = (0..2050).map(|i| format!(r#"{{"symbol":"COIN{i}USDT","status":"TRADING","baseAsset":"COIN{i}","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8}}"#)).collect::<Vec<_>>().join(",");
    h.respond(
        "/api/v3/exchangeInfo",
        200,
        &format!(r#"{{"symbols":[{symbols}]}}"#),
    );
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"COIN0USDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#);
    assert_eq!(h.sockets.len(), 4); // books and three independent ticker shards
    for socket in h
        .sockets
        .iter()
        .map(|(socket, _)| *socket)
        .collect::<Vec<_>>()
    {
        h.event(Event::WsOpened { socket });
    }
    assert_eq!(h.sent.len(), 3);
    assert!(
        h.sent
            .iter()
            .all(|(_, text)| text.matches("@ticker").count() <= 128 && text.len() < 4096)
    );
    h.advance(399.0);
    assert_eq!(
        h.sent.len(),
        3,
        "one frame per connection before the pacing interval"
    );
    h.advance(1.0);
    assert_eq!(h.sent.len(), 5);
    h.advance(2400.0);
    let ticker_topics = |socket| {
        h.sent
            .iter()
            .filter(|(id, text)| *id == socket && text.contains("\"method\":\"SUBSCRIBE\""))
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("")
    };
    let first = ticker_topics(h.sockets[1].0);
    let second = ticker_topics(h.sockets[2].0);
    let third = ticker_topics(h.sockets[3].0);
    assert_eq!(first.matches("@ticker").count(), 1024);
    assert_eq!(second.matches("@ticker").count(), 1024);
    assert_eq!(third.matches("@ticker").count(), 2);
    assert!(first.contains("coin0usdt@ticker"));
    assert!(second.contains("coin1024usdt@ticker"));
    assert!(third.contains("coin2049usdt@ticker"));
    let socket = h.sockets[2].0;
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(500.0);
    assert_eq!(h.sockets.len(), 5);
    let replacement = h.sockets[4].0;
    h.event(Event::WsOpened {
        socket: replacement,
    });
    let subscription = &h.sent.last().unwrap().1;
    assert!(subscription.contains("coin1024usdt@ticker"));
    assert!(!subscription.contains("coin0usdt@ticker"));
}

#[test]
fn spot_subscription_failure_closes_only_its_correlated_shard() {
    let mut h = spot();
    let ticker = h.sockets[1].0;
    let books = h.sockets[0].0;
    h.event(Event::WsMessage {
        socket: ticker,
        text: r#"{"id":9999,"code":2,"msg":"unrelated"}"#,
    });
    assert!(h.closed.is_empty());
    h.event(Event::WsMessage {
        socket: ticker,
        text: r#"{"id":11,"code":2,"msg":"invalid subscription"}"#,
    });
    assert!(h.closed.contains(&ticker));
    assert!(!h.closed.contains(&books));
    h.advance(500.0);
    assert_eq!(h.sockets.len(), 3);
    let replacement = h.sockets[2].0;
    h.event(Event::WsOpened {
        socket: replacement,
    });
    assert!(h.sent.last().unwrap().1.contains("btcusdt@ticker"));
    h.event(Event::WsMessage {
        socket: ticker,
        text: r#"{"e":"24hrTicker","s":"BTCUSDT","c":"1","P":"1","v":"1","q":"1","b":"1","a":"1"}"#,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 100.0);
}

#[test]
fn spot_book_and_candle_stream_limit_rejects_before_subscribing() {
    let mut h = spot();
    for i in 0..1023 {
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: format!("COIN{i}USDT"),
        }));
    }
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    let sent = h.sent.len();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "overflow".into(),
    }));
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M5,
    }));
    assert_eq!(h.sent.len(), sent);
    assert_eq!(
        h.logs
            .iter()
            .filter(|message| message.contains("at most 1024"))
            .count(),
        2
    );
    h.event(Event::Command(Command::UnsubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "now-fits".into(),
    }));
    let books = h.sockets[0].0;
    h.event(Event::WsClosed {
        socket: books,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.sockets.last().unwrap().0;
    h.event(Event::WsOpened { socket });
    h.advance(2800.0);
    let restore = h
        .sent
        .iter()
        .filter(|(id, _)| *id == socket)
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(restore.contains("now-fits@depth"));
    assert!(!restore.contains("overflow@depth"));
    assert!(!restore.contains("btcusdt@kline_5m"));
}

#[test]
fn spot_reconnect_discards_queued_subscriptions_and_unsubscribes() {
    let mut h = spot();
    let books = h.sockets[0].0;
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "ETHUSDT".into(),
    }));
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.event(Event::WsClosed {
        socket: books,
        code: 1006,
    });
    h.advance(500.0);
    let replacement = h.sockets.last().unwrap().0;
    h.event(Event::WsOpened {
        socket: replacement,
    });
    h.advance(900.0);
    let messages: Vec<_> = h
        .sent
        .iter()
        .filter(|(socket, _)| *socket == replacement)
        .collect();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].1.contains("ethusdt@depth"));
    assert!(!messages[0].1.contains("btcusdt@depth"));
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "ETHUSDT".into(),
    }));
    assert!(h.sent.last().unwrap().1.contains("UNSUBSCRIBE"));
}

#[test]
fn spot_control_frame_bound_counts_utf8_bytes() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        public_data_only: true,
        ..Options::default()
    });
    let name = "界".repeat(100);
    let symbols=(0..128).map(|i|format!(r#"{{"symbol":"{name}{i}USDT","status":"TRADING","baseAsset":"TEST","quoteAsset":"USDT"}}"#)).collect::<Vec<_>>().join(",");
    h.respond(
        "/api/v3/exchangeInfo",
        200,
        &format!(r#"{{"symbols":[{symbols}]}}"#),
    );
    h.respond("/api/v3/ticker/24hr", 200, &format!(r#"[{{"symbol":"{name}0USDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}}]"#));
    let socket = h.sockets[1].0;
    h.event(Event::WsOpened { socket });
    h.advance(4800.0);
    let sent: Vec<_> = h.sent.iter().filter(|(id, _)| *id == socket).collect();
    assert!(sent.len() > 1);
    assert!(sent.iter().all(|(_, text)| text.len() < 4096));
    assert_eq!(
        sent.iter()
            .map(|(_, text)| text.matches("@ticker").count())
            .sum::<usize>(),
        128
    );
}

#[test]
fn replaced_book_subscription_ignores_its_old_snapshot_after_the_new_reply() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = if variant == Variant::Spot {
            spot()
        } else {
            inverse()
        };
        let symbol = if variant == Variant::Spot {
            "BTCUSDT"
        } else {
            "BTCUSD_PERP"
        };
        let depth = if variant == Variant::Spot {
            "/api/v3/depth"
        } else {
            "/dapi/v1/depth"
        };
        let socket = h.sockets[0].0;
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }));
        let first = format!(
            r#"{{"e":"depthUpdate","s":"{symbol}","U":100,"u":102,"pu":99,"b":[["99","2"]],"a":[]}}"#
        );
        h.event(Event::WsMessage {
            socket,
            text: &first,
        });
        let stale = h.take(depth);
        h.event(Event::Command(Command::UnsubscribeOrderBook {
            symbol: symbol.into(),
        }));
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }));
        let current = format!(
            r#"{{"e":"depthUpdate","s":"{symbol}","U":200,"u":202,"pu":199,"b":[["100","4"]],"a":[]}}"#
        );
        h.event(Event::WsMessage {
            socket,
            text: &current,
        });
        h.respond(
            depth,
            200,
            r#"{"lastUpdateId":201,"bids":[["98","1"]],"asks":[["103","1"]]}"#,
        );
        assert_eq!(h.engine.book(symbol).unwrap().best_bid(), Some(100.0));
        h.reply(
            stale.id,
            200,
            "",
            r#"{"lastUpdateId":101,"bids":[["97","1"]],"asks":[["104","1"]]}"#,
        );
        assert_eq!(
            h.engine.book(symbol).map(|book| book.best_bid()),
            Some(Some(100.0)),
            "{variant:?} old request cannot reset synchronization"
        );
        let next = format!(
            r#"{{"e":"depthUpdate","s":"{symbol}","U":203,"u":204,"pu":202,"b":[["101","5"]],"a":[]}}"#
        );
        h.event(Event::WsMessage {
            socket,
            text: &next,
        });
        assert_eq!(h.engine.book(symbol).unwrap().best_bid(), Some(101.0));
        assert!(!h.has_request(depth));
    }
}
