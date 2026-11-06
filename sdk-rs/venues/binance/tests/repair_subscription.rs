//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Command, Event, Level, SubscriptionTopic};
use support::{Harness, Options};

const PUBLIC: &str = "/public/ws";
const MARKET: &str = "/market/ws";

fn harness() -> Harness {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.load_public();
    h.advance(100.0);
    h
}

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

/// Native `(method, params)` frames `command` sent on any socket, by socket path.
fn frames(h: &mut Harness, command: Command) -> Vec<(&'static str, String, Vec<String>)> {
    let before = h.sent.len();
    h.event(Event::Command(command));
    h.advance(100.0);
    let path = |h: &Harness, socket| {
        [PUBLIC, MARKET]
            .into_iter()
            .find(|path| h.url_of(socket).contains(path))
            .unwrap_or("other")
    };
    h.sent[before..]
        .iter()
        .map(|(socket, text)| {
            let frame: serde_json::Value = serde_json::from_str(text).unwrap();
            let params = frame["params"]
                .as_array()
                .unwrap_or_else(|| panic!("{text}"))
                .iter()
                .map(|p| p.as_str().unwrap().to_owned())
                .collect();
            (
                path(h, *socket),
                frame["method"].as_str().unwrap().to_owned(),
                params,
            )
        })
        .collect()
}

fn repair_warnings(h: &Harness) -> Vec<String> {
    h.events
        .iter()
        .filter(|event| event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn)
        .map(|event| event.message.clone())
        .collect()
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    socket: &'static str,
    key: &str,
) {
    let mut h = harness();
    let named = |method: &str| (socket, method.to_owned(), vec![key.to_owned()]);
    assert_eq!(frames(&mut h, subscribe), [named("SUBSCRIBE")]);
    let repaired = frames(&mut h, Command::RepairSubscription(topic));
    assert_eq!(repaired, [named("UNSUBSCRIBE"), named("SUBSCRIBE")]);
    assert!(repair_warnings(&h).is_empty(), "{:?}", repair_warnings(&h));

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    assert_eq!(frames(&mut h, unsubscribe), [named("UNSUBSCRIBE")]);
    let before = h.sent.len();
    h.advance(100.0);
    assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
}

#[test]
fn repairing_a_live_kline_resubscribes_exactly_that_topic_on_the_market_socket() {
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
        MARKET,
        "btcusdt@kline_5m",
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_topic_on_the_public_socket() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        book(),
        PUBLIC,
        "btcusdt@depth",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut h = harness();
        let sent = frames(&mut h, Command::RepairSubscription(topic.clone()));
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&h);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!(
                "BinanceFutures cannot repair the {topic} subscription"
            )),
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
