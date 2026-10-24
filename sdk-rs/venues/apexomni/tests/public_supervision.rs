mod support;
use safe_cex_engine::{Action, Command, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::TestEngine;

const PING: &str = r#"{"op":"ping"}"#;
const PONG: &str = r#"{"op":"pong"}"#;

struct Harness {
    engine: TestEngine,
    now: f64,
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
        let mut engine = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(0.0);
        let request = support::requests(&mut engine, 0.0).remove(0);
        support::response(&mut engine, &request, 10.0, support::MARKETS);
        let mut h = Self {
            engine,
            now: 10.0,
            timers: vec![],
            sockets: vec![],
            sent: vec![],
            closed: vec![],
            fired: 0,
        };
        h.pump();
        h
    }

    fn pump(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => self.sockets.push(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => (),
            }
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.pump();
    }

    fn socket(&self) -> SocketId {
        *self.sockets.last().unwrap()
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
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        let delay = h.close(socket);
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let sockets = h.sockets.len();
        let until = h.now + delay;
        h.advance(until);
        assert_eq!(h.sockets.len(), sockets + 1, "one reconnect per close");
    }
}

#[test]
fn reconnected_socket_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::new();
    let old = h.socket();
    h.event(Event::WsOpened { socket: old });
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let delay = h.close(old);
    let until = h.now + delay;
    h.advance(until);
    let socket = h.socket();
    assert_ne!(socket, old);
    h.sent.clear();
    h.event(Event::WsOpened { socket });
    let subscribe: Vec<_> = h.sent.iter().filter(|(s, _)| *s == socket).collect();
    assert!(
        subscribe
            .iter()
            .any(|(_, t)| t.contains("instrumentInfo.all") && t.contains("orderBook200.H.BTCUSDT"))
    );
    assert!(subscribe.iter().any(|(_, t)| t == PING));
    let ticker = |price| {
        format!(r#"{{"topic":"instrumentInfo.all","data":[{{"s":"BTCUSDT","p":"{price}"}}]}}"#)
    };
    let stale = ticker(99);
    h.event(Event::WsMessage {
        socket: old,
        text: &stale,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 0.0);
    let live = ticker(101);
    h.event(Event::WsMessage {
        socket,
        text: &live,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 101.0);
    h.event(Event::WsMessage {
        socket,
        text: r#"{"topic":"orderBook200.H.BTCUSDT","type":"snapshot","data":{"u":1,"a":[["2","1"]],"b":[["1","2"]]}}"#,
    });
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_ask(), Some(2.0));
}

#[test]
fn open_socket_pings_every_ten_seconds_and_silence_retires_it_at_thirty() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    assert_eq!(h.pings(socket), 1);
    h.advance(opened + 9_999.0);
    assert_eq!(h.pings(socket), 1);
    h.advance(opened + 20_000.0);
    assert_eq!(h.pings(socket), 3);
    h.advance(opened + 29_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 30_000.0);
    assert!(h.closed.contains(&socket));
}

#[test]
fn traffic_keeps_open_socket_alive() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for step in 1..=12 {
        h.advance(opened + f64::from(step) * 10_000.0);
        h.event(Event::WsMessage { socket, text: PONG });
    }
    assert!(!h.closed.contains(&socket));
    assert_eq!(h.pings(socket), 13);
}

#[test]
fn never_opened_socket_expires_at_thirty_seconds() {
    let mut h = Harness::new();
    let socket = h.socket();
    let connecting = h.now;
    h.advance(connecting + 29_999.0);
    assert!(!h.closed.contains(&socket));
    assert_eq!(h.pings(socket), 0);
    h.advance(connecting + 30_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = h.sockets.len();
    h.advance(connecting + 31_000.0);
    assert_eq!(h.sockets.len(), sockets + 1);
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    for _ in 0..3 {
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        let delay = h.close(socket);
        let until = h.now + delay;
        h.advance(until);
    }
    let socket = h.socket();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for step in 1..=4 {
        h.advance(opened + f64::from(step) * 10_000.0);
        h.event(Event::WsMessage { socket, text: PONG });
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
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 0, "no supervision timers fire");
}
