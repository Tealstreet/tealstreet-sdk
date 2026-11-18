//! `Command::RepairSubscription`: a dead Crypto.com stream is re-subscribed
//! natively without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, RequestId, SocketId, SubscriptionTopic, TimerId,
};
use safe_cex_venue_cryptocom::Cryptocom;
use support::fixture::Options;

const BOOK: &str = "book.BTCUSD-PERP.50";
const CANDLES: &str = "candlestick.5m.BTCUSD-PERP";

struct Public {
    engine: Engine<Cryptocom>,
    now: f64,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    http: Vec<RequestId>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl Public {
    /// Markets loaded and the market socket open past its subscribe delay.
    fn ready() -> Self {
        let mut engine = support::fixture::engine(Options {
            public_data_only: true,
            ..Default::default()
        });
        engine.start(0.0);
        let mut h = Self {
            engine,
            now: 0.0,
            socket: None,
            timers: vec![],
            http: vec![],
            sent: vec![],
            warnings: vec![],
        };
        h.collect();
        h.answer(support::INSTRUMENTS);
        h.answer(support::TICKERS);
        h.open();
        h
    }

    fn collect(&mut self) {
        for action in support::fixture::actions(&mut self.engine, self.now) {
            match action {
                Action::WsConnect { socket, url, .. } if url.ends_with("/market") => {
                    self.socket = Some(socket)
                }
                Action::Http(request) => self.http.push(request.id),
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

    fn answer(&mut self, body: &str) {
        let request = self.http.remove(0);
        self.event(Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }

    /// Opens the market socket and lets its subscribe delay pass.
    fn open(&mut self) {
        let socket = self.socket.unwrap();
        self.event(Event::WsOpened { socket });
        let until = self.now + 1_000.0;
        self.advance(until);
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

    /// Every (method, channels) frame `f` sends, on any socket.
    fn frames(&mut self, f: impl FnOnce(&mut Self)) -> Vec<(String, Vec<String>)> {
        let before = self.sent.len();
        f(self);
        self.sent[before..]
            .iter()
            .map(|(_, text)| parts(text))
            .collect()
    }
}

fn parts(text: &str) -> (String, Vec<String>) {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    let channels = body["params"]["channels"]
        .as_array()
        .map(|channels| {
            channels
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    (body["method"].as_str().unwrap().to_owned(), channels)
}

fn frame(method: &str, channel: &str) -> (String, Vec<String>) {
    (method.to_owned(), vec![channel.to_owned()])
}

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTCUSD-PERP".into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: "BTCUSD-PERP".into(),
    }
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    channel: &str,
) {
    let mut h = Public::ready();
    let subscribed = h.frames(|h| h.command(subscribe));
    assert_eq!(subscribed, [frame("subscribe", channel)]);

    let socket = h.socket.unwrap();
    let before = h.sent.len();
    h.command(Command::RepairSubscription(topic));
    let repaired = &h.sent[before..];
    assert!(repaired.iter().all(|(s, _)| *s == socket));
    let repaired: Vec<_> = repaired.iter().map(|(_, t)| parts(t)).collect();
    assert_eq!(
        repaired,
        [frame("unsubscribe", channel), frame("subscribe", channel)]
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let released = h.frames(|h| h.command(unsubscribe.clone()));
    assert_eq!(released, [frame("unsubscribe", channel)]);
    assert!(h.frames(|h| h.command(unsubscribe)).is_empty());
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_channel() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: "BTCUSD-PERP".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "BTCUSD-PERP".into(),
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
            symbol: "BTCUSD-PERP".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSD-PERP".into(),
        },
        book(),
        BOOK,
    );
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
                "Cryptocom cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_market_socket_is_down_is_refused_not_queued() {
    let mut h = Public::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSD-PERP".into(),
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
            "Cryptocom cannot repair the order book BTCUSD-PERP subscription: \
             the market socket carrying {BOOK} is not open"
        )]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    let resubscribed = h.frames(|h| {
        while h.socket.is_none() {
            let next = h.timers.iter().map(|(_, at)| *at).reduce(f64::min);
            h.advance(next.expect("a reconnect timer"));
        }
        h.open();
    });
    let books: Vec<_> = resubscribed
        .iter()
        .filter(|(_, channels)| channels.iter().any(|c| c == BOOK))
        .collect();
    assert_eq!(books.len(), 1, "{resubscribed:?}");
    assert_eq!(books[0].0, "subscribe");
}
