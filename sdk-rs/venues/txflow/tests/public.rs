//! Independent payloads from https://docs.txflow-testnet.xyz/api.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, RequestRoute, SocketChannel};
use safe_cex_venue_txflow::{Options, TxFlow};
use support::*;

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
    assert_eq!(frames.iter().filter(|f| f.contains("l2Book")).count(), 1);
    assert_eq!(frames.iter().filter(|f| f.contains("candle")).count(), 1);
}
