//! Contract payloads follow BingX-API/api-ai-skills swap-market/api-reference.md.
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;

/// Managed reads are released only through engine preparation.
fn drain(engine: &mut Engine<Bingx>, now: f64) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now).into_iter()
}
/// The next released request, and the public socket if it connected meanwhile.
fn next_request(engine: &mut Engine<Bingx>, now: f64) -> (RequestId, Option<SocketId>) {
    let actions: Vec<_> = drain(engine, now).collect();
    let socket = actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, .. } => Some(*socket),
        _ => None,
    });
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .expect("request");
    (request, socket)
}
fn reply(engine: &mut Engine<Bingx>, now: f64, request: RequestId, body: &str) {
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
fn respond(engine: &mut Engine<Bingx>, now: f64, body: &str) -> Option<SocketId> {
    let (request, socket) = next_request(engine, now);
    reply(engine, now, request, body);
    socket
}
fn seeded() -> Engine<Bingx> {
    seeded_with_socket().0
}
/// The seeded engine and its public socket, connected once markets load.
fn seeded_with_socket() -> (Engine<Bingx>, SocketId) {
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(0.);
    let mut socket = respond(
        &mut engine,
        1.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#,
    );
    socket = socket.or(respond(
        &mut engine,
        2.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#,
    ));
    socket = socket.or(respond(
        &mut engine,
        3.,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#,
    ));
    (engine, socket.expect("public socket"))
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
    let (mut e, socket) = seeded_with_socket();
    e.handle(
        4.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(5., Event::WsOpened { socket });
    drain(&mut e, 5.).for_each(drop);
    let frame = r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"59999","volume":"1"},{"price":"59998","volume":"2"}],"asks":[{"price":"60000","volume":"3"}]}}"#;
    e.handle(
        6.,
        Event::WsMessage {
            socket,
            text: frame,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids().len(), 2);
    e.handle(7.,Event::WsMessage{socket,text:r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"59997","volume":"4"}],"asks":[]}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(59997., 4.)]);
    assert!(e.book("BTCUSDT").unwrap().asks().is_empty());
    e.handle(
        7.5,
        Event::WsMessage {
            socket,
            text: r#"{"dataType":"market.depth.BTC-USDT.step0.level100","data":{}}"#,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(59997., 4.)]);
    e.handle(
        8.,
        Event::WsMessage {
            socket,
            text: "Ping",
        },
    );
    assert!(drain(&mut e, 8.).any(|a| matches!(a,Action::WsSend{text,..} if text=="Pong")));
}
#[test]
fn error_envelope_after_streamed_array_does_not_publish_markets() {
    let mut e = support::engine(Options::default());
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
            interval: safe_cex_engine::model::Interval::M1,
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
    let out = drain(&mut e, 11.)
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandDone { result, .. }) => Some(result),
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
            interval: safe_cex_engine::model::Interval::S1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(drain(&mut e, 10.).any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 77, .. })
    )));
}
#[test]
fn startup_failure_retries_the_read_without_loading_empty_markets() {
    let mut e = support::engine(Options::default());
    e.start(0.);
    respond(&mut e, 1., r#"{"code":100004,"msg":"temporary","data":[]}"#);
    assert!(!e.store().loaded().markets);
    let timer = drain(&mut e, 1.)
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1001., Event::TimerFired { timer });
    assert!(
        drain(&mut e, 1001.).any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/contracts")))
    );
}

#[test]
fn live_ticker_cache_handles_early_frames_filtered_rows_and_reconnects() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let (markets, _) = next_request(&mut e, 0.);
    reply(
        &mut e,
        1.,
        markets,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":1,"tradeMinLimit":"0.001"},{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":2,"tradeMinLimit":"0.001"}]}"#,
    );
    let (daily, socket) = next_request(&mut e, 1.);
    let socket = socket.expect("public socket");
    let update = r#"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"BTC-USDT","tradePrice":"90","fairPrice":"91","indexPrice":"92","fundingRate":"0.01"},{"symbol":"ETH-USDT","tradePrice":"101","fairPrice":"102","indexPrice":"103","fundingRate":"0.01"}]}}"#;
    e.handle(1.1, Event::WsOpened { socket });
    e.handle(
        1.2,
        Event::WsMessage {
            socket,
            text: update,
        },
    );
    assert_eq!(e.store().tickers().len(), 2);
    assert!(!e.store().loaded().tickers);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 90.);
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().last, 101.);
    // Daily order differs from market order, and Premium removes the leading daily row.
    reply(
        &mut e,
        2.,
        daily,
        r#"{"code":0,"data":[{"symbol":"ETH-USDT","lastPrice":"100","priceChangePercent":"1","volume":"1","quoteVolume":"100"},{"symbol":"BTC-USDT","lastPrice":"80","priceChangePercent":"1","volume":"1","quoteVolume":"80"}]}"#,
    );
    e.handle(
        2.1,
        Event::WsMessage {
            socket,
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
    // Like TS, a market the funding read omitted is dropped even though live
    // frames staged a row for it before the seed committed.
    assert!(e.store().ticker("ETHUSDT").is_none());
    e.handle(
        4.,
        Event::WsMessage {
            socket,
            text: update,
        },
    );
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().tickers()[0].symbol, "BTCUSDT");
    assert_eq!(e.store().tickers()[0].last, 90.);
    assert!(
        e.store().ticker("ETHUSDT").is_none(),
        "live frames after the seed cannot recreate a market TS omits"
    );
    drain(&mut e, 4.).for_each(drop);
    e.handle(5., Event::WsClosed { socket, code: 1006 });
    let timer = drain(&mut e, 5.)
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1005., Event::TimerFired { timer });
    let socket = drain(&mut e, 1005.)
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

#[test]
fn supervisor_retains_backoff_across_open_flaps_and_bounds_connecting() {
    let (mut e, mut socket) = seeded_with_socket();
    drain(&mut e, 3.).for_each(drop);
    let mut now = 10.0;
    e.handle(now, Event::WsOpened { socket });
    drain(&mut e, now).for_each(drop);
    let mut previous = 0.0;
    for nominal in [1000.0, 2000.0, 4000.0] {
        e.handle(now, Event::WsClosed { socket, code: 1006 });
        let (timer, at) = drain(&mut e, now)
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .unwrap();
        assert!((nominal * 0.75..=nominal).contains(&(at - now)));
        assert!(at - now > previous);
        previous = at - now;
        now = at;
        e.handle(now, Event::TimerFired { timer });
        let actions: Vec<_> = drain(&mut e, now).collect();
        socket = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        if nominal < 4000.0 {
            e.handle(now, Event::WsOpened { socket });
            drain(&mut e, now).for_each(drop);
        } else {
            let timer = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, .. } => Some(*timer),
                    _ => None,
                })
                .unwrap();
            e.handle(now + 10_000.0, Event::TimerFired { timer });
            assert!(
                drain(&mut e, now + 10_000.0)
                    .any(|a| matches!(a,Action::WsClose{socket:s} if s==socket))
            );
        }
    }
}

#[derive(Default)]
struct Suppressed {
    requests: Vec<RequestId>,
    timers: Vec<(TimerId, f64)>,
    connects: usize,
    closes: usize,
}

impl Suppressed {
    fn collect(&mut self, e: &mut Engine<Bingx>, now: f64) {
        for action in drain(e, now) {
            match action {
                Action::Http(r) => self.requests.push(r.id),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { .. } => self.connects += 1,
                Action::WsClose { .. } => self.closes += 1,
                _ => {}
            }
        }
    }
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.set_public_feed_policy(PublicFeedPolicy::Shared).unwrap();
    e.start(0.);
    let mut h = Suppressed::default();
    h.collect(&mut e, 0.);
    for body in [
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#,
    ] {
        let request = h.requests.remove(0);
        let response = Response {
            status: 200,
            headers: "",
            body,
        };
        e.handle(1., Event::HttpResponse { request, response });
        h.collect(&mut e, 1.);
    }
    assert_eq!(e.store().markets().len(), 1, "public startup completed");
    let mut fired = 0;
    while let Some(&(timer, at)) = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= 600_000.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        h.collect(&mut e, at);
        fired += 1;
    }
    assert_eq!(
        (h.connects, h.closes),
        (0, 0),
        "no public connects or closes"
    );
    assert_eq!(fired, 0, "no supervision timers fire");
}
