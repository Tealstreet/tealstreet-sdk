use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_blofin::{Blofin, Options};

const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
const TICKERS: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"100","open24h":"80","bidPrice":"99","askPrice":"101","volCurrency24h":"42","vol24h":"4200"}]}"#;
const TICKER: &str = r#"{"arg":{"channel":"tickers","instId":"BTC-USDT"},"data":[{"instId":"BTC-USDT","last":"101","open24h":"80","bidPrice":"100","askPrice":"102","volCurrency24h":"42","vol24h":"4200"}]}"#;

struct Harness {
    engine: Engine<Blofin>,
    now: f64,
    requests: Vec<RequestId>,
    timers: Vec<(TimerId, f64)>,
    sockets: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    fired: usize,
}

impl Harness {
    fn new() -> Self {
        Self::with_policy(PublicFeedPolicy::Direct)
    }

    fn with_policy(policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
            public_data_only: true,
            ..Options::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        let mut h = Self {
            engine,
            now: 0.0,
            requests: vec![],
            timers: vec![],
            sockets: vec![],
            sent: vec![],
            closed: vec![],
            fired: 0,
        };
        h.engine.start(0.0);
        h.collect();
        for body in [INSTRUMENT, TICKERS] {
            let request = h.requests.pop().expect("public read");
            h.event(Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            });
        }
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
        h.event(Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }));
        h
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request.id),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { socket, .. } => self.sockets.push(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                _ => {}
            }
        }
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

    fn socket(&self) -> SocketId {
        *self.sockets.last().unwrap()
    }

    fn close(&mut self) -> f64 {
        let socket = self.socket();
        let before = self.timers.clone();
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

    fn reconnect(&mut self, delay: f64) {
        let sockets = self.sockets.len();
        let until = self.now + delay;
        self.advance(until);
        assert_eq!(self.sockets.len(), sockets + 1, "one reconnect per close");
    }

    fn pings(&self, socket: SocketId) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t == "ping")
            .count()
    }
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    let mut h = Harness::new();
    for attempt in 0..7 {
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = h.close();
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        h.reconnect(delay);
    }
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::new();
    let old = h.socket();
    h.event(Event::WsOpened { socket: old });
    let delay = h.close();
    h.reconnect(delay);
    let socket = h.socket();
    assert_ne!(socket, old);
    h.sent.clear();
    h.event(Event::WsOpened { socket });
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    for channel in ["tickers", "funding-rate", "books", "books5", "candle1m"] {
        let topic = format!(r#""channel":"{channel}""#);
        assert!(sent.iter().any(|t| t.contains(&topic)), "{channel}");
    }
    assert!(sent.contains(&"ping"));
    h.event(Event::WsMessage {
        socket: old,
        text: TICKER,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 100.0);
    h.event(Event::WsMessage {
        socket,
        text: TICKER,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 101.0);
}

#[test]
fn open_socket_pings_every_five_seconds_and_silence_retires_it_at_twenty() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    h.sent.clear();
    h.advance(opened + 15_000.0);
    assert_eq!(h.pings(socket), 3);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 19_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 20_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = h.sockets.len();
    h.advance(h.now + 1_000.0);
    assert_eq!(h.sockets.len(), sockets + 1, "idle close schedules a retry");
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_twenty_seconds() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for step in 1..=8 {
        h.advance(opened + f64::from(step) * 5_000.0);
        h.event(Event::WsMessage {
            socket,
            text: "pong",
        });
    }
    assert!(!h.closed.contains(&socket));

    let delay = h.close();
    h.reconnect(delay);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + 19_999.0);
    assert!(!h.closed.contains(&pending));
    h.advance(connecting + 20_000.0);
    assert!(h.closed.contains(&pending));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    for _ in 0..3 {
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        let delay = h.close();
        h.reconnect(delay);
    }
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for step in 1..=7 {
        h.advance(opened + f64::from(step) * 5_000.0);
        h.event(Event::WsMessage {
            socket,
            text: "pong",
        });
    }
    assert!((750.0..=1_000.0).contains(&h.close()));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::with_policy(PublicFeedPolicy::Shared);
    let until = h.now + 600_000.0;
    h.advance(until);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 0, "no supervision timers fire");
}
