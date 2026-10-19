//! Payload shapes from https://docs.backpack.exchange/ (markets, depth, klines).
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, Venue};
use safe_cex_venue_backpack::{Backpack, Options};

type Driver = Engine<Backpack>;
fn http(e: &mut Driver, needle: &str) -> HttpRequest {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(needle) => Some(r),
            _ => None,
        })
        .expect(needle)
}
fn reply(e: &mut Driver, r: &HttpRequest, body: &str) {
    e.handle(
        0.0,
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
    let mut e = Engine::new(Backpack::new(Options {
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
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
            );
        }
    }
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
            interval: "1m".into(),
            limit: None,
            from_ms: Some(1732182900123.0),
            to_ms: None,
        }),
    );
    let r = http(&mut e, "klines");
    assert!(r.url.contains("startTime=1732182900"));
    reply(
        &mut e,
        &r,
        r#"[{"start":"2025-12-15 13:28:00","open":"140","high":"142","low":"139","close":"141","volume":"10"}]"#,
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { result: Ok(_), .. })
    )));
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "SOL_USDC_PERP".into(),
            interval: "1m".into(),
        }),
    );
    e.take_frame(0.0);
    e.handle(1.0,Event::WsMessage { socket,text:r#"{"stream":"kline.1m.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","t":"2025-12-15T13:28:00","o":null,"h":null,"l":null,"c":null,"v":null}}"# });
    assert!(e.take_frame(1.0).is_none());
}

fn retry(e: &mut Driver) -> HttpRequest {
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
    http(e, "depth")
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
        let r = retry(&mut e);
        reply(&mut e, &r, SNAP);
    }
    assert!(e.venue().book("SOL_USDC_PERP").unwrap().is_empty());
    assert!(!e.venue().book("BTC_USDC_PERP").unwrap().is_empty());
    let errors = e
        .drain_actions()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::Log {
                    level: safe_cex_engine::LogLevel::Error,
                    ..
                })
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
        let r = retry(&mut e);
        let snapshot = SNAP.replace(
            "\"lastUpdateId\":10",
            &format!("\"lastUpdateId\":{}", 10 * i + 10),
        );
        reply(&mut e, &r, &snapshot);
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
            interval: "1m".into(),
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
fn failed_seed_reads_retry_independently_and_start_the_socket_after_recovery() {
    let mut e = Engine::new(Backpack::new(Options {
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
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let timer = actions
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2000.0, Event::TimerFired { timer });
    let marks = http(&mut e, "markPrices");
    reply(&mut e, &marks, "[]");
    assert_eq!(e.store().tickers()[0].last, 141.0);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
