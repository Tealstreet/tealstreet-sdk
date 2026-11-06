//! `Command::RepairSubscription`: a dead Coinbase stream is re-subscribed
//! natively without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Event, Level, Notice, SocketId, SubscriptionTopic, TimerId, Venue,
};
use support::Driver;

const SYMBOL: &str = "BTC-PERP-INTX";

struct Public {
    engine: Driver,
    now: f64,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl Public {
    /// Markets loaded and the public socket open.
    fn ready() -> Self {
        let (engine, socket) = support::ready();
        Self {
            engine,
            now: 1000.,
            socket: Some(socket),
            timers: vec![],
            sent: vec![],
            warnings: vec![],
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        for action in self.engine.drain_actions().collect::<Vec<_>>() {
            match action {
                Action::WsConnect { socket, .. } => self.socket = Some(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::Event(event))
                    if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
                {
                    self.warnings.push(event.message)
                }
                _ => {}
            }
        }
    }

    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }

    /// Every (type, channel) frame `f` sends, on any socket, naming only `SYMBOL`.
    fn frames(&mut self, f: impl FnOnce(&mut Self)) -> Vec<(String, String)> {
        let before = self.sent.len();
        f(self);
        self.sent[before..].iter().map(|(_, t)| parts(t)).collect()
    }
}

fn parts(text: &str) -> (String, String) {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(body["product_ids"], serde_json::json!([SYMBOL]), "{text}");
    (
        body["type"].as_str().unwrap().to_owned(),
        body["channel"].as_str().unwrap().to_owned(),
    )
}

fn frame(kind: &str, channel: &str) -> (String, String) {
    (kind.to_owned(), channel.to_owned())
}

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: SYMBOL.into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: SYMBOL.into(),
    }
}

fn depth(h: &mut Public, sequence: u64, kind: &str, bid: &str) {
    let socket = h.socket.unwrap();
    let text = format!(
        r#"{{"channel":"l2_data","sequence_num":{sequence},"events":[{{"type":"{kind}","product_id":"{SYMBOL}","updates":[{{"side":"bid","price_level":"{bid}","new_quantity":"1"}}]}}]}}"#
    );
    h.event(Event::WsMessage {
        socket,
        text: &text,
    });
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    channels: &[&str],
) -> Public {
    let mut h = Public::ready();
    let subscribed = h.frames(|h| h.command(subscribe));
    let expect = |kind| channels.iter().map(|c| frame(kind, c)).collect::<Vec<_>>();
    assert_eq!(subscribed, expect("subscribe"));

    let socket = h.socket.unwrap();
    let before = h.sent.len();
    h.command(Command::RepairSubscription(topic));
    let repaired = &h.sent[before..];
    assert!(repaired.iter().all(|(s, _)| *s == socket));
    let repaired: Vec<_> = repaired.iter().map(|(_, t)| parts(t)).collect();
    assert_eq!(
        repaired,
        [expect("unsubscribe"), expect("subscribe")].concat()
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let released = h.frames(|h| h.command(unsubscribe));
    assert_eq!(released, expect("unsubscribe"));
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
    h
}

#[test]
fn repairing_a_live_candle_resubscribes_both_of_its_channels() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        candles(),
        &["candles", "market_trades"],
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_channel() {
    let mut h = assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        book(),
        &["level2"],
    );
    let unsubscribe = Command::UnsubscribeOrderBook {
        symbol: SYMBOL.into(),
    };
    assert!(h.frames(|h| h.command(unsubscribe)).is_empty());
}

#[test]
fn a_repaired_book_is_replaced_by_the_fresh_snapshot() {
    let mut h = Public::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    });
    depth(&mut h, 0, "snapshot", "100");
    depth(&mut h, 1, "update", "99");
    h.command(Command::RepairSubscription(book()));
    // The socket-wide sequence continues; the new snapshot replaces every level.
    depth(&mut h, 2, "snapshot", "101");
    assert_eq!(h.engine.venue().book(SYMBOL).unwrap().bids(), [(101., 1.)]);
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns_once() {
    for topic in [candles(), book()] {
        let mut h = Public::ready();
        let sent = h.frames(|h| h.command(Command::RepairSubscription(topic.clone())));
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(
            h.warnings,
            [format!(
                "Coinbase cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_public_socket_is_down_is_refused_not_queued() {
    let mut h = Public::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    });
    let old = h.socket.take().unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let sent = h.frames(|h| h.command(Command::RepairSubscription(book())));
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(
        h.warnings,
        [format!(
            "Coinbase cannot repair the order book {SYMBOL} subscription: \
             the public socket carrying level2 {SYMBOL} is not open"
        )]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    while h.socket.is_none() {
        let (timer, at) = h
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a reconnect timer");
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    let socket = h.socket.unwrap();
    let before = h.sent.len();
    h.event(Event::WsOpened { socket });
    let books: Vec<_> = h.sent[before..]
        .iter()
        .filter(|(_, t)| t.contains("level2"))
        .map(|(_, t)| parts(t))
        .collect();
    assert_eq!(books, [frame("subscribe", "level2")]);
}
