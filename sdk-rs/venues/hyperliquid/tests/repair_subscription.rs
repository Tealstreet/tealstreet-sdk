//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, Response, SocketChannel, SubscriptionTopic,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const SPOT_META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"coin":"PURR/USDC","midPx":"0.2","markPx":"0.2"}]]"#;

/// Socket frames and repair warnings one step produced.
#[derive(Default)]
struct Step {
    sent: Vec<String>,
    warnings: Vec<String>,
    events: Vec<safe_cex_engine::log::LogEvent>,
    other_io: usize,
}

fn step(engine: &mut Engine<Hyperliquid>, now: f64, command: Option<Command>) -> Step {
    if let Some(command) = command {
        engine.handle(now, Event::Command(command));
    }
    let mut step = Step::default();
    for action in prepared_now(engine) {
        match action {
            Action::WsSend { text, .. } => step.sent.push(text),
            Action::Notice(Notice::Event(event)) => {
                if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn {
                    step.warnings.push(event.message.clone());
                }
                step.events.push(event);
            }
            Action::Http(_)
            | Action::WsConnect { .. }
            | Action::WsClose { .. }
            | Action::SetTimer { .. } => step.other_io += 1,
            _ => {}
        }
    }
    step
}

/// A public-data engine with its socket open and fixed topics flushed.
fn open(spot: bool) -> Engine<Hyperliquid> {
    open_with_meta(spot, support::META).0
}
fn open_with_meta(spot: bool, meta: &str) -> (Engine<Hyperliquid>, safe_cex_engine::SocketId) {
    let options = Options {
        public_data_only: true,
        ..Options::default()
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    });
    engine.start(1000.0);
    for _ in 0..8 {
        for action in prepared_now(&mut engine) {
            match action {
                Action::Http(request) => {
                    let body = match support::kind(&request).as_str() {
                        "metaAndAssetCtxs" => meta,
                        "spotMetaAndAssetCtxs" => SPOT_META,
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
                    engine.handle(1000.0, Event::WsOpened { socket });
                    step(&mut engine, 1000.0, None);
                    return (engine, socket);
                }
                _ => {}
            }
        }
    }
    panic!("discovery did not connect");
}

fn unsubscribe_frame(key: &str) -> String {
    key.replacen(r#""method":"subscribe""#, r#""method":"unsubscribe""#, 1)
}

/// Repairing `topic` re-sends exactly `keys`, each unsubscribed once before it
/// is subscribed once, and leaves the topic's single subscriber in place.
fn assert_repair_resubscribes_once(
    spot: bool,
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    keys: &[&str],
) {
    let mut engine = open(spot);
    let subscribed = step(&mut engine, 2000.0, Some(subscribe));
    for key in keys {
        assert!(
            subscribed.sent.iter().any(|f| f == key),
            "{key} in {:?}",
            subscribed.sent
        );
    }

    let repaired = step(
        &mut engine,
        2001.0,
        Some(Command::RepairSubscription(topic)),
    );
    assert!(repaired.warnings.is_empty(), "{:?}", repaired.warnings);
    assert_eq!(repaired.sent.len(), 2 * keys.len(), "{:?}", repaired.sent);
    for key in keys {
        let position = |frame: &str| {
            let at: Vec<_> = (0..repaired.sent.len())
                .filter(|i| repaired.sent[*i] == frame)
                .collect();
            assert_eq!(at.len(), 1, "{frame} in {:?}", repaired.sent);
            at[0]
        };
        assert!(position(&unsubscribe_frame(key)) < position(key));
    }

    // Intent is unchanged: the one subscriber still releases each stream once.
    let released = step(&mut engine, 2002.0, Some(unsubscribe));
    for key in keys {
        let frame = unsubscribe_frame(key);
        let naming = released.sent.iter().filter(|f| **f == frame).count();
        assert_eq!(naming, 1, "{frame} in {:?}", released.sent);
    }
    assert!(step(&mut engine, 2003.0, None).sent.is_empty());
}

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTC-PERP".into(),
        interval: Interval::M5,
    }
}

fn book(symbol: &str) -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: symbol.into(),
    }
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        false,
        Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: Interval::M5,
        },
        candles(),
        &[
            r#"{"method":"subscribe","subscription":{"type":"candle","coin":"BTC","interval":"5m"}}"#,
        ],
    );
}

#[test]
fn repairing_a_live_perps_book_resubscribes_its_l2_book_and_bbo() {
    assert_repair_resubscribes_once(
        false,
        Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        book("BTC-PERP"),
        &[
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#,
            r#"{"method":"subscribe","subscription":{"type":"bbo","coin":"BTC"}}"#,
        ],
    );
}

#[test]
fn repairing_a_live_spot_book_resubscribes_every_depth_tier() {
    assert_repair_resubscribes_once(
        true,
        Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        },
        book("PURR/USDC"),
        &[
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":null}}"#,
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":2}}"#,
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":3}}"#,
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":4}}"#,
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":5}}"#,
        ],
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book("BTC-PERP")] {
        let mut engine = open(false);
        let repaired = step(
            &mut engine,
            2000.0,
            Some(Command::RepairSubscription(topic.clone())),
        );
        assert!(repaired.sent.is_empty(), "{:?}", repaired.sent);
        assert_eq!(repaired.warnings.len(), 1, "{:?}", repaired.warnings);
        let warning = &repaired.warnings[0];
        assert!(
            warning.starts_with(&format!(
                "Hyperliquid cannot repair the {topic} subscription"
            )),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}

const REPAIR_META: &str = r#"[{"universe":[{"name":"HYPE","szDecimals":5,"maxLeverage":50},{"name":"DOGE","szDecimals":4,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100","prevDayPx":"90"},{"midPx":"200","markPx":"200","oraclePx":"200"}]]"#;
const UI_INTERVALS: [Interval; 14] = [
    Interval::M1,
    Interval::M3,
    Interval::M5,
    Interval::M15,
    Interval::M30,
    Interval::H1,
    Interval::H2,
    Interval::H4,
    Interval::H8,
    Interval::H12,
    Interval::D1,
    Interval::D3,
    Interval::W1,
    Interval::Mo1,
];
fn candle_key(coin: &str, interval: Interval) -> String {
    format!(
        r#"{{"method":"subscribe","subscription":{{"type":"candle","coin":"{coin}","interval":"{}"}}}}"#,
        interval.as_str()
    )
}
fn repair_candle(
    engine: &mut Engine<Hyperliquid>,
    now: f64,
    symbol: &str,
    interval: Interval,
) -> Step {
    step(
        engine,
        now,
        Some(Command::RepairSubscription(SubscriptionTopic::Candles {
            symbol: symbol.into(),
            interval,
        })),
    )
}
#[test]
fn every_ui_interval_repairs_hype_and_doge_with_the_original_native_key() {
    for (symbol, coin) in [("HYPE-PERP", "HYPE"), ("DOGE-PERP", "DOGE")] {
        for interval in UI_INTERVALS {
            let (mut e, _) = open_with_meta(false, REPAIR_META);
            let key = candle_key(coin, interval);
            let subscribed = step(
                &mut e,
                2000.,
                Some(Command::SubscribeCandles {
                    symbol: symbol.into(),
                    interval,
                }),
            );
            let trades = format!(
                r#"{{"method":"subscribe","subscription":{{"type":"trades","coin":"{coin}"}}}}"#
            );
            assert_eq!(subscribed.sent, vec![key.clone(), trades.clone()]);
            let repaired = repair_candle(&mut e, 2001., symbol, interval);
            assert!(repaired.warnings.is_empty(), "{:?}", repaired.warnings);
            assert_eq!(repaired.sent, vec![unsubscribe_frame(&key), key.clone()]);
            let released = step(
                &mut e,
                2002.,
                Some(Command::UnsubscribeCandles {
                    symbol: symbol.into(),
                    interval,
                }),
            );
            let mut expected = vec![unsubscribe_frame(&key), unsubscribe_frame(&trades)];
            expected.sort();
            let mut released = released.sent;
            released.sort();
            assert_eq!(released, expected);
        }
    }
}
fn pending_bootstrap() -> (Engine<Hyperliquid>, safe_cex_engine::RequestId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    // A cached native catalogue is known before this instance's first metadata
    // response registers its feed. Do not restore the old wait-for-perpDexs path:
    // master now starts the feed with the first catalogue.
    let (cached, _) = open_with_meta(false, REPAIR_META);
    e.store_mut().set_markets(cached.store().markets().to_vec());
    e.set_log_level(Level::Debug);
    e.start(1000.);
    let meta = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if support::kind(&r) == "metaAndAssetCtxs" => Some(r.id),
            _ => None,
        })
        .unwrap();
    (e, meta)
}
#[test]
fn known_candles_defer_before_feed_start_then_deliver_without_reset_or_new_timer() {
    let (mut e, meta) = pending_bootstrap();
    for (symbol, interval) in [("HYPE-PERP", Interval::H4), ("DOGE-PERP", Interval::D1)] {
        step(
            &mut e,
            1000.,
            Some(Command::SubscribeCandles {
                symbol: symbol.into(),
                interval,
            }),
        );
        let deferred = repair_candle(&mut e, 1000., symbol, interval);
        assert!(deferred.warnings.is_empty(), "{:?}", deferred.warnings);
        assert!(deferred.sent.is_empty());
        assert_eq!(deferred.other_io, 0);
        let note = deferred
            .events
            .iter()
            .find(|event| event.code == "subscription.deferred")
            .unwrap();
        assert_eq!(note.level, Level::Debug);
        for field in [
            "feed-not-started",
            "engineBoot=",
            "symbolKnown=true",
            "catalogueSize=2",
            "discovery=pending",
        ] {
            assert!(note.message.contains(field), "{}", note.message);
        }
    }
    e.handle(
        1000.,
        Event::HttpResponse {
            request: meta,
            response: Response {
                status: 200,
                headers: "",
                body: REPAIR_META,
            },
        },
    );
    let socket = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(1000., Event::WsOpened { socket });
    let sent = step(&mut e, 1000., None).sent;
    assert!(sent.contains(&candle_key("HYPE", Interval::H4)));
    assert!(sent.contains(&candle_key("DOGE", Interval::D1)));
    e.take_frame(1000.);
    e.handle(1001.,Event::WsMessage{socket,text:r#"{"channel":"candle","data":{"t":0,"T":14400000,"s":"HYPE","i":"4h","o":"100","c":"101","h":"102","l":"99","v":"10","n":2}}"#});
    assert!(e.take_market_frame(1001.).is_some());
    for (symbol, interval) in [("HYPE-PERP", Interval::H4), ("DOGE-PERP", Interval::D1)] {
        let released = step(
            &mut e,
            1002.,
            Some(Command::UnsubscribeCandles {
                symbol: symbol.into(),
                interval,
            }),
        );
        assert_eq!(
            released.sent.len(),
            2,
            "one unsubscribe releases the candle and its companion trades stream"
        );
    }
}
fn unknown_market_boot(e: &mut Engine<Hyperliquid>, now: f64) -> String {
    let refused = repair_candle(e, now, "MISSING-PERP", Interval::D1);
    assert!(refused.sent.is_empty());
    assert_eq!(refused.other_io, 0);
    assert_eq!(refused.warnings.len(), 1);
    let message = &refused.warnings[0];
    for field in [
        "unknown-market",
        "engineBoot=",
        "symbolKnown=false",
        "catalogueSize=2",
        "discovery=",
    ] {
        assert!(message.contains(field), "{message}");
    }
    message
        .split("engineBoot=")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .to_string()
}
#[test]
fn unknown_market_stays_visible_and_identifies_the_native_engine_and_catalogue() {
    let (mut a, _) = open_with_meta(false, REPAIR_META);
    let (mut b, _) = open_with_meta(false, REPAIR_META);
    let boot = unknown_market_boot(&mut a, 2000.);
    assert_eq!(unknown_market_boot(&mut a, 2001.), boot);
    assert_ne!(unknown_market_boot(&mut b, 2000.), boot);
    let (mut pending, _) = pending_bootstrap();
    unknown_market_boot(&mut pending, 1000.);
}
#[test]
fn book_resub_and_mid_session_timeframe_switch_keep_the_registered_feed() {
    let (mut e, _) = open_with_meta(false, REPAIR_META);
    let boot = unknown_market_boot(&mut e, 2000.);
    for subscribe in [true, false, true] {
        let command = if subscribe {
            Command::SubscribeOrderBook {
                symbol: "HYPE-PERP".into(),
            }
        } else {
            Command::UnsubscribeOrderBook {
                symbol: "HYPE-PERP".into(),
            }
        };
        step(&mut e, 2001., Some(command));
    }
    let book = step(
        &mut e,
        2002.,
        Some(Command::RepairSubscription(SubscriptionTopic::OrderBook {
            symbol: "HYPE-PERP".into(),
        })),
    );
    assert!(book.warnings.is_empty());
    assert_eq!(book.sent.len(), 4);
    assert_eq!(book.other_io, 0);
    step(
        &mut e,
        2003.,
        Some(Command::SubscribeCandles {
            symbol: "HYPE-PERP".into(),
            interval: Interval::H4,
        }),
    );
    step(
        &mut e,
        2004.,
        Some(Command::UnsubscribeCandles {
            symbol: "HYPE-PERP".into(),
            interval: Interval::H4,
        }),
    );
    step(
        &mut e,
        2005.,
        Some(Command::SubscribeCandles {
            symbol: "HYPE-PERP".into(),
            interval: Interval::H4,
        }),
    );
    let repaired = repair_candle(&mut e, 2006., "HYPE-PERP", Interval::H4);
    assert!(repaired.warnings.is_empty());
    assert_eq!(repaired.other_io, 0);
    let key = candle_key("HYPE", Interval::H4);
    assert_eq!(repaired.sent, vec![unsubscribe_frame(&key), key]);
    assert_eq!(unknown_market_boot(&mut e, 2007.), boot);
}
#[test]
fn transport_reconnect_keeps_feed_identity_and_candle_repair_recovers() {
    let (mut e, socket) = open_with_meta(false, REPAIR_META);
    let boot = unknown_market_boot(&mut e, 2000.);
    step(
        &mut e,
        2001.,
        Some(Command::SubscribeCandles {
            symbol: "HYPE-PERP".into(),
            interval: Interval::H4,
        }),
    );
    e.handle(2002., Event::WsClosed { socket, code: 1006 });
    let (timer, at) = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let replacement = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement, socket);
    e.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let opened = step(&mut e, at, None);
    assert!(opened.sent.contains(&candle_key("HYPE", Interval::H4)));
    step(
        &mut e,
        at + 1.,
        Some(Command::SubscribeCandles {
            symbol: "DOGE-PERP".into(),
            interval: Interval::D1,
        }),
    );
    let repaired = repair_candle(&mut e, at + 2., "DOGE-PERP", Interval::D1);
    assert!(repaired.warnings.is_empty());
    assert_eq!(repaired.other_io, 0);
    let key = candle_key("DOGE", Interval::D1);
    assert_eq!(repaired.sent, vec![unsubscribe_frame(&key), key]);
    assert_eq!(unknown_market_boot(&mut e, at + 3.), boot);
}

#[test]
fn first_catalogue_registers_the_feed_before_hip3_discovery_finishes() {
    let (mut e, meta) = pending_bootstrap();
    e.handle(
        1000.,
        Event::HttpResponse {
            request: meta,
            response: Response {
                status: 200,
                headers: "",
                body: REPAIR_META,
            },
        },
    );
    let actions = prepared_now(&mut e);
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Http(r) if support::kind(r) == "perpDexs"))
    );
    let socket = actions
        .into_iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("first catalogue opens the socket without waiting for perpDexs");
    e.handle(1000., Event::WsOpened { socket });
    step(&mut e, 1000., None);
    step(
        &mut e,
        1001.,
        Some(Command::SubscribeCandles {
            symbol: "HYPE-PERP".into(),
            interval: Interval::H4,
        }),
    );
    let repaired = repair_candle(&mut e, 1002., "HYPE-PERP", Interval::H4);
    let key = candle_key("HYPE", Interval::H4);
    assert_eq!(repaired.sent, vec![unsubscribe_frame(&key), key]);
    assert!(repaired.warnings.is_empty());
    assert!(
        !repaired
            .events
            .iter()
            .any(|event| event.code == code::SUBSCRIPTION_DEFERRED)
    );
}
