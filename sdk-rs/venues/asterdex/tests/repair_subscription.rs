//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Level, Notice, Response, SocketId,
    SubscriptionTopic, TimerId,
};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;

/// A public-data engine with its public socket open, its timers, and what it
/// sent on that socket.
struct Wire {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    public: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<String>,
    warnings: Vec<String>,
}

impl Wire {
    fn new() -> Self {
        let mut wire = Self {
            engine: support::engine(Options {
                public_data_only: true,
                ..Options::default()
            }),
            now: 1_700_000_000_000.0,
            requests: Vec::new(),
            public: None,
            timers: Vec::new(),
            sent: Vec::new(),
            warnings: Vec::new(),
        };
        wire.engine.start(wire.now);
        wire.collect();
        if wire.requests.iter().any(|r| r.url.ends_with("/time")) {
            wire.reply("/time", r#"{"serverTime":1700000000000}"#);
        }
        wire.reply("/exchangeInfo", MARKETS);
        wire.reply("/leverageBracket", "[]");
        wire.reply(
            "/ticker/24hr",
            r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#,
        );
        wire.reply("/openInterest", "[]");
        let socket = wire.public.expect("public connect");
        wire.engine.handle(wire.now, Event::WsOpened { socket });
        wire.settle();
        wire
    }

    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { socket, url, .. } if url.ends_with("/stream") => {
                    self.public = Some(socket)
                }
                Action::WsSend { socket, text } if Some(socket) == self.public => {
                    self.sent.push(text)
                }
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

    fn reply(&mut self, suffix: &str, body: &str) {
        let position = self
            .requests
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}"));
        let request = self.requests.remove(position);
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
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

    /// Native `(method, params)` frames sent on the public socket by `command`.
    fn frames(&mut self, command: Command) -> Vec<(String, Vec<String>)> {
        let before = self.sent.len();
        self.engine
            .handle_command(self.now, command, None)
            .expect("valid caller receipt");
        self.settle();
        self.sent[before..]
            .iter()
            .map(|text| native(text))
            .collect()
    }
}

fn native(text: &str) -> (String, Vec<String>) {
    let frame: serde_json::Value = serde_json::from_str(text).unwrap();
    let params = frame["params"]
        .as_array()
        .unwrap_or_else(|| panic!("{text}"))
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect();
    (frame["method"].as_str().unwrap().to_owned(), params)
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
    let named = |method: &str| (method.to_owned(), vec![key.to_owned()]);
    let repaired = wire.frames(Command::RepairSubscription(topic));
    assert_eq!(repaired, [named("UNSUBSCRIBE"), named("SUBSCRIBE")]);
    assert!(wire.warnings.is_empty(), "{:?}", wire.warnings);

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    assert_eq!(wire.frames(unsubscribe), [named("UNSUBSCRIBE")]);
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
        "btcusdt@kline_5m",
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
        "btcusdt@depth20@100ms",
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
            warning.starts_with(&format!("AsterDex cannot repair the {topic} subscription")),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}
