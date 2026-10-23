//! Original-source controls for the whole multiplexed socket boundary.
mod support;

use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, Response, SocketChannel, SocketId,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const MIDS: &str = r#"{"channel":"allMids","data":{"mids":{"BTC":"123"}}}"#;
const PRIVATE_STATE: &str = r#"{"channel":"clearinghouseState","data":{"user":"x","dex":"","clearinghouseState":{"assetPositions":[{"position":{"coin":"BTC","szi":"3","entryPx":"100","positionValue":"300","unrealizedPnl":"0","marginUsed":"30","leverage":{"type":"isolated","value":8}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"30"},"time":47001}}}"#;

fn public_connecting() -> (Engine<Hyperliquid>, SocketId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(1000.0);
    for _ in 0..8 {
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.is_empty(), "discovery must reach a real socket");
        for action in actions {
            match action {
                Action::Http(request) => {
                    let body = match support::kind(&request).as_str() {
                        "metaAndAssetCtxs" => support::META,
                        "perpDexs" => "[null]",
                        other => panic!("unexpected public read {other}"),
                    };
                    engine.handle(
                        1000.0,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    assert_eq!(channel, Some(SocketChannel::Public));
                    assert_ne!(socket, SocketId(0));
                    return (engine, socket);
                }
                _ => {}
            }
        }
    }
    panic!("discovery did not connect");
}

fn public_open() -> (Engine<Hyperliquid>, SocketId) {
    let (mut engine, socket) = public_connecting();
    engine.handle(1000.0, Event::WsOpened { socket });
    (engine, socket)
}

fn private_connecting_before_mode() -> (
    Engine<Hyperliquid>,
    SocketId,
    safe_cex_engine::HttpRequest,
    safe_cex_engine::TimerId,
) {
    private_connecting_before_mode_with_meta(support::META)
}

fn private_connecting_before_mode_with_meta(
    meta: &str,
) -> (
    Engine<Hyperliquid>,
    SocketId,
    safe_cex_engine::HttpRequest,
    safe_cex_engine::TimerId,
) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    engine.start(1000.0);
    let mut socket = None;
    let mut mode = None;
    let mut mode_timer = None;
    for _ in 0..20 {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return (engine, socket.unwrap(), mode.unwrap(), mode_timer.unwrap());
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    let body = match support::kind(&request).as_str() {
                        "metaAndAssetCtxs" => meta,
                        "perpDexs" => "[null]",
                        "userAbstraction" => {
                            assert!(mode.replace(request).is_none());
                            continue;
                        }
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "clearinghouseState" => support::STATE,
                        "frontendOpenOrders" => "[]",
                        "maxBuilderFee" => "30",
                        other => panic!("unexpected pre-mode read {other}"),
                    };
                    support::reply(&mut engine, &request, 200, body);
                }
                Action::SetTimer {
                    timer,
                    at_ms: 7000.0,
                } => {
                    assert!(mode_timer.replace(timer).is_none());
                }
                Action::WsConnect {
                    socket: current,
                    channel,
                    ..
                } => {
                    assert_eq!(channel, Some(SocketChannel::Private));
                    assert!(socket.replace(current).is_none());
                }
                _ => {}
            }
        }
    }
    panic!("pre-mode discovery did not settle");
}

#[test]
fn accepted_public_open_preserves_exact_native_fixed_wire() {
    let (mut engine, socket) = public_open();
    let sent: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend {
                socket: current,
                text,
            } => {
                assert_eq!(current, socket);
                Some(text)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [
            r#"{"method":"subscribe","subscription":{"type":"allMids"}}"#,
            r#"{"method":"subscribe","subscription":{"type":"fastAssetCtxs"}}"#,
        ]
    );
}

#[test]
fn duplicate_public_open_never_replays_subscriptions() {
    let (mut engine, socket) = public_open();
    engine.drain_actions().for_each(drop);
    engine.handle(2000.0, Event::WsOpened { socket });
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. })),
        "duplicate opening must not replay native topics"
    );
}

#[test]
fn duplicate_private_open_never_replays_topics_or_account_refresh() {
    let (mut engine, socket) = support::ready("disabled");
    engine.drain_actions().for_each(drop);
    engine.handle(2000.0, Event::WsOpened { socket });
    assert!(
        !engine.drain_actions().any(|a| matches!(
            a,
            Action::WsSend { .. } | Action::Http(_) | Action::ManagedHttp(_)
        )),
        "duplicate opening cannot resubscribe or mint a financial refresh"
    );
}

#[test]
fn late_open_cannot_revive_a_connect_without_the_watchdog_turn() {
    let (mut engine, socket) = public_connecting();
    engine.handle(11001.0, Event::WsOpened { socket });
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. })),
        "connect deadline must be checked at ingress"
    );
}

#[test]
fn expired_public_frame_cannot_publish_without_the_watchdog_turn() {
    let (mut engine, socket) = public_open();
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.store().ticker("BTC-PERP").unwrap().last, 100.0);
    engine.handle(46001.0, Event::WsMessage { socket, text: MIDS });
    assert_eq!(
        engine.store().ticker("BTC-PERP").unwrap().last,
        100.0,
        "expired frame must not publish or renew liveness"
    );
}

#[test]
fn live_private_frame_proves_native_financial_publication() {
    let (mut engine, socket) = support::ready("disabled");
    assert_eq!(engine.store().positions()[0].contracts, 2.0);
    engine.handle(
        2000.0,
        Event::WsMessage {
            socket,
            text: PRIVATE_STATE,
        },
    );
    assert_eq!(engine.store().positions()[0].contracts, 3.0);
    assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 8.0);
    assert_eq!(engine.store().balance().used, 30.0);
}

#[test]
fn expired_private_frame_cannot_publish_without_the_watchdog_turn() {
    let (mut engine, socket) = support::ready("disabled");
    engine.drain_actions().for_each(drop);
    engine.handle(
        46001.0,
        Event::WsMessage {
            socket,
            text: PRIVATE_STATE,
        },
    );
    assert_eq!(
        engine.store().positions()[0].contracts,
        2.0,
        "idle-expired private ingress must not mutate financial state"
    );
    assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 10.0);
    assert_eq!(engine.store().balance().used, 20.0);
}

#[test]
fn retired_private_frame_cannot_publish() {
    let (mut engine, socket) = support::ready("disabled");
    engine.handle(2000.0, Event::WsClosed { socket, code: 1006 });
    engine.drain_actions().for_each(drop);
    engine.handle(
        2001.0,
        Event::WsMessage {
            socket,
            text: PRIVATE_STATE,
        },
    );
    assert_eq!(engine.store().positions()[0].contracts, 2.0);
    assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 10.0);
    assert_eq!(engine.store().balance().used, 20.0);
}

#[test]
fn private_socket_keeps_financial_ingress_under_both_non_direct_policies() {
    for policy in [PublicFeedPolicy::Shared, PublicFeedPolicy::Disabled] {
        let (mut engine, socket, sent) = support::ready_with_policy("disabled", policy);
        let topics: Vec<serde_json::Value> = sent
            .iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .collect();
        assert!(
            topics
                .iter()
                .any(|topic| topic["subscription"]["type"] == "orderUpdates")
        );
        assert!(
            !topics
                .iter()
                .any(|topic| topic["subscription"]["type"] == "allMids")
        );
        engine.handle(
            2000.0,
            Event::WsMessage {
                socket,
                text: PRIVATE_STATE,
            },
        );
        assert_eq!(engine.store().positions()[0].contracts, 3.0);
        assert_eq!(engine.store().balance().used, 30.0);
    }
}

#[test]
fn standalone_spot_keeps_all_mids_and_five_native_depth_variants() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new_spot(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(1000.0);
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    assert_eq!(support::kind(&request), "spotMetaAndAssetCtxs");
    support::reply(
        &mut engine,
        &request,
        200,
        r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"coin":"PURR/USDC","midPx":"0.2","markPx":"0.2"}]]"#,
    );
    let socket = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket, channel, ..
            } => {
                assert_eq!(channel, Some(SocketChannel::Public));
                Some(socket)
            }
            _ => None,
        })
        .unwrap();
    engine.handle(1000.0, Event::WsOpened { socket });
    let sent: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [r#"{"method":"subscribe","subscription":{"type":"allMids"}}"#]
    );
    engine.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let depths: Vec<serde_json::Value> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => {
                Some(serde_json::from_str::<serde_json::Value>(&text).unwrap())
            }
            _ => None,
        })
        .collect();
    assert_eq!(depths.len(), 5);
    for (depth, figures) in depths.iter().zip([
        serde_json::Value::Null,
        2.into(),
        3.into(),
        4.into(),
        5.into(),
    ]) {
        assert_eq!(
            *depth,
            serde_json::json!({"method":"subscribe", "subscription":{
                "type":"l2Book", "coin":"PURR/USDC", "nSigFigs":figures
            }})
        );
    }
    engine.handle(
        2001.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    engine.handle(
        2002.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    engine.handle(
        2003.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        }),
    );
    let sent: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(sent,["null","2","3","4","5"].map(|figures|format!(
        r#"{{"method":"unsubscribe","subscription":{{"type":"l2Book","coin":"PURR/USDC","nSigFigs":{figures}}}}}"#)));
}

#[test]
fn private_reconnect_hands_subscriptions_off_before_account_refresh() {
    let (mut engine, socket) = support::ready("disabled");
    engine.drain_actions().for_each(drop);
    engine.handle(2000.0, Event::WsClosed { socket, code: 1006 });
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(4000.0, Event::TimerFired { timer });
    let replacement = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket, channel, ..
            } => {
                assert_eq!(channel, Some(SocketChannel::Private));
                Some(socket)
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement, socket);
    engine.handle(
        4000.0,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let mut actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "refresh waits for the subscription batch handoff"
    );
    let wake = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 4000.0 => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    engine.handle(wake.1, Event::TimerFired { timer: wake.0 });
    actions.extend(engine.drain_actions());
    let refresh = actions
        .iter()
        .position(|a| matches!(a, Action::Http(_)))
        .unwrap();
    let private: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(index, a)| match a {
            Action::WsSend { socket, text } => {
                assert_eq!(*socket, replacement);
                let value: serde_json::Value = serde_json::from_str(text).unwrap();
                value["subscription"]["user"].is_string().then_some(index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        private.len(),
        9,
        "seven fixed private and two active assets"
    );
    assert!(
        private.iter().all(|index| *index < refresh),
        "full-action native handoff order"
    );
    let public = actions
        .iter()
        .position(|a| match a {
            Action::WsSend { text, .. } => text.contains(r#""type":"allMids""#),
            _ => false,
        })
        .unwrap();
    assert!(private.iter().all(|index| *index < public));
    assert!(
        public < refresh,
        "reviewed handoff moves HTTP after public frames and real wake"
    );
}

#[test]
fn unresolved_mode_does_not_park_the_native_open_refresh() {
    let (mut engine, socket, _mode, _mode_timer) = private_connecting_before_mode();
    engine.handle(2000.0, Event::WsOpened { socket });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(a,
        Action::Http(request) if support::kind(request) == "clearinghouseState")));
    assert!(
        !actions.iter().any(|a| match a {
            Action::WsSend { text, .. } => serde_json::from_str::<serde_json::Value>(text).unwrap()
                ["subscription"]["user"]
                .is_string(),
            _ => false,
        }),
        "unresolved mode emits no private subscription obligation"
    );
}

#[test]
fn mode_before_first_open_still_refreshes_spot_without_a_subscription() {
    let (mut engine, _socket, mode, mode_timer) = private_connecting_before_mode();
    support::reply(&mut engine, &mode, 200, "\"portfolioMargin\"");
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == mode_timer))
    );
    assert_eq!(engine.store().balance().total, 0.0);
    assert_eq!(engine.store().balance().used, 0.0);
    assert!(actions.iter().any(|a| matches!(a,
        Action::Http(request) if support::kind(request) == "spotClearinghouseState")));
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}

#[test]
fn mode_while_closed_still_refreshes_spot_without_a_subscription() {
    let (mut engine, socket, mode, mode_timer) = private_connecting_before_mode();
    engine.handle(2000.0, Event::WsClosed { socket, code: 1006 });
    engine.drain_actions().for_each(drop);
    engine.handle(
        2001.0,
        Event::HttpResponse {
            request: mode.id,
            response: Response {
                status: 200,
                headers: "",
                body: "\"portfolioMargin\"",
            },
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == mode_timer))
    );
    assert_eq!(engine.store().balance().total, 0.0);
    assert_eq!(engine.store().balance().used, 0.0);
    assert!(actions.iter().any(|a| matches!(a,
        Action::Http(request) if support::kind(request) == "spotClearinghouseState")));
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}

#[test]
fn mode_after_open_clears_its_timer_then_hands_private_topics_off_before_spot_read() {
    let (mut engine, socket, mode, mode_timer) = private_connecting_before_mode();
    engine.handle(2000.0, Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    engine.handle(
        2001.0,
        Event::HttpResponse {
            request: mode.id,
            response: Response {
                status: 200,
                headers: "",
                body: "\"portfolioMargin\"",
            },
        },
    );
    let mut actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == mode_timer))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(engine.store().balance().total, 0.0);
    let private: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(index, a)| match a {
            Action::WsSend { text, .. } => {
                let value: serde_json::Value = serde_json::from_str(text).unwrap();
                value["subscription"]["user"].is_string().then_some(index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(private.len(), 10);
    let wake = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2001.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    engine.handle(2001.0, Event::TimerFired { timer: wake });
    actions.extend(engine.drain_actions());
    let (read_index, read) = actions
        .iter()
        .enumerate()
        .find_map(|(index, a)| match a {
            Action::Http(request) if support::kind(request) == "spotClearinghouseState" => {
                Some((index, request))
            }
            _ => None,
        })
        .unwrap();
    assert!(private.iter().all(|index| *index < read_index));
    engine.handle(2002.0, Event::HttpResponse {
        request:read.id,
        response:Response { status:200, headers:"", body:r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"# },
    });
    assert_eq!(engine.store().balance().total, 1100.0);
    assert_eq!(engine.store().balance().free, 1050.0);
    engine.drain_actions().for_each(drop);
    engine.handle(7000.0, Event::TimerFired { timer: mode_timer });
    assert!(
        !engine.drain_actions().any(|a| matches!(a, Action::Http(_))),
        "retired mode timer is inert"
    );
}

#[test]
fn retired_mode_handoff_never_starts_its_spot_read() {
    let (mut engine, socket, mode, _mode_timer) = private_connecting_before_mode();
    engine.handle(2000.0, Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    engine.handle(
        2001.0,
        Event::HttpResponse {
            request: mode.id,
            response: Response {
                status: 200,
                headers: "",
                body: "\"portfolioMargin\"",
            },
        },
    );
    let wake = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 2001.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(2001.0, Event::WsClosed { socket, code: 1006 });
    let retired: Vec<_> = engine.drain_actions().collect();
    engine.handle(2002.0, Event::TimerFired { timer: wake });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(engine.store().balance().total, 0.0);
    let (retry, at) = retired
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 2001.0 && *at_ms <= 18001.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    engine.handle(at, Event::TimerFired { timer: retry });
    let replacement = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    engine.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let mut released: Vec<_> = engine.drain_actions().collect();
    assert!(!released.iter().any(|a| matches!(a, Action::Http(_))));
    let fresh = released
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap();
    assert_ne!(fresh, wake);
    engine.handle(at, Event::TimerFired { timer: wake });
    assert!(
        !engine.drain_actions().any(|a| matches!(a, Action::Http(_))),
        "old-epoch wake cannot consume the due work before its fresh handoff"
    );
    engine.handle(at, Event::TimerFired { timer: fresh });
    released.extend(engine.drain_actions());
    let reads: Vec<_> = released
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) if support::kind(request) == "spotClearinghouseState" => {
                Some(request)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        reads.len(),
        1,
        "retired native mode work resumes once after fresh handoff"
    );
    engine.handle(at, Event::TimerFired { timer: wake });
    engine.handle(at, Event::TimerFired { timer: fresh });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(at,Event::HttpResponse{
        request:reads[0].id,
        response:Response{status:200,headers:"",body:r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#},
    });
    assert_eq!(engine.store().balance().total, 1100.0);
    assert_eq!(engine.store().balance().free, 1050.0);
}

#[test]
fn final_perps_book_unsubscribe_preserves_native_topic_order() {
    let (mut engine, _) = public_open();
    engine.drain_actions().for_each(drop);
    engine.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    let sent: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    let expected = [
        r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#,
        r#"{"method":"subscribe","subscription":{"type":"bbo","coin":"BTC"}}"#,
    ];
    assert_eq!(sent, expected);
    engine.handle(
        2001.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    engine.handle(
        2002.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    engine.handle(
        2003.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    let sent: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        expected.map(|text| text.replace("subscribe", "unsubscribe"))
    );
}

#[test]
fn refused_mode_handoff_recovers_its_spot_refresh_after_capacity_is_freed() {
    use safe_cex_engine::socket_lifecycle::MAX_INTERESTS;
    let universe: Vec<_> = (0..MAX_INTERESTS)
        .map(|index| {
            let name = match index {
                0 => "BTC".into(),
                1 => "ETH".into(),
                _ => format!("ASSET{index}"),
            };
            format!(r#"{{"name":"{name}","szDecimals":5,"maxLeverage":50}}"#)
        })
        .collect();
    let contexts = vec![r#"{"midPx":"100","markPx":"100"}"#; MAX_INTERESTS];
    let meta = format!(
        r#"[{{"universe":[{}]}},[{}]]"#,
        universe.join(","),
        contexts.join(",")
    );
    let (mut engine, socket, mode, mode_timer) = private_connecting_before_mode_with_meta(&meta);
    assert_eq!(engine.store().markets().len(), MAX_INTERESTS);
    engine.handle(2000.0, Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    engine.handle(
        2001.0,
        Event::HttpResponse {
            request: mode.id,
            response: Response {
                status: 200,
                headers: "",
                body: "\"portfolioMargin\"",
            },
        },
    );
    let mut refused: Vec<_> = engine.drain_actions().collect();
    assert!(
        refused.iter().any(
            |a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event))
        if event.message.contains("Capacity"))
        ),
        "real interest-count capacity refusal"
    );
    assert!(
        refused
            .iter()
            .any(|a| matches!(a,Action::ClearTimer{timer} if *timer==mode_timer))
    );
    assert!(!refused.iter().any(
        |a| matches!(a,Action::Http(request) if support::kind(request)=="spotClearinghouseState")
    ));
    let markets = engine.store().markets().iter().take(2).cloned().collect();
    engine.store_mut().set_markets(markets);
    engine.handle(2002.0, Event::WsClosed { socket, code: 1006 });
    refused.extend(engine.drain_actions());
    let (retry, at) = refused
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 2001.0 && *at_ms <= 18002.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("shared reconnect after refusal or transport close");
    engine.handle(at, Event::TimerFired { timer: retry });
    let replacement = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    engine.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let mut released: Vec<_> = engine.drain_actions().collect();
    assert!(!released.iter().any(|a| matches!(a, Action::Http(_))));
    let wake = released
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap();
    engine.handle(at, Event::TimerFired { timer: wake });
    released.extend(engine.drain_actions());
    let read = released
        .iter()
        .find_map(|a| match a {
            Action::Http(request) if support::kind(request) == "spotClearinghouseState" => {
                Some(request)
            }
            _ => None,
        })
        .expect("native mode completion survives refusal and fresh shared handoff");
    engine.handle(at,Event::HttpResponse {
        request:read.id,
        response:Response{status:200,headers:"",body:r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#},
    });
    assert_eq!(engine.store().balance().total, 1100.0);
    assert_eq!(engine.store().balance().free, 1050.0);
}
