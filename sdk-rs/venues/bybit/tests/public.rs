//! Public data against Bybit's documented payloads.
mod support;

use safe_cex_engine::{Command, Event};
use support::Harness;

/// "Orderbook": the documented delta, and a snapshot of the same book.
const SNAPSHOT: &str = r#"{"topic":"orderbook.1000.BTCUSDT","type":"snapshot","ts":1687940967465,"data":{"s":"BTCUSDT","b":[["30247.20","1.000"],["30240.00","0.500"]],"a":[["30248.70","0.250"],["30249.60","2.000"]],"u":177400506,"seq":66544703341},"cts":1687940967464}"#;
const DELTA: &str = r#"{"topic":"orderbook.1000.BTCUSDT","type":"delta","ts":1687940967466,"data":{"s":"BTCUSDT","b":[["30247.20","30.028"],["30245.40","0.224"],["30242.10","1.593"],["30240.30","1.305"],["30240.00","0"]],"a":[["30248.70","0"],["30249.30","0.892"],["30249.50","1.778"],["30249.60","0"],["30251.90","2.947"],["30252.20","0.659"],["30252.50","4.591"]],"u":177400507,"seq":66544703342},"cts":1687940967464}"#;

fn subscribed_book() -> Harness {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.flush_subscriptions();
    h
}

#[test]
fn startup_loads_markets_and_tickers_then_subscribes_every_ticker() {
    let h = Harness::new();
    let store = h.engine.store();
    let market = store.market("BTCUSDT").unwrap();
    assert_eq!(market.id, "BTCUSDT");
    assert_eq!(
        (market.base.as_str(), market.quote.as_str()),
        ("BTC", "USDT")
    );
    assert_eq!(
        (market.precision.amount, market.precision.price),
        (0.001, 0.1)
    );
    assert_eq!(
        (market.limits.amount.min, market.limits.amount.max),
        (0.001, 1190.0)
    );
    assert_eq!(market.limits.leverage.max, 100.0);
    assert_eq!(market.is_expiring, Some(false));

    let ticker = store.ticker("BTCUSDT").unwrap();
    assert_eq!(
        (ticker.bid, ticker.ask, ticker.last),
        (66666.6, 66666.7, 66666.6)
    );
    assert!((ticker.percentage - -15.8315).abs() < 1e-9);
    assert_eq!(ticker.funding_rate, -0.005);
    assert_eq!(ticker.funding_time, 1760342400000.0);
    assert_eq!(ticker.volume, 73191.387);
    assert_eq!(ticker.quote_volume, 73191.387 * 66666.6);
    let loaded = store.loaded();
    assert!(loaded.markets && loaded.tickers && loaded.orders && loaded.balance);

    let sent = h.sent_on("/v5/public/linear");
    assert_eq!(
        sent[0],
        r#"{"op":"subscribe","args":["tickers.BTCUSDT","tickers.ETHUSDT"]}"#
    );
}

#[test]
fn ticker_deltas_change_only_the_fields_they_carry() {
    let mut h = Harness::new();
    h.message(
        "/v5/public/linear",
        r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","bid1Price":"66660.00","turnover24h":"5000000000","price24hPcnt":"0.01"},"cs":9532239430,"ts":1760325052730}"#,
    );
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.bid, 66660.0);
    assert_eq!(ticker.ask, 66666.7, "absent fields are unchanged");
    assert_eq!(ticker.quote_volume, 5_000_000_000.0);
    assert_eq!(ticker.percentage, 1.0);
}

#[test]
fn a_snapshot_then_deltas_build_the_book() {
    let mut h = subscribed_book();
    assert!(
        h.sent_on("/v5/public/linear")
            .iter()
            .any(|m| m == r#"{"op":"subscribe","args":["orderbook.1000.BTCUSDT"]}"#)
    );
    h.message("/v5/public/linear", DELTA);
    assert!(
        h.engine.book("BTCUSDT").is_none(),
        "no book before a snapshot"
    );
    h.message("/v5/public/linear", SNAPSHOT);
    h.message("/v5/public/linear", DELTA);
    let book = h.engine.book("BTCUSDT").unwrap();
    assert_eq!(
        book.bids(),
        [
            (30247.2, 30.028),
            (30245.4, 0.224),
            (30242.1, 1.593),
            (30240.3, 1.305)
        ]
    );
    assert_eq!(
        book.asks(),
        [
            (30249.3, 0.892),
            (30249.5, 1.778),
            (30251.9, 2.947),
            (30252.2, 0.659),
            (30252.5, 4.591)
        ]
    );
    // A repeated update id is a duplicate, not a gap.
    h.message("/v5/public/linear", DELTA);
    assert!(h.engine.book("BTCUSDT").is_some());
}

#[test]
fn a_skipped_update_drops_the_book_and_resubscribes_for_a_snapshot() {
    let mut h = subscribed_book();
    h.message("/v5/public/linear", SNAPSHOT);
    h.message(
        "/v5/public/linear",
        &DELTA.replace("177400507", "177400509"),
    );
    assert!(h.engine.book("BTCUSDT").is_none(), "book dropped on a gap");
    let unsubscribe = r#"{"op":"unsubscribe","args":["orderbook.1000.BTCUSDT"]}"#;
    let subscribe = r#"{"op":"subscribe","args":["orderbook.1000.BTCUSDT"]}"#;
    assert_eq!(h.sent_on("/v5/public/linear").last().unwrap(), unsubscribe);
    // Frames still in flight from the old subscription are ignored.
    h.message("/v5/public/linear", SNAPSHOT);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.advance(100.0);
    assert_eq!(h.sent_on("/v5/public/linear").last().unwrap(), subscribe);
    h.message("/v5/public/linear", &SNAPSHOT.replace("177400506", "1"));
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(30247.2));
}

#[test]
fn klines_stream_into_candle_frames() {
    let mut h = Harness::new();
    h.command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: "5m".into(),
    });
    h.flush_subscriptions();
    assert!(
        h.sent_on("/v5/public/linear")
            .iter()
            .any(|m| m.contains(r#""kline.5.BTCUSDT""#))
    );
    h.engine.take_frame(h.now);
    // "Kline", the documented frame.
    h.message(
        "/v5/public/linear",
        r#"{"topic":"kline.5.BTCUSDT","data":[{"start":1672324800000,"end":1672325099999,"interval":"5","open":"16649.5","close":"16677","high":"16677","low":"16608","volume":"2.081","turnover":"34666.4005","confirm":false,"timestamp":1672324988882}],"ts":1672324988882,"type":"snapshot"}"#,
    );
    let frame = h.engine.take_frame(h.now).unwrap().to_vec();
    assert!(safe_cex_engine::wire::sections(&frame).any(|(kind, _)| kind == 6));
}

#[test]
fn fetched_candles_come_back_oldest_first() {
    let mut h = Harness::new();
    h.command(Command::FetchCandles {
        command: 3,
        symbol: "BTCUSD".into(),
        interval: "1h".into(),
        limit: None,
        from_ms: Some(1670601600000.0),
        to_ms: Some(1670608800000.0),
    });
    let request = h.take("/v5/market/kline");
    assert!(request.url.ends_with(
        "?category=linear&symbol=BTCUSD&interval=60&start=1670601600000&end=1670608800000&limit=500"
    ));
    // "Get Kline", the documented response.
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"symbol":"BTCUSD","category":"inverse","list":[["1670608800000","17071","17073","17027","17055.5","268611","15.74462667"],["1670605200000","17071.5","17071.5","17061","17071","4177","0.24469757"],["1670601600000","17086.5","17088","16978","17071.5","6356","0.37288112"]]},"retExtInfo":{},"time":1672025956592}"#,
    );
    let result = h.result(3).unwrap().as_ref().unwrap();
    assert!(
        result.starts_with(r#"[{"timestamp":1670601600.0,"open":17086.5"#),
        "{result}"
    );
}

#[test]
fn markets_follow_the_page_cursor() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    let first = h.take("/v5/market/instruments-info");
    let page =
        support::INSTRUMENTS.replace(r#""nextPageCursor":"""#, r#""nextPageCursor":"first%3D5""#);
    h.reply(first.id, 200, &page);
    assert!(
        h.engine.store().markets().is_empty(),
        "waits for the last page"
    );
    let second = h.take("/v5/market/instruments-info");
    assert!(
        second.url.ends_with("&cursor=first%253D5"),
        "{}",
        second.url
    );
    h.reply(second.id, 200, support::INSTRUMENTS);
    assert_eq!(h.engine.store().markets().len(), 4);
}

#[test]
fn an_error_envelope_in_place_of_the_market_list_is_retried() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond(
        "/v5/market/instruments-info",
        200,
        r#"{"retCode":10006,"retMsg":"Too many visits!","result":{},"retExtInfo":{},"time":1672025956592}"#,
    );
    assert!(!h.engine.store().loaded().markets);
    assert!(h.logs.iter().any(|m| m.contains("Too many visits!")));
    h.advance(1_000.0);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    assert!(h.engine.store().loaded().markets);
}

#[test]
fn a_ticker_retry_does_not_restart_in_flight_market_pagination() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/tickers", 503, "unavailable");
    let page = support::INSTRUMENTS.replace(
        r#""nextPageCursor":"""#,
        r#""nextPageCursor":"second-page""#,
    );
    h.respond("/v5/market/instruments-info", 200, &page);
    h.advance(1_000.0);
    let requests: Vec<_> = h
        .requests
        .iter()
        .filter(|request| request.url.contains("/v5/market/instruments-info"))
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "the existing second page stays in flight"
    );
    assert!(requests[0].url.contains("cursor=second-page"));
    let second = support::INSTRUMENTS
        .replace("BTCUSDT", "SOLUSDT")
        .replace("ETHUSDT", "DOGEUSDT");
    h.respond("cursor=second-page", 200, &second);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    assert_eq!(h.engine.store().markets().len(), 4);
}

#[test]
fn a_ticker_retry_does_not_refetch_loaded_markets() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 503, "unavailable");
    h.advance(1_000.0);
    assert_eq!(h.requests.len(), 1);
    assert!(h.requests[0].url.contains("/v5/market/tickers"));
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    assert!(h.engine.store().loaded().tickers);
}

#[test]
fn ticker_stream_starts_after_a_slow_rest_seed() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.advance(1_000.0);
    assert!(h.engine.store().loaded().markets);
    assert!(!h.engine.store().loaded().tickers);
    assert!(h.socket("/v5/public/linear").is_none());
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    let socket = h.socket("/v5/public/linear").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "/v5/public/linear",
        r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","markPrice":"70000","openInterest":"123"}}"#,
    );
    h.message(
        "/v5/public/linear",
        r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","lastPrice":"70001"}}"#,
    );
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.mark, 70000.0);
    assert_eq!(ticker.open_interest, 123.0);
    assert_eq!(ticker.last, 70001.0);
}

#[test]
fn streamed_lists_arrive_in_chunks() {
    let mut h = Harness::started(Default::default());
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    let request = h.take("/v5/market/instruments-info").id;
    h.event(Event::HttpStart {
        request,
        status: 200,
        headers: "",
    });
    for chunk in support::INSTRUMENTS.as_bytes().chunks(7) {
        h.event(Event::HttpChunk {
            request,
            data: chunk,
        });
    }
    h.event(Event::HttpEnd { request });
    assert_eq!(h.engine.store().markets().len(), 2);
}

#[test]
fn a_quiet_socket_is_pinged_and_a_silent_one_reconnected() {
    let mut h = Harness::new();
    let socket = h.socket("/v5/public/linear").unwrap();
    h.advance(10_000.0);
    assert_eq!(
        h.sent_on("/v5/public/linear").last().unwrap(),
        r#"{"op":"ping"}"#
    );
    // "Connect": the linear pong.
    h.message(
        "/v5/public/linear",
        r#"{"success":true,"ret_msg":"pong","conn_id":"465772b1-7630-4fdc-a492-e003e6f0f260","req_id":"","op":"ping"}"#,
    );
    h.advance(20_000.0);
    assert!(!h.closed.contains(&socket), "the pong kept it alive");
    h.advance(5_000.0);
    assert!(h.closed.contains(&socket));
    h.advance(1_000.0);
    assert_ne!(h.socket("/v5/public/linear"), Some(socket));
}
