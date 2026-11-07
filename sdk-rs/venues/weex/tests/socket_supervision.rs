//! Public and private WEEX sockets: 30s connect/idle deadlines, 5s PING, 1s-16s backoff.
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, RequestId, Response, SocketChannel, SocketId,
    TimerId,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::PreparedActions;
use support::setup::Options;

const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"90000"}]"#;
const BOOK_TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"89999","askPrice":"90001"}]"#;
const DEPTH: &str = r#"{"lastUpdateId":160,"bids":[["89999","2"]],"asks":[["90001","1"]]}"#;
const TICKER: &str = r#"{"e":"ticker","s":"BTCUSDT","d":[{"c":"91000"}]}"#;
const PUBLIC_PING: &str = r#"{"event":"ping","time":"1693208170000"}"#;
const PRIVATE_PING: &str = r#"{"type":"ping","time":"1693208170000"}"#;
const PRIVATE_SUBSCRIBE: &str = r#""params":["account","positions","orders","fill"]"#;

#[derive(Clone, Copy, PartialEq)]
enum Feed {
    Public,
    Private,
}

struct Harness {
    engine: Engine<WeexFutures>,
    feed: Feed,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    requests: Vec<(RequestId, String)>,
    public: Vec<SocketId>,
    private: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    fired: usize,
}

impl Harness {
    fn new(feed: Feed) -> Self {
        Self::with_policy(feed, PublicFeedPolicy::Direct)
    }
    fn with_policy(feed: Feed, policy: PublicFeedPolicy) -> Self {
        let private = feed == Feed::Private;
        let mut engine = support::setup::engine(Options {
            public_data_only: !private,
            key: private.then(|| "key".into()),
            secret: private.then(|| "secret".into()),
            passphrase: private.then(|| "passphrase".into()),
            private_ws_headers: private,
            ..Options::default()
        });
        engine.set_public_feed_policy(policy).unwrap();
        let mut h = Self {
            engine,
            feed,
            now: 0.0,
            timers: Vec::new(),
            requests: Vec::new(),
            public: Vec::new(),
            private: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            fired: 0,
        };
        h.engine.start(0.0);
        h.collect();
        h.respond("exchangeInfo", INFO);
        h.respond("24hr", DAILY);
        h.respond("bookTicker", BOOK_TICKERS);
        h.requests.clear();
        h
    }
    fn collect(&mut self) {
        for action in self.engine.prepared_actions(self.now) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Http(r) => self.requests.push((r.id, r.url)),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => self.public.push(socket),
                Action::WsConnect {
                    socket, headers, ..
                } if !headers.is_empty() => self.private.push(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                _ => (),
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn respond(&mut self, path: &str, body: &str) {
        let index = self
            .requests
            .iter()
            .rposition(|(_, url)| url.contains(path))
            .unwrap_or_else(|| panic!("{path}: {:?}", self.requests));
        let (request, _) = self.requests.remove(index);
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn socket(&self) -> SocketId {
        *match self.feed {
            Feed::Public => &self.public,
            Feed::Private => &self.private,
        }
        .last()
        .unwrap()
    }
    fn sockets(&self) -> usize {
        match self.feed {
            Feed::Public => self.public.len(),
            Feed::Private => self.private.len(),
        }
    }
    fn open(&mut self) -> SocketId {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
        socket
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }
    fn traffic(&mut self, socket: SocketId) {
        let ping = match self.feed {
            Feed::Public => PUBLIC_PING,
            Feed::Private => PRIVATE_PING,
        };
        self.message(socket, ping);
    }
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
            self.fired += 1;
        }
        self.now = until;
    }
    fn close(&mut self, socket: SocketId) -> f64 {
        let before = self.timers.clone();
        self.event(Event::WsClosed { socket, code: 1006 });
        let fresh: Vec<_> = self
            .timers
            .iter()
            .filter(|t| !before.contains(t))
            .map(|(_, at)| at - self.now)
            .collect();
        assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
        fresh[0]
    }
    fn sent_to(&self, socket: SocketId, needle: &str) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t.contains(needle))
            .count()
    }
}

fn flapping_socket_backs_off_although_each_attempt_opens(feed: Feed) {
    let mut h = Harness::new(feed);
    for attempt in 0..8 {
        let socket = h.open();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = h.close(socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let sockets = h.sockets();
        let until = h.now + delay;
        h.advance(until);
        assert_eq!(h.sockets(), sockets + 1, "one reconnect per close");
    }
}

fn open_socket_pings_every_five_seconds_and_silence_retires_it_at_thirty(feed: Feed) {
    let mut h = Harness::new(feed);
    let socket = h.open();
    let opened = h.now;
    h.advance(opened + 25_000.0);
    assert_eq!(h.sent_to(socket, r#""method":"PING""#), 5);
    h.advance(opened + 29_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 30_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = h.sockets();
    let until = h.now + 1_000.0;
    h.advance(until);
    assert_eq!(h.sockets(), sockets + 1, "idle timeout retries");
}

fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_thirty(feed: Feed) {
    let mut h = Harness::new(feed);
    let socket = h.open();
    let start = h.now;
    for step in 1..=12 {
        h.advance(start + f64::from(step) * 5_000.0);
        h.traffic(socket);
    }
    assert!(!h.closed.contains(&socket));
    assert!(h.sent_to(socket, r#""method":"PONG""#) >= 12);

    let delay = h.close(socket);
    let until = h.now + delay;
    h.advance(until);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + 29_999.0);
    assert!(!h.closed.contains(&pending));
    assert_eq!(h.sent_to(pending, r#""method":"PING""#), 0);
    h.advance(connecting + 30_000.0);
    assert!(h.closed.contains(&pending));
}

fn stable_traffic_resets_backoff(feed: Feed) {
    let mut h = Harness::new(feed);
    for _ in 0..3 {
        let socket = h.open();
        let delay = h.close(socket);
        let until = h.now + delay;
        h.advance(until);
    }
    let socket = h.open();
    let opened = h.now;
    for step in 1..=6 {
        h.advance(opened + f64::from(step) * 5_000.0);
        h.traffic(socket);
    }
    assert!((750.0..=1_000.0).contains(&h.close(socket)));
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off_although_each_attempt_opens(Feed::Public);
}

#[test]
fn flapping_private_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off_although_each_attempt_opens(Feed::Private);
}

#[test]
fn open_public_socket_pings_every_five_seconds_and_silence_retires_it_at_thirty() {
    open_socket_pings_every_five_seconds_and_silence_retires_it_at_thirty(Feed::Public);
}

#[test]
fn open_private_socket_pings_every_five_seconds_and_silence_retires_it_at_thirty() {
    open_socket_pings_every_five_seconds_and_silence_retires_it_at_thirty(Feed::Private);
}

#[test]
fn public_traffic_keeps_socket_alive_and_never_opened_socket_expires_at_thirty() {
    traffic_keeps_socket_alive_and_never_opened_socket_expires_at_thirty(Feed::Public);
}

#[test]
fn private_traffic_keeps_socket_alive_and_never_opened_socket_expires_at_thirty() {
    traffic_keeps_socket_alive_and_never_opened_socket_expires_at_thirty(Feed::Private);
}

#[test]
fn stable_public_traffic_resets_backoff() {
    stable_traffic_resets_backoff(Feed::Public);
}

#[test]
fn stable_private_traffic_resets_backoff() {
    stable_traffic_resets_backoff(Feed::Private);
}

#[test]
fn public_reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::new(Feed::Public);
    let old = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.respond("/depth", DEPTH);
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    assert_eq!(h.sent_to(old, "BTCUSDT@depth200"), 1);
    let delay = h.close(old);
    let until = h.now + delay;
    h.advance(until);
    assert_ne!(h.socket(), old);
    let socket = h.open();
    assert_eq!(h.sent_to(socket, "BTCUSDT@ticker"), 1);
    assert_eq!(h.sent_to(socket, "BTCUSDT@kline_1m_LAST_PRICE"), 1);
    h.respond("/depth", DEPTH);
    assert_eq!(h.sent_to(socket, "BTCUSDT@depth200"), 1);
    assert!(h.engine.book("BTCUSDT").is_some());

    h.message(old, TICKER);
    h.message(old, PUBLIC_PING);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 90000.0);
    assert_eq!(h.sent_to(old, r#""method":"PONG""#), 0);
    h.message(socket, TICKER);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 91000.0);
}

#[test]
fn private_reconnect_resubscribes_and_ignores_retired_socket() {
    let mut h = Harness::new(Feed::Private);
    let old = h.open();
    assert_eq!(h.sent_to(old, PRIVATE_SUBSCRIBE), 1);
    let delay = h.close(old);
    let until = h.now + delay;
    h.advance(until);
    let socket = h.open();
    assert_ne!(socket, old);
    assert_eq!(h.sent_to(socket, PRIVATE_SUBSCRIBE), 1);
    h.message(old, PRIVATE_PING);
    assert_eq!(h.sent_to(old, r#""method":"PONG""#), 0);
    h.message(socket, PRIVATE_PING);
    assert_eq!(h.sent_to(socket, r#""method":"PONG""#), 1);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::with_policy(Feed::Public, PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let until = h.now + 600_000.0;
    h.advance(until);
    assert!(h.public.is_empty(), "no public connects: {:?}", h.public);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 0, "no public watchdog or retry timers");
}
