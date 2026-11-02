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
}

fn step(engine: &mut Engine<Hyperliquid>, now: f64, command: Option<Command>) -> Step {
    if let Some(command) = command {
        engine.handle(now, Event::Command(command));
    }
    let mut step = Step::default();
    for action in prepared_now(engine) {
        match action {
            Action::WsSend { text, .. } => step.sent.push(text),
            Action::Notice(Notice::Event(event))
                if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
            {
                step.warnings.push(event.message)
            }
            _ => {}
        }
    }
    step
}

/// A public-data engine with its socket open and fixed topics flushed.
fn open(spot: bool) -> Engine<Hyperliquid> {
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
                        "metaAndAssetCtxs" => support::META,
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
                    return engine;
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
