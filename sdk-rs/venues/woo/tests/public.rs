mod support;
use safe_cex_engine::{Action, Command, Event, Notice, SocketChannel, Venue};
use safe_cex_venue_woo::{Options, Woo};
use support::*;
fn subscribe(e: &mut TestEngine) {
    subscribe_at(e, 40.);
}
fn subscribe_at(e: &mut TestEngine, now: f64) {
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
}
fn result(e: &mut TestEngine) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(Ok(result))
            } else if let Action::Notice(Notice::CommandFailed { error, .. }) = a {
                Some(Err(error.legacy_text()))
            } else {
                None
            }
        })
        .unwrap()
}
const UPDATE: &str = r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":102,"data":{"bids":[[59000,4],[58000,5]],"asks":[[61000,0]]}}"#;

#[test]
fn initial_sockets_have_explicit_roles_and_unmodified_venue_urls_on_both_environments() {
    for (testnet, host) in [(false, "wss.woox.io"), (true, "wss.staging.woox.io")] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
            testnet,
            key: Some("fixture-key".into()),
            secret: Some("fixture-secret".into()),
            application_id: Some("socket-role-test".into()),
            ..Default::default()
        }));
        e.start(0.0);
        let r = prepared_seed_requests(&mut e, 0.).remove(0);
        response(&mut e, &r, 10.0, MARKETS);
        let mut early = prepared_seed_actions(&mut e, 10.);
        let ticker = early
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("/public/futures") => Some(r.clone()),
                _ => None,
            })
            .unwrap();
        response(&mut e, &ticker, 20.0, TICKERS);
        early.extend(e.drain_actions());
        let sockets: Vec<_> = early
            .into_iter()
            .filter_map(|action| {
                if let Action::WsConnect {
                    url,
                    channel,
                    context,
                    ..
                } = action
                {
                    assert!(context.is_none());
                    Some((url, channel))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(sockets.len(), 2);
        assert!(sockets.contains(&(
            format!("wss://{host}/ws/stream/socket-role-test"),
            Some(SocketChannel::Public)
        )));
        assert!(sockets.contains(&(
            format!("wss://{host}/v2/ws/private/stream/socket-role-test"),
            Some(SocketChannel::Private)
        )));
    }
}

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
fn repeated_ticker_fields_and_fields_for_other_topics_emit_no_frame() {
    let (mut e, s) = started();
    assert!(e.take_frame(35.0).is_some());
    for (i, payload) in [
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":60000,"volume":4,"amount":240000,"price":1,"bid":1}]}"#,
        r#"{"topic":"bbos","data":[{"symbol":"PERP_BTC_USDT","bid":60000,"ask":60000,"close":1,"price":1}]}"#,
        r#"{"topic":"markprices","data":[{"symbol":"PERP_BTC_USDT","price":60001,"close":1,"bid":1}]}"#,
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":null,"volume":null,"amount":null}]}"#,
    ]
    .iter()
    .enumerate()
    {
        let now = 40.0 + i as f64;
        ws(&mut e, s, now, payload);
        assert!(e.take_frame(now).is_none(), "{payload}");
    }
}

#[test]
fn ticker_signed_zero_changes_emit_frames_and_repeated_bits_do_not() {
    let (mut e, s) = started();
    e.take_frame(35.0);
    for (topic, field) in [
        ("tickers", "close"),
        ("tickers", "volume"),
        ("tickers", "amount"),
        ("bbos", "bid"),
        ("bbos", "ask"),
        ("markprices", "price"),
    ] {
        for (value, emits) in [
            ("-0.0", true),
            ("-0.0", false),
            ("0.0", true),
            ("0.0", false),
        ] {
            let payload = format!(
                r#"{{"topic":"{topic}","data":[{{"symbol":"PERP_BTC_USDT","{field}":{value}}}]}}"#
            );
            ws(&mut e, s, 40.0, &payload);
            assert_eq!(e.take_frame(40.0).is_some(), emits, "{payload}");
            let t = e.store().ticker("BTCUSDT").unwrap();
            let actual = match field {
                "close" => t.last,
                "volume" => t.volume,
                "amount" => t.quote_volume,
                "bid" => t.bid,
                "ask" => t.ask,
                _ => t.mark,
            };
            assert_eq!(actual.to_bits(), value.parse::<f64>().unwrap().to_bits());
        }
    }
}

#[test]
fn nullable_index_keeps_missing_value_without_rejecting_the_seed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
    response(
        &mut e,
        &r,
        20.0,
        &TICKERS.replace("\"index_price\":59999", "\"index_price\":null"),
    );
    let ticker = e.store().ticker("BTCUSDT").unwrap();
    assert!(ticker.index.is_nan());
    assert!(
        serde_json::to_string(ticker)
            .unwrap()
            .contains("\"index\":null")
    );
    assert!(e.store().loaded().tickers);
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
fn ticker_batches_validate_all_rows_before_committing_the_valid_prefix() {
    let (mut e, s) = started();
    ws(
        &mut e,
        s,
        40.0,
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":1},{"symbol":"SPOT_UNKNOWN_USDT","close":"bad"}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    ws(&mut e, s, 50.0, r#"{"topic":"bbos","data":[]}"#);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 60000.0);
}

#[test]
fn ticker_batch_capacity_is_a_hint_and_never_limits_delivered_rows() {
    let (mut e, s) = started();
    let rows = (1..=1000)
        .map(|price| format!(r#"{{"symbol":"PERP_BTC_USDT","close":{price}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    ws(
        &mut e,
        s,
        40.0,
        &format!(r#"{{"topic":"tickers","data":[{rows}]}}"#),
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 1000.0);
}

#[test]
fn ticker_envelope_accepts_data_before_topic_and_numeric_strings() {
    let (mut e, s) = started();
    ws(
        &mut e,
        s,
        40.0,
        r#"{"data":[{"symbol":"PERP_BTC_USDT","close":"60004","volume":"7","amount":420028}],"success":true,"ts":"1000","topic":"tickers"}"#,
    );
    let ticker = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (ticker.last, ticker.volume, ticker.quote_volume),
        (60004.0, 7.0, 420028.0)
    );
}

#[test]
fn control_event_wins_over_valid_or_malformed_ticker_data() {
    let (mut e, s) = started();
    e.drain_actions().for_each(drop);
    for data in [
        r#"[{"symbol":"PERP_BTC_USDT","close":1}]"#,
        r#"[{"symbol":"PERP_BTC_USDT","close":"bad"}]"#,
        r#"{"close":1}"#,
    ] {
        ws(
            &mut e,
            s,
            40.0,
            &format!(r#"{{"data":{data},"topic":"tickers","event":"ping"}}"#),
        );
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
        assert!(
            e.drain_actions()
                .any(|a| matches!(a,Action::WsSend{text,..}if text==r#"{"event":"pong"}"#))
        );
    }
}

#[test]
fn malformed_or_duplicate_ticker_envelope_fields_cannot_commit_rows() {
    let (mut e, s) = started();
    for fields in [
        r#""topic":"tickers","topic":"markprices""#,
        r#""topic":"tickers","ts":"bad""#,
        r#""topic":"tickers","ts":null"#,
        r#""topic":"tickers","success":1"#,
        r#""topic":"tickers","success":true,"success":true"#,
    ] {
        ws(
            &mut e,
            s,
            40.0,
            &format!(r#"{{{fields},"data":[{{"symbol":"PERP_BTC_USDT","close":1,"price":2}}]}}"#),
        );
        let ticker = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!((ticker.last, ticker.mark), (60000.0, 60001.0));
    }
    ws(
        &mut e,
        s,
        50.0,
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":1}],"data":[]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
}
#[test]
fn pre_seed_book_subscription_survives_until_socket_open() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    e.handle(
        1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    response(&mut e, &r, 10.0, MARKETS);
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
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
    let r = released_requests(&mut e, 30.).remove(0);
    assert!(r.url.ends_with("/orderbook/PERP_BTC_USDT"));
}
#[test]
fn buffers_before_snapshot_discards_old_and_preserves_full_depth() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = released_requests(&mut e, 40.).remove(0);
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
    let old = released_requests(&mut e, 40.).remove(0);
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
            interval: safe_cex_engine::model::Interval::M1,
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
        .as_chunks::<8>()
        .0
        .iter()
        .map(|v| f64::from_le_bytes(*v))
        .collect();
    assert_eq!(values, vec![1700000000.0, 10.0, 12.0, 9.0, 11.0, 3.0]);
}
#[test]
fn snapshot_keeps_more_than_one_thousand_levels() {
    let (mut e, _) = started();
    subscribe(&mut e);
    let r = released_requests(&mut e, 40.).remove(0);
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
    let old = released_requests(&mut e, 40.).remove(0);
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
    let new = released_requests(&mut e, 1050.).remove(0);
    snapshot(&mut e, &new, 1060.0, 110);
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids(), &[(59000.0, 2.0)]);
}
#[test]
fn live_malformed_frame_never_commits_its_valid_prefix() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = released_requests(&mut e, 40.).remove(0);
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
    let old = released_requests(&mut e, 40.).remove(0);
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
    subscribe_at(&mut e, 52.);
    let new = released_requests(&mut e, 52.).remove(0);
    snapshot(&mut e, &old, 60.0, 200);
    assert!(e.venue().book("BTCUSDT").is_none());
    snapshot(&mut e, &new, 70.0, 100);
    assert!(e.venue().book("BTCUSDT").is_some());
}
#[test]
fn funding_waits_sixty_seconds_and_failed_reads_preserve_seed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
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
    let r = prepared_seed_requests(&mut e, 60020.).remove(0);
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options::default()));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
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
    let r = prepared_seed_requests(&mut e, 1010.).remove(0);
    response(
        &mut e,
        &r,
        1020.0,
        &MARKETS.replace(",\"price_range\":0.01", ""),
    );
    assert_eq!(e.venue().price_range("BTCUSDT"), None);
    assert_eq!(e.store().markets().len(), 1);
}
// Unknown spellings never reach the venue: the command decoder rejects them.
#[test]
fn unicode_and_invalid_candle_inputs_are_rejected_before_the_venue() {
    for interval in ["日", "", "0m", "NaNm"] {
        let json = format!(
            r#"{{"op":"fetchCandles","command":9,"symbol":"BTCUSDT","interval":"{interval}"}}"#
        );
        assert!(
            safe_cex_engine::command::decode_command(&json).is_err(),
            "{interval}"
        );
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
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: Some(1600000000000.0),
            to_ms: Some(1600000120000.0),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.starts_with("https://api-pub.woox.io/v1/hist/kline"));
    assert_eq!(r.route, Some(safe_cex_engine::RequestRoute::Primary));
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
    let old = released_requests(&mut e, 40.).remove(0);
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
            if let Action::WsConnect {
                socket,
                url,
                channel,
                context,
                ..
            } = a
            {
                assert_eq!(channel, Some(SocketChannel::Public));
                assert_eq!(url, "wss://wss.woox.io/ws/stream/undefined");
                assert!(context.is_none());
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(26010.0, Event::WsOpened { socket: new });
    let r = released_requests(&mut e, 26010.).remove(0);
    snapshot(&mut e, &old, 26020.0, 300);
    assert!(e.venue().book("BTCUSDT").is_none());
    snapshot(&mut e, &r, 26030.0, 200);
    assert!(e.venue().book("BTCUSDT").is_some());
}
#[test]
fn malformed_market_and_ticker_rows_are_skipped_at_startup() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(
        &mut e,
        &r,
        10.0,
        &MARKETS.replace(
            "]}",
            r#",{"symbol":"PERP_ETH_USDT","base_tick":0,"quote_tick":0.01,"base_min":0.01,"base_max":100},{"symbol":"PERP_SOL_USDT","base_tick":"x"},{"symbol":"PERP_XRP_USDT","base_tick":0.1,"quote_tick":0.0001,"base_min":1,"base_max":1000,"base_imr":0.1}]}"#,
        ),
    );
    assert_eq!(e.store().markets().len(), 2);
    assert!(e.store().market("ETHUSDT").is_none());
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
    response(
        &mut e,
        &r,
        20.0,
        &TICKERS.replace("]}", r#",{"symbol":"PERP_XRP_USDT","24h_open":"bad"}]}"#),
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    assert!(e.store().ticker("XRPUSDT").is_none());
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let warnings: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        warnings
            .iter()
            .any(|m| m.contains("WOO tickers") && m.contains("PERP_XRP_USDT"))
    );
}
#[test]
fn ticker_without_open_keeps_last_price_with_zero_percentage() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
    response(
        &mut e,
        &r,
        20.0,
        &TICKERS.replace("\"24h_open\":50000", "\"24h_open\":0"),
    );
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!((t.last, t.percentage), (60000.0, 0.0));
}

fn empty_book_frame(frame: &[u8]) -> bool {
    safe_cex_engine::wire::sections(frame).any(|(kind, payload)| {
        kind == safe_cex_engine::wire::section::BOOK && payload[4..12] == [0; 8]
    })
}

#[test]
fn prev_ts_gap_hides_book_and_publishes_clear_until_new_snapshot() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = released_requests(&mut e, 40.).remove(0);
    snapshot(&mut e, &r, 70.0, 100);
    ws(
        &mut e,
        s,
        80.0,
        &UPDATE.replace("\"bids\"", "\"prevTs\":100,\"bids\""),
    );
    assert!(e.book("BTCUSDT").is_some());
    // Repeated delivery of the same linked event is stale, not a new gap.
    ws(
        &mut e,
        s,
        80.5,
        &UPDATE.replace("\"bids\"", "\"prevTs\":100,\"bids\""),
    );
    assert!(e.book("BTCUSDT").is_some());
    e.take_frame(81.0);
    ws(
        &mut e,
        s,
        90.0,
        &UPDATE
            .replace("\"ts\":102", "\"ts\":104")
            .replace("\"bids\"", "\"prevTs\":103,\"bids\""),
    );
    assert!(e.book("BTCUSDT").is_none());
    assert!(empty_book_frame(e.take_frame(91.0).unwrap()));
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1090.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1090.0, Event::TimerFired { timer });
    let r = released_requests(&mut e, 1090.).remove(0);
    snapshot(&mut e, &r, 1100.0, 105);
    assert!(e.book("BTCUSDT").is_some());
}

#[test]
fn buffered_prev_ts_gap_rejects_snapshot_but_overlap_can_bridge() {
    for (previous, visible) in [(99, true), (101, false)] {
        let (mut e, s) = started();
        subscribe(&mut e);
        let r = released_requests(&mut e, 40.).remove(0);
        ws(
            &mut e,
            s,
            60.0,
            &UPDATE.replace("\"bids\"", &format!("\"prevTs\":{previous},\"bids\"")),
        );
        snapshot(&mut e, &r, 70.0, 100);
        assert_eq!(e.book("BTCUSDT").is_some(), visible);
    }
}

#[test]
fn disconnect_publishes_clear_before_reconnect_snapshot() {
    let (mut e, s) = started();
    subscribe(&mut e);
    let r = released_requests(&mut e, 40.).remove(0);
    snapshot(&mut e, &r, 70.0, 100);
    e.take_frame(71.0);
    e.handle(
        80.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.book("BTCUSDT").is_none());
    assert!(empty_book_frame(e.take_frame(81.0).unwrap()));
    assert!(
        empty_book_frame(e.snapshot_frame(82.0)),
        "a consumer may have missed the clear flush"
    );
    e.handle(
        83.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(
        !safe_cex_engine::wire::sections(e.snapshot_frame(84.0))
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::BOOK)
    );
}
