//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Event, Level, Notice, SocketId, SubscriptionTopic, TimerId,
};
use support::TestEngine;

/// The started public engine, its timers and what it sent on the public socket.
struct Wire {
    engine: TestEngine,
    socket: SocketId,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<String>,
    warnings: Vec<String>,
}

impl Wire {
    fn new() -> Self {
        let (engine, socket) = support::started();
        let mut wire = Self {
            engine,
            socket,
            now: 20.0,
            timers: Vec::new(),
            sent: Vec::new(),
            warnings: Vec::new(),
        };
        wire.settle();
        wire
    }

    fn collect(&mut self) {
        for action in prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsSend { socket, text } if socket == self.socket => self.sent.push(text),
                Action::Notice(Notice::Event(event))
                    if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
                {
                    self.warnings.push(event.message)
                }
                _ => {}
            }
        }
    }

    /// Lets interest pacing flush everything queued so far.
    fn settle(&mut self) {
        self.collect();
        let until = self.now + 100.0;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.remove(0);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.collect();
        }
        self.now = until;
    }

    fn command(&mut self, command: Command) {
        self.engine
            .handle_command(self.now, command, None)
            .expect("valid caller receipt");
        self.collect();
    }

    /// Frames sent on the public socket by `command`, after pacing has flushed them.
    fn frames(&mut self, command: Command) -> Vec<String> {
        let before = self.sent.len();
        self.command(command);
        self.settle();
        self.sent[before..].to_vec()
    }
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
    key: &str,
) {
    let mut wire = Wire::new();
    wire.frames(subscribe);
    let repaired = wire.frames(Command::RepairSubscription(topic));
    assert_eq!(
        repaired,
        [
            format!(r#"{{"op":"unsubscribe","args":["{key}"]}}"#),
            format!(r#"{{"op":"subscribe","args":["{key}"]}}"#),
        ]
    );
    assert!(wire.warnings.is_empty(), "{:?}", wire.warnings);

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let released = wire.frames(unsubscribe);
    assert_eq!(
        released,
        [format!(r#"{{"op":"unsubscribe","args":["{key}"]}}"#)]
    );
    let before = wire.sent.len();
    wire.settle();
    assert_eq!(wire.sent.len(), before, "{:?}", &wire.sent[before..]);
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
        "candle.5.BTCUSDT",
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
        "orderBook200.H.BTCUSDT",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut wire = Wire::new();
        let sent = wire.frames(Command::RepairSubscription(topic.clone()));
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(wire.warnings.len(), 1, "{:?}", wire.warnings);
        let warning = &wire.warnings[0];
        assert!(
            warning.starts_with(&format!("ApexOmni cannot repair the {topic} subscription")),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}
