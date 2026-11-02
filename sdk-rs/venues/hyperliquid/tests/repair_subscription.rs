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

fn assert_repair_resubscribes_once(
    spot: bool,
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    key: &str,
) {
    let mut engine = open(spot);
    let subscribed = step(&mut engine, 2000.0, Some(subscribe));
    assert!(
        subscribed.sent.iter().any(|f| f == key),
        "{:?}",
        subscribed.sent
    );

    let repaired = step(
        &mut engine,
        2001.0,
        Some(Command::RepairSubscription(topic)),
    );
    let unsubscribe_frame = key.replacen(r#""method":"subscribe""#, r#""method":"unsubscribe""#, 1);
    assert_eq!(repaired.sent, [unsubscribe_frame.as_str(), key]);
    assert!(repaired.warnings.is_empty(), "{:?}", repaired.warnings);

    // Intent is unchanged: the one subscriber still releases the topic once.
    let released = step(&mut engine, 2002.0, Some(unsubscribe));
    let naming: Vec<_> = released
        .sent
        .iter()
        .filter(|f| f.replacen("unsubscribe", "subscribe", 1) == key)
        .collect();
    assert_eq!(naming, [&unsubscribe_frame], "{:?}", released.sent);
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
        r#"{"method":"subscribe","subscription":{"type":"candle","coin":"BTC","interval":"5m"}}"#,
    );
}

#[test]
fn repairing_a_live_perps_book_resubscribes_its_l2_book_alone() {
    // `bbo` is a sibling topic; the book's depth is the `l2Book` stream.
    assert_repair_resubscribes_once(
        false,
        Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        book("BTC-PERP"),
        r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#,
    );
}

#[test]
fn repairing_a_live_spot_book_resubscribes_its_full_precision_depth() {
    assert_repair_resubscribes_once(
        true,
        Command::SubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "PURR/USDC".into(),
        },
        book("PURR/USDC"),
        r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"PURR/USDC","nSigFigs":null}}"#,
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
