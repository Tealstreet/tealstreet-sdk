mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Venue};
use safe_cex_venue_woo::{Options, Woo};
use support::*;
fn subscribe(e: &mut TestEngine) {
    e.drain_actions().for_each(drop);
    e.handle(
        40.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
}
fn result(e: &mut TestEngine) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap()
}
const UPDATE: &str = r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":102,"data":{"bids":[[59000,4],[58000,5]],"asks":[[61000,0]]}}"#;

#[test]
fn seed_uses_native_perpetual_units_and_skips_irrelevant_rows() {
    let (e, _) = started();
    let m = e.store().market("BTCUSDT").unwrap();
    assert_eq!(m.id, "BTC/USDT:USDT");
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(m.limits.leverage.max, 50.0);
    assert_eq!(e.store().markets().len(), 1);
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!((t.bid, t.ask, t.last), (60000.0, 60000.0, 60000.0));
    assert_eq!(t.percentage, 20.0);
    assert_eq!(t.open_interest, 10000.0);
    assert_eq!(e.venue().price_range("BTCUSDT"), Some(0.01));
}
#[test]
fn public_topics_and_control_frames_cannot_dispatch_payloads() {
    let (mut e, s) = started();
    let sent: Vec<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sent.len(), 4);
    ws(
        &mut e,
        s,
        40.0,
        r#"{"event":"subscribe","topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":1}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":"60002","volume":5,"amount":300010}]}"#,
    );
    ws(
        &mut e,
        s,
        60.0,
        r#"{"topic":"bbos","data":[{"symbol":"PERP_BTC_USDT","bid":60000,"ask":60001}]}"#,
    );
    ws(
        &mut e,
        s,
        70.0,
        r#"{"topic":"markprices","data":[{"symbol":"PERP_BTC_USDT","price":60003}]}"#,
    );
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (t.last, t.bid, t.ask, t.mark),
        (60002.0, 60000.0, 60001.0, 60003.0)
    );
    assert_eq!(t.percentage, 20.0);
    ws(&mut e, s, 80.0, r#"{"event":"ping"}"#);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..}if text==r#"{"event":"pong"}"#))
    );
}
#[test]
fn pre_seed_book_subscription_survives_until_socket_open() {
    let mut e = Engine::new(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    e.handle(
        1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    response(&mut e, &r, 10.0, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 20.0, TICKERS);
    let s = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(30.0, Event::WsOpened { socket: s });
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/orderbook/PERP_BTC_USDT"));
}
#[test]
fn buffers_before_snapshot_discards_old_and_preserves_full_depth() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = requests(&mut e).remove(0);
    ws(&mut e, s, 50.0, &UPDATE.replace("102", "99"));
    ws(&mut e, s, 60.0, UPDATE);
    snapshot(&mut e, &r, 70.0, 100);
    let b = e.venue().book("BTCUSDT").unwrap();
    assert_eq!(b.bids(), &[(59000.0, 4.0), (58000.0, 5.0)]);
    assert!(b.asks().is_empty());
    ws(
        &mut e,
        s,
        80.0,
        &UPDATE.replace("102", "98").replace("59000,4", "59000,1"),
    );
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids()[0].1, 4.0);
}

#[test]
fn overflowing_snapshot_buffer_invalidates_the_old_response() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let old = requests(&mut e).remove(0);
    for n in 0..2001 {
        ws(&mut e, s, 50.0 + n as f64, UPDATE);
    }
    snapshot(&mut e, &old, 3000.0, 100);
    assert!(e.venue().book("BTCUSDT").is_none());
}

#[test]
fn streamed_candles_keep_open_and_monotonic_time() {
    let (mut e, s) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        40.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    let frame = r#"{"topic":"PERP_BTC_USDT@kline_1m","ts":1700000000000,"data":{"startTime":1700000000000,"open":10,"high":12,"low":9,"close":11,"volume":3}}"#;
    ws(&mut e, s, 50.0, frame);
    ws(
        &mut e,
        s,
        60.0,
        &frame
            .replace("\"open\":10", "\"open\":11")
            .replace("\"high\":12", "\"high\":11")
            .replace("\"low\":9", "\"low\":10"),
    );
    ws(
        &mut e,
        s,
        70.0,
        &frame.replace("\"startTime\":1700000000000", "\"startTime\":1699999940000"),
    );
    let bytes = e.take_frame(80.0).unwrap();
    let (_, payload) = safe_cex_engine::wire::sections(bytes)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap();
    assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
    let values: Vec<_> = payload[16..]
        .chunks_exact(8)
        .map(|v| f64::from_le_bytes(v.try_into().unwrap()))
        .collect();
    assert_eq!(values, vec![1700000000.0, 10.0, 12.0, 9.0, 11.0, 3.0]);
}
#[test]
fn snapshot_keeps_more_than_one_thousand_levels() {
    let (mut e, _) = started();
    subscribe(&mut e);
    let r = requests(&mut e).remove(0);
    let bids = (1..=1501)
        .rev()
        .map(|p| format!(r#"{{"price":{p},"quantity":1}}"#))
        .collect::<Vec<_>>()
        .join(",");
    response(
        &mut e,
        &r,
        60.0,
        &format!(r#"{{"success":true,"timestamp":100,"bids":[{bids}],"asks":[]}}"#),
    );
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids().len(), 1501);
}
#[test]
fn malformed_buffer_invalidates_in_flight_snapshot_and_resyncs() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let old = requests(&mut e).remove(0);
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":101,"data":{"bids":[[59000,9]],"asks":[[61000,-1]]}}"#,
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
    snapshot(&mut e, &old, 60.0, 100);
    assert!(e.venue().book("BTCUSDT").is_none());
    e.handle(1050.0, Event::TimerFired { timer });
    let new = requests(&mut e).remove(0);
    snapshot(&mut e, &new, 1060.0, 110);
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids(), &[(59000.0, 2.0)]);
}
#[test]
fn live_malformed_frame_never_commits_its_valid_prefix() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = requests(&mut e).remove(0);
    snapshot(&mut e, &r, 50.0, 100);
    ws(
        &mut e,
        s,
        60.0,
        r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":101,"data":{"bids":[[59000,9],[58000,"bad"]],"asks":[]}}"#,
    );
    assert!(e.venue().book("BTCUSDT").is_none());
}
#[test]
fn reference_counts_and_generations_reject_unsubscribed_snapshot() {
    let (mut e, _) = started();
    subscribe(&mut e);
    let old = requests(&mut e).remove(0);
    subscribe(&mut e);
    assert!(requests(&mut e).is_empty());
    for n in 0..2 {
        e.handle(
            50.0 + n as f64,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            }),
        );
    }
    subscribe(&mut e);
    let new = requests(&mut e).remove(0);
    snapshot(&mut e, &old, 60.0, 200);
    assert!(e.venue().book("BTCUSDT").is_none());
    snapshot(&mut e, &new, 70.0, 100);
    assert!(e.venue().book("BTCUSDT").is_some());
}
#[test]
fn funding_waits_sixty_seconds_and_failed_reads_preserve_seed() {
    let mut e = Engine::new(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 20.0, TICKERS);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == 60020.0).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(60020.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        60030.0,
        r#"{"success":false,"rows":[{"symbol":"PERP_BTC_USDT","est_funding_rate":1,"next_funding_time":1700028800000}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().funding_rate, 0.0001);
}
#[test]
fn failed_market_page_cannot_poison_price_caps_after_retry() {
    let mut e = Engine::new(Woo::new(Options::default()));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        10.0,
        &MARKETS
            .replace("\"success\":true", "\"success\":false")
            .replace("0.01}", "0.9}"),
    );
    assert_eq!(e.venue().price_range("BTCUSDT"), None);
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
    e.handle(1010.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        1020.0,
        &MARKETS.replace(",\"price_range\":0.01", ""),
    );
    assert_eq!(e.venue().price_range("BTCUSDT"), None);
    assert_eq!(e.store().markets().len(), 1);
}
#[test]
fn unicode_and_invalid_candle_inputs_do_not_panic_or_write() {
    for interval in ["日", "", "0m", "NaNm"] {
        let (mut e, _) = started();
        e.drain_actions().for_each(drop);
        e.handle(
            40.0,
            Event::Command(Command::FetchCandles {
                command: 9,
                symbol: "BTCUSDT".into(),
                interval: interval.into(),
                limit: None,
                from_ms: None,
                to_ms: None,
            }),
        );
        assert!(result(&mut e).is_err());
    }
}
#[test]
fn candle_ranges_use_history_shape_and_millisecond_to_seconds() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        1700000000000.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            limit: Some(2),
            from_ms: Some(1600000000000.0),
            to_ms: Some(1600000120000.0),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.starts_with("https://api-pub.woox.io/v1/hist/kline"));
    assert!(r.url.contains("start_time=1600000000000"));
    response(
        &mut e,
        &r,
        1700000000100.0,
        r#"{"success":true,"data":{"rows":[{"start_timestamp":1600000060000,"open":10,"high":12,"low":9,"close":11,"volume":3},{"start_timestamp":1600000000000,"open":9,"high":10,"low":8,"close":10,"volume":2}]}}"#,
    );
    let r = result(&mut e).unwrap();
    assert!(r.starts_with(r#"[{"timestamp":1600000000.0"#));
    assert_eq!(r.matches("timestamp").count(), 2);
}
#[test]
fn silence_closes_socket_then_reconnects_with_fresh_book_generation() {
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
    subscribe(&mut e);
    let old = requests(&mut e).remove(0);
    e.handle(25000.0, Event::TimerFired { timer: watchdog });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s))
    );
    let retry = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(26000.0, Event::TimerFired { timer: retry });
    let new = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(26010.0, Event::WsOpened { socket: new });
    let r = requests(&mut e).remove(0);
    snapshot(&mut e, &old, 26020.0, 300);
    assert!(e.venue().book("BTCUSDT").is_none());
    snapshot(&mut e, &r, 26030.0, 200);
    assert!(e.venue().book("BTCUSDT").is_some());
}
