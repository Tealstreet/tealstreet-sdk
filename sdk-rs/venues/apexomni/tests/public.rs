mod support;
use safe_cex_engine::{Action, Command, Event, Notice, SocketChannel};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::*;

#[test]
fn streams_nested_symbols_and_seeds_zero_tickers_before_socket() {
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    assert!(r.url.ends_with("/v3/symbols"));
    e.handle(
        1.0,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    for part in MARKETS.as_bytes().chunks(7) {
        e.handle(
            2.0,
            Event::HttpChunk {
                request: r.id,
                data: part,
            },
        );
    }
    assert!(e.store().markets().is_empty());
    e.handle(3.0, Event::HttpEnd { request: r.id });
    let m = &e.store().markets()[0];
    assert_eq!(m.id, "BTC-USDT");
    assert_eq!(m.symbol, "BTCUSDT");
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(m.limits.leverage.max, 100.0);
    assert_eq!(e.store().tickers()[0].last, 0.0);
    assert!(e.prepared(3.0).any(|a|matches!(a,Action::WsConnect{channel:Some(SocketChannel::Public),url,..} if url.starts_with("wss://quote.omni.apex.exchange/realtime_public?v=2&timestamp=3&"))));
}
#[test]
fn error_seed_does_not_commit_partial_markets() {
    let body = MARKETS.replace("\"code\":0", "\"code\":20016");
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 1.0, &body);
    assert!(e.store().markets().is_empty());
    assert!(
        !e.prepared(1.0)
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn instrument_batch_maps_native_id_mark_fallback_and_signed_funding() {
    let (mut e, s) = started();
    ws(
        &mut e,
        s,
        100.0,
        r#"{"topic":"instrumentInfo.all","data":[{"s":"UNKNOWN","p":"123"},{"s":"BTC-USDT","p":"60000","xp":"60001","pr":"0.0123","o":"20","fr":"-0.0002","v":"4","to":"240000"}]}"#,
    );
    let t = &e.store().tickers()[0];
    assert_eq!((t.last, t.mark, t.index), (60000.0, 60001.0, 60001.0));
    assert_eq!(t.percentage, 1.23);
    assert_eq!(t.funding_rate, -0.0002);
    assert_eq!(t.quote_volume, 240000.0);
}
#[test]
fn depth_snapshot_delta_keeps_full_sorted_book_and_zero_deletes() {
    let (mut e, s) = started();
    e.handle(
        30.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    ws(
        &mut e,
        s,
        40.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"a":[["60003","3"],["60002","2"]],"b":[["59998","2"],["59999","1"]],"u":10}}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        [(59999.0, 1.0), (59998.0, 2.0)]
    );
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"a":[["60002","0"],["60001","4"]],"b":[["59999","5"]],"u":11}}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().asks(),
        [(60001.0, 4.0), (60003.0, 3.0)]
    );
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(59999.0));
}
#[test]
fn malformed_depth_is_atomic_and_unknown_topics_are_ignored() {
    let (mut e, s) = started();
    e.handle(
        30.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    ws(
        &mut e,
        s,
        40.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"u":1,"a":[["2","1"]],"b":[["1","2"]]}}"#,
    );
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"u":2,"a":[["3","2"]],"b":[["NaN","1"]]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().best_ask(), Some(2.0));
}
#[test]
fn subscriptions_are_reference_counted_and_reconnect_uses_real_testnet_public_role() {
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let s = e
        .prepared(10.0)
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                url,
                channel,
                ..
            } = a
            {
                assert!(url.starts_with("wss://qa-quote.omni.apex.exchange/"));
                assert_eq!(channel, Some(SocketChannel::Public));
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(20.0, Event::WsOpened { socket: s });
    e.prepared(20.0).for_each(drop);
    for _ in 0..2 {
        e.handle(
            30.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            }),
        );
    }
    assert_eq!(
        e.prepared(30.0)
            .filter(|a| matches!(a,Action::WsSend{text,..} if text.contains("orderBook200")))
            .count(),
        1
    );
    e.handle(
        40.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(e.book("BTCUSDT").is_some());
    e.handle(
        50.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let t = e
        .prepared(50.0)
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1050.0, Event::TimerFired { timer: t });
    assert!(e.prepared(1050.0).any(|a|matches!(a,Action::WsConnect{channel:Some(SocketChannel::Public),url,..} if url.contains("timestamp=1050"))));
}
#[test]
fn socket_ping_pong_and_watchdog_close_dead_feed() {
    let (mut e, s) = started();
    let watchdog = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    ws(&mut e, s, 100.0, r#"{"op":"ping"}"#);
    assert!(
        e.prepared(100.0).any(
            |a| matches!(a,Action::WsSend{text,..} if text==r#"{"op":"pong","args":["100"]}"#)
        )
    );
    e.handle(30100.0, Event::TimerFired { timer: watchdog });
    assert!(
        e.prepared(30100.0)
            .any(|a| matches!(a,Action::WsClose{socket} if socket==s))
    );
}
#[test]
fn fetch_candles_uses_seconds_bounds_daily_omission_and_sorted_results() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        1_700_000_000_000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::D1,
            limit: Some(2),
            from_ms: Some(0.0),
            to_ms: Some(1_700_000_000_000.0),
        }),
    );
    let r = prepared_requests(&mut e, 1_700_000_000_000.0).remove(0);
    assert!(r.headers.is_empty());
    assert!(r.url.contains("interval=D"));
    assert!(r.url.contains("start=1699827200"));
    assert!(!r.url.contains("end="));
    response(
        &mut e,
        &r,
        1_700_000_000_001.0,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":2000,"o":"2","h":"4","l":"1","c":"3","v":"7"},{"t":1000,"o":"1","h":"3","l":"1","c":"2","v":"5"}]}}"#,
    );
    let result = e
        .prepared(1_700_000_000_001.0)
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap();
    assert!(result.starts_with("[{\"timestamp\":1.0,"));
}
#[test]
fn candle_stream_rejects_backward_snapshots() {
    let (mut e, s) = started();
    e.handle(
        30.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    ws(
        &mut e,
        s,
        40.0,
        r#"{"topic":"candle.1.BTCUSDT","data":[{"start":120000,"open":"2","high":"4","low":"1","close":"3","volume":"5"}]}"#,
    );
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"candle.1.BTCUSDT","data":[{"start":60000,"open":"2","high":"4","low":"1","close":"2","volume":"4"}]}"#,
    );
    let frame = e.take_frame(60.0).unwrap();
    let (_, payload) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap();
    assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
    assert_eq!(
        f64::from_le_bytes(payload[16..24].try_into().unwrap()),
        120.0
    );
}

#[test]
fn depth_gap_recovers_snapshot_and_ignores_duplicates_and_pre_snapshot_deltas() {
    let (mut e, s) = started();
    e.handle(
        30.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    ws(
        &mut e,
        s,
        31.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"u":9,"a":[],"b":[["1","1"]]}}"#,
    );
    assert!(e.book("BTCUSDT").unwrap().is_empty());
    ws(
        &mut e,
        s,
        32.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"u":10,"a":[],"b":[["1","2"]]}}"#,
    );
    ws(
        &mut e,
        s,
        33.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"u":10,"a":[],"b":[["1","99"]]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(1.0, 2.0)]);
    e.prepared(33.0).for_each(drop);
    ws(
        &mut e,
        s,
        34.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"u":12,"a":[],"b":[["1","99"]]}}"#,
    );
    assert!(e.book("BTCUSDT").unwrap().is_empty());
    let sends: Vec<_> = e
        .prepared(34.0)
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        sends,
        [
            r#"{"op":"unsubscribe","args":["orderBook200.H.BTCUSDT"]}"#,
            r#"{"op":"subscribe","args":["orderBook200.H.BTCUSDT"]}"#
        ]
    );
    ws(
        &mut e,
        s,
        35.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"u":13,"a":[],"b":[["1","99"]]}}"#,
    );
    assert!(e.book("BTCUSDT").unwrap().is_empty());
    ws(
        &mut e,
        s,
        36.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"u":1,"a":[],"b":[["2","3"]]}}"#,
    );
    ws(
        &mut e,
        s,
        37.0,
        r#"{"topic":"orderBook200.H.BTCUSDT","type":"delta","data":{"u":2,"a":[],"b":[["2","4"]]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(2.0, 4.0)]);
}

#[test]
fn secondary_trade_uses_decimal_quote_conversion_and_rollover_without_backflow() {
    // A credential-less exchange still has the public-only guard internally;
    // the option controls whether recent trades supplement subscribed candles.
    let mut other = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options::default()));
    other.start(0.0);
    let request = requests(&mut other, 0.0).remove(0);
    response(&mut other, &request, 10.0, MARKETS);
    let socket = other
        .prepared(10.0)
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
    other.handle(20.0, Event::WsOpened { socket });
    other.handle(
        21.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    ws(
        &mut other,
        socket,
        22.0,
        r#"{"topic":"candle.1.BTCUSDT","data":[{"start":60000,"open":"2","high":"2","low":"2","close":"2","volume":"0"}]}"#,
    );
    ws(
        &mut other,
        socket,
        23.0,
        r#"{"topic":"recentlyTrade.H.BTCUSDT","data":[{"T":61000,"p":"3","v":"1"}]}"#,
    );
    let frame = other.take_frame(24.0).unwrap();
    let (_, data) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap();
    assert_eq!(
        f64::from_le_bytes(data[56..64].try_into().unwrap()),
        0.33333333
    );
    ws(
        &mut other,
        socket,
        24.5,
        r#"{"topic":"recentlyTrade.H.BTCUSDT","data":[{"T":61000,"p":"3","v":"1.000000000"}]}"#,
    );
    let frame = other.take_frame(24.6).unwrap();
    let (_, data) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap();
    assert_eq!(
        f64::from_le_bytes(data[56..64].try_into().unwrap()),
        0.666666663
    );
    ws(
        &mut other,
        socket,
        25.0,
        r#"{"topic":"recentlyTrade.H.BTCUSDT","data":[{"T":121000,"p":"4","v":"2"}]}"#,
    );
    ws(
        &mut other,
        socket,
        26.0,
        r#"{"topic":"recentlyTrade.H.BTCUSDT","data":[{"T":61000,"p":"100","v":"100"}]}"#,
    );
    let frame = other.take_frame(27.0).unwrap();
    let (_, data) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap();
    let values: Vec<_> = data[16..64]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| f64::from_le_bytes(*b))
        .collect();
    assert_eq!(values, [120.0, 3.0, 4.0, 3.0, 4.0, 0.5]);
}

fn candle_query_actions(body: &str) -> Vec<Action> {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        100.0,
        Event::Command(Command::FetchCandles {
            command: 88,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    let request = prepared_requests(&mut e, 100.0).remove(0);
    response(&mut e, &request, 101.0, body);
    e.prepared(101.0).collect()
}

#[test]
fn rest_candles_isolate_zero_price_with_coded_warning_and_keep_sorted_peers() {
    let actions = candle_query_actions(
        r#"{"code":0,"data":{"BTCUSDT":[{"t":3000,"o":"2","h":"4","l":"1","c":"3","v":"7"},{"t":2000,"o":"0","h":"0","l":"0","c":"0","v":"0"},{"t":1000,"o":"1","h":"3","l":"1","c":"2","v":"5"}]}}"#,
    );
    let result = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 88,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("valid peers survive");
    let rows: serde_json::Value = serde_json::from_str(result).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["timestamp"], 1.0);
    assert_eq!(rows[1]["timestamp"], 3.0);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::ROW_SKIPPED)));
}

#[test]
fn rest_candles_keep_valid_empty_and_fail_unusable_envelopes_or_timestamps() {
    let empty = candle_query_actions(r#"{"code":0,"data":{"BTCUSDT":[]}}"#);
    assert!(empty.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command:88,result }) if result == "[]")
    ));
    for body in [
        r#"{"code":0,"data":{"BTCUSDT":[{"t":1000,"o":"0","h":"0","l":"0","c":"0","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":"bad","o":"1","h":"1","l":"1","c":"1","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":"","o":"1","h":"1","l":"1","c":"1","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":-1,"o":"1","h":"1","l":"1","c":"1","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":1.5,"o":"1","h":"1","l":"1","c":"1","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":[{"t":1000,"o":"bad","h":"1","l":"1","c":"1","v":"0"}]}}"#,
        r#"{"code":0,"data":{"BTCUSDT":{}}}"#,
        r#"{"code":0,"data":{}}"#,
    ] {
        let actions = candle_query_actions(body);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 88, .. }))),
            "{body}"
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 88, .. })))
        );
    }
}
