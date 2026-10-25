//! Private aop_p socket liveness, snapshot deadline and reconnect backoff.
mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketChannel, SocketId};
use safe_cex_engine::{RequestId, TimerId};
use safe_cex_venue_phemex::PhemexPerpetuals;
use std::collections::HashMap;
use support::{Options, PreparedEngine as Engine};

const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
const AUTHED: &str = r#"{"id":999,"error":null,"result":{"status":"success"}}"#;
const REJECTED: &str = r#"{"id":999,"error":{"code":6001,"message":"invalid"},"result":null}"#;
const PONG: &str = r#"{"id":1201,"error":null,"result":"pong"}"#;
const ORDER: &str = r#"{"orderID":"one","symbol":"BTCUSDT","side":"Buy","posSide":"Merged","ordType":"Limit","ordStatus":"New","orderQty":"0.3","cumQty":"0","leavesQty":"0.3","priceRp":"69000","timeInForce":"PostOnly","transactTimeNs":1700000000000000001}"#;

fn snapshot(sequence: u64) -> String {
    format!(r#"{{"type":"snapshot","sequence":{sequence},"orders_p":[{ORDER}]}}"#)
}

struct Harness {
    e: Engine<PhemexPerpetuals>,
    now: f64,
    timers: HashMap<TimerId, f64>,
    http: Vec<HttpRequest>,
    private: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            e: support::prepared_engine(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            }),
            now: NOW,
            timers: HashMap::new(),
            http: vec![],
            private: vec![],
            sent: vec![],
            closed: vec![],
        };
        h.e.start(NOW);
        h.collect();
        h.respond("products", PRODUCTS);
        h.respond("24hr/all", DAILY);
        let usdt = r#"{"code":0,"data":{"account":{"currency":"USDT","accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":[]}}"#;
        h.respond("currency=USDT", usdt);
        h.respond(
            "currency=USDC",
            &usdt.replace("\"USDT\"", "\"USDC\"").replace("1000", "200"),
        );
        assert_eq!(h.private.len(), 1);
        h
    }
    fn collect(&mut self) {
        for action in self.e.drain_actions() {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(timer, at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(&timer);
                }
                Action::Http(request) => self.http.push(request),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => self.private.push(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } if self.private.contains(&socket) => {
                    self.closed.push(socket)
                }
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.e.handle(self.now, event);
        self.collect();
    }
    fn respond(&mut self, query: &str, body: &str) {
        let id: RequestId = self
            .http
            .iter()
            .rev()
            .find(|r| r.url.contains(query))
            .unwrap()
            .id;
        self.event(Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn socket(&self) -> SocketId {
        *self.private.last().unwrap()
    }
    fn open(&mut self) {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
    }
    fn message(&mut self, text: &str) {
        let socket = self.socket();
        self.event(Event::WsMessage { socket, text });
    }
    fn host_close(&mut self) {
        let socket = self.socket();
        self.event(Event::WsClosed { socket, code: 1006 });
    }
    fn advance(&mut self, until: f64) {
        while let Some((&timer, &at)) = self
            .timers
            .iter()
            .filter(|(_, at)| **at <= until)
            .min_by(|a, b| a.1.total_cmp(b.1))
        {
            self.now = at;
            self.timers.remove(&timer);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }
    /// Advances until a fresh private socket connects; returns the waited delay.
    fn reconnect(&mut self) -> f64 {
        let count = self.private.len();
        let start = self.now;
        while self.private.len() == count {
            let at = self
                .timers
                .values()
                .copied()
                .min_by(f64::total_cmp)
                .unwrap();
            assert!(at - start <= 16_000., "reconnect exceeded the backoff cap");
            self.advance(at);
        }
        self.now - start
    }
    fn sent_to(&self, socket: SocketId, needle: &str) -> usize {
        self.sent
            .iter()
            .filter(|(s, text)| *s == socket && text.contains(needle))
            .count()
    }
}

fn assert_backoff(delay: f64, nominal: f64) {
    assert!(
        (nominal * 0.75..=nominal).contains(&delay),
        "delay {delay} outside jittered {nominal}"
    );
}

#[test]
fn flapping_authenticated_socket_backs_off_exponentially_to_cap() {
    let mut h = Harness::new();
    for nominal in [1_000., 2_000., 4_000., 8_000., 16_000., 16_000.] {
        h.open();
        h.message(AUTHED);
        h.message(&snapshot(1));
        h.message(PONG);
        h.host_close();
        assert_backoff(h.reconnect(), nominal);
    }
}

#[test]
fn rejected_auth_closes_and_backs_off() {
    let mut h = Harness::new();
    for nominal in [1_000., 2_000., 4_000.] {
        let socket = h.socket();
        h.open();
        h.message(REJECTED);
        assert_eq!(h.closed.last(), Some(&socket));
        assert_backoff(h.reconnect(), nominal);
    }
}

#[test]
fn reconnect_reauthenticates_resubscribes_and_ignores_retired_frames() {
    let mut h = Harness::new();
    let old = h.socket();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    assert_eq!(h.sent_to(old, "aop_p.subscribe"), 1);
    h.host_close();
    h.reconnect();
    let fresh = h.socket();
    assert_ne!(fresh, old);
    h.open();
    assert_eq!(h.sent_to(fresh, "user.auth"), 1);
    h.message(AUTHED);
    assert_eq!(h.sent_to(fresh, "aop_p.subscribe"), 1);

    let opened = h.now;
    for at in [4_000., 8_000., 12_000.] {
        h.advance(opened + at);
        h.event(Event::WsMessage {
            socket: old,
            text: PONG,
        });
        h.event(Event::WsOpened { socket: old });
    }
    assert_eq!(h.sent_to(old, "user.auth"), 1);
    h.advance(opened + 12_500.);
    assert_eq!(h.closed.last(), Some(&fresh));
}

#[test]
fn pings_every_five_seconds_after_accepted_login() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.advance(NOW + 4_000.);
    assert!(h.sent_to(socket, "server.ping") == 0);
    h.open();
    assert_eq!(h.sent_to(socket, "server.ping"), 0);
    h.message(AUTHED);
    assert_eq!(h.sent_to(socket, "server.ping"), 1);
    h.message(&snapshot(1));
    let opened = h.now;
    for n in 1..=6 {
        h.advance(opened + f64::from(n) * 5_000. - 1.);
        assert_eq!(h.sent_to(socket, "server.ping"), n as usize);
        h.message(PONG);
        h.advance(opened + f64::from(n) * 5_000.);
        assert_eq!(h.sent_to(socket, "server.ping"), n as usize + 1);
    }
    assert!(h.closed.is_empty());
}

#[test]
fn only_pongs_keep_the_private_socket_alive() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    h.advance(NOW + 5_000.);
    h.message(PONG);
    for sequence in 2..12 {
        h.advance(h.now + 1_000.);
        h.message(&snapshot(sequence).replace("snapshot", "incremental"));
    }
    h.advance(NOW + 17_499.);
    assert!(h.closed.is_empty());
    h.advance(NOW + 17_500.);
    assert_eq!(h.closed, [socket]);
}

#[test]
fn never_opened_socket_expires_after_fifteen_seconds() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.advance(NOW + 14_999.);
    assert!(h.closed.is_empty());
    h.advance(NOW + 15_000.);
    assert_eq!(h.closed, [socket]);
    assert_backoff(h.reconnect(), 1_000.);
}

#[test]
fn missing_snapshot_expires_twenty_seconds_after_connect() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.advance(NOW + 4_000.);
    h.open();
    h.message(AUTHED);
    for at in [8_000., 12_000., 16_000.] {
        h.advance(NOW + at);
        h.message(PONG);
    }
    h.advance(NOW + 19_999.);
    assert!(h.closed.is_empty());
    h.advance(NOW + 20_000.);
    assert_eq!(h.closed, [socket]);
}

#[test]
fn snapshot_ends_the_deadline_and_stable_traffic_resets_backoff() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.open();
        h.host_close();
        h.reconnect();
    }
    let closed_before_ready = h.closed.len();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    let ready = h.now;
    for n in 1..=7 {
        h.advance(ready + f64::from(n) * 5_000.);
        h.message(PONG);
    }
    assert_eq!(h.closed.len(), closed_before_ready);
    h.host_close();
    assert_backoff(h.reconnect(), 1_000.);
}
