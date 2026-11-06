//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Command, Level, Notice, SubscriptionTopic};
use support::Harness;

const PUBLIC: &str = "/v5/public/linear";

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

/// Frames sent on the public socket by `f`, after pacing has flushed them.
fn frames(h: &mut Harness, f: impl FnOnce(&mut Harness)) -> Vec<String> {
    let before = h.sent_on(PUBLIC).len();
    f(h);
    h.flush_subscriptions();
    h.sent_on(PUBLIC)[before..].to_vec()
}

fn repair_warnings(h: &Harness) -> Vec<String> {
    h.notices
        .iter()
        .filter_map(|notice| match notice {
            Notice::Event(event)
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
    let mut h = Harness::new();
    h.command(subscribe);
    h.flush_subscriptions();
    let unsubscribe_frame = format!(r#"{{"op":"unsubscribe","args":["{key}"]}}"#);
    let subscribe_frame = format!(r#"{{"op":"subscribe","args":["{key}"]}}"#);

    let repaired = frames(&mut h, |h| h.command(Command::RepairSubscription(topic)));
    assert_eq!(repaired, [unsubscribe_frame, subscribe_frame]);
    assert!(repair_warnings(&h).is_empty(), "{:?}", repair_warnings(&h));

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let released = frames(&mut h, |h| h.command(unsubscribe));
    let quoted = format!(r#""{key}""#);
    let naming: Vec<_> = released.iter().filter(|f| f.contains(&quoted)).collect();
    assert_eq!(naming.len(), 1, "{released:?}");
    assert!(
        naming[0].starts_with(r#"{"op":"unsubscribe""#),
        "{released:?}"
    );
    // Nothing is left to release.
    assert!(frames(&mut h, |h| h.flush_subscriptions()).is_empty());
}

#[test]
fn repairing_a_live_kline_resubscribes_exactly_that_topic() {
    // The candle's shared trade topic is not part of the kline repair.
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
        "kline.5.BTCUSDT",
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
        "orderbook.1000.BTCUSDT",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut h = Harness::new();
        let sent = frames(&mut h, |h| {
            h.command(Command::RepairSubscription(topic.clone()))
        });
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&h);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!(
                "BybitLinear cannot repair the {topic} subscription"
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
