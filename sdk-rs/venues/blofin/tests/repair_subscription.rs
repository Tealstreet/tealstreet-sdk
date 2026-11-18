//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Level, Notice, Response, SocketChannel, SocketId,
    SubscriptionTopic, TimerId,
};
use safe_cex_venue_blofin::{Blofin, Options};

const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
const EMPTY: &str = r#"{"code":"0","data":[]}"#;

/// A public-data engine with its public socket open, its timers, and what it
/// sent on that socket. Every public read is answered as it is made.
struct Wire {
    engine: Engine<Blofin>,
    now: f64,
    public: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<String>,
    warnings: Vec<String>,
}

impl Wire {
    fn new() -> Self {
        let mut wire = Self {
            engine: safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
                public_data_only: true,
                ..Options::default()
            })),
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
            let body = if request.url.contains("/instruments") {
                INSTRUMENT
            } else {
                EMPTY
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

    /// Native `(op, channels)` frames sent on the public socket by `command`.
    fn frames(&mut self, command: Command) -> Vec<(String, Vec<String>)> {
        let before = self.sent.len();
        self.engine.handle(self.now, Event::Command(command));
        self.settle();
        self.sent[before..]
            .iter()
            .filter(|text| text.as_str() != "ping")
            .map(|text| {
                let frame: serde_json::Value = serde_json::from_str(text).unwrap();
                let channels = frame["args"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{text}"))
                    .iter()
                    .map(|arg| {
                        assert_eq!(arg["instId"], "BTC-USDT", "{text}");
                        arg["channel"].as_str().unwrap().to_owned()
                    })
                    .collect();
                (frame["op"].as_str().unwrap().to_owned(), channels)
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
    channel: &str,
) {
    let mut wire = Wire::new();
    let subscribed = wire.frames(subscribe);
    assert!(
        subscribed
            .iter()
            .any(|(_, channels)| channels.iter().any(|c| c == channel)),
        "{subscribed:?}"
    );
    let named = |op: &str| (op.to_owned(), vec![channel.to_owned()]);
    let repaired = wire.frames(Command::RepairSubscription(topic));
    assert_eq!(repaired, [named("unsubscribe"), named("subscribe")]);
    assert!(wire.warnings.is_empty(), "{:?}", wire.warnings);

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let released = wire.frames(unsubscribe);
    let naming: Vec<_> = released
        .iter()
        .filter(|(_, channels)| channels.iter().any(|c| c == channel))
        .collect();
    assert_eq!(naming.len(), 1, "{released:?}");
    assert_eq!(naming[0].0, "unsubscribe", "{released:?}");
    let before = wire.sent.len();
    wire.settle();
    assert_eq!(wire.sent.len(), before, "{:?}", &wire.sent[before..]);
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_channel() {
    // The candle's shared books5 stream is not part of the candle repair.
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
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut wire = Wire::new();
        let sent = wire.frames(Command::RepairSubscription(topic.clone()));
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(wire.warnings.len(), 1, "{:?}", wire.warnings);
        let warning = &wire.warnings[0];
        assert!(
            warning.starts_with(&format!("Blofin cannot repair the {topic} subscription")),
            "{warning}"
        );
        assert!(warning.ends_with("is not subscribed"), "{warning}");
    }
}
