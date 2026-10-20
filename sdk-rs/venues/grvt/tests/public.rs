use safe_cex_engine::{Action, Command, Engine, Event, Response, SocketChannel, SocketId, Venue};
use safe_cex_venue_grvt::{Grvt, Options};
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6},{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.01","ms":"0.01","bd":9,"qd":6}]}"#;
fn boot() -> (Engine<Grvt>, SocketId) {
    let mut e = Engine::new(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1700000000000.);
    let request = e
        .drain_actions()
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
                body: CATALOG,
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
    e.drain_actions();
    (e, socket)
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
            interval: "1m".into(),
            limit: Some(2),
            from_ms: Some(1700000000000.),
            to_ms: None,
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
    assert!(r.body.unwrap().contains("1700000000000000000"));
    e.handle(1700000000010.,Event::HttpResponse{request:r.id,response:Response{status:200,headers:"",body:r#"{"r":[{"ot":"1700000000000000000","o":"100","h":"102","l":"99","vb":"4"}]}"#}});
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone {
            command: 8,
            result: Err(_)
        })
    )));
}
#[test]
fn silent_unopened_socket_has_watchdog_and_reconnect() {
    let mut e = Engine::new(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.);
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
    for interval in ["1m", "5m"] {
        e.handle(
            1700000000010.,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTC_USDT_PERP".into(),
                interval: interval.into(),
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
    for (index, interval) in ["1m", "5m"].iter().enumerate() {
        e.handle(
            1700000000012.,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "BTC_USDT_PERP".into(),
                interval: (*interval).into(),
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
            interval: "1分钟".into(),
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone {
            command: 33,
            result: Err(_)
        })
    )));
}
