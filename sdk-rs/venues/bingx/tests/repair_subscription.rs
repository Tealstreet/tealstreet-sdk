//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Level, Notice, Response, SocketId,
    SubscriptionTopic, TimerId,
};
use safe_cex_venue_bingx::Bingx;
use support::Options;

const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0","tradeMinQuantity":"0.0001","maxLongLeverage":125}]}"#;
const TICKERS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;

/// A public-data engine with its public socket open, its timers, and what it
/// sent on that socket. Every public read is answered as it is made.
struct Wire {
    engine: Engine<Bingx>,
    now: f64,
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
            now: 0.0,
            public: None,
            timers: Vec::new(),
            sent: Vec::new(),
            warnings: Vec::new(),
        };
        wire.engine.start(wire.now);
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
                Action::WsConnect { socket, .. } => self.public = Some(socket),
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
            let path = request.url.split('?').next().unwrap();
            let body = if path.ends_with("/quote/contracts") {
                MARKETS
            } else if path.ends_with("/quote/ticker") {
                TICKERS
            } else if path.ends_with("/quote/premiumIndex") {
                PREMIUM
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

    /// Native `(reqType, dataType)` frames sent on the public socket by `command`.
    fn frames(&mut self, command: Command) -> Vec<(String, String)> {
        let before = self.sent.len();
        self.engine.handle(self.now, Event::Command(command));
        self.settle();
        self.sent[before..]
            .iter()
            .map(|text| {
                let frame: serde_json::Value = serde_json::from_str(text).unwrap();
                (
                    frame["reqType"].as_str().unwrap().to_owned(),
                    frame["dataType"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
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
    let named = |op: &str| (op.to_owned(), key.to_owned());
    assert_eq!(wire.frames(subscribe), [named("sub")]);
    let repaired = wire.frames(Command::RepairSubscription(topic));
    assert_eq!(repaired, [named("unsub"), named("sub")]);
    assert!(wire.warnings.is_empty(), "{:?}", wire.warnings);

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    assert_eq!(wire.frames(unsubscribe), [named("unsub")]);
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
        "market.kline.BTC-USDT.5min.trade.utc+8",
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
        "market.depth.BTC-USDT.step0.level100",
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
            warning.starts_with(&format!("Bingx cannot repair the {topic} subscription")),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}
