//! Managed-socket controls for fast overlays over slow depth.
mod support;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Engine, Event, SocketId};
use safe_cex_venue_hyperliquid::Hyperliquid;

fn ready() -> (Engine<Hyperliquid>, SocketId) {
    ready_options(Some(true))
}
fn ready_options(fast: Option<bool>) -> (Engine<Hyperliquid>, SocketId) {
    use safe_cex_engine::Response;
    use safe_cex_venue_hyperliquid::Options;
    let mut options = Options {
        public_data_only: true,
        ..Options::default()
    };
    if let Some(fast) = fast {
        options.fast_book = fast;
    }
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(options));
    e.start(1000.);
    let mut socket = None;
    for _ in 0..10 {
        for action in prepared_now(&mut e) {
            match action {
                Action::Http(r) => {
                    let body = if support::kind(&r) == "perpDexs" {
                        "[null]"
                    } else {
                        support::META
                    };
                    e.handle(
                        1000.,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect { socket: s, .. } => socket = Some(s),
                _ => {}
            }
        }
        if socket.is_some() {
            break;
        }
    }
    let socket = socket.expect("public native socket");
    e.handle(1000., Event::WsOpened { socket });
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    (e, socket)
}
fn frame(
    e: &mut Engine<Hyperliquid>,
    s: SocketId,
    time: u64,
    fast: bool,
    bids: &[u32],
    asks: &[u32],
) {
    let rows = |prices: &[u32]| {
        prices
            .iter()
            .map(|p| serde_json::json!({"px":p.to_string(),"sz":if fast {"7"} else {"2"}}))
            .collect::<Vec<_>>()
    };
    let text = serde_json::json!({"channel":"l2Book","data":{"coin":"BTC","time":time,"fast":fast,"levels":[rows(bids),rows(asks)]}}).to_string();
    let now = safe_cex_engine::test_support::dispatch_now(e).max(2100.);
    e.handle(
        now,
        Event::WsMessage {
            socket: s,
            text: &text,
        },
    );
}
fn prices(e: &Engine<Hyperliquid>) -> (Vec<f64>, Vec<f64>) {
    let b = e.book("BTC-PERP").unwrap();
    (
        b.bids().iter().map(|r| r.0).collect(),
        b.asks().iter().map(|r| r.0).collect(),
    )
}
#[test]
fn subscribes_fast_slow_bbo_once_and_releases_after_last_reference() {
    let (mut e, _) = ready();
    let sent: Vec<_> = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 3);
    assert!(sent.iter().any(|s| s
        == r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC","fast":true}}"#));
    for command in [
        Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
    ] {
        e.handle(2100., Event::Command(command));
        assert!(
            prepared_now(&mut e)
                .into_iter()
                .all(|a| !matches!(a, Action::WsSend { .. }))
        );
    }
    e.handle(
        2100.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        3
    );
}
#[test]
fn fast_preserves_deep_levels_without_resurrecting_consumed_levels() {
    let (mut e, s) = ready();
    frame(
        &mut e,
        s,
        100,
        false,
        &[100, 99, 98, 97, 96, 95, 94],
        &[101, 102, 103, 104, 105, 106, 107],
    );
    frame(
        &mut e,
        s,
        101,
        true,
        &[101, 100, 99, 98, 97],
        &[102, 103, 104, 105, 106],
    );
    assert_eq!(
        prices(&e),
        (
            vec![101., 100., 99., 98., 97., 96., 95., 94.],
            vec![102., 103., 104., 105., 106., 107.]
        )
    );
    assert_eq!(e.book("BTC-PERP").unwrap().bids()[0].1, 7.);
    frame(
        &mut e,
        s,
        102,
        true,
        &[99, 98, 97, 96, 95],
        &[104, 105, 106, 107, 108],
    );
    frame(
        &mut e,
        s,
        103,
        true,
        &[100, 99, 98, 97, 96],
        &[103, 104, 105, 106, 107],
    );
    assert_eq!(prices(&e).0, vec![100., 99., 98., 97., 96., 94.]);
    assert_eq!(prices(&e).1, vec![103., 104., 105., 106., 107.]);
}
#[test]
fn late_frames_cannot_regress_prices_and_sparse_slow_replaces() {
    let (mut e, s) = ready();
    frame(
        &mut e,
        s,
        100,
        false,
        &[100, 99, 98, 97, 96, 95],
        &[101, 102, 103, 104, 105, 106],
    );
    frame(
        &mut e,
        s,
        110,
        true,
        &[101, 100, 99, 98, 97],
        &[102, 103, 104, 105, 106],
    );
    let before = prices(&e);
    frame(&mut e, s, 105, false, &[80, 79, 78], &[81, 82, 83]);
    frame(&mut e, s, 109, true, &[90], &[91]);
    assert_eq!(prices(&e), before);
    frame(&mut e, s, 111, false, &[90, 89], &[91, 92]);
    assert_eq!(prices(&e), (vec![90., 89.], vec![91., 92.]));
}
#[test]
fn fast_initializes_and_newer_bbo_overlays_without_crossing() {
    let (mut e, s) = ready();
    frame(
        &mut e,
        s,
        100,
        true,
        &[100, 99, 98, 97, 96],
        &[101, 102, 103, 104, 105],
    );
    e.handle(2100.,Event::WsMessage{socket:s,text:r#"{"channel":"bbo","data":{"coin":"BTC","time":101,"bbo":[{"px":"102","sz":"3"},{"px":"103","sz":"4"}]}}"#});
    assert_eq!(
        prices(&e),
        (vec![102., 100., 99., 98., 97., 96.], vec![103., 104., 105.])
    );
    frame(&mut e, s, 102, true, &[90, 89], &[]);
    assert!(prices(&e).1.is_empty());
}
#[test]
fn reconnect_discards_retired_fast_depth_and_replays_topic() {
    let (mut e, s) = ready();
    frame(
        &mut e,
        s,
        100,
        false,
        &[100, 99, 98, 97, 96, 95],
        &[101, 102, 103, 104, 105, 106],
    );
    frame(&mut e, s, 110, true, &[101], &[102]);
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        2200.,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.book("BTC-PERP").is_none());
    let timer = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(10000., Event::TimerFired { timer });
    let next = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(10000., Event::WsOpened { socket: next });
    let sent = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(sent.iter().any(|f| f.contains("\"fast\":true")));
    frame(&mut e, s, 500, true, &[500], &[501]);
    assert!(e.book("BTC-PERP").is_none());
    frame(&mut e, next, 1, true, &[10], &[11]);
    assert_eq!(prices(&e), (vec![10.], vec![11.]));
}

#[test]
fn many_books_stay_under_the_venue_limit_and_refuse_only_the_excess() {
    use safe_cex_engine::{Notice, Response};
    use safe_cex_venue_hyperliquid::Options;
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        fast_book: true,
        ..Options::default()
    }));
    e.start(1000.);
    let universe = (0..333)
        .map(|i| serde_json::json!({"name":format!("C{i}"),"szDecimals":2}))
        .collect::<Vec<_>>();
    let ctx = (0..333)
        .map(|_| serde_json::json!({"midPx":"100"}))
        .collect::<Vec<_>>();
    let meta = serde_json::json!([{"universe":universe},ctx]).to_string();
    let mut socket = None;
    for _ in 0..10 {
        for a in prepared_now(&mut e) {
            match a {
                Action::Http(r) => {
                    let body = if support::kind(&r) == "perpDexs" {
                        "[null]"
                    } else {
                        &meta
                    };
                    e.handle(
                        1000.,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect { socket: s, .. } => socket = Some(s),
                _ => {}
            }
        }
        if socket.is_some() {
            break;
        }
    }
    let socket = socket.unwrap();
    e.handle(1000., Event::WsOpened { socket });
    let mut sent = prepared_now(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::WsSend { .. }))
        .count();
    for i in 0..332 {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: format!("C{i}-PERP"),
            }),
        );
        sent += prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count();
    }
    assert_eq!(sent, 998);
    e.handle(
        2000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "C332-PERP".into(),
        }),
    );
    let refused = prepared_now(&mut e);
    assert!(!refused.iter().any(|a| matches!(a, Action::WsSend { .. })));
    assert!(refused.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::SUBSCRIPTION_REJECTED)));
    e.handle(2100., Event::WsMessage {socket, text: r#"{"channel":"l2Book","data":{"coin":"C332","time":1,"fast":true,"levels":[[{"px":"100","sz":"1"}],[{"px":"101","sz":"1"}]]}}"#});
    assert!(e.book("C332-PERP").is_none());
    e.handle(
        2000.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "C0-PERP".into(),
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "C332-PERP".into(),
        }),
    );
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        3
    );
}

#[test]
fn same_timestamp_fast_wins_in_either_delivery_order_and_bad_frames_are_inert() {
    for fast_first in [false, true] {
        let (mut e, s) = ready();
        if fast_first {
            frame(
                &mut e,
                s,
                100,
                true,
                &[100, 99, 98, 97, 96],
                &[101, 102, 103, 104, 105],
            );
        }
        frame(
            &mut e,
            s,
            100,
            false,
            &[100, 99, 98, 97, 96, 95],
            &[101, 102, 103, 104, 105, 106],
        );
        if !fast_first {
            frame(
                &mut e,
                s,
                100,
                true,
                &[100, 99, 98, 97, 96],
                &[101, 102, 103, 104, 105],
            );
        }
        assert_eq!(e.book("BTC-PERP").unwrap().bids()[0].1, 7.);
        assert_eq!(prices(&e).0, vec![100., 99., 98., 97., 96., 95.]);
        let before = prices(&e);
        for (bid, ask) in [
            (&[102][..], &[101][..]),
            (&[100, 101], &[102]),
            (&[0], &[1]),
            (&[100, 99, 98, 97, 96, 95], &[101]),
        ] {
            frame(&mut e, s, 101, true, bid, ask);
            assert_eq!(prices(&e), before);
        }
    }
}

#[test]
fn keyed_hip3_topic_budget_includes_private_topics_books_and_candles() {
    let (mut e, _, initial) =
        support::ready_with_policy("disabled", safe_cex_engine::PublicFeedPolicy::Direct);
    let topics = |sent: Vec<String>| {
        sent.into_iter()
            .filter(|f| f.contains("\"method\":\"subscribe\""))
            .count()
    };
    // Two core markets, one HIP-3 scope: 9 + 2 + 5 = 16 fixed topics.
    assert_eq!(topics(initial), 16);
    for symbol in ["BTC-PERP", "ETH-PERP", "XYZ|FOO-PERP"] {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: symbol.into(),
            }),
        );
        let sent = prepared_now(&mut e)
            .into_iter()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(topics(sent), 2);
    }
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert_eq!(
        topics(
            prepared_now(&mut e)
                .into_iter()
                .filter_map(|a| match a {
                    Action::WsSend { text, .. } => Some(text),
                    _ => None,
                })
                .collect()
        ),
        1
    );
}

#[test]
fn sparse_fast_side_proves_no_deeper_rows_exist_on_that_side() {
    let (mut e, s) = ready();
    frame(
        &mut e,
        s,
        100,
        false,
        &[100, 99, 98, 97, 96, 95],
        &[101, 102, 103, 104, 105, 106],
    );
    frame(&mut e, s, 101, true, &[100, 99], &[101, 102, 103, 104, 105]);
    assert_eq!(
        prices(&e),
        (vec![100., 99.], vec![101., 102., 103., 104., 105., 106.])
    );
}

#[test]
fn review_3110_excess_candle_rolls_back_without_retiring_accepted_topics() {
    use safe_cex_engine::{Notice, Response};
    use safe_cex_venue_hyperliquid::Options;
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        fast_book: true,
        ..Options::default()
    }));
    e.start(1000.);
    let universe = (0..333)
        .map(|i| serde_json::json!({"name":format!("C{i}"),"szDecimals":2}))
        .collect::<Vec<_>>();
    let ctx = (0..333)
        .map(|_| serde_json::json!({"midPx":"100"}))
        .collect::<Vec<_>>();
    let meta = serde_json::json!([{"universe":universe},ctx]).to_string();
    let mut socket = None;
    for _ in 0..10 {
        for a in prepared_now(&mut e) {
            match a {
                Action::Http(r) => {
                    let body = if support::kind(&r) == "perpDexs" {
                        "[null]"
                    } else {
                        &meta
                    };
                    e.handle(
                        1000.,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect { socket: s, .. } => socket = Some(s),
                _ => {}
            }
        }
        if socket.is_some() {
            break;
        }
    }
    let socket = socket.unwrap();
    e.handle(1000., Event::WsOpened { socket });
    let mut sent = prepared_now(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::WsSend { .. }))
        .count();
    for i in 0..332 {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: format!("C{i}-PERP"),
            }),
        );
        sent += prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count();
    }
    assert_eq!(sent, 998);
    for interval in [
        safe_cex_engine::model::Interval::M1,
        safe_cex_engine::model::Interval::M5,
    ] {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeCandles {
                symbol: "C0-PERP".into(),
                interval,
            }),
        );
        let actions = prepared_now(&mut e);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::WsSend { .. }))
                .count(),
            1,
            "first two candle additions are within capacity"
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    }
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M15,
        }),
    );
    let refused = prepared_now(&mut e);

    assert!(refused.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::SUBSCRIPTION_REJECTED)));
    assert!(
        !refused.iter().any(|a| matches!(a, Action::WsClose { .. })),
        "excess candle must not retire all 1000 accepted topics"
    );
    assert!(!refused.iter().any(|a| matches!(a, Action::WsSend { .. })));
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M15,
        }),
    );
    let again = prepared_now(&mut e);
    assert!(again.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::SUBSCRIPTION_REJECTED)));
    assert!(
        !again
            .iter()
            .any(|a| matches!(a, Action::WsSend { .. } | Action::WsClose { .. }))
    );
    e.handle(
        2000.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        1
    );
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M15,
        }),
    );
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        1
    );
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M5,
        }),
    );
    assert!(
        !prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::WsSend { .. } | Action::WsClose { .. }))
    );
    e.handle(
        2000.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M5,
        }),
    );
    assert!(
        !prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::WsSend { .. } | Action::WsClose { .. }))
    );
    e.handle(
        2000.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M5,
        }),
    );
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        1
    );
}

#[test]
fn review_3110_reconnect_replays_only_the_accepted_thousand_topics() {
    use safe_cex_engine::Response;
    use safe_cex_venue_hyperliquid::Options;
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        fast_book: true,
        ..Options::default()
    }));
    e.start(1000.);
    let universe = (0..333)
        .map(|i| serde_json::json!({"name":format!("C{i}"),"szDecimals":2}))
        .collect::<Vec<_>>();
    let ctx = (0..333)
        .map(|_| serde_json::json!({"midPx":"100"}))
        .collect::<Vec<_>>();
    let meta = serde_json::json!([{"universe":universe},ctx]).to_string();
    let mut socket = None;
    for _ in 0..10 {
        for a in prepared_now(&mut e) {
            match a {
                Action::Http(r) => {
                    let body = if support::kind(&r) == "perpDexs" {
                        "[null]"
                    } else {
                        &meta
                    };
                    e.handle(
                        1000.,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect { socket: s, .. } => socket = Some(s),
                _ => {}
            }
        }
        if socket.is_some() {
            break;
        }
    }
    let socket = socket.unwrap();
    e.handle(1000., Event::WsOpened { socket });
    let mut sent = prepared_now(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::WsSend { .. }))
        .count();
    for i in 0..332 {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: format!("C{i}-PERP"),
            }),
        );
        sent += prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count();
    }
    assert_eq!(sent, 998);
    for interval in [
        safe_cex_engine::model::Interval::M1,
        safe_cex_engine::model::Interval::M5,
    ] {
        e.handle(
            2000.,
            Event::Command(Command::SubscribeCandles {
                symbol: "C0-PERP".into(),
                interval,
            }),
        );
        let actions = prepared_now(&mut e);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::WsSend { .. }))
                .count(),
            1,
            "first two candle additions are within capacity"
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    }
    e.handle(
        2000.,
        Event::Command(Command::SubscribeCandles {
            symbol: "C0-PERP".into(),
            interval: safe_cex_engine::model::Interval::M15,
        }),
    );
    let refused = prepared_now(&mut e);

    assert!(
        !refused.iter().any(|a| matches!(a, Action::WsClose { .. })),
        "rejected demand leaves the old epoch live"
    );
    e.handle(2100., Event::WsClosed { socket, code: 1006 });
    let closed = prepared_now(&mut e);
    let (timer, at) = closed
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("captured genuine close retry");
    e.handle(at, Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    let replacement = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .expect("genuine replacement epoch");
    e.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let replay = prepared_now(&mut e);
    assert!(
        !replay.iter().any(|a| matches!(a, Action::WsClose { .. })),
        "retained rejected candle must not poison replacement replay"
    );
    assert_eq!(
        replay
            .iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        1000
    );
}

#[test]
fn default_options_keep_fast_depth_off() {
    let (mut e, _) = ready_options(None);
    let sent = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 2);
    assert!(!sent.iter().any(|s| s.contains("\"fast\":true")));
}

#[test]
fn opted_in_book_repair_releases_each_of_its_three_native_topics_once() {
    let (mut e, _) = ready();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        2100.,
        Event::Command(Command::RepairSubscription(
            safe_cex_engine::SubscriptionTopic::OrderBook {
                symbol: "BTC-PERP".into(),
            },
        )),
    );
    let sent = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 6);
    for key in [
        r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#,
        r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC","fast":true}}"#,
        r#"{"method":"subscribe","subscription":{"type":"bbo","coin":"BTC"}}"#,
    ] {
        assert_eq!(sent.iter().filter(|f| f.as_str() == key).count(), 1);
        assert_eq!(
            sent.iter()
                .filter(|f| **f == key.replace("subscribe", "unsubscribe"))
                .count(),
            1
        );
    }
}
