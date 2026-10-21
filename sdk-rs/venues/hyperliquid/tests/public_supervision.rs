mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const MIDS: &str = r#"{"channel":"allMids","data":{"mids":{"BTC":"123"}}}"#;
const PING: &str = r#"{"method":"ping"}"#;

struct Harness {
    e: Engine<Hyperliquid>,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    connects: Vec<(SocketId, Option<SocketChannel>)>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
}

impl Harness {
    fn public(policy: PublicFeedPolicy) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
            public_data_only: true,
            ..Options::default()
        }));
        e.set_public_feed_policy(policy).unwrap();
        e.start(1000.0);
        Self::wrap(e, 1000.0)
    }
    fn wrap(e: Engine<Hyperliquid>, now: f64) -> Self {
        let mut h = Self {
            e,
            now,
            timers: Vec::new(),
            connects: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
        };
        h.collect();
        h
    }
    fn collect(&mut self) {
        loop {
            let actions: Vec<_> = self.e.drain_actions().collect();
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::Http(r) => {
                        let body = match support::kind(&r).as_str() {
                            "metaAndAssetCtxs" => support::META,
                            "perpDexs" => "[null]",
                            _ => continue,
                        };
                        support::reply(&mut self.e, &r, 200, body);
                    }
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                    Action::WsConnect {
                        socket, channel, ..
                    } => self.connects.push((socket, channel)),
                    Action::WsSend { socket, text } => self.sent.push((socket, text)),
                    Action::WsClose { socket } => self.closed.push(socket),
                    _ => {}
                }
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.e.handle(self.now, event);
        self.collect();
    }
    fn socket(&self) -> SocketId {
        self.connects.last().unwrap().0
    }
    fn open(&mut self) {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
    }
    fn close(&mut self) -> f64 {
        let socket = self.socket();
        let before = self.timers.clone();
        self.event(Event::WsClosed { socket, code: 1006 });
        self.retry_delay(&before)
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
        }
        self.now = until;
    }
    fn retry_delay(&self, before: &[(TimerId, f64)]) -> f64 {
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
fn flapping_socket_backs_off_although_each_attempt_opens() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    for attempt in 0..7 {
        h.open();
        let delay = h.close();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let sockets = h.connects.len();
        let until = h.now + delay;
        h.advance(until);
        assert_eq!(h.connects.len(), sockets + 1, "one reconnect per close");
    }
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    h.open();
    let old = h.socket();
    for command in [
        Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        },
        Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        },
    ] {
        h.event(Event::Command(command));
    }
    let delay = h.close();
    let until = h.now + delay;
    h.advance(until);
    let socket = h.socket();
    assert_ne!(socket, old);
    assert_eq!(h.connects.last().unwrap().1, Some(SocketChannel::Public));
    h.sent.clear();
    h.open();
    for kind in ["allMids", "fastAssetCtxs", "l2Book", "bbo", "candle"] {
        assert!(
            h.sent
                .iter()
                .any(|(s, t)| *s == socket && t.contains(&format!(r#""type":"{kind}""#))),
            "missing {kind} resubscribe"
        );
    }
    h.message(old, MIDS);
    assert_eq!(h.e.store().ticker("BTC-PERP").unwrap().last, 100.0);
    h.message(socket, MIDS);
    assert_eq!(h.e.store().ticker("BTC-PERP").unwrap().last, 123.0);
}

#[test]
fn open_socket_pings_every_fifteen_seconds_and_silence_retires_it_at_forty_five() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    h.open();
    let socket = h.socket();
    let opened = h.now;
    h.advance(opened + 44_999.0);
    assert_eq!(h.pings(socket), 2);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 45_000.0);
    assert!(h.closed.contains(&socket));
    let until = h.now + 1_000.0;
    h.advance(until);
    assert_ne!(h.socket(), socket, "idle timeout reconnects");
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_ten_seconds() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    h.open();
    let socket = h.socket();
    let start = h.now;
    for step in 1..=8 {
        h.advance(start + f64::from(step) * 15_000.0);
        h.message(socket, MIDS);
    }
    assert!(!h.closed.contains(&socket));
    assert_eq!(h.pings(socket), 8);

    let delay = h.close();
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
fn stable_traffic_resets_backoff() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    for _ in 0..3 {
        h.open();
        let delay = h.close();
        let until = h.now + delay;
        h.advance(until);
    }
    h.open();
    let socket = h.socket();
    let opened = h.now;
    for step in 1..=3 {
        h.advance(opened + f64::from(step) * 12_000.0);
        h.message(socket, MIDS);
    }
    assert!((750.0..=1_000.0).contains(&h.close()));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::public(PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTC-PERP".into(),
    }));
    let until = h.now + 600_000.0;
    h.advance(until);
    assert!(h.connects.is_empty(), "no connects: {:?}", h.connects);
    assert!(h.closed.is_empty(), "no closes: {:?}", h.closed);
    assert!(h.timers.is_empty(), "no timers: {:?}", h.timers);
}

#[test]
fn non_direct_account_socket_stays_supervised() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let (e, socket, _) = support::ready_with_policy("disabled", policy);
        let mut h = Harness::wrap(e, 2_000.0);
        h.connects.push((socket, Some(SocketChannel::Private)));
        let delay = h.close();
        assert!((750.0..=1_000.0).contains(&delay), "{delay}");
        let until = h.now + delay;
        h.advance(until);
        let socket = h.socket();
        assert_eq!(h.connects.last().unwrap().1, Some(SocketChannel::Private));
        h.open();
        let opened = h.now;
        h.advance(opened + 44_999.0);
        assert_eq!(h.pings(socket), 2);
        assert!(!h.closed.contains(&socket));
        h.advance(opened + 45_000.0);
        assert!(h.closed.contains(&socket));
    }
}
