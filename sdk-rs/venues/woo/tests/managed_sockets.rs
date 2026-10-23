mod support;

use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId};
use safe_cex_venue_woo::{Options, Woo};
use support::{MARKETS, TICKERS, TestEngine, requests, response};

const ACCEPTED: &str = r#"{"event":"auth","success":true}"#;
const TICKER: &str = r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":"61000","volume":5,"amount":305000}]}"#;

fn private_open() -> (TestEngine, SocketId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        application_id: Some("fixture-app".into()),
        ..Default::default()
    }));
    engine.start(0.);
    let market = requests(&mut engine).remove(0);
    response(&mut engine, &market, 10., MARKETS);
    let actions: Vec<_> = engine.drain_actions().collect();
    let socket = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let ticker = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/public/futures") => Some(request),
            _ => None,
        })
        .unwrap();
    response(&mut engine, ticker, 20., TICKERS);
    engine.drain_actions().for_each(drop);
    engine.handle(21., Event::WsOpened { socket });
    (engine, socket)
}

fn frames(engine: &mut Engine<Woo>, target: SocketId) -> Vec<String> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if socket == target => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn duplicate_public_open_cannot_replay_initial_interests() {
    let (mut engine, socket) = support::started();
    assert_eq!(
        frames(&mut engine, socket),
        [
            r#"{"event":"ping"}"#,
            r#"{"event":"subscribe","topic":"tickers"}"#,
            r#"{"event":"subscribe","topic":"bbos"}"#,
            r#"{"event":"subscribe","topic":"markprices"}"#,
        ]
    );
    engine.handle(31., Event::WsOpened { socket });
    assert!(
        frames(&mut engine, socket).is_empty(),
        "duplicate open replayed public interests"
    );
}

#[test]
fn duplicate_private_open_cannot_repeat_authentication_or_reads() {
    let (mut engine, socket) = private_open();
    assert_eq!(frames(&mut engine, socket).len(), 1);
    engine.handle(22., Event::WsOpened { socket });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. } | Action::Http(_))),
        "duplicate private open issued authentication or account reads"
    );
}

#[test]
fn expired_public_data_retires_before_it_can_refresh_or_mutate() {
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    let before = engine.store().ticker("BTCUSDT").unwrap().last;
    engine.handle(
        20_030.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    assert_eq!(
        engine.store().ticker("BTCUSDT").unwrap().last,
        before,
        "expired epoch published a ticker before its timer callback"
    );
    assert_eq!(
        engine
            .drain_actions()
            .filter(
                |action| matches!(action, Action::WsClose { socket: value } if *value == socket)
            )
            .count(),
        1
    );
}

#[test]
fn expired_private_auth_cannot_release_subscriptions() {
    let (mut engine, socket) = private_open();
    engine.drain_actions().for_each(drop);
    engine.handle(
        10_021.,
        Event::WsMessage {
            socket,
            text: ACCEPTED,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions.iter().any(
            |action| matches!(action, Action::WsSend { socket: value, .. } if *value == socket)
        ),
        "expired auth released private subscriptions"
    );
    assert_eq!(
        actions
            .iter()
            .filter(
                |action| matches!(action, Action::WsClose { socket: value } if *value == socket)
            )
            .count(),
        1
    );
}

#[test]
fn live_public_data_keeps_its_native_mapping() {
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    engine.handle(
        31.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 61_000.);
    assert!(frames(&mut engine, socket).is_empty());
}

#[test]
fn live_private_auth_preserves_native_subscription_bytes_and_order() {
    let (mut engine, socket) = private_open();
    assert_eq!(frames(&mut engine, socket).len(), 1);
    engine.handle(
        22.,
        Event::WsMessage {
            socket,
            text: ACCEPTED,
        },
    );
    assert_eq!(
        frames(&mut engine, socket),
        [
            r#"{"topic":"executionreport","event":"subscribe"}"#,
            r#"{"topic":"algoexecutionreportv2","event":"subscribe"}"#,
            r#"{"event":"ping"}"#,
        ]
    );
}
