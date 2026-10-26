//! Independent payloads from https://docs.txflow-testnet.xyz/api.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, RequestRoute, SocketChannel};
use safe_cex_venue_txflow::{Options, TxFlow};
use support::*;

fn flush_subscriptions(
    e: &mut Engine<TxFlow>,
    until: f64,
) -> Vec<(safe_cex_engine::SocketId, String)> {
    let mut frames = Vec::new();
    loop {
        let mut timers = Vec::new();
        for action in e.drain_actions() {
            match action {
                Action::WsSend { socket, text } => frames.push((socket, text)),
                Action::SetTimer { timer, at_ms } if at_ms <= until => timers.push((timer, at_ms)),
                _ => {}
            }
        }
        if timers.is_empty() {
            return frames;
        }
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (timer, at) in timers {
            e.handle(at, Event::TimerFired { timer });
        }
    }
}

#[test]
fn mainnet_and_invalid_agent_fail_before_io() {
    for opts in [
        Options::default(),
        Options {
            testnet: true,
            ..Default::default()
        },
        Options {
            testnet: true,
            public_data_only: true,
            vault: Some("bad".into()),
            ..Default::default()
        },
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(opts));
        e.start(1000.0);
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
    }
}
#[test]
fn public_requests_carry_version_and_public_route() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    assert_eq!(r.route, Some(RequestRoute::PublicCors));
    assert_eq!(r.url, "https://rpc.txflow-testnet.xyz/info");
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "x-api-version" && v == "2026-08-16")
    );
}
#[test]
fn explicit_metadata_ids_and_percent_funding() {
    let (e, _, _) = ready();
    let market = &e.store().markets()[0];
    assert_eq!((&*market.id, &*market.symbol), ("1", "BTC-USDC"));
    assert_eq!(market.precision.price, 0.1);
    let t = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!(t.funding_rate, 0.0001);
    assert_eq!(t.funding_time, 7200000.0);
    assert_eq!(t.open_interest, 130002.0);
}
#[test]
fn socket_roles_and_control_plane_are_distinct() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    reply(&mut e, &r, 200, META);
    let startup = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
    let r = startup
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, r, 200, TICKER);
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                Some((timer, at_ms))
            } else {
                None
            }
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let sockets = startup
        .into_iter()
        .chain(e.drain_actions())
        .filter_map(|a| {
            if let Action::WsConnect {
                socket,
                url,
                channel,
                ..
            } = a
            {
                Some((socket, url, channel))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(sockets.len(), 2);
    for (socket, url, channel) in sockets {
        assert_eq!(channel, Some(SocketChannel::Public));
        e.handle(2000.0, Event::WsOpened { socket });
        let frames = e
            .drain_actions()
            .filter_map(|a| {
                if let Action::WsSend { text, .. } = a {
                    Some(text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(!frames.is_empty());
        for frame in frames {
            let frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
            if url.ends_with("/api-ws") {
                assert!(frame.get("type").is_none());
                assert_eq!(frame["subscription"]["type"], "ticker");
            } else {
                assert_eq!(frame["type"], frame["subscription"]["type"]);
            }
        }
    }
}
#[test]
fn full_snapshot_book_not_truncated_and_stale_is_ignored() {
    let (mut e, node, _) = ready();
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }),
    );
    let bids = (1..=80)
        .rev()
        .map(|n| format!("{{\"px\":\"{n}\",\"sz\":\"1\"}}"))
        .collect::<Vec<_>>()
        .join(",");
    let frame = format!(
        "{{\"channel\":\"l2Book\",\"data\":{{\"coin\":\"1\",\"time\":3000,\"levels\":[[{bids}],[{{\"px\":\"81\",\"sz\":\"2\"}}]]}}}}"
    );
    message(&mut e, node, 3000.0, &frame);
    let book = e.book("BTC-USDC").unwrap();
    assert_eq!(book.bids().len(), 80);
    assert_eq!(book.best_bid(), Some(80.0));
    message(
        &mut e,
        node,
        4000.0,
        r#"{"channel":"l2Book","data":{"coin":"1","time":2000,"levels":[[{"px":"70","sz":"1"}],[]]}}"#,
    );
    assert_eq!(e.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
    e.handle(
        5000.0,
        Event::WsClosed {
            socket: node,
            code: 1006,
        },
    );
    assert!(e.book("BTC-USDC").is_none());
}
#[test]
fn snapshot_books_skip_unsynchronized_subscriptions_but_clear_stale_ones() {
    use safe_cex_engine::Venue;
    let listed = |e: &Engine<TxFlow>| {
        e.venue()
            .books()
            .map(|(symbol, _)| symbol.to_owned())
            .collect::<Vec<_>>()
    };
    let (mut e, node, _) = ready();
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }),
    );
    // No snapshot yet: recovery frames must not publish an empty book.
    assert!(e.book("BTC-USDC").is_none());
    assert!(listed(&e).is_empty());
    message(
        &mut e,
        node,
        3000.0,
        r#"{"channel":"l2Book","data":{"coin":"1","time":3000,"levels":[[{"px":"80","sz":"1"}],[{"px":"81","sz":"2"}]]}}"#,
    );
    assert_eq!(listed(&e), ["BTC-USDC"]);
    e.handle(
        4000.0,
        Event::WsClosed {
            socket: node,
            code: 1006,
        },
    );
    // Resyncing: not live, but still listed so recovery snapshots clear it.
    assert!(e.book("BTC-USDC").is_none());
    assert_eq!(listed(&e), ["BTC-USDC"]);
}
#[test]
fn live_frames_publish_no_book_before_first_snapshot_but_clear_stale_ones() {
    use safe_cex_engine::wire::{section, sections};
    let has_book = |e: &mut Engine<TxFlow>, now: f64| {
        e.take_frame(now)
            .is_some_and(|frame| sections(frame).any(|(kind, _)| kind == section::BOOK))
    };
    let (mut e, node, _) = ready();
    has_book(&mut e, 1500.0);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }),
    );
    // With book-deltas (feature-unified builds) subscribing alone forces a BOOK; drain it.
    has_book(&mut e, 2050.0);
    // The node opening resets every ladder; none has been published yet.
    e.handle(2100.0, Event::WsOpened { socket: node });
    assert!(!has_book(&mut e, 2300.0));
    message(
        &mut e,
        node,
        3000.0,
        r#"{"channel":"l2Book","data":{"coin":"1","time":3000,"levels":[[{"px":"80","sz":"1"}],[{"px":"81","sz":"2"}]]}}"#,
    );
    assert!(has_book(&mut e, 3100.0));
    e.handle(
        4000.0,
        Event::WsClosed {
            socket: node,
            code: 1006,
        },
    );
    // A published book that goes stale is cleared in the next live frame.
    assert!(has_book(&mut e, 4100.0));
    assert!(e.book("BTC-USDC").is_none());
}
#[test]
fn documented_funding_and_api_last_are_independent() {
    let (mut e, node, api) = ready();
    message(
        &mut e,
        node,
        3000.0,
        r#"{"channel":"allAssetCtxs","data":{"ctxs":[{"coin":"1","ctx":{"funding":"-0.01","markPx":"65010","oraclePx":"65000","openInterest":"4","midPx":"65005","dayBaseVlm":"8","dayNtlVlm":"520000"}}]}}"#,
    );
    message(
        &mut e,
        api,
        3001.0,
        r#"{"channel":"ticker","data":{"instrumentId":1,"coin":"BTC","lastPrice":"65020","prevDayPx":"65000","coinVlm":"8","dayNtlVlm":"520000"}}"#,
    );
    let t = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!(t.funding_rate, -0.0001);
    assert_eq!(t.mark, 65010.0);
    assert_eq!(t.last, 65020.0);
    assert_eq!(t.open_interest, 260040.0);
}

#[test]
fn public_payload_field_order_and_escaped_aliases_preserve_atomic_updates() {
    for data_first in [false, true] {
        let (mut e, node, api) = ready();
        e.handle(
            2500.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTC-USDC".into(),
            }),
        );
        let frame = |channel: &str, data: &str| {
            if data_first {
                format!(r#"{{"data":{data},"ignored":{{"nested":[1,2]}},"channel":"{channel}"}}"#)
            } else {
                format!(r#"{{"channel":"{channel}","ignored":{{"nested":[1,2]}},"data":{data}}}"#)
            }
        };
        message(
            &mut e,
            api,
            3000.0,
            &frame(
                "ticker",
                r#"{"instrumentId":1,"lastPrice":"100","prevDayPx":"80","coinVlm":"4","dayNtlVlm":"400"}"#,
            ),
        );
        let ticker = e.store().ticker("BTC-USDC").unwrap();
        assert_eq!(
            (ticker.last, ticker.percentage, ticker.volume),
            (100.0, 25.0, 4.0)
        );
        message(
            &mut e,
            node,
            3100.0,
            &frame(
                "allAssetCtxs",
                r#"{"ctxs":[{"coin":"\u0031","ctx":{"markPx":"101","oraclePx":"99","funding":"0.01","openInterest":"2"}}]}"#,
            ),
        );
        let ticker = e.store().ticker("BTC-USDC").unwrap();
        assert_eq!(
            (ticker.mark, ticker.open_interest, ticker.volume),
            (101.0, 202.0, 4.0)
        );
        message(
            &mut e,
            node,
            3200.0,
            &frame(
                "allAssetCtxs",
                r#"{"ctxs":[{"coin":"1","ctx":{"markPx":"102"}},{"coin":"1","ctx":{"markPx":"NaN"}}]}"#,
            ),
        );
        assert_eq!(e.store().ticker("BTC-USDC").unwrap().mark, 101.0);
        message(
            &mut e,
            node,
            3300.0,
            &frame(
                "l2Book",
                r#"{"coin":"\u0042TC","time":3300,"levels":[[{"px":"99","sz":"2"}],[{"px":"101","sz":"3"}]]}"#,
            ),
        );
        assert_eq!(e.book("BTC-USDC").unwrap().best_bid(), Some(99.0));
        message(
            &mut e,
            node,
            3400.0,
            &frame(
                "l2Book",
                r#"{"coin":"1","time":3400,"levels":[[{"px":"100","sz":"2"}],[{"px":"NaN","sz":"3"}]]}"#,
            ),
        );
        assert_eq!(e.book("BTC-USDC").unwrap().best_bid(), Some(99.0));
        for channel in ["subscriptionResponse", "unrecognized-channel"] {
            message(
                &mut e,
                api,
                3500.0,
                &frame(channel, r#"{"arbitrary":[true,null,{"coin":"1"}]}"#),
            );
        }
        message(&mut e, node, 3501.0, r#"{"method":"PONG"}"#);
        message(&mut e, api, 3502.0, r#"{"channel":"pong"}"#);
        message(
            &mut e,
            api,
            3600.0,
            &frame("ticker", r#"{"instrumentId":9,"lastPrice":"999"}"#),
        );
        assert_eq!(e.store().tickers().len(), 1);
        assert_eq!(e.store().ticker("BTC-USDC").unwrap().last, 100.0);
    }
}
#[test]
fn failed_envelope_never_commits_markets() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    reply(
        &mut e,
        &r,
        200,
        &META.replace("\"success\":true", "\"success\":false"),
    );
    assert!(e.store().markets().is_empty());
}
#[test]
fn candle_subscription_uses_api_socket_and_numeric_asset() {
    let (mut e, _, api) = ready();
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let (socket, text) = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsSend { socket, text } = a {
                Some((socket, text))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(socket, api);
    let f: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["subscription"]["coin"], "1");
    assert!(f.get("type").is_none());
}
#[test]
fn candle_query_validates_and_streams_documented_envelope() {
    let (mut e, _, _) = ready();
    e.handle(
        2000.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: Some(0.0),
            to_ms: Some(120000.0),
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 2000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"code":0,"data":[{"t":60000,"s":"1","i":"1m","o":"2","h":"3","l":"1","c":"2.5","v":"4"}]}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(safe_cex_engine::Notice::CommandDone { command: 9, result }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(rows[0]["timestamp"], 60.0);
    assert_eq!(rows[0]["close"], 2.5);
}

#[test]
fn ping_is_periodic_despite_public_pushes_and_watchdog_recovers() {
    let (mut e, node, _, pending) = ready_with_timers(TICKER);
    message(&mut e, node, 11999.0, r#"{"method":"PONG"}"#);
    let mut actions = Vec::new();
    for (timer, at) in pending.into_iter().filter(|(_, at)| *at == 12000.0) {
        e.handle(at, Event::TimerFired { timer });
        actions.extend(e.drain_actions());
    }
    assert!(actions.iter().any(
        |a| matches!(a,Action::WsSend{socket,text}if *socket==node&&text=="{\"method\":\"ping\"}")
    ));
    for next in actions.into_iter().filter_map(|a| {
        if let Action::SetTimer { timer, .. } = a {
            Some(timer)
        } else {
            None
        }
    }) {
        e.handle(50000.0, Event::TimerFired { timer: next });
    }
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket}if socket==node))
    );
}
#[test]
fn nullable_market_context_does_not_drop_valid_fields() {
    let (mut e, node, _) = ready();
    message(
        &mut e,
        node,
        3000.0,
        r#"{"channel":"activeAssetCtx","data":{"coin":"1","ctx":{"funding":"0.02","markPx":"65000","oraclePx":"64000","midPx":null,"lastPrice":null,"openInterest":"2"}}}"#,
    );
    assert_eq!(e.store().ticker("BTC-USDC").unwrap().funding_rate, 0.0002);
}

#[test]
fn metadata_filters_other_settlement_and_delisting_without_reindexing() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    let mut meta: serde_json::Value = serde_json::from_str(META).unwrap();
    let mut other = meta["data"]["universe"][0].clone();
    other["index"] = 7.into();
    other["currencyType"] = "btc".into();
    let mut delisted = other.clone();
    delisted["index"] = 8.into();
    delisted["currencyType"] = "usdc".into();
    delisted["delisted"] = true.into();
    meta["data"]["universe"]
        .as_array_mut()
        .unwrap()
        .extend([other, delisted]);
    reply(&mut e, &r, 200, &meta.to_string());
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].id, "1");
}
#[test]
fn metadata_duplicate_identity_rejects_the_whole_snapshot() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    let mut meta: serde_json::Value = serde_json::from_str(META).unwrap();
    let duplicate = meta["data"]["universe"][0].clone();
    meta["data"]["universe"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    reply(&mut e, &r, 200, &meta.to_string());
    assert!(e.store().markets().is_empty());
}

#[test]
fn mismatched_rest_ticker_keeps_seed_unloaded_until_valid_retry() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    reply(&mut e, &r, 200, META);
    assert!(!e.store().loaded().tickers);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
    let r = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        &r,
        200,
        &TICKER.replace("\"instrumentId\":1", "\"instrumentId\":2"),
    );
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
    let actions = e.drain_actions().collect::<Vec<_>>();
    let retry = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - 1000.0, 1000.0) => Some(*timer),
            _ => None,
        })
        .unwrap();
    let pace = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1025.0,
            } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1025.0, Event::TimerFired { timer: pace });
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
    e.drain_actions().for_each(drop);
    e.handle(2000.0, Event::TimerFired { timer: retry });
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 2000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        2000.0,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: TICKER,
            },
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 2025.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2025.0, Event::TimerFired { timer });
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().ticker("BTC-USDC").unwrap().last, 65000.0);
}

#[test]
fn book_and_chart_intents_before_metadata_subscribe_once_when_ready() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }),
    );
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
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
    reply(&mut e, &r, 200, META);
    let startup = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
    let r = startup
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, r, 200, TICKER);
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                Some((timer, at_ms))
            } else {
                None
            }
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let sockets = startup
        .into_iter()
        .chain(e.drain_actions())
        .filter_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for socket in sockets {
        e.handle(2000.0, Event::WsOpened { socket });
    }
    let frames = flush_subscriptions(&mut e, 3000.0);
    assert_eq!(
        frames.iter().filter(|(_, f)| f.contains("l2Book")).count(),
        1
    );
    assert_eq!(
        frames.iter().filter(|(_, f)| f.contains("candle")).count(),
        1
    );
}

#[test]
fn desired_subscriptions_coalesce_and_released_unsubscribe_keeps_150ms_pacing() {
    let (mut e, _, api) = ready();
    for interval in ["1m", "3m", "5m"] {
        e.handle(
            3000.0,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTC-USDC".into(),
                interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
            }),
        );
    }
    e.handle(
        3000.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let first: Vec<_> = e.drain_actions().collect();
    let timer = first
        .iter()
        .find_map(|action| match action {
            Action::SetTimer {
                timer,
                at_ms: 3150.0,
            } => Some(*timer),
            _ => None,
        })
        .expect("second desired frame is paced150ms");
    let mut intervals = Vec::new();
    let collect = |actions: &[Action], intervals: &mut Vec<String>| {
        let sent: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::WsSend { socket, text } => Some((*socket, text)),
                _ => None,
            })
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, api);
        let frame: serde_json::Value = serde_json::from_str(sent[0].1).unwrap();
        assert_eq!(frame["method"], "subscribe");
        intervals.push(frame["subscription"]["interval"].as_str().unwrap().into());
    };
    collect(&first, &mut intervals);
    e.handle(3150.0, Event::TimerFired { timer });
    collect(&e.drain_actions().collect::<Vec<_>>(), &mut intervals);
    intervals.sort();
    assert_eq!(
        intervals,
        ["3m", "5m"],
        "transient1m coalesces before host release"
    );
    e.handle(
        3150.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M3,
        }),
    );
    let queued: Vec<_> = e.drain_actions().collect();
    assert!(
        !queued
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. }))
    );
    let timer = queued
        .iter()
        .find_map(|action| match action {
            Action::SetTimer {
                timer,
                at_ms: 3300.0,
            } => Some(*timer),
            _ => None,
        })
        .expect("unsubscribe waits150ms after the last returned frame");
    e.handle(3300.0, Event::TimerFired { timer });
    let sent: Vec<_> = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } => {
                assert_eq!(socket, api);
                Some(text)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [r#"{"method":"unsubscribe","subscription":{"type":"candle","coin":"1","interval":"3m"}}"#]
    );
}

#[test]
fn coalesced_churn_reaches_final_desired_state_without_fifo_overflow() {
    let (mut e, _, api, pending) = ready_with_timers(TICKER);
    let flip = |e: &mut Engine<TxFlow>, now| {
        for add in [true, false] {
            let command = if add {
                Command::SubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: safe_cex_engine::model::Interval::M1,
                }
            } else {
                Command::UnsubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: safe_cex_engine::model::Interval::M1,
                }
            };
            e.handle(now, Event::Command(command));
        }
    };
    for _ in 0..100 {
        flip(&mut e, 3000.0);
    }
    e.handle(
        3000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let burst: Vec<_> = e.drain_actions().collect();
    assert!(
        !burst
            .iter()
            .any(|action| matches!(action, Action::WsClose { .. }))
    );
    let sent: Vec<_> = burst
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if *socket == api => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [r#"{"method":"subscribe","subscription":{"type":"candle","coin":"1","interval":"1m"}}"#]
    );
    for (timer, at) in pending.into_iter().filter(|(_, at)| *at == 12000.0) {
        e.handle(at, Event::TimerFired { timer });
    }
    assert!(
        e.drain_actions()
            .any(|action| matches!(action, Action::WsSend { socket, text }
        if socket == api && text == r#"{"method":"ping"}"#))
    );
    for _ in 0..2050 {
        flip(&mut e, 13000.0);
    }
    let stable: Vec<_> = e.drain_actions().collect();
    assert!(
        !stable
            .iter()
            .any(|action| matches!(action, Action::WsClose { .. } | Action::WsSend { .. }))
    );
    e.handle(
        13000.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let sent: Vec<_> = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if socket == api => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [r#"{"method":"unsubscribe","subscription":{"type":"candle","coin":"1","interval":"1m"}}"#]
    );
}

#[test]
fn socket_rate_limit_discards_old_queue_and_restores_current_intents_on_reconnect() {
    for (api, error) in [
        (
            false,
            r#"{"type":"error","code":"RISK_RATE_LIMITED","message":"rate limit exceeded"}"#,
        ),
        (
            true,
            r#"{"channel":"error","data":{"code":"RISK_RATE_LIMITED","message":"rate limit exceeded"}}"#,
        ),
        (
            true,
            r#"{"channel":"error","code":"RISK_RATE_LIMITED","data":{}}"#,
        ),
    ] {
        let (mut e, node_socket, api_socket) = ready();
        let socket = if api { api_socket } else { node_socket };
        e.handle(
            3000.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTC-USDC".into(),
            }),
        );
        for interval in ["1m", "3m"] {
            e.handle(
                3000.0,
                Event::Command(Command::SubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
                }),
            );
        }
        let old_timers = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .collect::<Vec<_>>();
        message(&mut e, socket, 3001.0, error);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        );
        let cleared = actions
            .iter()
            .filter_map(|a| match a {
                Action::ClearTimer { timer } => Some(*timer),
                _ => None,
            })
            .collect::<Vec<_>>();
        // Shared pacing has no extra empty-queue arm after the final frame.
        // Only timers actually cleared by this retirement belong to its epoch.
        for (timer, at) in old_timers {
            if cleared.contains(&timer) {
                e.handle(at, Event::TimerFired { timer });
            }
        }
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::WsSend { .. }))
        );
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        assert!(
            (3751.0..=4001.0).contains(&at),
            "jittered first retry: {at}"
        );
        e.handle(at, Event::TimerFired { timer });
        let replacement = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(
            at,
            Event::WsOpened {
                socket: replacement,
            },
        );
        let frames = flush_subscriptions(&mut e, 5000.0);
        assert!(frames.iter().all(|(s, _)| *s == replacement));
        if api {
            assert_eq!(
                frames
                    .iter()
                    .filter(|(_, text)| text.contains("ticker"))
                    .count(),
                1
            );
            assert_eq!(
                frames
                    .iter()
                    .filter(|(_, text)| text.contains("candle"))
                    .count(),
                2
            );
        } else {
            assert_eq!(
                frames
                    .iter()
                    .filter(|(_, text)| text.contains("allAssetCtxs"))
                    .count(),
                1
            );
            assert_eq!(
                frames
                    .iter()
                    .filter(|(_, text)| text.contains("l2Book"))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn absent_node_volume_preserves_rest_seed_and_stream_volume_until_explicit_zero() {
    let mut seed: serde_json::Value = serde_json::from_str(TICKER).unwrap();
    seed["data"]["coinVlm"] = "205.4498".into();
    seed["data"]["dayNtlVlm"] = "16892444.43121".into();
    seed["data"]["nodeCtx"]
        .as_object_mut()
        .unwrap()
        .remove("dayBaseVlm");
    seed["data"]["nodeCtx"]
        .as_object_mut()
        .unwrap()
        .remove("dayNtlVlm");
    let (mut e, node, api) = ready_with_ticker(&seed.to_string());
    let ticker = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!(
        (ticker.volume, ticker.quote_volume),
        (205.4498, 16892444.43121)
    );
    message(
        &mut e,
        node,
        3000.0,
        r#"{"channel":"allAssetCtxs","data":{"ctxs":[{"coin":"1","ctx":{"markPx":"65000","dayBaseVlm":null,"dayNtlVlm":null}}]}}"#,
    );
    let ticker = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!(
        (ticker.volume, ticker.quote_volume),
        (205.4498, 16892444.43121)
    );
    message(
        &mut e,
        api,
        4000.0,
        r#"{"channel":"ticker","data":{"instrumentId":1,"lastPrice":"65000","coinVlm":"300","dayNtlVlm":"19500000"}}"#,
    );
    message(
        &mut e,
        node,
        5000.0,
        r#"{"channel":"activeAssetCtx","data":{"coin":"1","ctx":{"markPx":"65000"}}}"#,
    );
    let ticker = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!((ticker.volume, ticker.quote_volume), (300.0, 19500000.0));
    message(
        &mut e,
        node,
        6000.0,
        r#"{"channel":"activeAssetCtx","data":{"coin":"1","ctx":{"dayBaseVlm":"0","dayNtlVlm":"0"}}}"#,
    );
    let ticker = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!((ticker.volume, ticker.quote_volume), (0.0, 0.0));
}
#[test]
fn historical_metadata_alias_conflicts_reject_before_active_markets_commit() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
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
    assert!(
        r.body
            .as_ref()
            .unwrap()
            .contains("\"includeDelisted\":true")
    );
    let mut meta: serde_json::Value = serde_json::from_str(META).unwrap();
    let mut old = meta["data"]["universe"][0].clone();
    old["index"] = 9.into();
    old["name"] = "OTHER-USDC".into();
    old["delisted"] = true.into();
    meta["data"]["universe"].as_array_mut().unwrap().push(old);
    reply(&mut e, &r, 200, &meta.to_string());
    assert!(e.store().markets().is_empty());
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn ambiguous_native_identity_preserves_store_and_retries_with_backoff() {
    let (previous, _, _) = ready();
    let prior_markets = previous.store().markets().to_vec();
    let prior_tickers = previous.store().tickers().to_vec();
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.store_mut().set_markets(prior_markets.clone());
    e.store_mut().set_tickers(prior_tickers.clone());
    e.start(1000.0);
    let request = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let mut meta: serde_json::Value = serde_json::from_str(META).unwrap();
    let duplicate = meta["data"]["universe"][0].clone();
    meta["data"]["universe"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    reply(&mut e, &request, 200, &meta.to_string());
    assert_eq!(e.store().markets(), prior_markets);
    assert_eq!(e.store().tickers(), prior_tickers);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Duplicate or ambiguous TxFlow"))));
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - 1000.0, 1000.0) => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(2000.0, Event::TimerFired { timer });
    let request = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        2000.0,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: META,
            },
        },
    );
    assert_eq!(e.store().markets(), prior_markets);
    assert_eq!(e.store().tickers(), prior_tickers);
}

#[test]
fn late_rest_seed_preserves_supplied_live_context_fields_and_seed_only_statistics() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    engine.start(1000.);
    let metadata = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut engine, &metadata, 200, META);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.);
    let node = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, url, .. } if url.ends_with("/ws") => Some(*socket),
            _ => None,
        })
        .unwrap();
    let ticker = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    engine.handle(1000., Event::WsOpened { socket: node });
    message(
        &mut engine,
        node,
        1100.,
        r#"{"channel":"activeAssetCtx","data":{"coin":"1","ctx":{"markPx":"70000","funding":"0","midPx":"70001"}}}"#,
    );
    engine.handle(
        1200.,
        Event::HttpResponse {
            request: ticker.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: TICKER,
            },
        },
    );
    let timer = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer {
                timer,
                at_ms: 1225.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(1225., Event::TimerFired { timer });
    let ticker = engine.store().ticker("BTC-USDC").unwrap();
    assert_eq!(
        (ticker.mark, ticker.funding_rate, ticker.mid_price),
        (70000., 0., Some(70001.))
    );
    assert_eq!(
        (ticker.last, ticker.index, ticker.volume),
        (65000., 65000., 3.)
    );
    assert!(engine.store().loaded().tickers);
}
