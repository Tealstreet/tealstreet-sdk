mod support;
use safe_cex_engine::{Action, Command, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId};
use safe_cex_venue_lighter::{Lighter, Options};
use support::*;

struct Rig {
    engine: Engine<Lighter>,
    timers: Timers,
    socket: SocketId,
    now: f64,
    fired: usize,
}

fn connected(actions: &[Action]) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Public),
            ..
        } => Some(*socket),
        _ => None,
    })
}

fn closed(actions: &[Action], socket: SocketId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
}

fn sent(actions: &[Action], socket: SocketId) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.clone()),
            _ => None,
        })
        .collect()
}

impl Rig {
    fn new() -> Self {
        Self::with_policy(PublicFeedPolicy::Direct).0
    }
    fn with_policy(policy: PublicFeedPolicy) -> (Self, Vec<Action>) {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(Options {
            public_data_only: true,
            ..Options::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(NOW);
        let mut timers = Timers::new();
        let meta = requests(&mut engine).pop().unwrap();
        reply(&mut engine, &meta, 200, META);
        let actions = drain(&mut engine, &mut timers);
        let socket = connected(&actions).unwrap_or(SocketId(0));
        let rig = Self {
            engine,
            timers,
            socket,
            now: NOW,
            fired: 0,
        };
        (rig, actions)
    }
    fn handle(&mut self, event: Event<'_>) -> Vec<Action> {
        self.engine.handle(self.now, event);
        self.drain()
    }
    fn drain(&mut self) -> Vec<Action> {
        let mut output = Vec::new();
        for _ in 0..50 {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
            if actions.is_empty() {
                return output;
            }
            for action in &actions {
                track(&mut self.timers, action);
                if let Action::Http(request) = action
                    && request.url.ends_with("exchangeStats")
                {
                    reply_at(
                        &mut self.engine,
                        self.now,
                        request,
                        200,
                        r#"{"code":200,"order_book_stats":[]}"#,
                    );
                }
            }
            output.extend(actions);
        }
        panic!("public fixture failed to settle");
    }
    fn open(&mut self) -> Vec<Action> {
        let socket = self.socket;
        self.handle(Event::WsOpened { socket })
    }
    fn message(&mut self, socket: SocketId, text: &str) -> Vec<Action> {
        self.handle(Event::WsMessage { socket, text })
    }
    fn advance(&mut self, until: f64) -> Vec<Action> {
        let mut actions = self.drain();
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(id, _)| *id != timer);
            self.now = at;
            self.fired += 1;
            self.engine.handle(at, Event::TimerFired { timer });
            actions.extend(self.drain());
        }
        self.now = until;
        actions
    }
    fn close_and_retry_delay(&mut self) -> f64 {
        let socket = self.socket;
        let actions = self.handle(Event::WsClosed { socket, code: 1006 });
        let delays: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms - self.now),
                _ => None,
            })
            .collect();
        assert_eq!(delays.len(), 1, "one retry timer: {delays:?}");
        delays[0]
    }
    fn reconnect(&mut self, delay: f64) {
        let old = self.socket;
        let actions = self.advance(self.now + delay);
        self.socket = connected(&actions).expect("one reconnect per close");
        assert_ne!(self.socket, old);
    }
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    let mut rig = Rig::new();
    for attempt in 0..7 {
        rig.open();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = rig.close_and_retry_delay();
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        rig.reconnect(delay);
    }
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut rig = Rig::new();
    rig.open();
    rig.handle(Event::Command(Command::SubscribeOrderBook {
        symbol: "ETH".into(),
    }));
    rig.handle(Event::Command(Command::SubscribeCandles {
        symbol: "ETH".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    let old = rig.socket;
    let delay = rig.close_and_retry_delay();
    rig.reconnect(delay);
    let socket = rig.socket;
    let subscribed = sent(&rig.open(), socket);
    for topic in [
        r#"{"type":"subscribe","channel":"market_stats/all"}"#,
        r#"{"type":"subscribe","channel":"ticker/1"}"#,
        r#"{"type":"subscribe","channel":"ticker/2"}"#,
        r#"{"type":"subscribe","channel":"order_book/1"}"#,
        r#"{"type":"subscribe","channel":"candle/1/1m"}"#,
        r#"{"type":"ping"}"#,
    ] {
        assert!(
            subscribed.iter().any(|t| t == topic),
            "{topic}: {subscribed:?}"
        );
    }
    assert!(sent(&rig.message(old, r#"{"type":"ping"}"#), old).is_empty());
    rig.message(
        old,
        r#"{"channel":"ticker:1","ticker":{"mark_price":"999"}}"#,
    );
    assert_eq!(rig.engine.store().ticker("ETH").unwrap().mark, 0.0);
    rig.message(
        socket,
        r#"{"channel":"ticker:1","ticker":{"mark_price":"999"}}"#,
    );
    assert_eq!(rig.engine.store().ticker("ETH").unwrap().mark, 999.0);
}

#[test]
fn open_socket_pings_every_ten_seconds_and_silence_retires_it_at_two_minutes() {
    let mut rig = Rig::new();
    let socket = rig.socket;
    rig.open();
    let pings = sent(&rig.advance(NOW + 50_000.0), socket)
        .iter()
        .filter(|t| *t == r#"{"type":"ping"}"#)
        .count();
    assert_eq!(pings, 5);
    assert!(!closed(&rig.advance(NOW + 119_999.0), socket));
    assert!(closed(&rig.advance(NOW + 120_000.0), socket));
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_two_minutes() {
    let mut rig = Rig::new();
    let socket = rig.socket;
    rig.open();
    for step in 1..=30 {
        assert!(!closed(
            &rig.advance(NOW + f64::from(step) * 10_000.0),
            socket
        ));
        rig.message(socket, r#"{"type":"pong"}"#);
    }
    let delay = rig.close_and_retry_delay();
    rig.reconnect(delay);
    let pending = rig.socket;
    let connecting = rig.now;
    assert!(!closed(&rig.advance(connecting + 119_999.0), pending));
    assert!(closed(&rig.advance(connecting + 120_000.0), pending));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut rig = Rig::new();
    for _ in 0..3 {
        rig.open();
        let delay = rig.close_and_retry_delay();
        rig.reconnect(delay);
    }
    let socket = rig.socket;
    rig.open();
    let opened = rig.now;
    for step in 1..=7 {
        rig.advance(opened + f64::from(step) * 5_000.0);
        rig.message(socket, r#"{"type":"pong"}"#);
    }
    assert!((750.0..=1_000.0).contains(&rig.close_and_retry_delay()));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    for policy in [PublicFeedPolicy::Shared, PublicFeedPolicy::Disabled] {
        let (mut rig, actions) = Rig::with_policy(policy);
        assert_eq!(connected(&actions), None);
        rig.handle(Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        }));
        let polls = rig.timers.len();
        assert_eq!(polls, 1, "only the public poll is scheduled");
        let previous = rig.fired;
        let actions = rig.advance(NOW + 600_000.0);
        let fired = rig.fired - previous;
        assert_eq!(fired, 300, "{policy:?}: only the 2s poll fires");
        assert!(connected(&actions).is_none());
        assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    }
}
