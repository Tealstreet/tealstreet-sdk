//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
#![allow(dead_code)]
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, Event, Level, Notice, SocketId, SubscriptionTopic};
use safe_cex_venue_okx::Okx;
use support::{FixtureOptions as Options, prepared, public};

const BOOK_KEY: &str = r#"{"channel":"books","instId":"BTC-USDT-SWAP"}"#;
const CANDLE_KEY: &str = r#"{"channel":"candle5m","instId":"BTC-USDT-SWAP"}"#;

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: "BTCUSDT".into(),
    }
}

struct Run {
    engine: Engine<Okx>,
    actions: Vec<Action>,
}

impl Run {
    fn command(&mut self, command: Command) -> Vec<Action> {
        self.engine.handle(5.0, Event::Command(command));
        let actions = prepared(&mut self.engine, 5.0);
        self.actions.extend(actions.clone());
        actions
    }
}

fn sent_on(actions: &[Action], on: SocketId) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if *socket == on => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn all_sent(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn repair_warnings(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
            {
                Some(event.message.clone())
            }
            _ => None,
        })
        .collect()
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    key: &str,
    on_business: bool,
) {
    let (engine, public_socket, business_socket) = public(Options::default());
    let socket = if on_business {
        business_socket
    } else {
        public_socket
    };
    let mut run = Run {
        engine,
        actions: Vec::new(),
    };
    run.command(subscribe);
    let unsubscribe_frame = format!(r#"{{"op":"unsubscribe","args":[{key}]}}"#);
    let subscribe_frame = format!(r#"{{"op":"subscribe","args":[{key}]}}"#);

    let repaired = run.command(Command::RepairSubscription(topic));
    assert_eq!(
        all_sent(&repaired),
        [unsubscribe_frame.clone(), subscribe_frame]
    );
    assert_eq!(sent_on(&repaired, socket).len(), 2);
    assert!(
        repair_warnings(&run.actions).is_empty(),
        "{:?}",
        repair_warnings(&run.actions)
    );

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let released = all_sent(&run.command(unsubscribe));
    assert_eq!(released, [unsubscribe_frame], "{released:?}");
    assert!(all_sent(&prepared(&mut run.engine, 6.0)).is_empty());
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M5,
        },
        candles(),
        CANDLE_KEY,
        true,
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        book(),
        BOOK_KEY,
        false,
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let (engine, _, _) = public(Options::default());
        let mut run = Run {
            engine,
            actions: Vec::new(),
        };
        let out = run.command(Command::RepairSubscription(topic.clone()));
        assert!(all_sent(&out).is_empty(), "{out:?}");
        let warnings = repair_warnings(&out);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!("Okx cannot repair the {topic} subscription")),
            "{}",
            warnings[0]
        );
        assert!(
            warnings[0].ends_with("is not subscribed"),
            "{}",
            warnings[0]
        );
    }
}
