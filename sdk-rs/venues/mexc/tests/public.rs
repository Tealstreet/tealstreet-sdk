mod support;
use safe_cex_engine::{Action, Command, Event};
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
    let r = public_requests(&mut e, 150.).remove(0);
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
    assert_eq!(public_requests(&mut e, 1000.).len(), 1);
}

#[test]
fn streamed_200_error_does_not_commit_markets_even_with_an_empty_array() {
    let mut e = support::TestEngine::from(safe_cex_engine::test_support::seeded_engine(
        MexcFutures::new(Options::default()),
    ));
    e.start(0.0);
    let r = public_requests(&mut e, 0.).remove(0);
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
                interval: safe_cex_engine::model::Interval::M1,
            }),
        );
    }
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(a.len(), 1);
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(!e.has_actions());
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
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
    let old = public_requests(&mut e, 0.).remove(0);
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(0.0, subscribe());
    let new = public_requests(&mut e, 0.).remove(0);
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
    let mut e = support::TestEngine::from(safe_cex_engine::test_support::seeded_engine(
        MexcFutures::new(Options::default()),
    ));
    e.start(0.0);
    let first = public_requests(&mut e, 0.).remove(0);
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
    let r = public_requests(&mut e, 1001.).remove(0);
    response(&mut e, &r, MARKETS);
    let ticker = public_requests(&mut e, 100.).remove(0);
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
    let r = public_requests(&mut e, 2002.).remove(0);
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
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700060000000.0),
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
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
                result: json,
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
        let r = public_requests(&mut e, 150.).remove(0);
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
            let r = public_requests(&mut e, 150.).remove(0);
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

#[test]
fn all_market_prices_and_selected_ticker_details_survive_reconnect() {
    let mut e = support::TestEngine::from(safe_cex_engine::test_support::seeded_engine(
        MexcFutures::new(Options {
            public_data_only: true,
            ..Default::default()
        }),
    ));
    e.handle(
        0.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["COIN104USDT".into(), "COIN104USDT".into(), "UNKNOWN".into()],
        }),
    );
    e.start(0.0);
    let request = public_requests(&mut e, 0.).remove(0);
    let template: serde_json::Value = serde_json::from_str(MARKETS).unwrap();
    let markets: Vec<_> = (0..105)
        .map(|i| {
            let mut row = template["data"][0].clone();
            row["symbol"] = format!("COIN{i}_USDT").into();
            row["baseCoin"] = format!("COIN{i}").into();
            row
        })
        .collect();
    response(
        &mut e,
        &request,
        &serde_json::json!({"success":true,"code":0,"data":markets}).to_string(),
    );
    let request = public_requests(&mut e, 100.).remove(0);
    let template: serde_json::Value = serde_json::from_str(TICKERS).unwrap();
    let tickers: Vec<_> = (0..105)
        .map(|i| {
            let mut row = template["data"][0].clone();
            row["symbol"] = format!("COIN{i}_USDT").into();
            row
        })
        .collect();
    response(
        &mut e,
        &request,
        &serde_json::json!({"success":true,"code":0,"data":tickers}).to_string(),
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
    e.handle(100.0, Event::WsOpened { socket });
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
    assert_eq!(sent.iter().filter(|s| s.contains("sub.tickers")).count(), 1);
    assert_eq!(
        sent.iter().filter(|s| s.contains("COIN104_USDT")).count(),
        1
    );
    assert!(!sent.iter().any(|s| s.contains("COIN0_USDT")));
    let before = e.store().ticker("COIN103USDT").unwrap().clone();
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.tickers","data":[{"symbol":"COIN102_USDT","lastPrice":"bad"},{"symbol":"COIN103_USDT","lastPrice":"61000","fairPrice":"62000","riseFallRate":0.02,"volume24":200},{"symbol":"COIN104_USDT","lastPrice":63000,"fairPrice":null,"volume24":null}]}"#,
    );
    let ticker = e.store().ticker("COIN103USDT").unwrap();
    assert_eq!(ticker.last, 61000.0);
    assert_eq!(ticker.mark, 62000.0);
    assert_eq!(ticker.percentage, 2.0);
    assert_eq!(ticker.volume, 0.02);
    assert!((ticker.open_interest - 62000.0).abs() < 1e-8);
    assert_eq!(ticker.index, before.index);
    assert_eq!(ticker.bid, before.bid);
    assert_eq!(ticker.ask, before.ask);
    assert_eq!(ticker.quote_volume, before.quote_volume);
    assert_eq!(ticker.funding_rate, before.funding_rate);
    assert_eq!(e.store().ticker("COIN104USDT").unwrap().mark, before.mark);
    assert_eq!(
        e.store().ticker("COIN104USDT").unwrap().volume,
        before.volume
    );
    e.handle(
        300.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["COIN101USDT".into()],
        }),
    );
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
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .any(|s| s.contains("unsub.ticker") && s.contains("COIN104_USDT"))
    );
    assert!(
        sent.iter()
            .any(|s| s.contains("sub.ticker") && s.contains("COIN101_USDT"))
    );
    e.handle(
        400.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["COIN101USDT".into(), "COIN101USDT".into()],
        }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    e.handle(500.0, Event::WsClosed { socket, code: 1006 });
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
    e.handle(1500.0, Event::TimerFired { timer });
    let new_socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1500.0, Event::WsOpened { socket: new_socket });
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
    assert_eq!(sent.iter().filter(|s| s.contains("sub.tickers")).count(), 1);
    assert_eq!(
        sent.iter().filter(|s| s.contains("COIN101_USDT")).count(),
        1
    );
    assert!(!sent.iter().any(|s| s.contains("COIN104_USDT")));

    // Explicit interests beyond 100 markets keep their detailed subscriptions;
    // unknown and duplicate symbols do not create extra subscriptions.
    let mut symbols = vec!["UNKNOWN".into(), "COIN104USDT".into(), "COIN104USDT".into()];
    symbols.extend((0..105).map(|i| format!("COIN{i}USDT")));
    e.handle(
        1600.0,
        Event::Command(Command::SetTickerSymbols { symbols }),
    );
    let selected: Vec<serde_json::Value> = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => serde_json::from_str(&text).ok(),
            _ => None,
        })
        .filter(|value: &serde_json::Value| value["method"] == "sub.ticker")
        .collect();
    assert_eq!(selected.len(), 104);
    assert_eq!(selected[0]["param"]["symbol"], "COIN104_USDT");
    assert_eq!(selected[103]["param"]["symbol"], "COIN103_USDT");
    e.handle(
        1700.0,
        Event::WsClosed {
            socket: new_socket,
            code: 1006,
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2700.0, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(2700.0, Event::WsOpened { socket });
    let selected_after_reconnect: Vec<serde_json::Value> = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => serde_json::from_str(&text).ok(),
            _ => None,
        })
        .filter(|value: &serde_json::Value| value["method"] == "sub.ticker")
        .collect();
    assert_eq!(selected_after_reconnect.len(), 105);
    assert_eq!(
        selected_after_reconnect[0]["param"]["symbol"],
        "COIN104_USDT"
    );
    let ids: std::collections::BTreeSet<_> = selected_after_reconnect
        .iter()
        .map(|row| row["param"]["symbol"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 105);
    assert!(ids.contains("COIN101_USDT"));
}

#[test]
fn clearing_ticker_selection_restores_default_details_without_duplicate_subscribes() {
    let (mut e, _) = started();
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["UNKNOWN".into()],
        }),
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsSend{text,..} if text.contains("unsub.ticker")))
    );
    e.handle(
        300.0,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    assert!(e.drain_actions().any(|a| matches!(a, Action::WsSend{text,..} if text.contains("sub.ticker") && text.contains("BTC_USDT"))));
    e.handle(
        400.0,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}

#[test]
fn invalid_mini_ticker_prices_and_volumes_preserve_the_previous_row() {
    let (mut e, socket) = started();
    let before = e.store().ticker("BTCUSDT").unwrap().clone();
    for invalid in [
        r#""lastPrice":-1"#,
        r#""fairPrice":-1"#,
        r#""indexPrice":-1"#,
        r#""volume24":-1"#,
        r#""amount24":-1"#,
        r#""lastPrice":"NaN""#,
    ] {
        ws(
            &mut e,
            socket,
            &format!(r#"{{"channel":"push.tickers","data":[{{"symbol":"BTC_USDT",{invalid}}}]}}"#),
        );
        let actual = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!(actual.last, before.last);
        assert_eq!(actual.mark, before.mark);
        assert_eq!(actual.volume, before.volume);
        assert_eq!(actual.quote_volume, before.quote_volume);
    }
}

#[test]
fn mini_ticker_zero_unavailable_prices_preserve_quotes_and_apply_valid_fields() {
    let (mut e, socket) = started();
    let before = e.store().ticker("BTCUSDT").unwrap().clone();
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.tickers","data":[{"symbol":"BTC_USDT","lastPrice":0,"fairPrice":62000,"indexPrice":"61900","volume24":0,"amount24":0}]}"#,
    );
    let ticker = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.last, before.last);
    assert_eq!(ticker.mark, 62000.0);
    assert_eq!(ticker.index, 61900.0);
    assert_eq!(ticker.volume, 0.0);
    assert_eq!(ticker.quote_volume, 0.0);
    assert_eq!(ticker.funding_rate, before.funding_rate);
    assert!((ticker.open_interest - 62000.0).abs() < 1e-8);
    for missing in ["0", "null"] {
        ws(
            &mut e,
            socket,
            &format!(
                r#"{{"channel":"push.tickers","data":[{{"symbol":"BTC_USDT","lastPrice":63000,"fairPrice":{missing},"indexPrice":{missing}}}]}}"#
            ),
        );
        let ticker = e.store().ticker("BTCUSDT").unwrap();
        assert_eq!(ticker.last, 63000.0);
        assert_eq!(ticker.mark, 62000.0);
        assert_eq!(ticker.index, 61900.0);
    }
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(
            safe_cex_engine::LogEvent { .. }
        ))
    )));
}

#[test]
fn mini_ticker_unknown_inactive_symbols_are_ignored_without_warning() {
    let (mut e, socket) = started();
    let before = e.store().ticker("BTCUSDT").unwrap().clone();
    e.drain_actions().for_each(drop);
    // The all-market feed includes inactive/unloaded symbols and their sentinels.
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.tickers","data":[{"symbol":"INACTIVE_USDT","lastPrice":0,"fairPrice":0,"indexPrice":0,"volume24":0},{"symbol":"UNKNOWN_USDT","lastPrice":"NaN"}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, before.last);
    assert_eq!(e.store().tickers().len(), 1);
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(
            safe_cex_engine::LogEvent { .. }
        ))
    )));
}
