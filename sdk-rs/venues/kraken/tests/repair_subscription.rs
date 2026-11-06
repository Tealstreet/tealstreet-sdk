//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod spot_support;
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, Event, Level, Notice, SubscriptionTopic, Venue};
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options};

/// Socket frames and repair warnings one step produced.
#[derive(Default)]
struct Step {
    sent: Vec<String>,
    warnings: Vec<String>,
}

fn step<V: Venue>(engine: &mut Engine<V>, command: Option<Command>) -> Step {
    if let Some(command) = command {
        engine.handle(support::NOW, Event::Command(command));
    }
    let mut step = Step::default();
    for action in safe_cex_engine::test_support::prepared_actions(engine, support::NOW) {
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

fn futures() -> Engine<Kraken> {
    let (mut engine, _) = support::ready_with(Options {
        public_data_only: true,
        ..Default::default()
    });
    step(&mut engine, None);
    engine
}

fn spot() -> Engine<KrakenSpot> {
    let (mut engine, _, _) = spot_support::ready(true);
    step(&mut engine, None);
    engine
}

fn assert_repair_resubscribes_once<V: Venue>(
    mut engine: Engine<V>,
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    subscribe_frame: &str,
    unsubscribe_frame: &str,
) {
    let subscribed = step(&mut engine, Some(subscribe));
    assert_eq!(subscribed.sent, [subscribe_frame]);

    let repaired = step(&mut engine, Some(Command::RepairSubscription(topic)));
    assert_eq!(repaired.sent, [unsubscribe_frame, subscribe_frame]);
    assert!(repaired.warnings.is_empty(), "{:?}", repaired.warnings);

    // Intent is unchanged: the one subscriber still releases the topic once.
    assert_eq!(
        step(&mut engine, Some(unsubscribe)).sent,
        [unsubscribe_frame]
    );
    assert!(step(&mut engine, None).sent.is_empty());
}

fn assert_one_warning(step: &Step, venue: &str, topic: &SubscriptionTopic, reason: &str) {
    assert!(step.sent.is_empty(), "{:?}", step.sent);
    assert_eq!(step.warnings.len(), 1, "{:?}", step.warnings);
    let warning = &step.warnings[0];
    assert!(
        warning.starts_with(&format!("{venue} cannot repair the {topic} subscription")),
        "{warning}"
    );
    assert!(warning.ends_with(reason), "{warning}");
}

fn book(symbol: &str) -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: symbol.into(),
    }
}

fn candles(symbol: &str) -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: symbol.into(),
        interval: Interval::M5,
    }
}

#[test]
fn repairing_a_live_futures_book_resubscribes_exactly_that_feed() {
    assert_repair_resubscribes_once(
        futures(),
        Command::SubscribeOrderBook {
            symbol: "PF_XBTUSD".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "PF_XBTUSD".into(),
        },
        book("PF_XBTUSD"),
        r#"{"event":"subscribe","feed":"book","product_ids":["PF_XBTUSD"]}"#,
        r#"{"event":"unsubscribe","feed":"book","product_ids":["PF_XBTUSD"]}"#,
    );
}

#[test]
fn futures_candles_are_rest_polled_so_repair_names_no_stream() {
    let mut engine = futures();
    engine.handle(
        support::NOW,
        Event::Command(Command::SubscribeCandles {
            symbol: "PF_XBTUSD".into(),
            interval: Interval::M5,
        }),
    );
    step(&mut engine, None);
    let topic = candles("PF_XBTUSD");
    let repaired = step(
        &mut engine,
        Some(Command::RepairSubscription(topic.clone())),
    );
    assert_one_warning(
        &repaired,
        "Kraken",
        &topic,
        "the venue cannot name its stream for this topic",
    );
}

#[test]
fn repairing_a_live_spot_candle_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        spot(),
        Command::SubscribeCandles {
            symbol: "XBT/USD".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "XBT/USD".into(),
            interval: Interval::M5,
        },
        candles("XBT/USD"),
        r#"{"method":"subscribe","params":{"channel":"ohlc","symbol":["BTC/USD"],"interval":5}}"#,
        r#"{"method":"unsubscribe","params":{"channel":"ohlc","symbol":["BTC/USD"],"interval":5}}"#,
    );
}

#[test]
fn repairing_a_live_spot_book_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        spot(),
        Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "XBT/USD".into(),
        },
        book("XBT/USD"),
        r#"{"method":"subscribe","params":{"channel":"book","symbol":["BTC/USD"],"depth":100}}"#,
        r#"{"method":"unsubscribe","params":{"channel":"book","symbol":["BTC/USD"],"depth":100}}"#,
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    let topic = book("PF_XBTUSD");
    let repaired = step(
        &mut futures(),
        Some(Command::RepairSubscription(topic.clone())),
    );
    assert_one_warning(&repaired, "Kraken", &topic, "is not subscribed");
    for topic in [candles("XBT/USD"), book("XBT/USD")] {
        let repaired = step(
            &mut spot(),
            Some(Command::RepairSubscription(topic.clone())),
        );
        assert_one_warning(&repaired, "KrakenSpot", &topic, "is not subscribed");
    }
}
