mod support;
use safe_cex_engine::{Action, Command, Engine, Event, PublicFeedPolicy, SocketId, TimerId};
use safe_cex_venue_txflow::{Options, TxFlow};
use std::collections::HashMap;
use support::*;

const PING: &str = r#"{"method":"ping"}"#;
const BOOK: &str = r#"{"channel":"l2Book","data":{"coin":"1","time":3000,"levels":[[{"px":"80","sz":"1"}],[{"px":"81","sz":"2"}]]}}"#;
const API_TICKER: &str = r#"{"channel":"ticker","data":{"instrumentId":1,"lastPrice":"999","coinVlm":"3","dayNtlVlm":"3000"}}"#;

struct Rig {
    engine: Engine<TxFlow>,
    now: f64,
    timers: HashMap<TimerId, f64>,
    node: SocketId,
    api: SocketId,
    connected_at: f64,
}

fn connected(actions: &[Action], api: bool) -> Option<SocketId> {
    let path = if api { "/api-ws" } else { "/ws" };
    actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, url, .. } if url.ends_with(path) => Some(*socket),
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
        let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
            testnet: true,
            public_data_only: true,
            ..Default::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(1000.0);
        let mut rig = Self {
            engine,
            now: 1000.0,
            timers: HashMap::new(),
            node: SocketId(0),
            api: SocketId(0),
            connected_at: 1000.0,
        };
        let mut actions = rig.drain();
        if connected(&actions, false).is_none() {
            actions.extend(rig.advance(1025.0));
            rig.connected_at = rig.now;
        }
        rig.node = connected(&actions, false).unwrap_or(SocketId(0));
        rig.api = connected(&actions, true).unwrap_or(SocketId(0));
        (rig, actions)
    }
    fn drain(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        while self.engine.has_actions() {
            for action in
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now)
            {
                match action {
                    Action::Http(r) => {
                        let body = if r.body.as_deref().unwrap_or("").contains("perpMeta") {
                            META
                        } else {
                            TICKER
                        };
                        self.engine.handle(
                            self.now,
                            Event::HttpResponse {
                                request: r.id,
                                response: safe_cex_engine::Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                    }
                    Action::SetTimer { timer, at_ms } => {
                        self.timers.insert(timer, at_ms);
                        out.push(action);
                    }
                    Action::ClearTimer { timer } => {
                        self.timers.remove(&timer);
                        out.push(action);
                    }
                    other => out.push(other),
                }
            }
        }
        out
    }
    fn handle(&mut self, event: Event<'_>) -> Vec<Action> {
        self.engine.handle(self.now, event);
        self.drain()
    }
    fn socket(&self, api: bool) -> SocketId {
        if api { self.api } else { self.node }
    }
    fn open(&mut self, api: bool) -> Vec<Action> {
        let socket = self.socket(api);
        let mut actions = self.handle(Event::WsOpened { socket });
        actions.extend(self.advance(self.now + 1_000.0));
        actions
    }
    fn message(&mut self, socket: SocketId, text: &str) -> Vec<Action> {
        self.handle(Event::WsMessage { socket, text })
    }
    fn advance(&mut self, until: f64) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut limit = 10_000;
        while let Some((&timer, &at)) = self
            .timers
            .iter()
            .filter(|(_, at)| **at <= until)
            .min_by(|a, b| a.1.total_cmp(b.1).then(a.0.0.cmp(&b.0.0)))
        {
            limit -= 1;
            assert!(limit > 0, "timers failed to settle");
            self.timers.remove(&timer);
            self.now = self.now.max(at);
            actions.extend(self.handle(Event::TimerFired { timer }));
        }
        self.now = self.now.max(until);
        actions
    }
    fn close_and_retry_delay(&mut self, api: bool) -> f64 {
        let socket = self.socket(api);
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
    fn reconnect(&mut self, api: bool, delay: f64) {
        let old = self.socket(api);
        let actions = self.advance(self.now + delay);
        let socket = connected(&actions, api).expect("one reconnect per close");
        assert_ne!(socket, old);
        if api {
            self.api = socket;
        } else {
            self.node = socket;
        }
    }
}

#[test]
fn flapping_sockets_back_off_although_each_attempt_opens() {
    for api in [false, true] {
        let mut rig = Rig::new();
        for attempt in 0..7 {
            rig.open(api);
            let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
            let delay = rig.close_and_retry_delay(api);
            assert!(
                (nominal * 0.75..=nominal).contains(&delay),
                "api={api} attempt {attempt}: {delay}"
            );
            rig.reconnect(api, delay);
        }
    }
}

#[test]
fn node_reconnect_resubscribes_books_and_ignores_retired_socket() {
    let mut rig = Rig::new();
    rig.open(false);
    rig.handle(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTC-USDC".into(),
    }));
    rig.advance(rig.now + 1_000.0);
    let old = rig.node;
    let delay = rig.close_and_retry_delay(false);
    rig.reconnect(false, delay);
    let socket = rig.node;
    let subscribed = sent(&rig.open(false), socket);
    for topic in [
        r#"{"method":"subscribe","type":"allAssetCtxs","subscription":{"type":"allAssetCtxs"}}"#,
        r#"{"method":"subscribe","type":"l2Book","subscription":{"type":"l2Book","coin":"1"}}"#,
    ] {
        assert!(
            subscribed.iter().any(|t| t == topic),
            "{topic}: {subscribed:?}"
        );
    }
    rig.message(old, BOOK);
    assert!(rig.engine.book("BTC-USDC").is_none());
    rig.message(socket, BOOK);
    assert_eq!(rig.engine.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
}

#[test]
fn api_reconnect_resubscribes_tickers_and_candles_and_ignores_retired_socket() {
    let mut rig = Rig::new();
    rig.open(true);
    rig.handle(Event::Command(Command::SubscribeCandles {
        symbol: "BTC-USDC".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    rig.advance(rig.now + 1_000.0);
    let old = rig.api;
    let delay = rig.close_and_retry_delay(true);
    rig.reconnect(true, delay);
    let socket = rig.api;
    let subscribed = sent(&rig.open(true), socket);
    for topic in [
        r#"{"method":"subscribe","subscription":{"type":"ticker","coin":"BTC","instrumentId":1}}"#,
        r#"{"method":"subscribe","subscription":{"type":"candle","coin":"1","interval":"1m"}}"#,
    ] {
        assert!(
            subscribed.iter().any(|t| t == topic),
            "{topic}: {subscribed:?}"
        );
    }
    rig.message(old, API_TICKER);
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 65000.0);
    rig.message(socket, API_TICKER);
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 999.0);
}

#[test]
fn retired_socket_traffic_does_not_keep_replacement_alive() {
    for api in [false, true] {
        let mut rig = Rig::new();
        rig.open(api);
        let old = rig.socket(api);
        let delay = rig.close_and_retry_delay(api);
        rig.reconnect(api, delay);
        let socket = rig.socket(api);
        rig.handle(Event::WsOpened { socket });
        let opened = rig.now;
        for step in 1..=5 {
            assert!(!closed(
                &rig.advance(opened + f64::from(step) * 5_000.0),
                socket
            ));
            rig.message(old, PING);
        }
        assert!(!closed(&rig.advance(opened + 29_999.0), socket));
        assert!(closed(&rig.advance(opened + 30_000.0), socket), "api={api}");
    }
}

#[test]
fn open_sockets_ping_every_ten_seconds_and_silence_retires_them_at_thirty() {
    for api in [false, true] {
        let mut rig = Rig::new();
        let socket = rig.socket(api);
        rig.handle(Event::WsOpened { socket });
        let opened = rig.now;
        let actions = rig.advance(opened + 29_999.0);
        assert_eq!(
            sent(&actions, socket).iter().filter(|t| *t == PING).count(),
            2,
            "api={api}"
        );
        assert!(!closed(&actions, socket));
        let actions = rig.advance(opened + 30_000.0);
        assert!(closed(&actions, socket), "api={api}");
        assert!(sent(&actions, socket).iter().all(|t| t != PING));
    }
}

#[test]
fn traffic_keeps_sockets_alive_with_steady_pings() {
    for api in [false, true] {
        let mut rig = Rig::new();
        let socket = rig.socket(api);
        rig.handle(Event::WsOpened { socket });
        let opened = rig.now;
        let mut pings = 0;
        for step in 1..=24 {
            let actions = rig.advance(opened + f64::from(step) * 5_000.0);
            assert!(!closed(&actions, socket), "api={api} step {step}");
            pings += sent(&actions, socket).iter().filter(|t| *t == PING).count();
            rig.message(socket, r#"{"channel":"pong"}"#);
        }
        assert_eq!(pings, 12, "api={api}");
    }
}

#[test]
fn never_opened_sockets_expire_after_ten_seconds_and_reconnect() {
    for api in [false, true] {
        let mut rig = Rig::new();
        let socket = rig.socket(api);
        let started = rig.connected_at;
        assert!(!closed(&rig.advance(started + 9_999.0), socket));
        let actions = rig.advance(started + 10_000.0);
        assert!(closed(&actions, socket), "api={api}");
        let actions = rig.advance(started + 11_000.0);
        let replacement = connected(&actions, api).expect("connect deadline retries");
        assert_ne!(replacement, socket);
    }
}

#[test]
fn stable_traffic_resets_backoff() {
    for api in [false, true] {
        let mut rig = Rig::new();
        for _ in 0..3 {
            rig.open(api);
            let delay = rig.close_and_retry_delay(api);
            rig.reconnect(api, delay);
        }
        let socket = rig.socket(api);
        rig.handle(Event::WsOpened { socket });
        let opened = rig.now;
        for step in 1..=7 {
            rig.advance(opened + f64::from(step) * 5_000.0);
            rig.message(socket, r#"{"channel":"pong"}"#);
        }
        let delay = rig.close_and_retry_delay(api);
        assert!((750.0..=1_000.0).contains(&delay), "api={api}: {delay}");
    }
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    for policy in [PublicFeedPolicy::Shared, PublicFeedPolicy::Disabled] {
        let (mut rig, actions) = Rig::with_policy(policy);
        assert_eq!(connected(&actions, false), None);
        assert_eq!(connected(&actions, true), None);
        rig.handle(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }));
        assert!(rig.timers.is_empty(), "{policy:?}: {:?}", rig.timers);
        let actions = rig.advance(rig.now + 600_000.0);
        assert!(actions.is_empty(), "{policy:?}: {actions:?}");
    }
}
