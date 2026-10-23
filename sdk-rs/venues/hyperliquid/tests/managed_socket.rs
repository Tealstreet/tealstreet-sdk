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

fn private_connecting_before_mode() -> (Engine<Hyperliquid>, SocketId, safe_cex_engine::HttpRequest)
{
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    engine.start(1000.0);
    let mut socket = None;
    let mut mode = None;
    for _ in 0..20 {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return (engine, socket.unwrap(), mode.unwrap());
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    let body = match support::kind(&request).as_str() {
                        "metaAndAssetCtxs" => support::META,
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
    let actions: Vec<_> = engine.drain_actions().collect();
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
    assert!(
        refresh < public,
        "original native trace is private, HTTP, public"
    );
}

#[test]
fn unresolved_mode_does_not_park_the_native_open_refresh() {
    let (mut engine, socket, _mode) = private_connecting_before_mode();
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
    let (mut engine, _socket, mode) = private_connecting_before_mode();
    support::reply(&mut engine, &mode, 200, "\"portfolioMargin\"");
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(a,
        Action::Http(request) if support::kind(request) == "spotClearinghouseState")));
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}

#[test]
fn mode_while_closed_still_refreshes_spot_without_a_subscription() {
    let (mut engine, socket, mode) = private_connecting_before_mode();
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
    assert!(actions.iter().any(|a| matches!(a,
        Action::Http(request) if support::kind(request) == "spotClearinghouseState")));
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}
