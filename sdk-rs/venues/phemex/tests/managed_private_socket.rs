//! Whole-family adoption controls against identifiable native private production.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketChannel, SocketId};
use safe_cex_engine::{RequestId, TimerId};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};
use std::collections::HashMap;

const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
const AUTHED: &str = r#"{"id":999,"error":null,"result":{"status":"success"}}"#;
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
            e: safe_cex_engine::test_support::seeded_engine(PhemexPerpetuals::new(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            })),
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
    fn sent_to(&self, socket: SocketId, needle: &str) -> usize {
        self.sent
            .iter()
            .filter(|(s, text)| *s == socket && text.contains(needle))
            .count()
    }
}

#[test]
fn accepted_open_preserves_auth_then_ping_and_aop_wire() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, s)| s)
        .collect();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[0],
        r#"{"method":"user.auth","params":["API","key","46a48d7b73f809f511182a4807f8301321bf3efe6f397c40f50b393e649a1419",1700000060],"id":999}"#
    );
    assert_eq!(sent[1], r#"{"id":1201,"method":"server.ping","params":[]}"#);
    h.message(AUTHED);
    assert_eq!(
        h.sent.last().unwrap().1,
        r#"{"id":1202,"method":"aop_p.subscribe","params":[]}"#
    );
}

#[test]
fn accepted_current_snapshot_keeps_native_financial_result() {
    let mut h = Harness::new();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    let order =
        h.e.store()
            .order("one")
            .expect("live native snapshot publishes");
    assert_eq!(order.symbol, "BTCUSDT");
    assert_eq!(order.price, 69000.);
    assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Open);
    assert!(h.e.store().loaded().orders);
}

#[test]
fn duplicate_private_open_never_replays_login_or_ping() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    let previous = h.sent.len();
    h.now = NOW + 1.;
    h.open();
    assert_eq!(
        h.sent.len(),
        previous,
        "duplicate private open cannot release another login/ping"
    );
    assert_eq!(h.sent_to(socket, "user.auth"), 1);
}

#[test]
fn expired_private_connect_cannot_release_login() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.now = NOW + 15_000.;
    h.open();
    assert_eq!(
        h.sent_to(socket, "user.auth"),
        0,
        "late open cannot revive expired private connect"
    );
    assert!(h.closed.contains(&socket));
}

#[test]
fn duplicate_auth_ack_never_replays_aop_subscription() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    h.message(AUTHED);
    h.now = NOW + 1.;
    h.message(AUTHED);
    assert_eq!(
        h.sent_to(socket, "aop_p.subscribe"),
        1,
        "one native AOP subscription per accepted epoch"
    );
}

#[test]
fn snapshot_at_connect_deadline_cannot_publish_without_watchdog_delivery() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.now = NOW + 4_000.;
    h.open();
    h.message(AUTHED);
    h.now = NOW + 19_999.;
    h.message(PONG);
    h.now = NOW + 20_000.;
    h.message(&snapshot(1));
    assert!(
        h.e.store().order("one").is_none(),
        "connect-origin snapshot barrier expires at equality before native publication"
    );
    assert!(h.closed.contains(&socket));
}

#[test]
fn order_after_idle_expiry_cannot_publish_without_watchdog_delivery() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    h.now = NOW + 12_500.;
    h.message(
        &snapshot(2)
            .replace("snapshot", "incremental")
            .replace("one", "late"),
    );
    assert!(
        h.e.store().order("late").is_none(),
        "expired native financial ingress must not publish"
    );
    assert!(h.closed.contains(&socket));
}

#[test]
fn pong_at_idle_deadline_cannot_revive_expired_private_epoch() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1));
    h.now = NOW + 12_500.;
    h.message(PONG);
    assert!(
        h.closed.contains(&socket),
        "late pong must retire an expired private epoch before liveness refresh"
    );
}

#[test]
fn auth_ack_and_pongs_preserve_native_connect_origin_snapshot_deadline() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.advance(NOW + 4_000.);
    h.open();
    h.message(AUTHED);
    for delta in [8_000., 12_000., 16_000., 19_999.] {
        h.advance(NOW + delta);
        h.message(PONG);
    }
    assert!(h.closed.is_empty());
    h.advance(NOW + 20_000.);
    assert_eq!(h.closed, [socket]);
}

#[test]
fn malformed_snapshot_cannot_discharge_native_snapshot_deadline() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.advance(NOW + 4_000.);
    h.open();
    h.message(AUTHED);
    h.message(r#"{"type":"snapshot","sequence":1,"orders_p":{}}"#);
    assert!(!h.e.store().loaded().orders);
    for delta in [8_000., 12_000., 16_000., 19_999.] {
        h.advance(NOW + delta);
        h.message(PONG);
    }
    h.advance(NOW + 20_000.);
    assert_eq!(h.closed, [socket]);
}

#[test]
fn positions_or_accounts_only_snapshot_does_not_complete_orders_snapshot_barrier() {
    for body in [
        r#"{"type":"snapshot","sequence":1,"positions_p":[]}"#,
        r#"{"type":"snapshot","sequence":1,"accounts_p":[]}"#,
    ] {
        let mut h = Harness::new();
        let socket = h.socket();
        h.advance(NOW + 4_000.);
        h.open();
        h.message(AUTHED);
        h.message(body);
        for delta in [8_000., 12_000., 16_000., 19_999.] {
            h.advance(NOW + delta);
            h.message(PONG);
        }
        assert!(h.closed.is_empty());
        h.advance(NOW + 20_000.);
        assert_eq!(
            h.closed,
            [socket],
            "non-orders snapshot cannot discharge the native barrier: {body}"
        );
    }
}

#[test]
fn accepted_revision_conflicted_orders_snapshot_preserves_newer_order_and_completes_barrier() {
    let mut h = Harness::new();
    h.advance(NOW + 4_000.);
    h.open();
    h.message(AUTHED);
    h.message(&snapshot(1).replace("snapshot", "incremental"));
    assert!(h.e.store().order("one").is_some());
    h.now = NOW + 4_001.;
    h.message(r#"{"type":"snapshot","sequence":2,"orders_p":[]}"#);
    assert!(
        h.e.store().order("one").is_some(),
        "accepted conflicting snapshot preserves the newer native order"
    );
    for delta in [8_000., 12_000., 16_000., 20_000., 24_000.] {
        h.advance(NOW + delta);
        h.message(PONG);
    }
    assert!(
        h.closed.is_empty(),
        "native accepted orders snapshot ends the bound despite revision conflict"
    );
    assert_eq!(h.e.store().order("one").unwrap().price, 69000.);
}

#[test]
fn account_ping_and_aop_require_accepted_login() {
    let mut h = Harness::new();
    let socket = h.socket();
    h.open();
    assert_eq!(h.sent_to(socket, "user.auth"), 1);
    assert_eq!(
        h.sent_to(socket, "server.ping"),
        0,
        "opening cannot release account ping before accepted login"
    );
    assert_eq!(h.sent_to(socket, "aop_p.subscribe"), 0);
    h.message(AUTHED);
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent[1],
        r#"{"id":1202,"method":"aop_p.subscribe","params":[]}"#
    );
    assert_eq!(sent[2], r#"{"id":1201,"method":"server.ping","params":[]}"#);
}
