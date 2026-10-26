mod support;
use safe_cex_engine::{Action, Command, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId};
use safe_cex_venue_woo::{Options, Woo};
use support::{MARKETS, TICKERS, TestEngine, response};

const PING: &str = r#"{"event":"ping"}"#;
const PONG: &str = r#"{"event":"pong"}"#;
const TICKER: &str = r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":"61000","volume":5,"amount":305000}]}"#;

struct Harness {
    engine: TestEngine,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    public: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    fired: usize,
}

impl Harness {
    fn new() -> Self {
        Self::with_policy(PublicFeedPolicy::Direct)
    }
    fn with_policy(policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(0.0);
        let r = support::prepared_seed_requests(&mut engine, 0.).remove(0);
        response(&mut engine, &r, 10.0, MARKETS);
        let r = support::prepared_seed_requests(&mut engine, 10.).remove(0);
        let mut h = Self {
            engine,
            now: 20.0,
            timers: Vec::new(),
            public: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            fired: 0,
        };
        response(&mut h.engine, &r, 20.0, TICKERS);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => self.public.push(socket),
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
    fn socket(&self) -> SocketId {
        *self.public.last().unwrap()
    }
    fn open(&mut self) -> SocketId {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
        socket
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
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
    fn pings(&self, socket: SocketId) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t == PING)
            .count()
    }
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    let mut h = Harness::new();
    for attempt in 0..8 {
        let socket = h.open();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = h.close(socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let sockets = h.public.len();
        let until = h.now + delay;
        h.advance(until);
        assert_eq!(h.public.len(), sockets + 1, "one reconnect per close");
    }
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::new();
    let old = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let delay = h.close(old);
    let until = h.now + delay;
    h.advance(until);
    assert_ne!(h.socket(), old);
    h.sent.clear();
    let socket = h.open();
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    for topic in [
        "tickers",
        "bbos",
        "markprices",
        "PERP_BTC_USDT@orderbookupdate",
    ] {
        assert!(
            sent.contains(&format!(r#"{{"event":"subscribe","topic":"{topic}"}}"#).as_str()),
            "{topic}: {sent:?}"
        );
    }
    assert!(sent.contains(&PING));
    h.message(old, TICKER);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    h.message(socket, TICKER);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 61000.0);
}

#[test]
fn open_socket_pings_every_six_seconds_and_silence_retires_it_at_twenty() {
    let mut h = Harness::new();
    let socket = h.open();
    let opened = h.now;
    h.advance(opened + 18_000.0);
    assert_eq!(h.pings(socket), 4, "open ping plus three cadence pings");
    h.advance(opened + 19_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 20_000.0);
    assert!(h.closed.contains(&socket));
    assert_eq!(h.public.len(), 1);
    let until = h.now + 1_000.0;
    h.advance(until);
    assert_eq!(h.public.len(), 2, "idle timeout retries");
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_ten_seconds() {
    let mut h = Harness::new();
    let socket = h.open();
    let start = h.now;
    for step in 1..=8 {
        h.advance(start + f64::from(step) * 6_000.0);
        h.message(socket, PONG);
    }
    assert!(!h.closed.contains(&socket));

    let delay = h.close(socket);
    let until = h.now + delay;
    h.advance(until);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + 9_999.0);
    assert!(!h.closed.contains(&pending));
    h.advance(connecting + 10_000.0);
    assert!(h.closed.contains(&pending));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    for _ in 0..3 {
        let socket = h.open();
        let delay = h.close(socket);
        let until = h.now + delay;
        h.advance(until);
    }
    let socket = h.open();
    let opened = h.now;
    for step in 1..=6 {
        h.advance(opened + f64::from(step) * 6_000.0);
        h.message(socket, PONG);
    }
    assert!((750.0..=1_000.0).contains(&h.close(socket)));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::with_policy(PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let until = h.now + 600_000.0;
    h.advance(until);
    assert!(h.public.is_empty(), "no public connects: {:?}", h.public);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 10, "only the minute funding refresh fires");
}
