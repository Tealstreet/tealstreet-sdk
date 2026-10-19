mod support;
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_mexc::{MexcFutures, Options};
use support::*;

#[test]
fn documented_markets_and_tickers_convert_contract_volume() {
    let (e, _) = started();
    let m = e.store().market("BTCUSDT").unwrap();
    assert_eq!(m.contract_size, Some(0.0001));
    assert_eq!(m.precision.amount, 0.0001);
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(t.open_interest, 60001.0);
    assert_eq!(t.volume, 5.0);
    assert_eq!(t.percentage, 1.0);
}

#[test]
fn buffers_depth_discards_old_versions_and_invalidates_gaps() {
    let (mut e, socket) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        150.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":11,"bids":[[60000,30,1]],"asks":[]}}"#,
    );
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10,1],[59999,20,1]],"asks":[[60001,40,1]]}}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        [(60000.0, 0.003), (59999.0, 0.002)]
    );
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":10,"bids":[[60000,99,1]],"asks":[]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids()[0].1, 0.003);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":12,"bids":[[60000,0,0]],"asks":[]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(59999.0, 0.002)]);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":14,"bids":[],"asks":[]}}"#,
    );
    assert!(e.book("BTCUSDT").is_none());
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
    assert_eq!(requests(&mut e).len(), 1);
}

#[test]
fn streamed_200_error_does_not_commit_markets_even_with_an_empty_array() {
    let mut e = Engine::new(MexcFutures::new(Options::default()));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        r#"{"data":[],"success":false,"code":510,"message":"Requests are too frequent"}"#,
    );
    assert!(!e.store().loaded().markets);
}

#[test]
fn watchdog_closes_a_silent_socket_and_ignores_old_socket_frames() {
    let (mut e, socket) = started();
    let mut timers: Vec<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .collect();
    for now in [8100.0, 16100.0, 24100.0] {
        let timer = timers.remove(0);
        e.handle(now, Event::TimerFired { timer });
        let actions: Vec<_> = e.drain_actions().collect();
        if now == 24100.0 {
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a,Action::WsClose {socket:s} if *s==socket))
            );
        }
        timers = actions
            .into_iter()
            .filter_map(|a| {
                if let Action::SetTimer { timer, .. } = a {
                    Some(timer)
                } else {
                    None
                }
            })
            .collect();
    }
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.ticker","symbol":"BTC_USDT","data":{"lastPrice":1}}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.0);
}

#[test]
fn candle_subscriptions_are_reference_counted() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    for _ in 0..2 {
        e.handle(
            0.0,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: "1m".into(),
            }),
        );
    }
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(a.len(), 1);
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
        }),
    );
    assert!(!e.has_actions());
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
        }),
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend {text,..} if text.contains("unsub.kline")))
    );
}

#[test]
fn resubscribe_ignores_an_old_subscriptions_inflight_snapshot() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    let subscribe = || {
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
    };
    e.handle(0.0, subscribe());
    let old = requests(&mut e).remove(0);
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(0.0, subscribe());
    let new = requests(&mut e).remove(0);
    response(
        &mut e,
        &old,
        r#"{"success":true,"code":0,"data":{"version":1,"bids":[[1,1,1]],"asks":[]}}"#,
    );
    assert!(e.book("BTCUSDT").is_none());
    response(
        &mut e,
        &new,
        r#"{"success":true,"code":0,"data":{"version":2,"bids":[[2,1,1]],"asks":[]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(2.0));
}

#[test]
fn startup_network_failures_retry_reads_without_committing_partial_state() {
    let mut e = Engine::new(MexcFutures::new(Options::default()));
    e.start(0.0);
    let first = requests(&mut e).remove(0);
    e.handle(
        1.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset",
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
    assert!(!e.store().loaded().markets);
    e.handle(1001.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, MARKETS);
    let ticker = requests(&mut e).remove(0);
    e.handle(
        1002.0,
        Event::HttpFailed {
            request: ticker.id,
            error: "lost read",
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
    e.handle(2002.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, TICKERS);
    assert!(e.store().loaded().tickers);
}

#[test]
fn candle_ranges_are_not_truncated_without_an_explicit_limit() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        0.0,
        Event::Command(Command::FetchCandles {
            command: 17,
            symbol: "BTCUSDT".into(),
            interval: "1m".into(),
            limit: None,
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700060000000.0),
        }),
    );
    let r = requests(&mut e).remove(0);
    let times = (0..1000)
        .map(|i| (1700000000u64 + i * 60).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let numbers = vec!["1"; 1000].join(",");
    let body = format!(
        r#"{{"success":true,"code":0,"data":{{"time":[{times}],"open":[{numbers}],"high":[{numbers}],"low":[{numbers}],"close":[{numbers}],"vol":[{numbers}]}}}}"#
    );
    response(&mut e, &r, &body);
    let rows = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 17,
                result: Ok(json),
            }) = a
            {
                Some(json)
            } else {
                None
            }
        })
        .unwrap();
    let decoded: Vec<serde::de::IgnoredAny> = serde_json::from_str(&rows).unwrap();
    assert_eq!(decoded.len(), 1000);
}

#[test]
fn invalid_rest_depth_never_commits_a_partial_or_empty_anchor() {
    for body in [
        r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10],[59999]],"asks":[]}}"#,
        r#"{"success":true,"code":0,"data":{"version":10,"bids":[]}}"#,
        r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,-1]],"asks":[]}}"#,
    ] {
        let (mut e, _) = started();
        e.drain_actions().for_each(drop);
        e.handle(
            150.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            }),
        );
        let r = requests(&mut e).remove(0);
        response(&mut e, &r, body);
        assert!(e.book("BTCUSDT").is_none());
        assert!(
            e.drain_actions()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}

#[test]
fn malformed_live_or_buffered_deltas_invalidate_before_advancing_the_anchor() {
    for buffered in [false, true] {
        for levels in [
            "[[60000,50],[59999,-1]]",
            "[[60000,50],[0,1]]",
            "[[60000,null]]",
        ] {
            let (mut e, socket) = started();
            e.drain_actions().for_each(drop);
            e.handle(
                150.0,
                Event::Command(Command::SubscribeOrderBook {
                    symbol: "BTCUSDT".into(),
                }),
            );
            let r = requests(&mut e).remove(0);
            let anchor =
                r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10]],"asks":[]}}"#;
            if !buffered {
                response(&mut e, &r, anchor);
            }
            ws(
                &mut e,
                socket,
                &format!(
                    r#"{{"channel":"push.depth","symbol":"BTC_USDT","data":{{"version":11,"bids":{levels},"asks":[]}}}}"#
                ),
            );
            if buffered {
                response(&mut e, &r, anchor);
            }
            assert!(e.book("BTCUSDT").is_none());
            ws(
                &mut e,
                socket,
                r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":12,"bids":[[59998,20]],"asks":[]}}"#,
            );
            assert!(e.book("BTCUSDT").is_none());
            assert!(
                e.drain_actions()
                    .any(|a| matches!(a, Action::SetTimer { .. }))
            );
        }
    }
}
