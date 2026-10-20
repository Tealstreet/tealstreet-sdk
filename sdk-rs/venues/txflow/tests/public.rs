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
        let mut e = Engine::new(TxFlow::new(opts));
        e.start(1000.0);
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
    }
}
#[test]
fn public_requests_carry_version_and_public_route() {
    let mut e = Engine::new(TxFlow::new(Options {
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
    let mut e = Engine::new(TxFlow::new(Options {
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
    reply(&mut e, &r, 200, TICKER);
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
    let sockets = e
        .drain_actions()
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
    let mut e = Engine::new(TxFlow::new(Options {
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
            interval: "1m".into(),
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
            interval: "1m".into(),
            limit: Some(2),
            from_ms: Some(0.0),
            to_ms: Some(120000.0),
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
                Some(result.unwrap())
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
    let (mut e, node, _) = ready();
    e.handle(2000.0, Event::WsOpened { socket: node });
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer {
                timer,
                at_ms: 12000.0,
            } = a
            {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    message(&mut e, node, 11999.0, r#"{"method":"PONG"}"#);
    e.handle(12000.0, Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(
        |a| matches!(a,Action::WsSend{socket,text}if *socket==node&&text=="{\"method\":\"ping\"}")
    ));
    let next = actions
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(50000.0, Event::TimerFired { timer: next });
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
    let mut e = Engine::new(TxFlow::new(Options {
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
    let mut e = Engine::new(TxFlow::new(Options {
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
    let mut e = Engine::new(TxFlow::new(Options {
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
        &TICKER.replace("\"instrumentId\":1", "\"instrumentId\":2"),
    );
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn book_and_chart_intents_before_metadata_subscribe_once_when_ready() {
    let mut e = Engine::new(TxFlow::new(Options {
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
            interval: "1m".into(),
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
    reply(&mut e, &r, 200, TICKER);
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
    let sockets = e
        .drain_actions()
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

fn unopened_public_sockets(
    ids: &[u32],
) -> (
    Engine<TxFlow>,
    safe_cex_engine::SocketId,
    safe_cex_engine::SocketId,
) {
    let mut meta: serde_json::Value = serde_json::from_str(META).unwrap();
    let template = meta["data"]["universe"][0].clone();
    meta["data"]["universe"] = ids
        .iter()
        .map(|id| {
            let mut row = template.clone();
            row["index"] = (*id).into();
            row["name"] = format!("M{id}-USDC").into();
            row["baseCurrency"] = format!("M{id}").into();
            row
        })
        .collect();
    let mut e = Engine::new(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
    let mut now = 1000.0;
    let mut api = None;
    let mut node = None;
    for _ in 0..100 {
        for action in e.drain_actions().collect::<Vec<_>>() {
            match action {
                Action::Http(request) => {
                    let body: serde_json::Value =
                        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                    let response = if body["type"] == "perpMeta" {
                        meta.to_string()
                    } else {
                        assert_eq!(body["type"], "activeAssetCtx");
                        let mut ticker: serde_json::Value = serde_json::from_str(TICKER).unwrap();
                        ticker["data"]["instrumentId"] = body["coin"]
                            .as_str()
                            .unwrap()
                            .parse::<u32>()
                            .unwrap()
                            .into();
                        ticker.to_string()
                    };
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: request.id,
                            response: safe_cex_engine::Response {
                                status: 200,
                                headers: "",
                                body: &response,
                            },
                        },
                    );
                }
                Action::SetTimer { timer, at_ms } => {
                    now = at_ms;
                    e.handle(now, Event::TimerFired { timer });
                }
                Action::WsConnect { socket, url, .. } if url.ends_with("/api-ws") => {
                    api = Some(socket)
                }
                Action::WsConnect { socket, url, .. } if url.ends_with("/ws") => {
                    node = Some(socket)
                }
                _ => {}
            }
        }
        if api.is_some() {
            break;
        }
    }
    (e, node.unwrap(), api.unwrap())
}

#[test]
fn api_tickers_follow_numeric_instrument_order_on_initial_and_replacement_sockets() {
    let ids = [65, 2, 6, 10, 1, 100, 3, 11, 12, 8, 9, 7];
    let (mut e, _, mut api) = unopened_public_sockets(&ids);
    for opened_at in [3000.0, 6000.0] {
        e.handle(opened_at, Event::WsOpened { socket: api });
        let frames = flush_subscriptions(&mut e, opened_at + 150.0 * (ids.len() - 1) as f64);
        let subscriptions = frames
            .iter()
            .map(|(socket, text)| {
                assert_eq!(*socket, api);
                let frame: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(frame["method"], "subscribe");
                assert!(frame.get("type").is_none());
                frame["subscription"].clone()
            })
            .collect::<Vec<_>>();
        let mut sorted = ids;
        sorted.sort_unstable();
        assert_eq!(
            subscriptions,
            sorted
                .iter()
                .map(|id| serde_json::json!({
                    "type": "ticker", "coin": format!("M{id}"), "instrumentId": id,
                }))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            e.store()
                .markets()
                .iter()
                .map(|market| market.id.parse::<u32>().unwrap())
                .collect::<Vec<_>>(),
            ids
        );
        if opened_at == 3000.0 {
            e.handle(
                5000.0,
                Event::WsClosed {
                    socket: api,
                    code: 1006,
                },
            );
            let (timer, at) = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(at, 6000.0);
            e.handle(at, Event::TimerFired { timer });
            api = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::WsConnect {
                        socket,
                        channel: Some(SocketChannel::Public),
                        ..
                    } => Some(socket),
                    _ => None,
                })
                .unwrap();
        }
    }
}

#[test]
fn restored_books_follow_numeric_instrument_order() {
    assert_restored_intent_order(false);
}

#[test]
fn restored_candles_follow_instrument_then_interval_order() {
    assert_restored_intent_order(true);
}

fn assert_restored_intent_order(api: bool) {
    let ids = [65, 2, 6, 10, 1, 100, 3, 11, 12, 8, 9, 7];
    let (mut e, node, api_socket) = unopened_public_sockets(&ids);
    let mut socket = if api { api_socket } else { node };
    let candles = [
        (65, "5m"),
        (2, "5m"),
        (2, "1m"),
        (10, "15m"),
        (1, "1m"),
        (1, "15m"),
    ];
    for id in ids {
        e.handle(
            2000.0,
            Event::Command(Command::SubscribeOrderBook {
                symbol: format!("M{id}-USDC"),
            }),
        );
    }
    for (id, interval) in candles {
        e.handle(
            2000.0,
            Event::Command(Command::SubscribeCandles {
                symbol: format!("M{id}-USDC"),
                interval: interval.into(),
            }),
        );
    }
    assert!(
        !e.drain_actions()
            .any(|action| matches!(action, Action::WsSend { .. }))
    );
    let mut sorted_ids = ids;
    sorted_ids.sort_unstable();
    let expected = if api {
        let mut expected = sorted_ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "type": "ticker", "coin": format!("M{id}"), "instrumentId": id,
                })
            })
            .collect::<Vec<_>>();
        let mut sorted = candles;
        sorted.sort_unstable();
        expected.extend(sorted.iter().map(|(id, interval)| {
            serde_json::json!({
                "type": "candle", "coin": id.to_string(), "interval": interval,
            })
        }));
        expected
    } else {
        let mut expected = vec![serde_json::json!({"type": "allAssetCtxs"})];
        expected.extend(sorted_ids.iter().map(|id| {
            serde_json::json!({
                "type": "l2Book", "coin": id.to_string(),
            })
        }));
        expected
    };
    for opened_at in [3000.0, 8000.0] {
        e.handle(opened_at, Event::WsOpened { socket });
        let frames = flush_subscriptions(&mut e, opened_at + (expected.len() - 1) as f64 * 150.0);
        let subscriptions = frames
            .iter()
            .map(|(sent_socket, text)| {
                assert_eq!(*sent_socket, socket);
                let frame: serde_json::Value = serde_json::from_str(text).unwrap();
                assert_eq!(frame["method"], "subscribe");
                if api {
                    assert!(frame.get("type").is_none());
                } else {
                    assert_eq!(frame["type"], frame["subscription"]["type"]);
                }
                frame["subscription"].clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(subscriptions, expected);
        if opened_at == 3000.0 {
            e.handle(7000.0, Event::WsClosed { socket, code: 1006 });
            let (timer, at) = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(at, 8000.0);
            e.handle(at, Event::TimerFired { timer });
            socket = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::WsConnect {
                        socket,
                        channel: Some(SocketChannel::Public),
                        ..
                    } => Some(socket),
                    _ => None,
                })
                .unwrap();
        }
    }
}

#[test]
fn subscription_fifo_sends_one_frame_per_150ms_and_paces_unsubscribe() {
    let (mut e, _, api) = ready();
    for interval in ["5m", "1m", "3m"] {
        e.handle(
            3000.0,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTC-USDC".into(),
                interval: interval.into(),
            }),
        );
    }
    e.handle(
        3000.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTC-USDC".into(),
            interval: "1m".into(),
        }),
    );
    for (index, (method, interval)) in [
        ("subscribe", "5m"),
        ("subscribe", "1m"),
        ("subscribe", "3m"),
        ("unsubscribe", "1m"),
    ]
    .into_iter()
    .enumerate()
    {
        let actions = e.drain_actions().collect::<Vec<_>>();
        let sent = actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { socket, text } => Some((*socket, text)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, api);
        let frame: serde_json::Value = serde_json::from_str(sent[0].1).unwrap();
        assert_eq!(frame["method"], method);
        assert_eq!(frame["subscription"]["interval"], interval);
        let at = 3150.0 + index as f64 * 150.0;
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.handle(at, Event::TimerFired { timer });
    }
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}

#[test]
fn subscription_backlog_is_bounded_and_never_holds_up_heartbeat() {
    let (mut e, _, api) = ready();
    e.handle(3000.0, Event::WsOpened { socket: api });
    let ping = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 13000.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    for _ in 0..100 {
        for add in [true, false] {
            let command = if add {
                Command::SubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: "1m".into(),
                }
            } else {
                Command::UnsubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: "1m".into(),
                }
            };
            e.handle(3000.0, Event::Command(command));
        }
    }
    e.drain_actions().for_each(drop);
    e.handle(13000.0, Event::TimerFired { timer: ping });
    assert!(e.drain_actions().any(|a| matches!(a, Action::WsSend { socket, text } if socket == api && text == r#"{"method":"ping"}"#)));
    for _ in 0..2050 {
        for add in [true, false] {
            let command = if add {
                Command::SubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: "1m".into(),
                }
            } else {
                Command::UnsubscribeCandles {
                    symbol: "BTC-USDC".into(),
                    interval: "1m".into(),
                }
            };
            e.handle(13000.0, Event::Command(command));
        }
    }
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == api))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
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
                    interval: interval.into(),
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
        assert!(old_timers.iter().any(|(timer, _)| cleared.contains(timer)));
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
        assert_eq!(at, 4001.0);
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
    let mut e = Engine::new(TxFlow::new(Options {
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
