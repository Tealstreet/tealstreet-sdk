mod support;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId};
use safe_cex_venue_lighter::Lighter;
use support::*;

const PAYLOAD: &str = r#"{"type":"update/account_all","channel":"account_all:718895","data":{"orders":[],"positions":[],"fills":[]}}"#;

struct Rig {
    engine: Engine<Lighter>,
    timers: Timers,
    socket: SocketId,
    now: f64,
}

fn connected(actions: &[Action]) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Private),
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

fn subscriptions(texts: &[String]) -> usize {
    texts
        .iter()
        .filter(|t| t.contains(r#""type":"subscribe""#))
        .count()
}

impl Rig {
    fn new() -> Self {
        let (engine, socket, timers) = ready_tracked(options());
        Self {
            engine,
            timers,
            socket,
            now: NOW,
        }
    }
    fn handle(&mut self, event: Event<'_>) -> Vec<Action> {
        self.engine.handle(self.now, event);
        drain(&mut self.engine, &mut self.timers)
    }
    fn open(&mut self) -> Vec<Action> {
        let socket = self.socket;
        self.handle(Event::WsOpened { socket })
    }
    fn message(&mut self, socket: SocketId, text: &str) -> Vec<Action> {
        self.handle(Event::WsMessage { socket, text })
    }
    fn advance(&mut self, until: f64) -> Vec<Action> {
        let actions = advance(&mut self.engine, &mut self.timers, until);
        self.now = until;
        actions
    }
    fn close_and_retry_delay(&mut self, code: u16) -> f64 {
        let socket = self.socket;
        let actions = self.handle(Event::WsClosed { socket, code });
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
fn flapping_private_socket_backs_off_although_each_attempt_opens() {
    let mut rig = Rig::new();
    for attempt in 0..7 {
        rig.open();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = rig.close_and_retry_delay(1006);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        rig.reconnect(delay);
    }
}

// Managed sockets do not expose close codes, so 1013 no longer floors the
// retry at 30 s; it takes the ordinary jittered backoff like any other close.
#[test]
fn try_again_later_close_takes_the_managed_backoff() {
    let mut rig = Rig::new();
    let delay = rig.close_and_retry_delay(1013);
    assert!((750.0..=1_000.0).contains(&delay), "{delay}");
    let old = rig.socket;
    rig.reconnect(delay);
    assert_ne!(rig.socket, old);
}

#[test]
fn reconnect_resubscribes_account_streams_and_ignores_retired_socket() {
    let mut rig = Rig::new();
    let old = rig.socket;
    let delay = rig.close_and_retry_delay(1006);
    rig.reconnect(delay);
    let socket = rig.socket;
    let opened = sent(&rig.open(), socket);
    assert_eq!(opened.len(), 3, "{opened:?}");
    assert_eq!(
        opened[0],
        r#"{"type":"subscribe","channel":"account_all/718895"}"#
    );
    assert!(
        opened[1]
            .starts_with(r#"{"type":"subscribe","channel":"account_all_orders/718895","auth":""#)
    );
    assert_eq!(
        opened[2],
        format!(r#"{{"type":"ping","timestamp":{}}}"#, rig.now)
    );
    assert!(sent(&rig.message(old, r#"{"type":"ping"}"#), old).is_empty());
    assert_eq!(
        sent(&rig.message(socket, r#"{"type":"ping"}"#), socket),
        [format!(r#"{{"type":"pong","timestamp":{}}}"#, rig.now)]
    );
}

#[test]
fn open_socket_pings_with_timestamps_every_ten_seconds_and_silence_retires_it_at_three_minutes() {
    let mut rig = Rig::new();
    let socket = rig.socket;
    let pings = sent(&rig.advance(NOW + 170_000.0), socket);
    let expected: Vec<_> = (1..=17)
        .map(|n| {
            format!(
                r#"{{"type":"ping","timestamp":{}}}"#,
                NOW + f64::from(n) * 10_000.0
            )
        })
        .collect();
    assert_eq!(pings, expected);
    assert!(!closed(&rig.advance(NOW + 179_999.0), socket));
    let actions = rig.advance(NOW + 180_000.0);
    assert!(closed(&actions, socket));
    assert_eq!(subscriptions(&sent(&actions, socket)), 0);
}

#[test]
fn never_opened_private_socket_expires_at_three_minutes() {
    let mut rig = Rig::new();
    let delay = rig.close_and_retry_delay(1006);
    rig.reconnect(delay);
    let pending = rig.socket;
    let connecting = rig.now;
    assert!(!closed(&rig.advance(connecting + 179_999.0), pending));
    assert!(closed(&rig.advance(connecting + 180_000.0), pending));
}

#[test]
fn heartbeat_only_socket_resubscribes_on_the_ping_cadence_then_closes_twenty_seconds_later() {
    let mut rig = Rig::new();
    let socket = rig.socket;
    let mut resubscribed_at = None;
    for step in 1..=20 {
        let now = NOW + f64::from(step) * 10_000.0;
        rig.advance(now - 1.0);
        rig.message(socket, r#"{"type":"pong"}"#);
        let actions = rig.advance(now);
        if subscriptions(&sent(&actions, socket)) > 0 {
            assert_eq!(subscriptions(&sent(&actions, socket)), 2);
            assert!(resubscribed_at.is_none());
            resubscribed_at = Some(now);
        }
        assert_eq!(closed(&actions, socket), step == 20, "step {step}");
    }
    assert_eq!(resubscribed_at, Some(NOW + 180_000.0));
}

#[test]
fn stable_private_traffic_resets_backoff() {
    let mut rig = Rig::new();
    for _ in 0..3 {
        rig.open();
        let delay = rig.close_and_retry_delay(1006);
        rig.reconnect(delay);
    }
    let socket = rig.socket;
    rig.open();
    let opened = rig.now;
    for step in 1..=7 {
        rig.advance(opened + f64::from(step) * 5_000.0);
        rig.message(socket, PAYLOAD);
    }
    assert!((750.0..=1_000.0).contains(&rig.close_and_retry_delay(1006)));
}
