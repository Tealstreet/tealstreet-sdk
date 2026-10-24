//! Payload shapes from https://docs.backpack.exchange/ (markets, depth, klines).
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, Venue};
use safe_cex_venue_backpack::{Backpack, Options};

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

type Driver = Engine<Backpack>;
fn http(e: &mut Driver, needle: &str) -> HttpRequest {
    http_at(e, needle, 0.)
}
fn http_at(e: &mut Driver, needle: &str, now: f64) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(needle) => Some(r),
            _ => None,
        })
        .expect(needle)
}
fn reply(e: &mut Driver, r: &HttpRequest, body: &str) {
    reply_at(e, r, 0., body);
}
fn reply_at(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn seed() -> (Driver, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = http(&mut e, "markets");
    reply(
        &mut e,
        &r,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","visible":true,"quantityIncrement":"0.01","priceIncrement":"0.01","minQuantity":"0.01","imfFunction":{"base":"0.02"}},{"symbol":"SOL_USDC","marketType":"SPOT","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for r in requests {
        if r.url.contains("tickers") {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#,
            );
        } else {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
            );
        }
    }
    e.handle(0.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
fn subscribe(e: &mut Driver) -> HttpRequest {
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "SOL_USDC_PERP".into(),
        }),
    );
    http(e, "depth")
}
fn delta(e: &mut Driver, socket: SocketId, first: u64, last: u64) {
    let text = format!(
        r#"{{"stream":"depth.SOL_USDC_PERP","data":{{"e":"depth","s":"SOL_USDC_PERP","U":{first},"u":{last},"b":[["140","5"]],"a":[["142","0"]]}}}}"#
    );
    e.handle(
        1.0,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}
const SNAP: &str =
    r#"{"lastUpdateId":10,"bids":[["140","1"],["139","2"]],"asks":[["142","1"],["143","2"]]}"#;
#[test]
fn streams_perp_seed_and_bridges_aggregate_range_then_requires_exact_sequence() {
    let (mut e, socket) = seed();
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().tickers()[0].mark, 140.0);
    let r = subscribe(&mut e);
    delta(&mut e, socket, 9, 11);
    reply(&mut e, &r, SNAP);
    let book = e.venue().book("SOL_USDC_PERP").unwrap();
    assert_eq!(book.bids(), [(140.0, 5.0), (139.0, 2.0)]);
    assert_eq!(book.asks(), [(143.0, 2.0)]);
    delta(&mut e, socket, 11, 12);
    assert!(
        e.venue().book("SOL_USDC_PERP").is_none(),
        "overlap after bridge must resnapshot"
    );
}
#[test]
fn unsubscribed_snapshot_cannot_replace_a_new_subscription() {
    let (mut e, _) = seed();
    let old = subscribe(&mut e);
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "SOL_USDC_PERP".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    let current = subscribe(&mut e);
    reply(&mut e, &old, SNAP);
    assert!(e.venue().book("SOL_USDC_PERP").is_none());
    reply(&mut e, &current, SNAP);
    assert!(e.venue().book("SOL_USDC_PERP").is_some());
}
#[test]
fn watchdog_reconnects_a_half_open_socket() {
    let (mut e, socket) = seed();
    e.handle(1.0, Event::WsClosed { socket, code: 1006 });
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
    e.handle(1001.0, Event::TimerFired { timer });
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
    e.handle(1001.0, Event::WsOpened { socket });
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
    e.handle(22000.0, Event::TimerFired { timer });
    assert!(e.drain_actions().any(|a| a == Action::WsClose { socket }));
}
#[test]
fn candle_seconds_and_empty_stream_candles() {
    let (mut e, socket) = seed();
    e.handle(
        0.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "SOL_USDC_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: Some(1732182900123.0),
            to_ms: None,
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("klines") => Some(request),
            _ => None,
        })
        .expect("klines");
    assert!(r.url.contains("startTime=1732182900"));
    reply(
        &mut e,
        &r,
        r#"[{"start":"2025-12-15 13:28:00","open":"140","high":"142","low":"139","close":"141","volume":"10"}]"#,
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { .. })
    )));
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "SOL_USDC_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.take_frame(0.0);
    e.handle(1.0,Event::WsMessage { socket,text:r#"{"stream":"kline.1m.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","t":"2025-12-15T13:28:00","o":null,"h":null,"l":null,"c":null,"v":null}}"# });
    assert!(e.take_frame(1.0).is_none());
}

fn retry(e: &mut Driver) -> (HttpRequest, f64) {
    let (timer, at_ms) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at_ms, Event::TimerFired { timer });
    (http_at(e, "depth", at_ms), at_ms)
}

#[test]
fn gap_evidence_survives_repeated_stale_snapshots_and_failure_is_local() {
    let (mut e, socket) = seed();
    let r = subscribe(&mut e);
    reply(&mut e, &r, SNAP);
    e.drain_actions().for_each(drop);
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC_USDC_PERP".into(),
        }),
    );
    let btc = http(&mut e, "depth");
    reply(&mut e, &btc, SNAP);
    e.drain_actions().for_each(drop);
    delta(&mut e, socket, 20, 21);
    for _ in 0..3 {
        let (r, now) = retry(&mut e);
        reply_at(&mut e, &r, now, SNAP);
    }
    assert!(e.venue().book("SOL_USDC_PERP").unwrap().is_empty());
    assert!(!e.venue().book("BTC_USDC_PERP").unwrap().is_empty());
    let errors = e
        .drain_actions()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    level: safe_cex_engine::Level::Error,
                    ..
                }))
            )
        })
        .count();
    assert_eq!(errors, 1);
    delta(&mut e, socket, 22, 23);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::Notice(_)))
    );
}
#[test]
fn successful_resnapshot_resets_recovery_streak() {
    let (mut e, socket) = seed();
    let r = subscribe(&mut e);
    reply(&mut e, &r, SNAP);
    e.drain_actions().for_each(drop);
    for i in 1..=5 {
        delta(&mut e, socket, 10 * i + 10, 10 * i + 11);
        let (r, now) = retry(&mut e);
        let snapshot = SNAP.replace(
            "\"lastUpdateId\":10",
            &format!("\"lastUpdateId\":{}", 10 * i + 10),
        );
        reply_at(&mut e, &r, now, &snapshot);
        assert!(
            e.venue()
                .book("SOL_USDC_PERP")
                .is_some_and(|b| !b.is_empty())
        );
        e.drain_actions().for_each(drop);
    }
}

#[test]
fn malformed_candle_never_enters_frame() {
    let (mut e, socket) = seed();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "SOL_USDC_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.take_frame(0.0);
    for (time, open) in [("2025-12-15T13:28:00", "invalid"), ("NaN", "140")] {
        let text = format!(
            r#"{{"stream":"kline.1m.SOL_USDC_PERP","data":{{"s":"SOL_USDC_PERP","t":"{time}","o":"{open}","h":"142","l":"139","c":"141","v":"10"}}}}"#
        );
        e.handle(
            1.0,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        assert!(e.take_frame(1.0).is_none());
    }
}

#[test]
fn string_depth_sequences_preserve_integer_precision() {
    let (mut e, socket) = seed();
    let r = subscribe(&mut e);
    let text = r#"{"stream":"depth.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","U":"9007199254740994","u":"9007199254740994","b":[["140","7"]],"a":[]}}"#;
    e.handle(1.0, Event::WsMessage { socket, text });
    reply(
        &mut e,
        &r,
        r#"{"lastUpdateId":"9007199254740993","bids":[["140","1"]],"asks":[["142","1"]]}"#,
    );
    assert_eq!(
        e.venue().book("SOL_USDC_PERP").unwrap().bids(),
        [(140.0, 7.0)]
    );
}

#[test]
fn depth_changed_levels_do_not_override_authoritative_book_ticker() {
    let (mut e, socket) = seed();
    let text =
        r#"{"stream":"bookTicker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","b":"140","a":"142"}}"#;
    e.handle(1.0, Event::WsMessage { socket, text });
    let r = subscribe(&mut e);
    reply(&mut e, &r, SNAP);
    let text = r#"{"stream":"depth.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","U":11,"u":11,"b":[["139","3"]],"a":[["142","0"]]}}"#;
    e.handle(1.0, Event::WsMessage { socket, text });
    assert_eq!(e.store().tickers()[0].bid, 140.0);
    assert_eq!(e.store().tickers()[0].ask, 142.0);
    let text = r#"{"stream":"depth.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","U":13,"u":13,"b":[["999","3"]],"a":[]}}"#;
    e.handle(2.0, Event::WsMessage { socket, text });
    assert_eq!(
        e.store().tickers()[0].bid,
        140.0,
        "unbridged gaps cannot change quotes"
    );
}

#[test]
fn documented_ticker_uses_open_price_and_uppercase_quote_volume_and_empty_quotes_clear() {
    let (mut e, socket) = seed();
    let text = r#"{"stream":"ticker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","o":"100","c":"105","v":"32123","V":"928190","h":"110","l":"99","n":93828}}"#;
    e.handle(1.0, Event::WsMessage { socket, text });
    let t = &e.store().tickers()[0];
    assert_eq!(t.last, 105.0);
    assert_eq!(t.percentage, 5.0);
    assert_eq!(t.volume, 32123.0);
    assert_eq!(t.quote_volume, 928190.0);
    let text =
        r#"{"stream":"bookTicker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","b":"104","a":"106"}}"#;
    e.handle(2.0, Event::WsMessage { socket, text });
    let text =
        r#"{"stream":"bookTicker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","b":null,"a":null}}"#;
    e.handle(3.0, Event::WsMessage { socket, text });
    assert_eq!(e.store().tickers()[0].bid, 0.0);
    assert_eq!(e.store().tickers()[0].ask, 0.0);
}

#[test]
fn failed_seed_reads_retry_independently_without_reopening_the_early_socket() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let markets = http(&mut e, "markets");
    e.handle(
        0.0,
        Event::HttpFailed {
            request: markets.id,
            error: "temporary outage",
        },
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
    e.handle(1000.0, Event::TimerFired { timer });
    let markets = http(&mut e, "markets");
    reply(
        &mut e,
        &markets,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let requests: Vec<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for r in requests {
        if r.url.contains("tickers") {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#,
            );
        } else {
            reply(&mut e, &r, "malformed");
        }
    }
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "the public socket opened with the catalogue"
    );
    let ticker = &e.store().tickers()[0];
    assert_eq!(
        (ticker.last, ticker.mark, ticker.funding_rate),
        (141.0, 141.0, 0.0)
    );
    let timer = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2000.0, Event::TimerFired { timer });
    let marks = http(&mut e, "markPrices");
    reply(
        &mut e,
        &marks,
        r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
    );
    let ticker = &e.store().tickers()[0];
    assert_eq!(
        (ticker.last, ticker.mark, ticker.index, ticker.funding_rate),
        (141.0, 140.0, 139.0, 0.001)
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn malformed_market_ticker_and_mark_rows_are_skipped_without_blocking_startup() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let markets = http(&mut e, "markets");
    reply(
        &mut e,
        &markets,
        r#"[{"symbol":"BAD_USDC_PERP","marketType":null},{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"},{"symbol":"ETH_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let mut logs = vec![];
    let mut socket = None;
    let mut requests = vec![];
    for a in e.drain_actions() {
        match a {
            Action::Http(r) => requests.push(r),
            Action::WsConnect { socket: s, .. } => socket = Some(s),
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => logs.push(message),
            _ => {}
        }
    }
    assert_eq!(e.store().markets().len(), 2);
    for r in requests {
        if r.url.contains("tickers") {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"ETH_USDC_PERP","lastPrice":null,"priceChangePercent":"0","volume":"0","quoteVolume":"0"},{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#,
            );
        } else {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"ETH_USDC_PERP","markPrice":"1","indexPrice":"1","fundingRate":null,"nextFundingTimestamp":1000},{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
            );
        }
        logs.extend(e.drain_actions().filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            Action::WsConnect { socket: s, .. } => {
                socket = Some(s);
                None
            }
            _ => None,
        }));
    }
    let socket = socket.expect("public socket starts");
    assert!(
        logs.iter()
            .any(|l| l == "Skipped 1 malformed Backpack ticker rows: ETH_USDC_PERP")
    );
    assert!(
        logs.iter()
            .any(|l| l == "Skipped 1 malformed Backpack mark price rows: ETH_USDC_PERP")
    );
    let tickers = e.store().tickers();
    assert_eq!(tickers.len(), 2);
    let eth = tickers
        .iter()
        .find(|t| t.symbol == "ETH_USDC_PERP")
        .unwrap();
    assert_eq!((eth.last, eth.mark), (0.0, 0.0));
    let sol = tickers
        .iter()
        .find(|t| t.symbol == "SOL_USDC_PERP")
        .unwrap();
    assert_eq!(sol.mark, 140.0);
    e.handle(1.0, Event::WsOpened { socket });
    let text = r#"{"stream":"ticker.ETH_USDC_PERP","data":{"s":"ETH_USDC_PERP","o":"2000","c":"2100","v":"1","V":"2100"}}"#;
    e.handle(2.0, Event::WsMessage { socket, text });
    assert_eq!(e.store().ticker("ETH_USDC_PERP").unwrap().last, 2100.0);
}

#[test]
fn rest_query_success_failure_and_stream_resubscription_have_independent_state() {
    use safe_cex_engine::Notice;
    let (mut e, socket) = seed();
    let snapshot = subscribe(&mut e);
    reply(&mut e, &snapshot, SNAP);
    e.drain_actions().for_each(drop);
    let query = |e: &mut Driver, command| {
        e.handle(
            1.0,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: "SOL_USDC_PERP".into(),
                limit: Some(1),
            }),
        );
        safe_cex_engine::test_support::prepared_actions(e, 1.0)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("depth") => Some(request),
                _ => None,
            })
            .expect("depth")
    };
    let request = query(&mut e, 77);
    delta(&mut e, socket, 11, 11);
    reply(&mut e, &request, r#"{"bids":[["900.00","7"]],"asks":[]}"#);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone {command:77,result}) if result.contains("900.00"))));
    assert_eq!(
        e.venue().book("SOL_USDC_PERP").unwrap().bids(),
        [(140.0, 5.0), (139.0, 2.0)]
    );
    let request = query(&mut e, 78);
    e.handle(
        2.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 503,
                headers: "",
                body: "unavailable",
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 78, .. })))
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::WsClose { .. }
    )));
    delta(&mut e, socket, 12, 12);
    assert!(
        e.venue().book("SOL_USDC_PERP").is_some(),
        "query failure cannot disturb stream sequence"
    );
    e.drain_actions().for_each(drop);
    let request = query(&mut e, 79);
    e.handle(
        3.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "SOL_USDC_PERP".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    let new_snapshot = subscribe(&mut e);
    reply(&mut e, &request, r#"{"bids":[],"asks":[]}"#);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 79, .. }))),
        "independent query survives subscription generation replacement"
    );
    assert!(e.venue().book("SOL_USDC_PERP").is_none());
    reply(&mut e, &new_snapshot, SNAP);
    assert_eq!(
        e.venue().book("SOL_USDC_PERP").unwrap().bids(),
        [(140.0, 1.0), (139.0, 2.0)]
    );
}
