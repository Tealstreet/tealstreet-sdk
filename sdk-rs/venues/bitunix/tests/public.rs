//! Payload shapes: bitunix.com/api-docs/futures/market/{trading_pairs,tickers,kline}.html
//! Legacy depthop semantics: exchanges/bitunix/CLAUDE.md, captured 2026-07-15.
mod support;
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
    let mut e = support::engine(support::Options {
        public_data_only: true,
        ..support::Options::default()
    });
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    let Action::Http(r) = r else { panic!() };
    response(&mut e, r.id, PAIRS);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    let mut futures = None;
    for a in actions {
        match a {
            Action::Http(r) => response(&mut e, r.id, TICKERS),
            Action::WsConnect { socket, url, .. } => {
                if url.contains("ws-futures") {
                    futures = Some(socket);
                }
                e.handle(0.0, Event::WsOpened { socket });
            }
            _ => {}
        }
    }
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
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
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    e.handle(
        2.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
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
    e.handle(at, Event::WsOpened { socket });
    assert!(e.book("BTCUSDT").is_none());
    assert!(safe_cex_engine::test_support::prepared_now(&mut e).into_iter().any(
        |a| matches!(a,Action::WsSend {text,..} if text.contains("market_btcusdt_depthop_0.1"))
    ));
}
#[test]
fn an_authoritative_empty_snapshot_clears_the_consumer_book() {
    use safe_cex_engine::wire::{section, sections};
    let (mut e, socket) = ready();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(e.book("BTCUSDT").is_none());
    e.take_frame(0.0);
    e.handle(1.0, Event::WsMessage { socket, text: r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["96000","2"]],"asks":[["96001","3"]]}}"# });
    let frame = e.take_frame(1.0).unwrap();
    assert!(sections(frame).any(|(kind, _)| kind == section::BOOK));
    e.handle(
        2.0,
        Event::WsMessage {
            socket,
            text: r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[],"asks":[]}}"#,
        },
    );
    let frame = e.take_frame(2.0).expect("empty snapshot must be delivered");
    let book = sections(frame)
        .find(|(kind, _)| *kind == section::BOOK)
        .unwrap()
        .1;
    assert_eq!(book.len(), 16);
    assert_eq!(&book[4..12], &[0; 8]);
    assert!(e.book("BTCUSDT").unwrap().is_empty());
    assert!(
        sections(e.snapshot_frame(3.0))
            .any(|(kind, data)| kind == section::BOOK && data[4..12] == [0; 8])
    );
}
#[test]
fn incomplete_stream_never_commits_a_partial_market_list() {
    let mut e =
        safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options::default()));
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    let Action::Http(r) = r else { panic!() };
    response(&mut e, r.id, &PAIRS[..PAIRS.len() - 2]);
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
}
#[test]
fn candle_reads_use_quote_volume_and_seconds() {
    let (mut e, _) = ready();
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: Some(0.0),
            to_ms: Some(1000.0),
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
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
        r.id,
        r#"{"code":0,"data":[{"time":1000,"open":"1","high":"3","low":"1","close":"2","quoteVol":"99"}]}"#,
    );
    assert!(safe_cex_engine::test_support::prepared_now(&mut e).into_iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone {command:9,result:s}) if s.contains("\"timestamp\":1.0") && s.contains("\"volume\":99.0"))));
}

#[test]
fn rejects_http_200_failure_envelopes_even_with_an_empty_data_array() {
    for body in [
        r#"{"code":10001,"msg":"failure","data":[]}"#,
        r#"{"data":[],"code":10001,"msg":"failure"}"#,
        r#"{"code":0,"data":[]"#,
    ] {
        let mut e =
            safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options::default()));
        e.start(0.0);
        let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let Action::Http(r) = r else { panic!() };
        response(&mut e, r.id, body);
        assert!(!e.store().loaded().markets);
    }
}

#[test]
fn ticker_seed_failure_retries_only_seed_and_public_streams_wait_for_it() {
    let mut e =
        safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options::default()));
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    let Action::Http(r) = r else { panic!() };
    response(&mut e, r.id, PAIRS);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
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
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 5000.);
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
        safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
}

#[test]
fn watchdog_closes_a_silent_connection_and_reconnects() {
    let (mut e, original) = ready();
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    e.handle(
        1.0,
        Event::WsClosed {
            socket: original,
            code: 1006,
        },
    );
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(at + 45_000.0, Event::TimerFired { timer });
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn accepts_extended_legacy_levels_and_documented_iso_next_funding_time() {
    let (mut e, s) = ready();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(1.0,Event::WsMessage {socket:s,text:r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["96000","2","",""]],"asks":[["96001","3","",""]]}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(96000.0, 2.0)]);
    // Official websocket/public/MarketPrice Channel example, also seen in the live tape.
    e.handle(1.0,Event::WsMessage {socket:s,text:r#"{"ch":"price","symbol":"BTCUSDT","data":{"mp":"10000","ip":"0.0010","fr":"0.013461","ft":"2024-12-04T11:00:00Z","nft":"2024-12-04T12:00:00Z"}}"#});
    assert_eq!(
        e.store().ticker("BTCUSDT").unwrap().funding_time,
        1733313600000.0
    );
}

#[test]
fn public_envelopes_accept_arbitrary_field_order_and_reject_malformed_payloads() {
    let (mut e, s) = ready();
    for text in [
        r#"{"ch":"ticker","symbol":"BTCUSDT","data":{"la":"97000","o":"95000","b":"1234","q":"999"}}"#,
        r#"{"data":{"la":"97000","o":"95000","b":"1234","q":"999"},"symbol":"BTCUSDT","ch":"ticker"}"#,
        r#"{"ch":"ticker","data":{"la":"97000","o":"95000","b":"1234","q":"999"},"symbol":"BTCUSDT"}"#,
    ] {
        e.handle(1.0, Event::WsMessage { socket: s, text });
        let ticker = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!(ticker.last, 97000.0);
        assert_eq!(ticker.volume, 1234.0);
        assert_eq!(ticker.quote_volume, 999.0);
    }
    for text in [
        r#"{"ch":"price","data":{"mp":"97001","ip":"97002","fr":"0.0001","ft":"1800000000000"},"symbol":"BTCUSDT"}"#,
        r#"{"data":{"mp":"97001","ip":"97002","fr":"0.0001","ft":"1800000000000"},"ch":"price","symbol":"BTCUSDT"}"#,
    ] {
        e.handle(2.0, Event::WsMessage { socket: s, text });
        let ticker = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!(ticker.mark, 97001.0);
        assert_eq!(ticker.funding_time, 1800000000000.0);
    }
    for text in [
        r#"{"ch":"depth_book1","symbol":"BTCUSDT","data":{"b":[["96999","1"]],"a":[["97001","2"]]}}"#,
        r#"{"data":{"b":[["96999","1"]],"a":[["97001","2"]]},"symbol":"BTCUSDT","ch":"depth_book1"}"#,
    ] {
        e.handle(3.0, Event::WsMessage { socket: s, text });
        let ticker = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!(ticker.bid, 96999.0);
        assert_eq!(ticker.ask, 97001.0);
    }
    let before = e.store().ticker("BTCUSDT").unwrap().clone();
    for text in [
        r#"{"ch":"ticker","symbol":"BTCUSDT","data":{"la":"1"}}"#,
        r#"{"data":{"la":"1"},"symbol":"BTCUSDT","ch":"ticker"}"#,
        r#"{"ch":"price","symbol":"BTCUSDT","data":[]}"#,
        r#"{"ch":"depth_book1","symbol":"BTCUSDT","data":{"b":[],"a":[[]]}}"#,
        r#"{"ch":"ticker","symbol":"BTCUSDT","data":null}"#,
    ] {
        e.handle(4.0, Event::WsMessage { socket: s, text });
        assert_eq!(e.store().ticker("BTCUSDT").unwrap(), &before);
    }
}

#[test]
fn candle_requests_reject_malformed_intervals_and_compute_supported_durations() {
    let (mut e, _) = ready();
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    // Malformed text is rejected by the engine's typed Interval; Bitunix still
    // refuses valid intervals its kline API does not offer.
    use safe_cex_engine::model::Interval;
    for interval in [Interval::S1, Interval::H3, Interval::Mo1] {
        e.handle(
            100_000_000.0,
            Event::Command(Command::FetchCandles {
                command: 90,
                symbol: "BTCUSDT".into(),
                interval,
                limit: Some(2),
                from_ms: None,
                to_ms: Some(100_000_000.0),
            }),
        );
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 90, .. })
            )),
            "{interval:?}"
        );
    }
    for (interval, duration) in [("1m", 60_000), ("2h", 7_200_000), ("1w", 604_800_000)] {
        e.handle(
            100_000_000.0,
            Event::Command(Command::FetchCandles {
                command: 91,
                symbol: "BTCUSDT".into(),
                interval: Interval::parse(interval).unwrap(),
                limit: Some(2),
                from_ms: None,
                to_ms: Some(100_000_000.0),
            }),
        );
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 100_000_000.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert!(
            request
                .url
                .contains(&format!("startTime={}", 100_000_000 - duration * 2))
        );
        e.handle(
            100_000_000.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":0,"data":[]}"#,
                },
            },
        );
        safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .for_each(drop);
    }
}
#[test]
fn candle_interest_without_credentials_reads_no_symbol_settings() {
    let (mut e, _) = ready();
    e.handle(
        1.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    // Public-only accounts neither send the signed read nor log a credentials failure.
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("get_leverage_margin_mode")))
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("credentials")
    )));
}

fn pending_http(
    e: &mut Engine<BitunixFutures>,
    now: f64,
    path: &str,
) -> safe_cex_engine::HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r),
            _ => None,
        })
        .expect(path)
}
fn pending_http_actions(actions: &[Action], path: &str) -> safe_cex_engine::HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .expect(path)
}
fn public_engine(credentials: bool) -> Engine<BitunixFutures> {
    let mut e = support::engine(support::Options {
        key: credentials.then(|| "key".into()),
        secret: credentials.then(|| "secret".into()),
        ..support::Options::default()
    });
    e.start(0.0);
    e
}
#[test]
fn a_malformed_public_row_does_not_starve_valid_market_or_ticker_peers() {
    let mut e = public_engine(false);
    let market = pending_http(&mut e, 0., "trading_pairs");
    let pair: serde_json::Value = serde_json::from_str(PAIRS).unwrap();
    response(
        &mut e,
        market.id,
        &format!(
            r#"{{"code":0,"data":[{},{{"symbol":"BADUSDT","basePrecision":null}}]}}"#,
            pair["data"][0]
        ),
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::ROW_SKIPPED)));
    let ticker = pending_http_actions(&actions, "/tickers");
    let tick: serde_json::Value = serde_json::from_str(TICKERS).unwrap();
    response(
        &mut e,
        ticker.id,
        &format!(
            r#"{{"code":0,"data":[{{"symbol":"BADUSDT","last":null}},{}]}}"#,
            tick["data"][0]
        ),
    );
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 96000.0);
    assert!(safe_cex_engine::test_support::prepared_now(&mut e).into_iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::ROW_SKIPPED)));
}
#[test]
fn a_bad_ticker_row_cannot_discard_the_rest_of_a_usable_seed() {
    let mut e = public_engine(false);
    let r = pending_http(&mut e, 0., "trading_pairs");
    response(&mut e, r.id, PAIRS);
    let r = pending_http(&mut e, 0., "/tickers");
    let ticks: serde_json::Value = serde_json::from_str(TICKERS).unwrap();
    response(
        &mut e,
        r.id,
        &format!(r#"{{"code":0,"data":[{},null]}}"#, ticks["data"][0]),
    );
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 1);
}
#[test]
fn all_malformed_required_seeds_retry_without_loaded_empty_state() {
    for ticker in [false, true] {
        let mut e = public_engine(false);
        let mut r = pending_http(&mut e, 0., "trading_pairs");
        if ticker {
            response(&mut e, r.id, PAIRS);
            r = pending_http(&mut e, 0., "/tickers");
        }
        response(&mut e, r.id, r#"{"code":0,"data":[null,{}]}"#);
        assert!(
            !(if ticker {
                e.store().loaded().tickers
            } else {
                e.store().loaded().markets
            })
        );
        assert!(
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}
#[test]
fn private_initialization_starts_before_tickers_and_seed_recovery_never_restarts_it() {
    let mut e = public_engine(true);
    let r = pending_http(&mut e, 0., "trading_pairs");
    response(&mut e, r.id, PAIRS);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a,Action::WsConnect {url,..} if url.contains("/private/")))
            .count(),
        1,
        "private socket must not wait for ticker seed"
    );
    let balance = pending_http_actions(&out, "/account");
    let positions = pending_http_actions(&out, "get_pending_positions");
    let orders = pending_http_actions(&out, "get_pending_orders");
    let ticker = pending_http_actions(&out, "/tickers");
    assert!(!e.store().loaded().tickers);
    // The account can establish settings independently from the pending seed.
    response(
        &mut e,
        balance.id,
        r#"{"code":0,"data":[{"marginCoin":"USDT","available":"900","margin":"100","positionMode":"ONE_WAY"}]}"#,
    );
    response(
        &mut e,
        positions.id,
        r#"{"code":0,"data":{"positionList":[{"symbol":"BTCUSDT","positionId":"p","qty":"1","side":"BUY","avgOpenPrice":"100","leverage":20,"marginMode":"ISOLATION","positionMode":"ONE_WAY"}]}}"#,
    );
    response(&mut e, orders.id, r#"{"code":0,"data":{"orderList":[]}}"#);
    for a in safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect::<Vec<_>>()
    {
        if let Action::Http(r) = a {
            assert!(r.url.contains("/tpsl/"));
            response(&mut e, r.id, r#"{"code":0,"data":[]}"#);
        }
    }
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert!(
        e.store().loaded().balance && e.store().loaded().positions && e.store().loaded().orders,
        "private startup must complete while ticker seed remains pending"
    );
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    response(
        &mut e,
        ticker.id,
        r#"{"code":10001,"msg":"temporary failure","data":[]}"#,
    );
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let r = pending_http(&mut e, at, "/tickers");
    response(&mut e, r.id, TICKERS);
    let out: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::WsConnect {
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            }
    )));
    assert_eq!(
        out.iter()
            .filter(|a| matches!(
                a,
                Action::WsConnect {
                    channel: Some(safe_cex_engine::SocketChannel::Public),
                    ..
                }
            ))
            .count(),
        2
    );
}
#[test]
fn public_failures_use_capped_shared_backoff_and_report_recovery() {
    let mut e = public_engine(false);
    let mut request = pending_http(&mut e, 0., "trading_pairs");
    let mut now = 0.0;
    let mut degraded = 0;
    for nominal in [1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 30000.0, 30000.0] {
        e.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "public unavailable",
            },
        );
        let out: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        degraded+=out.iter().filter(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::STARTUP_DEGRADED)).count();
        let (timer, at) = out
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        assert!(
            (nominal * 0.75..=nominal).contains(&(at - now)),
            "retry delay {} vs nominal {nominal}",
            at - now
        );
        now = at;
        e.handle(now, Event::TimerFired { timer });
        request = pending_http(&mut e, now, "trading_pairs");
        e.handle(now, Event::TimerFired { timer });
        assert!(
            !safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Http(_))),
            "retired timer must not duplicate read"
        );
    }
    assert_eq!(degraded, 1);
    response(&mut e, request.id, PAIRS);
    let request = pending_http(&mut e, 0., "/tickers");
    response(&mut e, request.id, TICKERS);
    assert_eq!(safe_cex_engine::test_support::prepared_now(&mut e).into_iter().filter(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::STARTUP_RECOVERED)).count(),1);
}

#[test]
fn discovery_rate_limits_honor_headers_and_native_codes_without_misclassifying_network_errors() {
    for (status, code, headers, minimum) in [
        (429, 0, "Retry-After: 15\r\n", 15000.0),
        (200, 10005, "", 60000.0),
        (200, 10006, "", 60000.0),
        (200, 10001, "", 0.0),
    ] {
        let mut e = public_engine(false);
        let request = pending_http(&mut e, 0., "trading_pairs");
        let body = format!(r#"{{"code":{code},"msg":"temporary failure","data":[]}}"#);
        e.handle(
            0.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers,
                    body: &body,
                },
            },
        );
        let at = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .unwrap();
        if minimum > 0.0 {
            assert_eq!(at, minimum);
        } else {
            assert!(
                (750.0..=1000.0).contains(&at),
                "native10001 is not a rate limit"
            );
        }
    }
}
#[test]
fn a_truncated_ticker_stream_never_commits_its_valid_prefix() {
    let mut e = public_engine(false);
    let request = pending_http(&mut e, 0., "trading_pairs");
    response(&mut e, request.id, PAIRS);
    let request = pending_http(&mut e, 0., "/tickers");
    response(&mut e, request.id, &TICKERS[..TICKERS.len() - 2]);
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
}
#[test]
fn intentionally_empty_or_filtered_catalogues_are_distinct_from_malformed_rows() {
    for filtered in [false, true] {
        let mut e = public_engine(false);
        let request = pending_http(&mut e, 0., "trading_pairs");
        let pairs: serde_json::Value = serde_json::from_str(PAIRS).unwrap();
        let body = if filtered {
            format!(r#"{{"code":0,"data":[{}]}}"#, pairs["data"][1])
        } else {
            r#"{"code":0,"data":[]}"#.to_owned()
        };
        response(&mut e, request.id, &body);
        assert!(!e.store().loaded().markets);
        assert!(e.store().markets().is_empty());
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(!actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::ROW_SKIPPED)));
        assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::WsConnect { .. })),
            "no usable catalogue means no private/public sockets yet"
        );
    }
}

#[test]
fn discovery_stream_business_rate_limits_without_arrays_keep_the_native_pause() {
    for ticker in [false, true] {
        for code in [10005, 10006] {
            for data in ["", r#", "data":null"#, r#", "data":{}"#] {
                let mut e = public_engine(false);
                let mut request = pending_http(&mut e, 0., "trading_pairs");
                if ticker {
                    response(&mut e, request.id, PAIRS);
                    request = pending_http(&mut e, 0., "/tickers");
                }
                response(
                    &mut e,
                    request.id,
                    &format!(r#"{{"code":{code},"msg":"request too frequently"{data}}}"#),
                );
                let out: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
                    .into_iter()
                    .collect();
                assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
                assert!(
                    out.iter()
                        .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==60000.0)),
                    "code={code}, ticker={ticker}, data={data}"
                );
            }
        }
    }
}
#[test]
fn discovery_stream_errors_never_echo_raw_envelopes_into_failure_or_degradation() {
    for ticker in [false, true] {
        for body in [
            r#"{"code":0,"RAW_BODY_SENTINEL":"private row","data":{}}"#,
            r#"{"code":0,"RAW_BODY_SENTINEL":"private row"}"#,
            r#"{"code":0,"data":[],"RAW_BODY_SENTINEL":"private row","broken":"#,
        ] {
            let mut e = public_engine(false);
            let path = if ticker { "/tickers" } else { "trading_pairs" };
            let mut request = pending_http(&mut e, 0., "trading_pairs");
            if ticker {
                response(&mut e, request.id, PAIRS);
                request = pending_http(&mut e, 0., path);
            }
            let mut degraded = 0;
            for _ in 0..6 {
                response(&mut e, request.id, body);
                let out: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
                    .into_iter()
                    .collect();
                assert!(!format!("{out:?}").contains("RAW_BODY_SENTINEL"));
                degraded+=out.iter().filter(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code==safe_cex_engine::log::code::STARTUP_DEGRADED)).count();
                let (timer, at) = out
                    .iter()
                    .find_map(|a| match a {
                        Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                        _ => None,
                    })
                    .unwrap();
                e.handle(at, Event::TimerFired { timer });
                request = pending_http(&mut e, at, path);
            }
            assert_eq!(degraded, 1);
            assert!(
                !(if ticker {
                    e.store().loaded().tickers
                } else {
                    e.store().loaded().markets
                })
            );
        }
    }
}
