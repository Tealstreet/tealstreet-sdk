//! `Command::RepairSubscription`: a dead GRVT stream is re-subscribed natively
//! without changing what the account wants.
use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, RequestId, Response, SocketChannel, SocketId,
    SubscriptionTopic, TimerId, Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};

const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
const BOOK: (&str, &str) = ("v1.book.s", "BTC_USDT_Perp@500-50");
const CANDLES: (&str, &str) = ("v1.candle", "BTC_USDT_Perp@CI_5_M-TRADE");

struct Public {
    engine: Engine<Grvt>,
    now: f64,
    public: Option<SocketId>,
    candle: Option<SocketId>,
    http: Vec<RequestId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl Public {
    /// Markets loaded and the public socket open.
    fn ready() -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
            public_data_only: true,
            ..Options::default()
        }));
        let now = 1_700_000_000_000.;
        engine.start(now);
        let mut h = Self {
            engine,
            now,
            public: None,
            candle: None,
            http: vec![],
            timers: vec![],
            sent: vec![],
            warnings: vec![],
        };
        h.collect();
        let request = h.http.remove(0);
        h.event(Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        });
        let socket = h.public.unwrap();
        h.event(Event::WsOpened { socket });
        h
    }

    fn collect(&mut self) {
        for action in prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.http.push(request.id),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    context,
                    ..
                } => {
                    if context.is_some_and(|c| c.feed.starts_with("candles-")) {
                        self.candle = Some(socket);
                    } else {
                        self.public = Some(socket);
                    }
                }
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

    fn subscribe_candles(&mut self) {
        self.command(Command::SubscribeCandles {
            symbol: "BTC_USDT_PERP".into(),
            interval: Interval::M5,
        });
        let socket = self.candle.unwrap();
        self.event(Event::WsOpened { socket });
    }

    /// Every (method, stream, selectors) frame `f` sends, on any socket.
    fn frames(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Frame> {
        let before = self.sent.len();
        f(self);
        self.sent[before..]
            .iter()
            .filter(|(_, text)| text.starts_with('{'))
            .map(|(_, text)| parts(text))
            .collect()
    }
}

type Frame = (String, String, Vec<String>);

fn parts(text: &str) -> Frame {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    let params = &body["params"];
    (
        body["method"].as_str().unwrap().to_owned(),
        params["stream"].as_str().unwrap_or_default().to_owned(),
        params["selectors"]
            .as_array()
            .map(|s| s.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default(),
    )
}

fn frame(method: &str, (stream, selector): (&str, &str)) -> Frame {
    (
        method.to_owned(),
        stream.to_owned(),
        vec![selector.to_owned()],
    )
}

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTC_USDT_PERP".into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: "BTC_USDT_PERP".into(),
    }
}

fn book_snapshot(h: &mut Public, sequence: u64, bid: &str) {
    let socket = h.public.unwrap();
    let text = format!(
        r#"{{"stream":"v1.book.s","selector":"BTC_USDT_Perp@500-50","sequence_number":"{sequence}","feed":{{"instrument":"BTC_USDT_Perp","bids":[{{"price":"{bid}","size":"1"}}],"asks":[{{"price":"200","size":"1"}}]}}}}"#
    );
    h.event(Event::WsMessage {
        socket,
        text: &text,
    });
}

fn assert_repaired_once(
    h: &mut Public,
    socket: SocketId,
    topic: SubscriptionTopic,
    native: (&str, &str),
) {
    let before = h.sent.len();
    h.command(Command::RepairSubscription(topic));
    let repaired = &h.sent[before..];
    assert!(repaired.iter().all(|(s, _)| *s == socket));
    let repaired: Vec<_> = repaired.iter().map(|(_, t)| parts(t)).collect();
    assert_eq!(
        repaired,
        [frame("unsubscribe", native), frame("subscribe", native)]
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_channel() {
    let mut h = Public::ready();
    let subscribed = h.frames(Public::subscribe_candles);
    assert_eq!(subscribed, [frame("subscribe", CANDLES)]);
    let socket = h.candle.unwrap();
    assert_repaired_once(&mut h, socket, candles(), CANDLES);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let unsubscribe = Command::UnsubscribeCandles {
        symbol: "BTC_USDT_PERP".into(),
        interval: Interval::M5,
    };
    let released = h.frames(|h| h.command(unsubscribe.clone()));
    assert_eq!(released, [frame("unsubscribe", CANDLES)]);
    assert!(h.frames(|h| h.command(unsubscribe)).is_empty());
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_channel() {
    let mut h = Public::ready();
    let subscribe = Command::SubscribeOrderBook {
        symbol: "BTC_USDT_PERP".into(),
    };
    let subscribed = h.frames(|h| h.command(subscribe));
    assert_eq!(subscribed, [frame("subscribe", BOOK)]);
    book_snapshot(&mut h, 7, "100");
    let socket = h.public.unwrap();
    assert_repaired_once(&mut h, socket, book(), BOOK);

    // The new stream restarts its sequence; its first snapshot is the book.
    book_snapshot(&mut h, 1, "101");
    let book = h.engine.venue().book("BTC_USDT_PERP").unwrap();
    assert_eq!(book.bids(), [(101., 1.)]);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let unsubscribe = Command::UnsubscribeOrderBook {
        symbol: "BTC_USDT_PERP".into(),
    };
    let released = h.frames(|h| h.command(unsubscribe.clone()));
    assert_eq!(released, [frame("unsubscribe", BOOK)]);
    assert!(h.frames(|h| h.command(unsubscribe)).is_empty());
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
                "Grvt cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_repair_while_the_carrying_socket_is_down_is_refused_not_queued() {
    let mut h = Public::ready();
    h.subscribe_candles();
    let socket = h.candle.unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    let sent = h.frames(|h| h.command(Command::RepairSubscription(candles())));
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(
        h.warnings,
        [format!(
            "Grvt cannot repair the {topic} subscription: \
             the candle socket carrying {topic} is not open",
            topic = candles()
        )]
    );

    let mut h = Public::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTC_USDT_PERP".into(),
    });
    let old = h.public.take().unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let sent = h.frames(|h| h.command(Command::RepairSubscription(book())));
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(
        h.warnings,
        [
            "Grvt cannot repair the order book BTC_USDT_PERP subscription: \
             the public socket carrying v1.book.s BTC_USDT_Perp@500-50 is not open"
        ]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    let resubscribed = h.frames(|h| {
        while h.public.is_none() {
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
        let socket = h.public.unwrap();
        h.event(Event::WsOpened { socket });
    });
    let books: Vec<_> = resubscribed.iter().filter(|f| f.1 == BOOK.0).collect();
    assert_eq!(books, [&frame("subscribe", BOOK)]);
}
