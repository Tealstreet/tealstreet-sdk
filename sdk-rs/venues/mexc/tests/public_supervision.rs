mod support;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, PublicFeedPolicy, Response, SocketChannel, SocketId,
    TimerId,
};
use safe_cex_venue_mexc::{MexcFutures, Options};

const PING: &str = r#"{"method":"ping"}"#;
const PONG: &str = r#"{"channel":"pong","data":1}"#;
const TICKER: &str =
    r#"{"channel":"push.tickers","data":[{"symbol":"BTC_USDT","lastPrice":61000}]}"#;

struct Harness {
    engine: support::TestEngine,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    public: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    http: Vec<HttpRequest>,
    fired: usize,
}

impl Harness {
    fn new() -> Self {
        Self::with_policy(PublicFeedPolicy::Direct)
    }
    fn with_policy(policy: PublicFeedPolicy) -> Self {
        let mut engine = support::TestEngine::from(safe_cex_engine::test_support::seeded_engine(
            MexcFutures::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
        ));
        engine.set_public_feed_policy(policy).unwrap();
        let mut h = Self {
            engine,
            now: 0.0,
            timers: vec![],
            public: vec![],
            sent: vec![],
            closed: vec![],
            http: vec![],
            fired: 0,
        };
        h.engine.start(0.0);
        h.collect();
        h.now = 100.0;
        h.respond(support::MARKETS);
        h.respond(support::TICKERS);
        h
    }
    fn collect(&mut self) {
        for action in support::prepared(&mut self.engine, self.now) {
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
                Action::Http(request) => self.http.push(request),
                _ => {}
            }
        }
    }
    fn respond(&mut self, body: &str) {
        let request = self.http.remove(0);
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn socket(&self) -> SocketId {
        *self.public.last().unwrap()
    }
    fn open(&mut self) {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
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
    /// Closes the current public socket and returns the one retry delay it schedules.
    fn close(&mut self) -> f64 {
        let before = self.timers.clone();
        let socket = self.socket();
        self.event(Event::WsClosed { socket, code: 1006 });
        let fresh: Vec<_> = self
            .timers
            .iter()
            .filter(|timer| !before.contains(timer))
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
    for attempt in 0..7 {
        h.open();
        let delay = h.close();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(30_000.0);
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
fn reconnect_resubscribes_tickers_and_books_and_ignores_retired_socket() {
    let mut h = Harness::new();
    h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let old = h.socket();
    let until = h.now + h.close();
    h.advance(until);
    let socket = h.socket();
    assert_ne!(socket, old);
    h.sent.clear();
    h.http.clear();
    h.open();
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(sent[0], PING);
    assert!(sent.iter().any(|t| t.contains("sub.tickers")));
    assert!(
        sent.iter()
            .any(|t| t.contains("sub.depth") && t.contains("BTC_USDT"))
    );
    assert_eq!(h.http.len(), 1, "book snapshot is refetched");
    h.message(old, TICKER);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    h.message(socket, TICKER);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 61000.0);
}

#[test]
fn open_socket_pings_every_eight_seconds_and_silence_retires_it_at_twenty() {
    let mut h = Harness::new();
    h.open();
    let socket = h.socket();
    let opened = h.now;
    advance_and_check(&mut h, opened + 16_000.0, socket, 3);
    advance_and_check(&mut h, opened + 19_999.0, socket, 3);
    h.advance(opened + 20_000.0);
    assert_eq!(h.closed, [socket]);
    assert_eq!(h.public.len(), 1, "retry waits for backoff");
    // First retry backoff is at most the 1s base.
    let until = h.now + 1_000.0;
    h.advance(until);
    assert_eq!(h.public.len(), 2, "idle timeout reconnects once");
    assert_ne!(h.socket(), socket);
}

fn advance_and_check(h: &mut Harness, until: f64, socket: SocketId, pings: usize) {
    h.advance(until);
    assert_eq!(h.pings(socket), pings);
    assert!(h.closed.is_empty());
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_ten_seconds() {
    let mut h = Harness::new();
    h.open();
    let socket = h.socket();
    let opened = h.now;
    for step in 1..=10 {
        h.advance(opened + f64::from(step) * 8_000.0);
        h.message(socket, PONG);
    }
    assert!(h.closed.is_empty());
    assert_eq!(h.pings(socket), 11);

    let until = h.now + h.close();
    h.advance(until);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + 9_999.0);
    assert!(h.closed.is_empty());
    h.advance(connecting + 10_000.0);
    assert_eq!(h.closed, [pending]);
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.open();
        let until = h.now + h.close();
        h.advance(until);
    }
    h.open();
    let socket = h.socket();
    let opened = h.now;
    for step in 1..=4 {
        h.advance(opened + f64::from(step) * 8_000.0);
        h.message(socket, PONG);
    }
    assert!((750.0..=1_000.0).contains(&h.close()));
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
    assert_eq!(h.fired, 0, "no supervision timers fire");
}
