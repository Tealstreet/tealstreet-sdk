//! Original-source controls for the whole multiplexed socket boundary.
mod support;

use safe_cex_engine::{Action, Engine, Event, Response, SocketChannel, SocketId};
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
