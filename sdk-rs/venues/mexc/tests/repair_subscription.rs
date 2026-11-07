//! `Command::RepairSubscription`: a dead MEXC stream is re-subscribed
//! natively without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Event, Level, Notice, SocketId, SubscriptionTopic, TimerId,
};
use support::TestEngine;

struct H {
    engine: TestEngine,
    now: f64,
    socket: SocketId,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl H {
    /// Markets loaded and the public socket open.
    fn ready() -> Self {
        let (engine, socket) = support::started();
        let mut h = Self {
            engine,
            now: 100.0,
            socket,
            timers: vec![],
            sent: vec![],
            warnings: vec![],
        };
        h.collect();
        h
    }

    fn collect(&mut self) {
        for action in support::prepared(&mut self.engine, self.now) {
            match action {
                Action::WsConnect { socket, .. } => self.socket = socket,
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

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }

    /// Every (method, symbol, interval) frame `f` sends on the public socket.
    fn frames(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Frame> {
        let before = self.sent.len();
        f(self);
        let socket = self.socket;
        let sent = &self.sent[before..];
        assert!(sent.iter().all(|(s, _)| *s == socket), "{sent:?}");
        sent.iter().map(|(_, text)| parts(text)).collect()
    }

    /// Fires every timer due by `until`.
    fn advance(&mut self, until: f64) {
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }
}

type Frame = (String, String, String);

fn parts(text: &str) -> Frame {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    let field = |name: &str| body["param"][name].as_str().unwrap_or_default().to_owned();
    (
        body["method"].as_str().unwrap().to_owned(),
        field("symbol"),
        field("interval"),
    )
}

fn frame(method: &str, interval: &str) -> Frame {
    (
        method.to_owned(),
        "BTC_USDT".to_owned(),
        interval.to_owned(),
    )
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

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    channel: &str,
    interval: &str,
) {
    let mut h = H::ready();
    let subscribed = h.frames(|h| h.command(subscribe.clone()));
    assert_eq!(subscribed, [frame(&format!("sub.{channel}"), interval)]);
    assert!(h.frames(|h| h.command(subscribe)).is_empty());

    let repaired = h.frames(|h| h.command(Command::RepairSubscription(topic)));
    assert_eq!(
        repaired,
        [
            frame(&format!("unsub.{channel}"), interval),
            frame(&format!("sub.{channel}"), interval)
        ]
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);

    // Intent is unchanged: only the second subscriber's release unsubscribes.
    assert!(h.frames(|h| h.command(unsubscribe.clone())).is_empty());
    let released = h.frames(|h| h.command(unsubscribe));
    assert_eq!(released, [frame(&format!("unsub.{channel}"), interval)]);
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
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
        "kline",
        "Min5",
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
        "depth",
        "",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns_once() {
    for topic in [candles(), book()] {
        let mut h = H::ready();
        let sent = h.frames(|h| h.command(Command::RepairSubscription(topic.clone())));
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(
            h.warnings,
            [format!(
                "MexcFutures cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_public_socket_is_down_is_refused_not_queued() {
    let mut h = H::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    let old = h.socket;
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let sent = h.frames(|h| h.command(Command::RepairSubscription(book())));
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(
        h.warnings,
        [
            "MexcFutures cannot repair the order book BTCUSDT subscription: \
             the public socket carrying order book BTCUSDT is not open"
        ]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    while h.socket == old {
        let until = h.now + 100.0;
        h.advance(until);
        assert!(h.now < 60_000.0, "no reconnect");
    }
    let socket = h.socket;
    let resubscribed = h.frames(|h| h.event(Event::WsOpened { socket }));
    let books: Vec<_> = resubscribed
        .iter()
        .filter(|(method, ..)| method.ends_with(".depth"))
        .collect();
    assert_eq!(books, [&frame("sub.depth", "")], "{resubscribed:?}");
}
