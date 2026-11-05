//! `Command::RepairSubscription`: a dead Lighter stream is re-subscribed
//! natively without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, Response, SocketId, SubscriptionTopic, TimerId,
};
use safe_cex_venue_lighter::Lighter;
use support::Options;

const BOOK: &str = "order_book/1";
const CANDLES: &str = "candle/1/5m";

struct H {
    engine: Engine<Lighter>,
    now: f64,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl H {
    /// Markets loaded and the public socket open.
    fn ready() -> Self {
        let mut engine = support::engine(Options {
            public_data_only: true,
            ..Options::default()
        });
        engine.start(1_000.0);
        let mut h = Self {
            engine,
            now: 1_000.0,
            socket: None,
            timers: vec![],
            sent: vec![],
            warnings: vec![],
        };
        let request = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        h.event(Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: support::META,
            },
        });
        let socket = h.socket.unwrap();
        h.event(Event::WsOpened { socket });
        h
    }

    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
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

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }

    /// Every (type, channel) frame `f` sends on the public socket, without keep-alives.
    fn frames(&mut self, f: impl FnOnce(&mut Self)) -> Vec<(String, String)> {
        let before = self.sent.len();
        f(self);
        let socket = self.socket.unwrap();
        let sent = &self.sent[before..];
        assert!(sent.iter().all(|(s, _)| *s == socket), "{sent:?}");
        sent.iter()
            .map(|(_, text)| parts(text))
            .filter(|(kind, _)| kind != "ping")
            .collect()
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

fn parts(text: &str) -> (String, String) {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    (
        body["type"].as_str().unwrap().to_owned(),
        body["channel"].as_str().unwrap_or_default().to_owned(),
    )
}

fn frame(kind: &str, channel: &str) -> (String, String) {
    (kind.to_owned(), channel.to_owned())
}

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "ETH".into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: "ETH".into(),
    }
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    channel: &str,
) {
    let mut h = H::ready();
    let subscribed = h.frames(|h| h.command(subscribe));
    assert_eq!(subscribed, [frame("subscribe", channel)]);

    let repaired = h.frames(|h| h.command(Command::RepairSubscription(topic)));
    assert_eq!(
        repaired,
        [frame("unsubscribe", channel), frame("subscribe", channel)]
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let released = h.frames(|h| h.command(unsubscribe.clone()));
    let released: Vec<_> = released.into_iter().filter(|(_, c)| c == channel).collect();
    assert_eq!(released, [frame("unsubscribe", channel)]);
    assert!(h.frames(|h| h.command(unsubscribe)).is_empty());
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_channel() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: "ETH".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "ETH".into(),
            interval: Interval::M5,
        },
        candles(),
        CANDLES,
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_channel() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "ETH".into(),
        },
        book(),
        BOOK,
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns_once() {
    // The engine owns the managed feed's interests and names the unsubscribed key.
    for (topic, key) in [(candles(), CANDLES), (book(), BOOK)] {
        let mut h = H::ready();
        let sent = h.frames(|h| h.command(Command::RepairSubscription(topic.clone())));
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(
            h.warnings,
            [format!(
                "Lighter cannot repair the {topic} subscription: {key} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_public_socket_is_down_sends_nothing_and_the_reconnect_resubscribes_once() {
    let mut h = H::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: "ETH".into(),
    });
    let old = h.socket.unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let before = h.sent.len();
    h.command(Command::RepairSubscription(book()));
    assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    while h.socket == Some(old) {
        let until = h.now + 100.0;
        h.advance(until);
        assert!(h.now < 60_000.0, "no reconnect");
    }
    let socket = h.socket.unwrap();
    let resubscribed = h.frames(|h| h.event(Event::WsOpened { socket }));
    let books: Vec<_> = resubscribed.iter().filter(|(_, c)| c == BOOK).collect();
    assert_eq!(books, [&frame("subscribe", BOOK)], "{resubscribed:?}");
}
