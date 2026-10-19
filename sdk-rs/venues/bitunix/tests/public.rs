//! Payload shapes: bitunix.com/api-docs/futures/market/{trading_pairs,tickers,kline}.html
//! Legacy depthop semantics: exchanges/bitunix/CLAUDE.md, captured 2026-07-15.
use safe_cex_engine::{Action, Command, Engine, Event, Response, SocketId};
use safe_cex_venue_bitunix::{BitunixFutures, Options};
const PAIRS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"},{"symbol":"NOUSDT","base":"NO","quote":"USDT","symbolStatus":"OPEN","basePrecision":0,"quotePrecision":2,"maxLimitOrderVolume":"100","isApiSupported":"false"}]}"#;
const TICKERS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"}]}"#;
fn response(e: &mut Engine<BitunixFutures>, r: safe_cex_engine::RequestId, body: &str) {
    e.handle(
        0.0,
        Event::HttpResponse {
            request: r,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn ready() -> (Engine<BitunixFutures>, SocketId) {
    let mut e = Engine::new(BitunixFutures::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.0);
    let Action::Http(r) = e.drain_actions().next().unwrap() else {
        panic!()
    };
    response(&mut e, r.id, PAIRS);
    let actions: Vec<_> = e.drain_actions().collect();
    let mut futures = None;
    for a in actions {
        match a {
            Action::Http(r) => response(&mut e, r.id, TICKERS),
            Action::WsConnect { socket, url } => {
                if url.contains("ws-futures") {
                    futures = Some(socket);
                }
                e.handle(0.0, Event::WsOpened { socket });
            }
            _ => {}
        }
    }
    let actions: Vec<_> = e.drain_actions().collect();
    for a in actions {
        if let Action::WsConnect { socket, .. } = a {
            e.handle(0.0, Event::WsOpened { socket });
        }
    }
    (e, futures.unwrap())
}
#[test]
fn streams_seed_filters_api_disabled_and_updates_only_ticker_fields() {
    let (mut e, s) = ready();
    assert_eq!(e.store().markets().len(), 1);
    let m = &e.store().markets()[0];
    assert_eq!(m.id, "BTC/USDT");
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(m.precision.price, 0.1);
    e.handle(5.0,Event::WsMessage {socket:s,text:r#"{"ch":"ticker","symbol":"BTCUSDT","data":{"la":"97000","o":"95000","b":"1234","q":"999"}}"#});
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(t.last, 97000.0);
    assert_eq!(t.mark, 96001.0);
    assert_eq!(t.bid, 96000.0);
    e.handle(6.0,Event::WsMessage {socket:s,text:r#"{"ch":"price","symbol":"BTCUSDT","data":{"mp":"97001","ip":"97002","fr":"0.0001","ft":"1800000000000"}}"#});
    assert_eq!(
        e.store().ticker("BTCUSDT").unwrap().funding_time,
        1800000000000.0
    );
}
#[test]
fn full_snapshots_replace_both_sides_and_replay_on_reconnect() {
    let (mut e, s) = ready();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    for text in [
        r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["96000","2"],["95999","1"]],"asks":[["96001","3"]]}}"#,
        r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["95998","4"]],"asks":[]}}"#,
    ] {
        e.handle(1.0, Event::WsMessage { socket: s, text });
    }
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(95998.0, 4.0)]);
    assert!(e.book("BTCUSDT").unwrap().asks().is_empty());
    e.drain_actions().for_each(drop);
    e.handle(
        2.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3.0, Event::WsOpened { socket });
    assert!(e.book("BTCUSDT").is_none());
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::WsSend {text,..} if text.contains("market_btcusdt_depthop_0.1"))
    ));
}
#[test]
fn incomplete_stream_never_commits_a_partial_market_list() {
    let mut e = Engine::new(BitunixFutures::new(Options::default()));
    e.start(0.0);
    let Action::Http(r) = e.drain_actions().next().unwrap() else {
        panic!()
    };
    response(&mut e, r.id, &PAIRS[..PAIRS.len() - 2]);
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
}
#[test]
fn candle_reads_use_quote_volume_and_seconds() {
    let (mut e, _) = ready();
    e.drain_actions().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            limit: Some(1),
            from_ms: Some(0.0),
            to_ms: Some(1000.0),
        }),
    );
    let r = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    response(
        &mut e,
        r.id,
        r#"{"code":0,"data":[{"time":1000,"open":"1","high":"3","low":"1","close":"2","quoteVol":"99"}]}"#,
    );
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone {command:9,result:Ok(s)}) if s.contains("\"timestamp\":1.0") && s.contains("\"volume\":99.0"))));
}

#[test]
fn rejects_http_200_failure_envelopes_even_with_an_empty_data_array() {
    for body in [
        r#"{"code":10001,"msg":"failure","data":[]}"#,
        r#"{"data":[],"code":10001,"msg":"failure"}"#,
        r#"{"code":0,"data":[]"#,
    ] {
        let mut e = Engine::new(BitunixFutures::new(Options::default()));
        e.start(0.0);
        let Action::Http(r) = e.drain_actions().next().unwrap() else {
            panic!()
        };
        response(&mut e, r.id, body);
        assert!(!e.store().loaded().markets);
    }
}

#[test]
fn ticker_seed_failure_retries_only_seed_and_public_streams_wait_for_it() {
    let mut e = Engine::new(BitunixFutures::new(Options::default()));
    e.start(0.0);
    let Action::Http(r) = e.drain_actions().next().unwrap() else {
        panic!()
    };
    response(&mut e, r.id, PAIRS);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        1
    );
    let request = actions
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    response(
        &mut e,
        request.id,
        r#"{"code":10001,"data":[],"msg":"rate limit"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(5000.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let r = actions
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(r.url.ends_with("/tickers"));
    response(&mut e, r.id, TICKERS);
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
}

#[test]
fn watchdog_closes_a_silent_connection_and_reconnects() {
    let (mut e, s) = ready();
    let actions: Vec<_> = e.drain_actions().collect();
    // ready consumed connect timers, so isolate a fresh reconnection.
    drop(actions);
    e.handle(
        1.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(50_000.0, Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}
