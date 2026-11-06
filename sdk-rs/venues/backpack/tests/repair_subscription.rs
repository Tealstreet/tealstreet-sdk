//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Level, Notice, Response, SocketChannel, SocketId,
    SubscriptionTopic, TimerId,
};
use safe_cex_venue_backpack::{Backpack, Options};
use support::Driver;

const SYMBOL: &str = "SOL_USDC_PERP";
const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#;
const TICKERS: &str = r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#;
const MARKS: &str = r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#;
const DEPTH: &str = r#"{"lastUpdateId":1,"bids":[],"asks":[]}"#;

/// A public-data engine with its public socket open, its timers, and what it
/// sent on that socket. Every public read is answered as it is made.
struct Wire {
    engine: Driver,
    now: f64,
    public: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<String>,
    warnings: Vec<String>,
}

impl Wire {
    fn new() -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        engine.start(0.0);
        let mut wire = Self {
            engine,
            now: 0.0,
            public: None,
            timers: Vec::new(),
            sent: Vec::new(),
            warnings: Vec::new(),
        };
        wire.collect();
        let socket = wire.public.expect("public connect");
        wire.engine.handle(wire.now, Event::WsOpened { socket });
        wire.settle();
        wire
    }

    fn collect(&mut self) {
        let mut requests: Vec<HttpRequest> = Vec::new();
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => requests.push(request),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => self.public = Some(socket),
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
        for request in requests {
            let body = if request.url.contains("/markets") {
                MARKETS
            } else if request.url.contains("/tickers") {
                TICKERS
            } else if request.url.contains("/markPrices") {
                MARKS
            } else if request.url.contains("/depth") {
                DEPTH
            } else {
                panic!("unexpected public read {}", request.url)
            };
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
        self.engine.handle(self.now, Event::Command(command));
        self.settle();
        self.sent[before..]
            .iter()
            .filter(|text| !text.contains("PING"))
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
        symbol: SYMBOL.into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: SYMBOL.into(),
    }
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    key: &str,
) {
    let mut wire = Wire::new();
    let named = |method: &str| (method.to_owned(), vec![key.to_owned()]);
    assert_eq!(wire.frames(subscribe), [named("SUBSCRIBE")]);
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
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        candles(),
        "kline.5m.SOL_USDC_PERP",
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        book(),
        "depth.SOL_USDC_PERP",
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
            warning.starts_with(&format!("Backpack cannot repair the {topic} subscription")),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}
