//! `Command::RepairSubscription`: a dead KuCoin stream is re-subscribed
//! natively without changing what the account wants.
use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Level, Notice, Response, SocketId,
    SubscriptionTopic, TimerId, Venue,
};
use safe_cex_venue_kucoin::{Kucoin, Options};

const MARKET: &str = r#"{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001,"expireDate":null,"isInverse":false}"#;
const BULLET: &str = r#"{"code":"200000","data":{"token":"public-token","instanceServers":[{"endpoint":"wss://example.test/ws","pingInterval":18000,"pingTimeout":10000}]}}"#;
const BOOK: &str = "/contractMarket/level2:XBTUSDTM";
const CANDLES: &str = "/contractMarket/limitCandle:XBTUSDTM_5min";

struct H<V: Venue> {
    engine: Engine<V>,
    now: f64,
    connects: Vec<SocketId>,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    warnings: Vec<String>,
}

impl<V: Venue> H<V> {
    fn new(venue: V) -> Self {
        let mut h = Self {
            engine: safe_cex_engine::test_support::seeded_engine(venue),
            now: 0.0,
            connects: vec![],
            requests: vec![],
            timers: vec![],
            sent: vec![],
            warnings: vec![],
        };
        h.engine.start(0.0);
        h.collect();
        h
    }

    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::WsConnect { socket, .. } => self.connects.push(socket),
                Action::Http(request) => self.requests.push(request),
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

    fn reply(&mut self, path: &str, body: &str) {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}"));
        let request = self.requests.remove(index).id;
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }

    /// Answers one bullet and opens the socket it connects, past its welcome.
    fn connect(&mut self, bullet: &str) -> SocketId {
        self.reply(bullet, BULLET);
        let socket = self.connects.pop().expect("socket connect");
        self.event(Event::WsOpened { socket });
        self.event(Event::WsMessage {
            socket,
            text: r#"{"type":"welcome"}"#,
        });
        socket
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

    /// Every (type, topic) frame `f` sends on `socket`, ignoring keep-alive pings.
    fn frames(&mut self, socket: SocketId, f: impl FnOnce(&mut Self)) -> Vec<(String, String)> {
        let before = self.sent.len();
        f(self);
        let frames: Vec<_> = self.sent[before..].iter().map(parts).collect();
        assert!(
            self.sent[before..].iter().all(|(s, _)| *s == socket),
            "{frames:?}"
        );
        frames
            .into_iter()
            .filter(|(kind, _)| kind != "ping")
            .collect()
    }
}

fn parts((_, text): &(SocketId, String)) -> (String, String) {
    let body: serde_json::Value = serde_json::from_str(text).unwrap();
    (
        body["type"].as_str().unwrap().to_owned(),
        body["topic"].as_str().unwrap_or_default().to_owned(),
    )
}

fn frame(kind: &str, topic: &str) -> (String, String) {
    (kind.to_owned(), topic.to_owned())
}

fn on(topic: &str, frames: Vec<(String, String)>) -> Vec<(String, String)> {
    frames.into_iter().filter(|(_, t)| t == topic).collect()
}

/// Futures markets loaded and the detail socket welcomed.
fn futures() -> (H<Kucoin>, SocketId) {
    let mut h = H::new(Kucoin::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    h.reply(
        "/api/v1/contracts/active",
        &format!(r#"{{"code":"200000","data":[{MARKET}]}}"#),
    );
    h.reply(
        "/api/v1/allTickers",
        r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100"}]}"#,
    );
    let detail = h.connect("bullet-public");
    h.connect("bullet-public");
    (h, detail)
}

fn candles(symbol: &str) -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: symbol.into(),
        interval: Interval::M5,
    }
}

fn book(symbol: &str) -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: symbol.into(),
    }
}

fn subscribe(topic: &SubscriptionTopic) -> Command {
    match topic.clone() {
        SubscriptionTopic::Candles { symbol, interval } => {
            Command::SubscribeCandles { symbol, interval }
        }
        SubscriptionTopic::OrderBook { symbol } => Command::SubscribeOrderBook { symbol },
    }
}

fn unsubscribe(topic: &SubscriptionTopic) -> Command {
    match topic.clone() {
        SubscriptionTopic::Candles { symbol, interval } => {
            Command::UnsubscribeCandles { symbol, interval }
        }
        SubscriptionTopic::OrderBook { symbol } => Command::UnsubscribeOrderBook { symbol },
    }
}

fn assert_repair_resubscribes_once<V: Venue>(
    mut h: H<V>,
    socket: SocketId,
    topic: SubscriptionTopic,
    channel: &str,
) {
    let subscribed = h.frames(socket, |h| h.command(subscribe(&topic)));
    assert_eq!(on(channel, subscribed), [frame("subscribe", channel)]);

    let repaired = h.frames(socket, |h| {
        h.command(Command::RepairSubscription(topic.clone()))
    });
    assert_eq!(
        repaired,
        [frame("unsubscribe", channel), frame("subscribe", channel)]
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);

    // Intent is unchanged: the one subscriber still releases exactly once.
    let released = h.frames(socket, |h| h.command(unsubscribe(&topic)));
    assert_eq!(on(channel, released), [frame("unsubscribe", channel)]);
    assert!(
        h.frames(socket, |h| h.command(unsubscribe(&topic)))
            .is_empty()
    );
    assert!(h.warnings.is_empty(), "{:?}", h.warnings);
}

#[test]
fn repairing_a_live_futures_candle_resubscribes_exactly_that_channel() {
    let (h, detail) = futures();
    assert_repair_resubscribes_once(h, detail, candles("BTCUSDT"), CANDLES);
}

#[test]
fn repairing_a_live_futures_book_resubscribes_exactly_its_level2_channel() {
    let (h, detail) = futures();
    assert_repair_resubscribes_once(h, detail, book("BTCUSDT"), BOOK);
}

#[test]
fn repairing_an_unsubscribed_futures_topic_sends_nothing_and_warns_once() {
    for topic in [candles("BTCUSDT"), book("BTCUSDT")] {
        let (mut h, _) = futures();
        let before = h.sent.len();
        h.command(Command::RepairSubscription(topic.clone()));
        assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
        assert_eq!(
            h.warnings,
            [format!(
                "Kucoin cannot repair the {topic} subscription: {topic} is not subscribed"
            )]
        );
    }
}

#[test]
fn a_futures_repair_while_the_detail_socket_is_down_is_refused_not_queued() {
    let (mut h, detail) = futures();
    h.command(subscribe(&book("BTCUSDT")));
    h.event(Event::WsClosed {
        socket: detail,
        code: 1006,
    });
    let before = h.sent.len();
    h.command(Command::RepairSubscription(book("BTCUSDT")));
    assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
    assert_eq!(
        h.warnings,
        [format!(
            "Kucoin cannot repair the order book BTCUSDT subscription: \
             the public socket carrying {BOOK} is not open"
        )]
    );
    // Reconnecting resubscribes the book once, as before; the repair left nothing behind.
    while !h.requests.iter().any(|r| r.url.contains("bullet-public")) {
        let until = h.now + 100.0;
        h.advance(until);
        assert!(h.now < 60_000.0, "no reconnect");
    }
    let before = h.sent.len();
    let socket = h.connect("bullet-public");
    assert_ne!(socket, detail);
    let resubscribed = on(BOOK, h.sent[before..].iter().map(parts).collect());
    assert_eq!(resubscribed, [frame("subscribe", BOOK)]);
}

#[cfg(feature = "spot")]
mod spot {
    use super::*;
    use safe_cex_venue_kucoin::spot::KucoinSpot;

    const MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"}]}"#;
    const TICKERS: &str = r#"{"code":"200000","data":{"time":1700000000000,"ticker":[{"symbol":"BTC-USDT","last":"100","buy":"99","sell":"101"}]}}"#;
    const BOOK: &str = "/spotMarket/level2Depth50:BTC-USDT";
    const CANDLES: &str = "/market/candles:BTC-USDT_5min";

    /// Spot markets loaded and the public socket welcomed.
    fn spot() -> (H<KucoinSpot>, SocketId) {
        let mut h = H::new(KucoinSpot::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        h.reply("/api/v2/symbols", MARKETS);
        h.reply("/allTickers", TICKERS);
        let public = h.connect("/bullet-public");
        (h, public)
    }

    #[test]
    fn repairing_a_live_spot_candle_resubscribes_exactly_that_channel() {
        let (h, public) = spot();
        assert_repair_resubscribes_once(h, public, candles("BTC-USDT"), CANDLES);
    }

    #[test]
    fn repairing_a_live_spot_book_resubscribes_exactly_that_channel() {
        let (h, public) = spot();
        assert_repair_resubscribes_once(h, public, book("BTC-USDT"), BOOK);
    }

    #[test]
    fn repairing_an_unsubscribed_spot_topic_sends_nothing_and_warns_once() {
        for topic in [candles("BTC-USDT"), book("BTC-USDT")] {
            let (mut h, _) = spot();
            let before = h.sent.len();
            h.command(Command::RepairSubscription(topic.clone()));
            assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
            assert_eq!(
                h.warnings,
                [format!(
                    "KucoinSpot cannot repair the {topic} subscription: {topic} is not subscribed"
                )]
            );
        }
    }

    #[test]
    fn a_spot_repair_while_the_public_socket_is_down_is_refused_not_queued() {
        let (mut h, public) = spot();
        h.command(subscribe(&candles("BTC-USDT")));
        h.event(Event::WsClosed {
            socket: public,
            code: 1006,
        });
        let before = h.sent.len();
        h.command(Command::RepairSubscription(candles("BTC-USDT")));
        assert_eq!(h.sent.len(), before, "{:?}", &h.sent[before..]);
        assert_eq!(
            h.warnings,
            [format!(
                "KucoinSpot cannot repair the candles BTC-USDT 5m subscription: \
                 the public socket carrying {CANDLES} is not open"
            )]
        );
    }
}
