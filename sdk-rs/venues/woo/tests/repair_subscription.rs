//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Event, Level, Notice, SubscriptionTopic};
use support::{TestEngine, started};

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

/// Every socket frame `command` sends, and every action it produces.
fn command(e: &mut TestEngine, command: Command) -> (Vec<String>, Vec<Action>) {
    e.handle(40.0, Event::Command(command));
    let out = safe_cex_engine::test_support::prepared_actions(e, 40.0);
    let sent = out
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    (sent, out)
}

fn ready() -> TestEngine {
    let (mut e, _) = started();
    safe_cex_engine::test_support::prepared_actions(&mut e, 30.0);
    e
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
) {
    let mut e = ready();
    let subscribe_frame = format!(r#"{{"event":"subscribe","topic":"{key}"}}"#);
    let unsubscribe_frame = format!(r#"{{"event":"unsubscribe","topic":"{key}"}}"#);
    assert_eq!(
        command(&mut e, subscribe).0,
        std::slice::from_ref(&subscribe_frame)
    );

    let (repaired, out) = command(&mut e, Command::RepairSubscription(topic));
    assert_eq!(repaired, [unsubscribe_frame.clone(), subscribe_frame]);
    assert!(
        repair_warnings(&out).is_empty(),
        "{:?}",
        repair_warnings(&out)
    );

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    assert_eq!(command(&mut e, unsubscribe).0, [unsubscribe_frame]);
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, 50.0).is_empty());
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
        "PERP_BTC_USDT@kline_5m",
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
        "PERP_BTC_USDT@orderbookupdate",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut e = ready();
        let (sent, out) = command(&mut e, Command::RepairSubscription(topic.clone()));
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&out);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!("Woo cannot repair the {topic} subscription")),
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
