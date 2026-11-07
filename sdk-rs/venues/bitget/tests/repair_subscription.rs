//! `Command::RepairSubscription`: a dead Bitget stream is re-subscribed
//! natively without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Command, Event, Level, SocketId, SubscriptionTopic};
use support::Harness;

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

fn frame(op: &str, channel: &str) -> String {
    format!(
        r#"{{"op":"{op}","args":[{{"instType":"USDT-FUTURES","channel":"{channel}","instId":"BTCUSDT"}}]}}"#
    )
}

/// Every frame `f` sends, on any socket.
fn frames(h: &mut Harness, f: impl FnOnce(&mut Harness)) -> Vec<String> {
    let before = h.sent.len();
    f(h);
    h.sent[before..].iter().map(|(_, t)| t.clone()).collect()
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
    channel: &str,
) {
    let mut h = Harness::public();
    let subscribed = frames(&mut h, |h| h.command(subscribe));
    assert_eq!(subscribed, [frame("subscribe", channel)]);

    let public = primary(&h);
    let before = h.sent.len();
    h.command(Command::RepairSubscription(topic));
    let repaired = &h.sent[before..];
    assert!(repaired.iter().all(|(socket, _)| *socket == public));
    let repaired: Vec<_> = repaired.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        repaired,
        [frame("unsubscribe", channel), frame("subscribe", channel)]
    );
    assert!(repair_warnings(&h).is_empty(), "{:?}", repair_warnings(&h));

    // Intent is unchanged: the one subscriber still releases exactly once.
    let released = frames(&mut h, |h| h.command(unsubscribe.clone()));
    assert_eq!(released, [frame("unsubscribe", channel)]);
    assert!(frames(&mut h, |h| h.command(unsubscribe)).is_empty());
    assert!(repair_warnings(&h).is_empty(), "{:?}", repair_warnings(&h));
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_channel() {
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
        "candle5m",
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_channel() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        book(),
        "books",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns_once() {
    for topic in [candles(), book()] {
        let mut h = Harness::public();
        let sent = frames(&mut h, |h| {
            h.command(Command::RepairSubscription(topic.clone()))
        });
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&h);
        assert_eq!(
            warnings,
            [format!(
                "BitgetFutures cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_public_socket_is_down_is_refused_not_queued() {
    let mut h = Harness::public();
    h.book();
    let old = primary(&h);
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let sent = frames(&mut h, |h| h.command(Command::RepairSubscription(book())));
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(
        repair_warnings(&h),
        [
            "BitgetFutures cannot repair the order book BTCUSDT subscription: \
             the public socket carrying books BTCUSDT is not open"
        ]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    let until = h.now + 60_000.0;
    advance(&mut h, until);
    let socket = primary(&h);
    assert_ne!(socket, old);
    let resubscribed = frames(&mut h, |h| h.event(Event::WsOpened { socket }));
    let books: Vec<_> = resubscribed
        .iter()
        .filter(|t| t.contains(r#""channel":"books""#))
        .collect();
    assert_eq!(books.len(), 1, "{resubscribed:?}");
    assert!(books[0].starts_with(r#"{"op":"subscribe""#));
}

/// Fires every timer due by `until`.
fn advance(h: &mut Harness, until: f64) {
    while let Some((timer, at)) = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    h.now = until;
}

fn primary(h: &Harness) -> SocketId {
    h.socket_contexts
        .iter()
        .rev()
        .find(|(_, _, context)| context.as_ref().is_some_and(|c| c.feed == "primary"))
        .unwrap()
        .0
}
