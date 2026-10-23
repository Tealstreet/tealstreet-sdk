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

#[test]
fn current_private_execution_reaches_the_unchanged_native_mapper() {
    let (mut engine, socket) = private_open();
    engine.drain_actions().for_each(drop);
    engine.handle(
        22.,
        Event::WsMessage {
            socket,
            text: r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":123,"status":"NEW","side":"BUY","type":"LIMIT","quantity":0.02,"price":59000}}"#,
        },
    );
    assert!(engine.store().order("123").is_some());
}

#[test]
fn reconnect_replays_book_interest_and_keeps_native_seed_buffer_fences() {
    use safe_cex_engine::{Command, Venue};
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    engine.handle(
        40.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let old_seed = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/orderbook/") => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    assert!(actions.iter().any(|action| matches!(action, Action::WsSend { text, .. } if text == r#"{"event":"subscribe","topic":"PERP_BTC_USDT@orderbookupdate"}"#)));
    engine.handle(41., Event::WsClosed { socket, code: 1006 });
    assert!(engine.venue().book("BTCUSDT").is_none());
    let (timer, retry_at) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if (791. ..=1_041.).contains(&at_ms) => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .unwrap();
    support::snapshot(&mut engine, &old_seed, 42., 100);
    assert!(engine.venue().book("BTCUSDT").is_none());
    engine.drain_actions().for_each(drop);
    engine.handle(retry_at, Event::TimerFired { timer });
    let next = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect {
                socket: value,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(value),
            _ => None,
        })
        .unwrap();
    assert_ne!(next, socket);
    engine.handle(retry_at + 1., Event::WsOpened { socket: next });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_book_subscription_before_seed(&actions, next);
    let replay: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend {
                socket: value,
                text,
            } if *value == next => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        replay,
        [
            r#"{"event":"ping"}"#,
            r#"{"event":"subscribe","topic":"tickers"}"#,
            r#"{"event":"subscribe","topic":"bbos"}"#,
            r#"{"event":"subscribe","topic":"markprices"}"#,
            r#"{"event":"subscribe","topic":"PERP_BTC_USDT@orderbookupdate"}"#,
        ]
    );
    let seed = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/orderbook/") => Some(request.clone()),
            _ => None,
        })
        .unwrap();
    let update = r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":201,"data":{"bids":[[59000,4]],"asks":[[61000,0]]}}"#;
    support::ws(&mut engine, next, retry_at + 2., update);
    assert!(engine.venue().book("BTCUSDT").is_none());
    support::snapshot(&mut engine, &seed, retry_at + 3., 200);
    assert_eq!(
        engine.venue().book("BTCUSDT").unwrap().bids(),
        &[(59_000., 4.)]
    );
    support::ws(
        &mut engine,
        socket,
        retry_at + 4.,
        &update.replace("59000,4", "59000,9"),
    );
    assert_eq!(
        engine.venue().book("BTCUSDT").unwrap().bids(),
        &[(59_000., 4.)]
    );
}

#[test]
fn managed_candle_interests_preserve_reference_counts() {
    use safe_cex_engine::{Command, model::Interval};
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    for time in [40., 41.] {
        engine.handle(
            time,
            Event::Command(Command::SubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
            }),
        );
        let sent = frames(&mut engine, socket);
        if time == 40. {
            assert_eq!(
                sent,
                [r#"{"event":"subscribe","topic":"PERP_BTC_USDT@kline_1m"}"#]
            );
        } else {
            assert!(sent.is_empty());
        }
    }
    for time in [42., 43.] {
        engine.handle(
            time,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
            }),
        );
        let sent = frames(&mut engine, socket);
        if time == 43. {
            assert_eq!(
                sent,
                [r#"{"event":"unsubscribe","topic":"PERP_BTC_USDT@kline_1m"}"#]
            );
        } else {
            assert!(sent.is_empty());
        }
    }
}

#[test]
fn malformed_auth_retires_without_releasing_private_interests() {
    for malformed in [
        r#"{"event":"auth"}"#,
        r#"{"event":"auth","success":"true"}"#,
    ] {
        let (mut engine, socket) = private_open();
        engine.drain_actions().for_each(drop);
        engine.handle(
            22.,
            Event::WsMessage {
                socket,
                text: malformed,
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(
            |action| matches!(action, Action::WsSend { socket: value, .. } if *value == socket)
        ));
        assert_eq!(
            actions
                .iter()
                .filter(
                    |action| matches!(action, Action::WsClose { socket: value } if *value == socket)
                )
                .count(),
            1
        );
        engine.handle(
            23.,
            Event::WsMessage {
                socket,
                text: ACCEPTED,
            },
        );
        assert!(frames(&mut engine, socket).is_empty());
    }
}

fn assert_book_subscription_before_seed(actions: &[Action], socket: SocketId) {
    let subscribe = actions.iter().position(|action| matches!(action,
        Action::WsSend { socket: owner, text } if *owner == socket && text == r#"{"event":"subscribe","topic":"PERP_BTC_USDT@orderbookupdate"}"#
    )).expect("book subscription");
    let seed = actions
        .iter()
        .position(|action| {
            matches!(action,
                Action::Http(request) if request.url.contains("/orderbook/")
            )
        })
        .expect("book HTTP seed");
    assert!(
        subscribe < seed,
        "book HTTP seed preceded subscription release: subscribe={subscribe}, seed={seed}"
    );
}

#[test]
fn dynamic_book_subscription_is_released_before_its_http_seed() {
    use safe_cex_engine::Command;
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    engine.handle(
        40.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_book_subscription_before_seed(&actions, socket);
}
