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
    let actions = support::released_actions(&mut engine, 40.);
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
    let actions = support::released_actions(&mut engine, retry_at + 1.);
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
    let actions = support::released_actions(&mut engine, 40.);
    assert_book_subscription_before_seed(&actions, socket);
}

fn subscribe_book(engine: &mut TestEngine, now: f64, symbol: &str) {
    engine.handle(
        now,
        Event::Command(safe_cex_engine::Command::SubscribeOrderBook {
            symbol: symbol.into(),
        }),
    );
}
fn seed_wake(actions: &[Action], now: f64) -> safe_cex_engine::TimerId {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no seed before released-interest wake"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
            _ => None,
        })
        .unwrap_or_else(|| panic!("real shared handoff timer at {now}: {actions:?}"))
}
fn reconnect_book(engine: &mut TestEngine, now: f64, actions: &[Action]) -> (SocketId, f64) {
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= now + 750. && *at_ms <= now + 1000. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native shared backoff");
    engine.handle(at, Event::TimerFired { timer });
    let socket = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(socket),
            _ => None,
        })
        .expect("replacement public socket");
    engine.handle(at + 1., Event::WsOpened { socket });
    (socket, at + 1.)
}
#[test]
fn handoff_wake_preserves_intervening_book_delta_and_runs_once() {
    use safe_cex_engine::Venue;
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let mut actions: Vec<_> = engine.drain_actions().collect();
    let wake = seed_wake(&actions, 40.);
    support::ws(
        &mut engine,
        socket,
        41.,
        r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":101,"data":{"bids":[[59000,7]],"asks":[]}}"#,
    );
    assert!(requests(&mut engine).is_empty());
    engine.handle(42., Event::TimerFired { timer: wake });
    actions.extend(engine.drain_actions());
    assert_book_subscription_before_seed(&actions, socket);
    let seeds: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(seeds.len(), 1);
    engine.handle(43., Event::TimerFired { timer: wake });
    assert!(requests(&mut engine).is_empty());
    support::snapshot(&mut engine, &seeds[0], 44., 100);
    assert_eq!(
        engine.venue().book("BTCUSDT").unwrap().bids(),
        &[(59000., 7.)]
    );
}
#[test]
fn retired_admitted_seed_survives_reconnect_and_old_wakes_are_inert() {
    let (mut engine, old) = support::started();
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let wake = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 40.);
    engine.handle(
        41.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let retired: Vec<_> = engine.drain_actions().collect();
    engine.handle(42., Event::TimerFired { timer: wake });
    assert!(requests(&mut engine).is_empty());
    let (socket, at) = reconnect_book(&mut engine, 41., &retired);
    let mut fresh: Vec<_> = engine.drain_actions().collect();
    let next = seed_wake(&fresh, at);
    assert_ne!(next, wake);
    engine.handle(at, Event::TimerFired { timer: wake });
    assert!(requests(&mut engine).is_empty());
    engine.handle(at, Event::TimerFired { timer: next });
    fresh.extend(engine.drain_actions());
    assert_book_subscription_before_seed(&fresh, socket);
    assert_eq!(
        fresh
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
    engine.handle(at, Event::TimerFired { timer: next });
    assert!(requests(&mut engine).is_empty());
}
#[test]
fn remove_readd_book_cannot_be_seeded_by_its_old_wake() {
    use safe_cex_engine::Command;
    let (mut engine, _) = support::started();
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let old = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 40.);
    engine.handle(
        41.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(requests(&mut engine).is_empty());
    subscribe_book(&mut engine, 42., "BTCUSDT");
    let fresh = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 42.);
    assert_ne!(old, fresh);
    engine.handle(42., Event::TimerFired { timer: old });
    assert!(requests(&mut engine).is_empty());
    engine.handle(43., Event::TimerFired { timer: fresh });
    assert_eq!(requests(&mut engine).len(), 1);
}
#[test]
fn removing_one_required_book_rearms_the_other_due_seed() {
    use safe_cex_engine::Command;
    let (mut engine, _) = support::started();
    let mut eth = engine.store().market("BTCUSDT").unwrap().clone();
    eth.symbol = "ETHUSDT".into();
    eth.id = "PERP_ETH_USDT".into();
    eth.base = "ETH".into();
    let mut markets = engine.store().markets().to_vec();
    markets.push(eth);
    engine.store_mut().set_markets(markets);
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let a = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 40.);
    subscribe_book(&mut engine, 41., "ETHUSDT");
    let b = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 41.);
    engine.handle(
        42.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let c = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 42.);
    engine.handle(43., Event::TimerFired { timer: a });
    engine.handle(43., Event::TimerFired { timer: b });
    assert!(requests(&mut engine).is_empty());
    engine.handle(43., Event::TimerFired { timer: c });
    let seeds = requests(&mut engine);
    assert_eq!(seeds.len(), 1);
    assert!(seeds[0].url.ends_with("/orderbook/PERP_ETH_USDT"));
}
#[test]
fn unrelated_interests_do_not_bypass_native_book_retry_delay() {
    use safe_cex_engine::{Command, model::Interval};
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let seed = support::released_requests(&mut engine, 40.).remove(0);
    support::ws(
        &mut engine,
        socket,
        50.,
        r#"{"topic":"PERP_BTC_USDT@orderbookupdate","ts":101,"data":{"bids":[[59000,-1]],"asks":[]}}"#,
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let retry = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1050.,
            } => Some(*timer),
            _ => None,
        })
        .unwrap();
    engine.handle(
        51.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    assert!(support::released_requests(&mut engine, 51.).is_empty());
    support::snapshot(&mut engine, &seed, 52., 100);
    assert!(requests(&mut engine).is_empty());
    engine.handle(1050., Event::TimerFired { timer: retry });
    let wake = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 1050.);
    engine.handle(1050., Event::TimerFired { timer: wake });
    assert_eq!(requests(&mut engine).len(), 1);
}

#[test]
fn refused_seed_handoff_recovers_after_real_interest_capacity_is_freed() {
    use safe_cex_engine::socket_lifecycle::MAX_INTEREST_BYTES;
    let (mut engine, old) = support::started();
    let original = engine.store().markets().to_vec();
    let mut expanded = original.clone();
    expanded[0].base = "A".repeat(MAX_INTEREST_BYTES);
    engine.store_mut().set_markets(expanded);
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let refused: Vec<_> = engine.drain_actions().collect();
    assert!(refused.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("Capacity"))));
    assert!(!refused.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(
        refused
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == old))
            .count(),
        1
    );
    engine.store_mut().set_markets(original);
    let (socket, at) = reconnect_book(&mut engine, 40., &refused);
    assert_ne!(socket, old);
    let mut actions: Vec<_> = engine.drain_actions().collect();
    let wake = seed_wake(&actions, at);
    engine.handle(at, Event::TimerFired { timer: wake });
    actions.extend(engine.drain_actions());
    assert_book_subscription_before_seed(&actions, socket);
    let seeds: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(seeds.len(), 1);
    assert!(seeds[0].url.ends_with("/orderbook/PERP_BTC_USDT"));
    engine.handle(at, Event::TimerFired { timer: wake });
    assert!(requests(&mut engine).is_empty());
}

#[test]
fn expired_released_seed_wake_cannot_issue_http() {
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    subscribe_book(&mut engine, 40., "BTCUSDT");
    let wake = seed_wake(&engine.drain_actions().collect::<Vec<_>>(), 40.);
    engine.handle(20030., Event::TimerFired { timer: wake });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket: value } if *value == socket))
            .count(),
        1
    );
}

#[test]
fn first_open_and_reconnect_rebase_two_book_demand_before_unsubscribe_wakes() {
    for reconnect in [false, true] {
        let (mut engine, mut public) = support::started();
        let mut markets = engine.store().markets().to_vec();
        let mut second = markets[0].clone();
        second.symbol = "ETHUSDT".into();
        second.base = "ETH".into();
        markets.push(second);
        engine.store_mut().set_markets(markets);
        engine.drain_actions().for_each(drop);
        subscribe_book(&mut engine, 40., "BTCUSDT");
        engine.drain_actions().for_each(drop);
        subscribe_book(&mut engine, 40., "ETHUSDT");
        let mut actions: Vec<_> = engine.drain_actions().collect();
        let mut now = 40.;
        if reconnect {
            engine.handle(
                41.,
                Event::WsClosed {
                    socket: public,
                    code: 1006,
                },
            );
            actions = engine.drain_actions().collect();
            (public, now) = reconnect_book(&mut engine, 41., &actions);
            actions = engine.drain_actions().collect();
        }
        let old: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
                _ => None,
            })
            .collect();
        assert!(!old.is_empty(), "actual current grouped handoff is issued");
        engine.handle(
            now,
            Event::Command(safe_cex_engine::Command::UnsubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            }),
        );
        let replaced: Vec<_> = engine.drain_actions().collect();
        let current: Vec<_> = replaced
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(
            current.len(),
            1,
            "surviving ETH demand has its actual replacement wake"
        );
        assert!(current.iter().all(|timer| !old.contains(timer)));
        for timer in old {
            engine.handle(now, Event::TimerFired { timer });
            assert!(
                engine.drain_actions().next().is_none(),
                "every superseded wake is inert"
            );
        }
        engine.handle(now, Event::TimerFired { timer: current[0] });
        let http = requests(&mut engine);
        assert_eq!(http.len(), 1);
        assert!(http[0].url.ends_with("/orderbook/PERP_ETH_USDT"));
        subscribe_book(&mut engine, now, "BTCUSDT");
        let additions: Vec<_> = engine.drain_actions().collect();
        assert!(additions.iter().any(|action| matches!(action, Action::WsSend { socket, text } if *socket == public && text.contains("PERP_BTC_USDT@orderbookupdate"))));
        let timer = seed_wake(&additions, now);
        engine.handle(now, Event::TimerFired { timer });
        let http = requests(&mut engine);
        assert_eq!(http.len(), 1);
        assert!(http[0].url.ends_with("/orderbook/PERP_BTC_USDT"));
    }
}

fn full_store_frame(engine: &TestEngine) -> Vec<u8> {
    let store = engine.store();
    let mut writer = safe_cex_engine::wire::FrameWriter::new(vec![], 0.);
    store.write(&store.everything(), &mut writer);
    writer.finish()
}

#[test]
fn nonfinite_socket_observations_cannot_publish_or_schedule_work() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for private in [false, true] {
            let (mut engine, socket) = if private {
                private_open()
            } else {
                support::started()
            };
            engine.drain_actions().for_each(drop);
            let before = full_store_frame(&engine);
            engine.handle(
                now,
                Event::WsMessage {
                    socket,
                    text: if private { ACCEPTED } else { TICKER },
                },
            );
            let actions: Vec<_> = engine.drain_actions().collect();
            assert_eq!(full_store_frame(&engine), before);
            assert!(
                actions.is_empty(),
                "nonfinite ingress has no effects: {actions:?}"
            );
            engine.handle(
                32.,
                Event::WsMessage {
                    socket,
                    text: if private { ACCEPTED } else { TICKER },
                },
            );
            if private {
                assert_eq!(frames(&mut engine, socket).len(), 3);
            } else {
                assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 61_000.);
                assert!(engine.drain_actions().next().is_none());
            }
        }
    }
}

#[test]
fn retired_socket_payload_and_duplicate_close_have_no_effects() {
    let (mut engine, socket) = support::started();
    engine.drain_actions().for_each(drop);
    engine.handle(40., Event::WsClosed { socket, code: 1006 });
    let cleanup: Vec<_> = engine.drain_actions().collect();
    let before = full_store_frame(&engine);
    assert!(
        cleanup
            .iter()
            .any(|action| matches!(action, Action::SetTimer { .. }))
    );
    for event in [
        Event::WsOpened { socket },
        Event::WsMessage {
            socket,
            text: TICKER,
        },
        Event::WsMessage {
            socket,
            text: "not-json",
        },
        Event::WsClosed { socket, code: 1006 },
    ] {
        engine.handle(41., event);
        assert!(engine.drain_actions().next().is_none());
        assert_eq!(full_store_frame(&engine), before);
    }
}
