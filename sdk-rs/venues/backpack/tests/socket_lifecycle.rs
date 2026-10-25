mod support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{
    Action, Event, HttpRequest, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_backpack::Options;

const SYMBOL: &str = "SOL_USDC_PERP";
const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#;
const TICKERS: &str = r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#;
const TICKER: &str = r#"{"stream":"ticker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","o":"100","c":"105","v":"1","V":"105"}}"#;
const ORDER: &str = r#"{"stream":"account.orderUpdate","data":{"e":"orderAccepted","i":"unexpected","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}}"#;
const PING: &str = r#"{"method":"PING"}"#;

struct Harness {
    engine: support::Driver,
    now: f64,
    connects: Vec<(SocketChannel, SocketId)>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    http: Vec<HttpRequest>,
    closed: Vec<SocketId>,
    notices: usize,
    timer_writes: usize,
}

impl Harness {
    fn new(policy: PublicFeedPolicy) -> Self {
        let mut engine = support::configured_account(Options {
            key: Some("test-primary".into()),
            secret: Some(STANDARD.encode([7; 32])),
            ..Default::default()
        });
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(1000.);
        let mut h = Self {
            engine,
            now: 1000.,
            connects: vec![],
            timers: vec![],
            sent: vec![],
            http: vec![],
            closed: vec![],
            notices: 0,
            timer_writes: 0,
        };
        h.collect();
        for _ in 0..16 {
            if h.http.is_empty() {
                break;
            }
            for r in std::mem::take(&mut h.http) {
                let body = if r.url.ends_with("/markets") {
                    MARKETS
                } else if r.url.ends_with("/tickers") {
                    TICKERS
                } else if r.url.ends_with("/account") {
                    r#"{"leverageLimit":"10"}"#
                } else if r.url.ends_with("/capital/collateral") {
                    support::BALANCE
                } else {
                    assert!(
                        ["/markPrices", "/position", "/orders"]
                            .iter()
                            .any(|suffix| r.url.ends_with(suffix)),
                        "unexpected startup request {}",
                        r.url
                    );
                    "[]"
                };
                h.event(Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                });
            }
        }
        assert!(h.http.is_empty(), "startup converged");
        assert!(h.engine.store().loaded().orders);
        assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 141.);
        h.clear_observations();
        h
    }

    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::WsConnect {
                    socket,
                    channel: Some(channel),
                    ..
                } => self.connects.push((channel, socket)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::Http(request) => self.http.push(request),
                Action::SetTimer { timer, at_ms } => {
                    self.timer_writes += 1;
                    self.timers.push((timer, at_ms));
                }
                Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                Action::Notice(_) => self.notices += 1,
                _ => {}
            }
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn socket(&self, channel: SocketChannel) -> SocketId {
        self.connects
            .iter()
            .rev()
            .find(|(c, _)| *c == channel)
            .expect("genuine physical socket")
            .1
    }

    fn open(&mut self, channel: SocketChannel) -> SocketId {
        let socket = self.socket(channel);
        self.event(Event::WsOpened { socket });
        socket
    }

    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }

    fn clear_observations(&mut self) {
        self.sent.clear();
        self.http.clear();
        self.closed.clear();
        self.notices = 0;
        self.timer_writes = 0;
    }

    fn advance(&mut self, until: f64) {
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(id, _)| *id != timer);
            self.now = at.max(self.now);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }
}

fn expected_auth(stamp: u32) -> String {
    let payload = format!("instruction=subscribe&timestamp={stamp}&window=5000");
    let signature =
        STANDARD.encode(safe_cex_crypto::ed25519::sign(payload.as_bytes(), &[7; 32]).unwrap());
    format!(
        r#"{{"method":"SUBSCRIBE","params":["account.orderUpdate","account.positionUpdate"],"signature":["test-primary","{signature}","{stamp}","5000"]}}"#
    )
}

#[test]
fn native_private_first_open_has_exact_fresh_auth_ping_then_reads_without_ack() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.open(SocketChannel::Private);
    assert_eq!(
        h.sent,
        [(socket, expected_auth(1000)), (socket, PING.into())]
    );
    let paths: Vec<_> = h.http.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(paths.len(), 3);
    assert!(paths[0].ends_with("/orders"));
    assert!(paths[1].ends_with("/position"));
    assert!(paths[2].ends_with("/capital/collateral"));
    h.message(socket, ORDER);
    assert!(h.engine.store().order("unexpected").is_some());
}

#[test]
fn native_public_first_open_preserves_topics_and_ping() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.open(SocketChannel::Public);
    assert_eq!(
        h.sent,
        [
            (socket, r#"{"method":"SUBSCRIBE","params":["ticker.SOL_USDC_PERP","trade.SOL_USDC_PERP","bookTicker.SOL_USDC_PERP"]}"#.into()),
            (socket, PING.into()),
        ]
    );
    h.message(socket, TICKER);
    assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
}

#[test]
fn private_before_and_after_public_open_keep_independent_native_wire() {
    for private_first in [true, false] {
        let mut h = Harness::new(PublicFeedPolicy::Direct);
        if !private_first {
            h.open(SocketChannel::Public);
            h.clear_observations();
        }
        let private = h.open(SocketChannel::Private);
        assert_eq!(
            h.sent,
            [(private, expected_auth(1000)), (private, PING.into())]
        );
        h.clear_observations();
        if private_first {
            h.open(SocketChannel::Public);
        }
        h.message(private, ORDER);
        assert!(h.engine.store().order("unexpected").is_some());
    }
}

#[test]
fn suppressed_public_feed_does_not_block_private_open_or_authentication() {
    let mut h = Harness::new(PublicFeedPolicy::Shared);
    assert!(h.connects.iter().all(|(c, _)| *c == SocketChannel::Private));
    let private = h.open(SocketChannel::Private);
    assert_eq!(
        h.sent,
        [(private, expected_auth(1000)), (private, PING.into())]
    );
    h.message(private, ORDER);
    assert!(h.engine.store().order("unexpected").is_some());
}

#[test]
fn native_reconnect_signs_again_without_ack_and_old_socket_is_inert() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let old = h.open(SocketChannel::Private);
    h.now = 1001.;
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(2000.);
    h.clear_observations();
    let new = h.open(SocketChannel::Private);
    assert_ne!(old, new);
    assert_eq!(h.sent, [(new, expected_auth(2000)), (new, PING.into())]);
    h.clear_observations();
    h.message(old, ORDER);
    assert!(h.engine.store().order("unexpected").is_none());
    h.message(new, ORDER);
    assert!(h.engine.store().order("unexpected").is_some());
}

fn assert_denied_message(channel: SocketChannel, now: f64, opened: bool) {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.socket(channel.clone());
    if opened {
        h.open(channel.clone());
    }
    h.clear_observations();
    h.now = now;
    h.message(
        socket,
        if channel == SocketChannel::Public {
            TICKER
        } else {
            ORDER
        },
    );
    assert_eq!(
        h.engine.store().ticker(SYMBOL).unwrap().last,
        141.,
        "denied public DATA"
    );
    assert!(
        h.engine.store().order("unexpected").is_none(),
        "denied private DATA"
    );
    assert!(h.http.is_empty(), "denied ingress starts no HTTP");
    assert!(h.sent.is_empty(), "denied ingress starts no frames");
    if !now.is_finite() {
        assert_eq!(h.timer_writes, 0, "nonfinite ingress creates no timers");
    }
}

macro_rules! denial {
    ($name:ident, $channel:ident, $now:expr, $opened:expr) => {
        #[test]
        fn $name() {
            assert_denied_message(SocketChannel::$channel, $now, $opened);
        }
    };
}
denial!(public_before_open_cannot_publish, Public, 1001., false);
denial!(private_before_open_cannot_publish, Private, 1001., false);
denial!(
    public_idle_equality_before_timer_cannot_publish,
    Public,
    21000.,
    true
);
denial!(
    private_idle_equality_before_timer_cannot_publish,
    Private,
    21000.,
    true
);
denial!(public_nan_ingress_cannot_publish, Public, f64::NAN, true);
denial!(private_nan_ingress_cannot_publish, Private, f64::NAN, true);
denial!(
    public_positive_infinity_cannot_publish,
    Public,
    f64::INFINITY,
    true
);
denial!(
    private_positive_infinity_cannot_publish,
    Private,
    f64::INFINITY,
    true
);
denial!(
    public_negative_infinity_cannot_publish,
    Public,
    f64::NEG_INFINITY,
    true
);
denial!(
    private_negative_infinity_cannot_publish,
    Private,
    f64::NEG_INFINITY,
    true
);

#[test]
fn duplicate_private_open_does_not_repeat_auth_ping_or_account_refresh() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.open(SocketChannel::Private);
    h.clear_observations();
    h.now = 1001.;
    h.event(Event::WsOpened { socket });
    assert!(
        h.sent.is_empty(),
        "duplicate open does not repeat signed auth/PING"
    );
    assert!(
        h.http.is_empty(),
        "duplicate open does not repeat account observations"
    );
}

#[test]
fn duplicate_public_open_does_not_repeat_subscriptions_or_ping() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.open(SocketChannel::Public);
    h.clear_observations();
    h.now = 1001.;
    h.event(Event::WsOpened { socket });
    assert!(
        h.sent.is_empty(),
        "duplicate open does not replay interests/PING"
    );
}

#[test]
fn unopened_public_malformed_payload_is_inert_before_native_diagnostics() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    h.message(h.socket(SocketChannel::Public), "{");
    assert_eq!(
        h.notices, 0,
        "unopened ingress cannot emit native diagnostics"
    );
    assert!(h.sent.is_empty() && h.http.is_empty());
}

#[test]
fn unopened_private_malformed_payload_is_inert_before_native_diagnostics() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    h.message(h.socket(SocketChannel::Private), "{");
    assert_eq!(
        h.notices, 0,
        "unopened ingress cannot emit native diagnostics"
    );
    assert!(h.sent.is_empty() && h.http.is_empty());
}
