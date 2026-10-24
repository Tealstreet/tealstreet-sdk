use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, RequestId, Response, SocketChannel, SocketId,
    TimerId, Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6},{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.01","ms":"0.01","bd":9,"qd":6}]}"#;
fn boot() -> (Engine<Grvt>, SocketId) {
    let (engine, socket, _) = boot_with_catalog(CATALOG);
    (engine, socket)
}
fn boot_with_catalog(catalog: &str) -> (Engine<Grvt>, SocketId, Vec<String>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1700000000000.);
    let request = prepared_actions(&mut e, 1700000000000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        request.url,
        "https://market-data.grvt.io/lite/v1/all_instruments"
    );
    assert_eq!(request.body.as_deref(), Some("{}"));
    e.handle(
        1700000000001.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: catalog,
            },
        },
    );
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } = a
            {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1700000000002., Event::WsOpened { socket });
    let subscriptions = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    (e, socket, subscriptions)
}
fn ws(e: &mut Engine<Grvt>, s: SocketId, text: &str) {
    e.handle(1700000000010., Event::WsMessage { socket: s, text });
}
#[test]
fn catalog_and_case_are_native() {
    let (e, _) = boot();
    let m = e.store().market("BTC_USDT_PERP").unwrap();
    assert_eq!(m.id, "BTC_USDT_Perp");
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(e.store().markets().len(), 2);
}
#[test]
fn mapper_only_products_do_not_become_markets_tickers_or_book_subscriptions() {
    // GRVT_PRODUCTS_DECISION.md retires these latent TS mappings; their
    // presence in an unexpected response must not enable a new product.
    let catalog = r#"{"r":[
        {"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001"},
        {"i":"BTC_USDT_Fut_20Oct23","ih":"0x030502","b":"BTC","q":"USDT","k":"FUTURE","ts":"0.1","ms":"0.001"},
        {"instrument":"BTC_USDT_Call_20Oct23_2800","instrument_hash":"0x030503","base":"BTC","quote":"USDT","kind":"CALL","tick_size":"0.1","min_size":"0.001"},
        {"i":"BTC_USDT_Put_20Oct23_2800","ih":"0x030504","b":"BTC","q":"USDT","k":"PUT","ts":"0.1","ms":"0.001"}
    ]}"#;
    let (mut e, socket, subscriptions) = boot_with_catalog(catalog);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(subscriptions.len(), 1);
    assert!(subscriptions[0].contains("BTC_USDT_Perp@500"));
    assert!(!subscriptions[0].contains("Fut_"));
    assert!(!subscriptions[0].contains("Call_"));
    assert!(!subscriptions[0].contains("Put_"));
    for id in [
        "BTC_USDT_Fut_20Oct23",
        "BTC_USDT_Call_20Oct23_2800",
        "BTC_USDT_Put_20Oct23_2800",
    ] {
        assert!(e.store().market_by_id(id).is_none());
        let symbol = id.to_ascii_uppercase();
        e.handle(
            1700000000003.,
            Event::Command(Command::SubscribeOrderBook { symbol }),
        );
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::WsSend { .. }))
        );
        ws(
            &mut e,
            socket,
            &format!(r#"{{"s":"v1.ticker.d","f":{{"i":"{id}","lp":"999"}}}}"#),
        );
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"s":"v1.book.s","f":{{"i":"{id}","b":[{{"p":"999","s":"1"}}],"a":[]}}}}"#
            ),
        );
    }
    assert_eq!(e.store().ticker("BTC_USDT_PERP").unwrap().last, 0.);
    let subscriptions = e.venue().books().collect::<Vec<_>>();
    assert_eq!(subscriptions.len(), 3);
    for (symbol, book) in subscriptions {
        assert!(e.store().market(symbol).is_none());
        assert!(e.venue().book(symbol).is_none());
        assert!(book.bids().is_empty());
        assert!(book.asks().is_empty());
    }
}
#[test]
fn delta_fields_retain_sides_and_derive_totals_from_current_price() {
    let (mut e, s) = boot();
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","lp":"100","op":"80","bv":"2","sv":"3","fr2":"0.01","nf":"1700000400000000000"}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"1","f":{"i":"BTC_USDT_Perp","bv":"4"}}"#,
    );
    let t = e.store().ticker("BTC_USDT_PERP").unwrap();
    assert_eq!((t.volume, t.quote_volume, t.percentage), (7., 700., 25.));
    assert_eq!(t.funding_rate, 0.0001);
    assert_eq!(t.funding_time, 1700000400.);
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"2","f":{"i":"BTC_USDT_Perp","lp":"120"}}"#,
    );
    let t = e.store().ticker("BTC_USDT_PERP").unwrap();
    assert_eq!((t.volume, t.quote_volume, t.percentage), (7., 840., 50.));
}
#[test]
fn ticker_gap_closes_socket_and_duplicate_is_not_reapplied() {
    let (mut e, s) = boot();
    ws(
        &mut e,
        s,
        r#"{"result":{"stream":"v1.ticker.d","subs":["BTC_USDT_Perp@500"],"first_sequence_number":["514137"]}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","lp":"100"}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"514137","f":{"i":"BTC_USDT_Perp","lp":"101"}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"514137","f":{"i":"BTC_USDT_Perp","lp":"99"}}"#,
    );
    assert_eq!(e.store().ticker("BTC_USDT_PERP").unwrap().last, 101.);
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"514139","f":{"i":"BTC_USDT_Perp","lp":"98"}}"#,
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket} if socket==s))
    );
    assert_eq!(e.store().ticker("BTC_USDT_PERP").unwrap().last, 101.);
}
#[test]
fn full_book_replaces_previous_levels_and_invalid_keeps_good() {
    let (mut e, s) = boot();
    e.handle(
        1700000000003.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC_USDT_PERP".into(),
        }),
    );
    ws(
        &mut e,
        s,
        r#"{"stream":"v1.book.s","selector":"BTC_USDT_Perp","sequence_number":"0","feed":{"instrument":"BTC_USDT_Perp","bids":[{"price":"100","size":"2"},{"price":"99","size":"3"}],"asks":[{"price":"101","size":"1"}]}}"#,
    );
    assert_eq!(
        e.venue().book("BTC_USDT_PERP").unwrap().bids(),
        [(100., 2.), (99., 3.)]
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.book.s","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","b":[{"p":"98","s":"4"}],"a":[]}}"#,
    );
    assert_eq!(e.venue().book("BTC_USDT_PERP").unwrap().bids(), [(98., 4.)]);
    assert!(e.venue().book("BTC_USDT_PERP").unwrap().asks().is_empty());
    ws(
        &mut e,
        s,
        r#"{"s":"v1.book.s","f":{"i":"BTC_USDT_Perp","b":[{"p":"98","s":"-1"}],"a":[]}}"#,
    );
    assert_eq!(e.venue().book("BTC_USDT_PERP").unwrap().bids(), [(98., 4.)]);
}
#[test]
fn candle_requires_complete_valid_ohlc_and_nanosecond_time() {
    let (mut e, _) = boot();
    e.handle(
        1700000000003.,
        Event::Command(Command::FetchCandles {
            command: 8,
            symbol: "BTC_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: Some(1700000000000.),
            to_ms: None,
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 1700000000003.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(r.body.unwrap().contains("1700000000000000000"));
    e.handle(1700000000010.,Event::HttpResponse{request:r.id,response:Response{status:200,headers:"",body:r#"{"r":[{"ot":"1700000000000000000","o":"100","h":"102","l":"99","vb":"4"}]}"#}});
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 8, .. })
    )));
}
#[test]
fn silent_unopened_socket_has_watchdog_and_reconnect() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.);
    let r = prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        1001.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
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
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == 6001.).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(22000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket:s}if s==socket))
    );
}
#[test]
fn ticker_empty_string_clears_only_that_field() {
    let (mut e, s) = boot();
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","lp":"100","oi":"5"}}"#,
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"31337","f":{"i":"BTC_USDT_Perp","oi":"","bb":"99"}}"#,
    );
    let t = e.store().ticker("BTC_USDT_PERP").unwrap();
    assert_eq!((t.last, t.open_interest, t.bid), (100., 0., 99.));
}

#[test]
fn simultaneous_candle_intervals_use_independent_feed_sockets_and_dispose() {
    let (mut e, _) = boot();
    let mut sockets = vec![];
    for interval in [Interval::M1, Interval::M5] {
        e.handle(
            1700000000010.,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTC_USDT_PERP".into(),
                interval,
            }),
        );
        let socket = e
            .drain_actions()
            .find_map(|a| {
                if let Action::WsConnect {
                    socket,
                    context: Some(ctx),
                    channel: Some(SocketChannel::Public),
                    ..
                } = a
                {
                    assert!(ctx.feed.starts_with("candles-"));
                    Some(socket)
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(1700000000011., Event::WsOpened { socket });
        assert!(e.drain_actions().any(
            |a| matches!(a,Action::WsSend{socket:s,text}if s==socket && text.contains("v1.candle"))
        ));
        sockets.push(socket);
    }
    assert_ne!(sockets[0], sockets[1]);
    for (index, interval) in [Interval::M1, Interval::M5].iter().enumerate() {
        e.handle(
            1700000000012.,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "BTC_USDT_PERP".into(),
                interval: *interval,
            }),
        );
        assert!(
            e.drain_actions()
                .any(|a| matches!(a,Action::WsClose{socket}if socket==sockets[index]))
        );
    }
}
#[test]
fn unsubscribed_book_is_not_exposed_on_new_subscription() {
    let (mut e, s) = boot();
    e.handle(
        1700000000003.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC_USDT_PERP".into(),
        }),
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.book.s","f":{"i":"BTC_USDT_Perp","b":[{"p":"100","s":"1"}],"a":[]}}"#,
    );
    assert!(e.venue().book("BTC_USDT_PERP").is_some());
    e.handle(
        1700000000020.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC_USDT_PERP".into(),
        }),
    );
    e.handle(
        1700000000030.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC_USDT_PERP".into(),
        }),
    );
    assert!(e.venue().book("BTC_USDT_PERP").is_none());
}

#[test]
fn unsupported_unicode_candle_interval_fails_without_trapping() {
    let (mut e, _) = boot();
    e.handle(
        1700000000003.,
        Event::Command(Command::FetchCandles {
            command: 33,
            symbol: "BTC_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::S1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 33, .. })
    )));
}

#[test]
fn catalog_refresh_remaps_ticker_state_sequences_and_subscriptions_by_native_id() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.);
    let request = prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    e.handle(
        1001.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let refresh = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 301001.,
            } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1002., Event::WsOpened { socket });
    e.drain_actions();
    ws(
        &mut e,
        socket,
        r#"{"r":{"s":"v1.ticker.d","s1":["BTC_USDT_Perp@500","ETH_USDT_Perp@500"],"fs":["10","20"]}}"#,
    );
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"10","f":{"i":"BTC_USDT_Perp","lp":"100","op":"80","bv":"2","sv":"3"}}"#,
    );
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"ETH_USDT_Perp","sn":"0","f":{"i":"ETH_USDT_Perp","lp":"200","op":"100","bv":"10","sv":"4"}}"#,
    );
    e.handle(301001., Event::TimerFired { timer: refresh });
    let request = prepared_actions(&mut e, 301001.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    let catalog = r#"{"r":[{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.01","ms":"0.01"},{"i":"SOL_USDT_Perp","ih":"0x030503","b":"SOL","q":"USDT","k":"PERPETUAL","ts":"0.01","ms":"0.01"},{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001"}]}"#;
    e.handle(
        301002.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: catalog,
            },
        },
    );
    let subscriptions: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } if text.contains("v1.ticker.d") => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(subscriptions.len(), 1);
    assert!(subscriptions[0].contains("SOL_USDT_Perp@500"));
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"11","f":{"i":"BTC_USDT_Perp","lp":"120"}}"#,
    );
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"ETH_USDT_Perp","sn":"20","f":{"i":"ETH_USDT_Perp","lp":"220"}}"#,
    );
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"SOL_USDT_Perp","sn":"1","f":{"i":"SOL_USDT_Perp","lp":"1","bv":"4"}}"#,
    );
    for (symbol, last, volume, percentage) in [
        ("BTC_USDT_PERP", 120., 5., 50.),
        ("ETH_USDT_PERP", 220., 14., 120.),
        ("SOL_USDT_PERP", 1., 4., 0.),
    ] {
        let ticker = e.store().ticker(symbol).unwrap();
        assert_eq!(
            (ticker.last, ticker.volume, ticker.percentage),
            (last, volume, percentage)
        );
    }
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
    ws(
        &mut e,
        socket,
        r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"13","f":{"i":"BTC_USDT_Perp","lp":"999"}}"#,
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
    assert_eq!(e.store().ticker("BTC_USDT_PERP").unwrap().last, 120.);
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn public_supervisor_bounds_handshake_and_backs_off_flapping_connections() {
    let (e, socket) = boot();
    socket_checks::flaps(e, socket, 1_700_000_000_100.);
}

#[test]
fn candle_supervisor_retries_flaps_independently_and_stops_when_unsubscribed() {
    let (mut e, _) = boot();
    let now = 1_700_000_000_100.;
    e.handle(
        now,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(now, Event::WsOpened { socket });
    socket_checks::flaps(e, socket, now);

    let (mut e, _) = boot();
    e.handle(
        now,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(now, Event::WsClosed { socket, code: 1006 });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        now,
        Event::Command(Command::SubscribeCandles {
            symbol: "ETH_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "new interest cannot bypass backoff"
    );
    e.handle(
        now,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "ETH_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTC_USDT_PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::ClearTimer { timer: t } if t == timer))
    );
    e.handle(now + 1000., Event::TimerFired { timer });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsSend { .. }))
    );
}

#[derive(Default)]
struct Suppressed {
    requests: Vec<RequestId>,
    timers: Vec<(TimerId, f64)>,
    connects: usize,
    closes: usize,
}

impl Suppressed {
    fn collect(&mut self, e: &mut Engine<Grvt>, now: f64) {
        for action in prepared_actions(e, now) {
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.set_public_feed_policy(PublicFeedPolicy::Shared).unwrap();
    let start = 1700000000000.;
    e.start(start);
    let mut h = Suppressed::default();
    h.collect(&mut e, start);
    let request = h.requests.remove(0);
    let response = Response {
        status: 200,
        headers: "",
        body: CATALOG,
    };
    e.handle(start + 1., Event::HttpResponse { request, response });
    h.collect(&mut e, start + 1.);
    assert_eq!(e.store().markets().len(), 2, "public startup completed");
    let mut fired = 0;
    while let Some(&(timer, at)) = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= start + 600_000.)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        h.collect(&mut e, at);
        while let Some(request) = h.requests.pop() {
            e.handle(
                at,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: CATALOG,
                    },
                },
            );
            h.collect(&mut e, at);
        }
        fired += 1;
    }
    assert_eq!(
        (h.connects, h.closes),
        (0, 0),
        "no public connects or closes"
    );
    assert_eq!(fired, 1, "only the serviced catalog refresh fires");
}

#[test]
fn rest_queries_do_not_publish_or_repair_live_book_or_account_state() {
    let (mut e, s) = boot();
    let symbol = "BTC_USDT_PERP";
    e.handle(
        1700000000003.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }),
    );
    ws(
        &mut e,
        s,
        r#"{"s":"v1.book.s","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","b":[{"p":"99","s":"4"}],"a":[]}}"#,
    );
    e.drain_actions().for_each(drop);
    let before = (
        e.store().tickers().to_vec(),
        e.store().balance().clone(),
        e.store().positions().to_vec(),
        e.store().orders().to_vec(),
    );
    for (command, status, body) in [
        (7, 200, r#"{"r":{"b":[{"p":"1","s":"2"}],"a":[]}}"#),
        (8, 200, r#"{"c":1000,"m":"invalid session"}"#),
    ] {
        e.handle(
            1700000000011.0,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: symbol.into(),
                limit: None,
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1700000000011.)
            .into_iter()
            .filter(|action| !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
            .collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        let Action::Http(request) = &actions[0] else {
            panic!("public REST query")
        };
        e.handle(
            1700000000012.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "Set-Cookie: gravity=foreign; Path=/\nX-Grvt-Account-Id: foreign",
                    body,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        if command == 7 {
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone {command:7,result}) if serde_json::from_str::<serde_json::Value>(result).unwrap()["bids"][0]["price"]==1.0)));
        } else {
            assert!(actions.iter().any(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 8, .. })
            )));
        }
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::WsConnect { .. }
                | Action::WsClose { .. }
                | Action::SetTimer { .. }
                | Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
        )));
        assert_eq!(e.venue().book(symbol).unwrap().bids(), [(99.0, 4.0)]);
        assert_eq!(
            (
                e.store().tickers().to_vec(),
                e.store().balance().clone(),
                e.store().positions().to_vec(),
                e.store().orders().to_vec()
            ),
            before
        );
    }
    e.handle(
        1700000000013.0,
        Event::Command(Command::FetchOrderBook {
            command: 9,
            symbol: symbol.into(),
            limit: None,
        }),
    );
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1700000000013.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        1700000000014.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: symbol.into(),
        }),
    );
    e.handle(
        1700000000015.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1700000000016.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"r":{"b":[{"p":"1","s":"2"}],"a":[]}}"#,
            },
        },
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { command: 9, .. })
    )));
    assert!(e.venue().book(symbol).is_none());
    ws(
        &mut e,
        s,
        r#"{"s":"v1.book.s","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","b":[{"p":"98","s":"5"}],"a":[]}}"#,
    );
    assert_eq!(e.venue().book(symbol).unwrap().bids(), [(98.0, 5.0)]);
}
