//! Contract payloads follow BingX-API/api-ai-skills swap-market/api-reference.md.
use safe_cex_engine::{Action, Command, Engine, Event, Response, SocketId};
use safe_cex_venue_bingx::{Bingx, Options};

fn respond(engine: &mut Engine<Bingx>, now: f64, body: &str) {
    let request = engine
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .expect("request");
    engine.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn seeded() -> Engine<Bingx> {
    let mut engine = Engine::new(Bingx::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.);
    respond(
        &mut engine,
        1.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#,
    );
    respond(
        &mut engine,
        2.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#,
    );
    respond(
        &mut engine,
        3.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#,
    );
    engine
}
#[test]
fn seeds_decimal_precision_and_merges_ticker_sources() {
    let engine = seeded();
    let market = &engine.store().markets()[0];
    assert_eq!(market.symbol, "BTCUSDT");
    assert_eq!(market.precision.amount, 0.0001);
    let t = &engine.store().tickers()[0];
    assert_eq!(
        (t.bid, t.ask, t.mark, t.funding_rate),
        (59999.9, 60000.1, 60001., 0.0001)
    );
}
#[test]
fn full_depth_replaces_previous_snapshot_and_watchdog_reconnects() {
    let mut e = seeded();
    e.handle(
        4.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    // The first socket is allocated after the markets HTTP request.
    e.handle(
        5.,
        Event::WsOpened {
            socket: SocketId(2),
        },
    );
    e.drain_actions().for_each(drop);
    let frame = r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"59999","volume":"1"},{"price":"59998","volume":"2"}],"asks":[{"price":"60000","volume":"3"}]}}"#;
    e.handle(
        6.,
        Event::WsMessage {
            socket: SocketId(2),
            text: frame,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids().len(), 2);
    e.handle(7.,Event::WsMessage{socket:SocketId(2),text:r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"59997","volume":"4"}],"asks":[]}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(59997., 4.)]);
    assert!(e.book("BTCUSDT").unwrap().asks().is_empty());
    e.handle(
        7.5,
        Event::WsMessage {
            socket: SocketId(2),
            text: r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{}}"#,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(59997., 4.)]);
    e.handle(
        8.,
        Event::WsMessage {
            socket: SocketId(2),
            text: "Ping",
        },
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..} if text=="Pong"))
    );
}
#[test]
fn error_envelope_after_streamed_array_does_not_publish_markets() {
    let mut e = Engine::new(Bingx::new(Options::default()));
    e.start(0.);
    respond(&mut e, 1., r#"{"data":[],"code":100004,"msg":"denied"}"#);
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
}
#[test]
fn candle_rows_sort_by_time_and_use_seconds() {
    let mut e = seeded();
    e.handle(
        10.,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            limit: Some(2),
            from_ms: None,
            to_ms: Some(120000.),
        }),
    );
    respond(
        &mut e,
        11.,
        r#"{"code":0,"data":[{"time":120000,"open":"2","high":"3","low":"1","close":"2.5","volume":"4"},{"time":60000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}]}"#,
    );
    let out = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandDone { result, .. }) => {
                Some(result.unwrap())
            }
            _ => None,
        })
        .unwrap();
    assert!(out.starts_with(r#"[{"timestamp":60.0,"#));
}

#[test]
fn invalid_unicode_interval_returns_error_without_trapping() {
    let mut e = seeded();
    e.handle(
        10.,
        Event::Command(Command::FetchCandles {
            command: 77,
            symbol: "BTCUSDT".into(),
            interval: "💥".into(),
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone {
            command: 77,
            result: Err(_)
        })
    )));
}
#[test]
fn startup_failure_retries_the_read_without_loading_empty_markets() {
    let mut e = Engine::new(Bingx::new(Options::default()));
    e.start(0.);
    respond(&mut e, 1., r#"{"code":100004,"msg":"temporary","data":[]}"#);
    assert!(!e.store().loaded().markets);
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1001., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/contracts")))
    );
}

#[test]
fn live_ticker_cache_handles_early_frames_filtered_rows_and_reconnects() {
    let mut e = Engine::new(Bingx::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.);
    respond(
        &mut e,
        1.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":1,"tradeMinLimit":"0.001"},{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":2,"tradeMinLimit":"0.001"}]}"#,
    );
    let update = r#"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"BTC-USDT","tradePrice":"90","fairPrice":"91","indexPrice":"92","fundingRate":"0.01"},{"symbol":"ETH-USDT","tradePrice":"101","fairPrice":"102","indexPrice":"103","fundingRate":"0.01"}]}}"#;
    e.handle(
        1.1,
        Event::WsOpened {
            socket: SocketId(2),
        },
    );
    e.handle(
        1.2,
        Event::WsMessage {
            socket: SocketId(2),
            text: update,
        },
    );
    assert_eq!(e.store().tickers().len(), 2);
    assert!(!e.store().loaded().tickers);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 90.);
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().last, 101.);
    // Daily order differs from market order, and Premium removes the leading daily row.
    respond(
        &mut e,
        2.,
        r#"{"code":0,"data":[{"symbol":"ETH-USDT","lastPrice":"100","priceChangePercent":"1","volume":"1","quoteVolume":"100"},{"symbol":"BTC-USDT","lastPrice":"80","priceChangePercent":"1","volume":"1","quoteVolume":"80"}]}"#,
    );
    e.handle(
        2.1,
        Event::WsMessage {
            socket: SocketId(2),
            text: update,
        },
    );
    assert_eq!(e.store().tickers().len(), 2);
    assert!(!e.store().loaded().tickers);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 90.);
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().last, 101.);
    respond(
        &mut e,
        3.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"80","indexPrice":"80","lastFundingRate":"0.01","nextFundingTime":0}]}"#,
    );
    assert_eq!(
        e.store().ticker("BTCUSDT").unwrap().last,
        90.,
        "late REST cannot replace live quotes"
    );
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().last, 101.);
    e.handle(
        4.,
        Event::WsMessage {
            socket: SocketId(2),
            text: update,
        },
    );
    assert_eq!(e.store().tickers().len(), 2);
    assert_eq!(e.store().tickers()[0].symbol, "BTCUSDT");
    assert_eq!(e.store().tickers()[0].last, 90.);
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().last, 101.);
    e.drain_actions().for_each(drop);
    e.handle(
        5.,
        Event::WsClosed {
            socket: SocketId(2),
            code: 1006,
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1005., Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(1006., Event::WsOpened { socket });
    let after_reconnect = update.replace("\"fairPrice\":\"91\"", "\"fairPrice\":\"96\"");
    e.handle(
        1007.,
        Event::WsMessage {
            socket,
            text: &after_reconnect,
        },
    );
    assert_eq!(e.store().tickers()[0].mark, 96.);
}
