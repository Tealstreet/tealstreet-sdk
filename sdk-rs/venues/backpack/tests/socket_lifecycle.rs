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

fn store_frame(h: &Harness) -> Vec<u8> {
    let store = h.engine.store();
    let mut writer = safe_cex_engine::wire::FrameWriter::new(vec![], 0.);
    store.write(&store.everything(), &mut writer);
    writer.finish()
}

fn latency(h: &Harness) -> f64 {
    let frame = store_frame(h);
    let (_, status) = safe_cex_engine::wire::sections(&frame)
        .find(|(section, _)| *section == safe_cex_engine::wire::section::STATUS)
        .unwrap();
    f64::from_le_bytes(status[8..16].try_into().unwrap())
}

fn full_denied_ingress(h: &mut Harness, socket: SocketId, now: f64, text: &str) {
    let before = store_frame(h);
    let timers: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
    let reads: Vec<_> = h.http.iter().map(|r| r.id).collect();
    h.engine.handle(now, Event::WsMessage { socket, text });
    h.engine.prepare_dispatch(now);
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert_eq!(
        store_frame(h),
        before,
        "complete public/private Store unchanged"
    );
    for action in actions {
        match action {
            Action::WsClose { socket: closed } => assert_eq!(closed, socket),
            Action::ClearTimer { timer } => assert!(timers.contains(&timer)),
            Action::SetTimer { at_ms, .. } => {
                assert!(now.is_finite() && at_ms.is_finite() && at_ms >= now);
            }
            Action::ManagedHttp(action) => match *action {
                safe_cex_engine::request_context::HttpAction::Cancel { preparation, .. } => {
                    assert!(reads.contains(&preparation.attempt().request));
                }
                _ => panic!("denied ingress creates managed physical work"),
            },
            _ => panic!("denied ingress emits work or native diagnostics: {action:?}"),
        }
    }
}

#[test]
fn hostile_ingress_preserves_full_store_and_all_actions_are_exact_cleanup() {
    for channel in [SocketChannel::Public, SocketChannel::Private] {
        for now in [1001., 21000., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut h = Harness::new(PublicFeedPolicy::Direct);
            let socket = h.socket(channel.clone());
            if now != 1001. {
                h.open(channel.clone());
            }
            full_denied_ingress(
                &mut h,
                socket,
                now,
                if channel == SocketChannel::Public {
                    TICKER
                } else {
                    ORDER
                },
            );
        }
    }
}

#[test]
fn retired_socket_payload_and_malformed_text_are_inert_across_sibling_epochs() {
    for channel in [SocketChannel::Public, SocketChannel::Private] {
        let mut h = Harness::new(PublicFeedPolicy::Direct);
        let old = h.open(channel.clone());
        h.now = 1001.;
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        h.advance(2000.);
        let new = h.open(channel.clone());
        assert_ne!(old, new);
        for text in [
            TICKER,
            ORDER,
            "{",
            r#"{"error":{"code":4002,"message":"PONG"}}"#,
        ] {
            full_denied_ingress(&mut h, old, 2001., text);
        }
    }
}

#[test]
fn exact_action_order_is_private_auth_then_ping_then_account_observations() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let socket = h.socket(SocketChannel::Private);
    h.engine.handle(1000., Event::WsOpened { socket });
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, 1000.);
    let mut trace = vec![];
    for action in actions {
        match action {
            Action::WsSend { socket: sent, text } => {
                assert_eq!(sent, socket);
                trace.push(text);
            }
            Action::Http(request) => trace.push(request.url),
            Action::SetTimer { .. } | Action::ClearTimer { .. } => {}
            _ => panic!("unexpected private-open action: {action:?}"),
        }
    }
    assert_eq!(trace.len(), 5);
    assert_eq!(trace[0], expected_auth(1000));
    assert_eq!(trace[1], PING);
    assert!(trace[2].ends_with("/orders"));
    assert!(trace[3].ends_with("/position"));
    assert!(trace[4].ends_with("/capital/collateral"));
}

#[test]
fn closing_one_feed_keeps_the_open_sibling_live() {
    for close_private in [true, false] {
        let mut h = Harness::new(PublicFeedPolicy::Direct);
        let public = h.open(SocketChannel::Public);
        let private = h.open(SocketChannel::Private);
        h.clear_observations();
        let closed = if close_private { private } else { public };
        let sibling = if close_private { public } else { private };
        h.now = 1001.;
        h.event(Event::WsClosed {
            socket: closed,
            code: 1006,
        });
        assert!(!h.closed.contains(&sibling));
        h.advance(2000.);
        assert_eq!(
            h.socket(if close_private {
                SocketChannel::Public
            } else {
                SocketChannel::Private
            }),
            sibling
        );
        if close_private {
            h.message(public, TICKER);
            assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
        } else {
            h.message(private, ORDER);
            assert!(h.engine.store().order("unexpected").is_some());
        }
    }
}

#[test]
fn public_pong_keeps_native_half_rtt_private_pong_does_not_publish_latency() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let public = h.open(SocketChannel::Public);
    let private = h.open(SocketChannel::Private);
    h.now = 1040.;
    let pong = r#"{"error":{"code":4002,"message":"PONG"}}"#;
    h.message(public, pong);
    assert_eq!(latency(&h), 20.);
    let frame = store_frame(&h);
    h.now = 1080.;
    h.message(private, pong);
    assert_eq!(store_frame(&h), frame);
}

#[test]
fn malformed_current_public_text_keeps_any_text_liveness_and_native_diagnostic() {
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let public = h.open(SocketChannel::Public);
    h.now = 20000.;
    let before = store_frame(&h);
    h.message(public, "{");
    assert_eq!(h.notices, 1);
    assert_eq!(store_frame(&h), before);
    h.advance(21000.);
    assert!(
        !h.closed.contains(&public),
        "malformed current text renewed native idle observation"
    );
    h.advance(40000.);
    assert!(
        h.closed.contains(&public),
        "last actual observation expires at equality"
    );
}

fn startup_with_options(options: Options, fail_initial_orders: bool) -> Harness {
    let mut engine = support::configured_account(options);
    engine.start(1000.);
    let mut h = Harness {
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
            let (status, body) = if r.url.ends_with("/markets") {
                (200, MARKETS)
            } else if r.url.ends_with("/tickers") {
                (200, TICKERS)
            } else if r.url.ends_with("/account") {
                (200, r#"{"leverageLimit":"10"}"#)
            } else if r.url.ends_with("/capital/collateral") {
                (200, support::BALANCE)
            } else if fail_initial_orders && r.url.ends_with("/orders") {
                (500, "orders unavailable")
            } else {
                assert!(
                    ["/markPrices", "/position", "/orders"]
                        .iter()
                        .any(|s| r.url.ends_with(s)),
                    "unexpected startup URL {}",
                    r.url
                );
                (200, "[]")
            };
            h.event(Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            });
        }
    }
    assert!(h.http.is_empty());
    assert!(h.engine.store().loaded().markets && h.engine.store().loaded().tickers);
    h.clear_observations();
    h
}

#[test]
fn unloaded_orders_first_private_open_authenticates_without_account_refresh() {
    let mut h = startup_with_options(
        Options {
            key: Some("test-primary".into()),
            secret: Some(STANDARD.encode([7; 32])),
            ..Default::default()
        },
        true,
    );
    assert!(!h.engine.store().loaded().orders);
    let private = h.open(SocketChannel::Private);
    assert_eq!(
        h.sent,
        [(private, expected_auth(1000)), (private, PING.into())]
    );
    assert!(
        h.http.is_empty(),
        "unloaded native Orders guard prevents the three opening reads"
    );
    h.message(private, ORDER);
    assert!(h.engine.store().order("unexpected").is_some());
}

#[test]
fn public_only_with_installed_primary_and_saved_read_identity_never_starts_private_socket() {
    for secondary in [false, true] {
        let mut h = startup_with_options(
            Options {
                key: Some("test-primary".into()),
                secret: Some(STANDARD.encode([7; 32])),
                public_data_only: true,
                read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                    key: "test-read".into(),
                    secret: STANDARD.encode([9; 32]),
                    passphrase: None,
                }),
                ..Default::default()
            },
            false,
        );
        assert!(
            h.connects
                .iter()
                .all(|(channel, _)| *channel == SocketChannel::Public)
        );
        assert!(h.engine.store().loaded().orders);
        assert!(h.engine.store().orders().is_empty());
        assert!(h.engine.store().positions().is_empty());
        assert_eq!(h.engine.store().balance().total, 0.);
        let public = h.open(SocketChannel::Public);
        assert_eq!(h.sent.len(), 2);
        h.advance(11000.);
        assert!(
            h.connects
                .iter()
                .all(|(channel, _)| *channel == SocketChannel::Public)
        );
        assert!(h.http.is_empty(), "no private startup or account poll");
        h.message(public, TICKER);
        assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
    }
}

fn command(h: &mut Harness, command: safe_cex_engine::Command) {
    h.event(Event::Command(command));
}

fn current_wakes(h: &Harness) -> Vec<TimerId> {
    h.timers
        .iter()
        .filter(|(_, at)| *at == h.now)
        .map(|(timer, _)| *timer)
        .collect()
}

fn fire_timer(h: &mut Harness, timer: TimerId) {
    h.timers.retain(|(id, _)| *id != timer);
    h.event(Event::TimerFired { timer });
}

fn wake_current_interests(h: &mut Harness) -> Vec<TimerId> {
    let timers = current_wakes(h);
    for timer in &timers {
        fire_timer(h, *timer);
    }
    timers
}

fn accept_depth_reads(h: &mut Harness, urls: &mut Vec<String>) {
    for request in std::mem::take(&mut h.http) {
        assert!(request.url.contains("/depth?"));
        urls.push(request.url);
        h.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"lastUpdateId":1,"bids":[],"asks":[]}"#,
            },
        });
    }
}

#[test]
fn two_book_handoff_keeps_survivor_when_sibling_is_removed_before_wake() {
    use safe_cex_engine::Command;
    for reconnect in [false, true] {
        let mut h = Harness::new(PublicFeedPolicy::Direct);
        for symbol in [SYMBOL, "B"] {
            command(
                &mut h,
                Command::SubscribeOrderBook {
                    symbol: symbol.into(),
                },
            );
        }
        let old = h.open(SocketChannel::Public);
        if reconnect {
            assert_eq!(wake_current_interests(&mut h).len(), 2);
            assert_eq!(h.http.len(), 2);
            accept_depth_reads(&mut h, &mut vec![]);
            h.now = 1001.;
            h.event(Event::WsClosed {
                socket: old,
                code: 1006,
            });
            h.advance(2000.);
            assert_ne!(h.open(SocketChannel::Public), old);
        }
        assert!(
            h.http.is_empty(),
            "snapshot waits for genuine post-handoff ingress"
        );
        let old_wakes = current_wakes(&h);
        assert_eq!(old_wakes.len(), 2);
        command(
            &mut h,
            Command::UnsubscribeOrderBook {
                symbol: SYMBOL.into(),
            },
        );
        let current = current_wakes(&h);
        assert_eq!(current.len(), 1);
        assert!(
            current.iter().all(|timer| !old_wakes.contains(timer)),
            "successful replacement rebased surviving work"
        );
        for timer in &old_wakes {
            fire_timer(&mut h, *timer);
        }
        assert!(h.http.is_empty(), "all old wakes are inert");
        fire_timer(&mut h, current[0]);
        assert_eq!(h.http.len(), 1);
        assert!(h.http[0].url.ends_with("symbol=B"));
        accept_depth_reads(&mut h, &mut vec![]);
        h.clear_observations();
        command(
            &mut h,
            Command::SubscribeOrderBook {
                symbol: SYMBOL.into(),
            },
        );
        assert!(h.http.is_empty());
        assert_eq!(wake_current_interests(&mut h).len(), 1);
        assert_eq!(h.http.len(), 1);
        assert!(h.http[0].url.ends_with(&format!("symbol={SYMBOL}")));
    }
}

fn pending_books(count: usize) -> Harness {
    use safe_cex_engine::Command;
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    for n in 0..count {
        command(
            &mut h,
            Command::SubscribeOrderBook {
                symbol: format!("B{n}"),
            },
        );
    }
    h.open(SocketChannel::Public);
    assert!(h.http.is_empty());
    assert_eq!(
        current_wakes(&h).len(),
        512,
        "real engine pending handoff capacity"
    );
    h
}

#[test]
fn pending_handoff_credit_refusal_rolls_back_addition_and_retry_has_no_phantom_refcount() {
    use safe_cex_engine::Command;
    let mut h = pending_books(512);
    let public = h.socket(SocketChannel::Public);
    let old = current_wakes(&h);
    h.clear_observations();
    command(
        &mut h,
        Command::SubscribeOrderBook {
            symbol: "NEW".into(),
        },
    );
    assert_eq!(h.notices, 1);
    assert!(h.sent.is_empty() && h.http.is_empty() && h.closed.is_empty());
    h.clear_observations();
    command(
        &mut h,
        Command::UnsubscribeOrderBook {
            symbol: "B0".into(),
        },
    );
    assert_eq!(
        h.notices, 1,
        "last removal rolls back under the same real proposal pressure"
    );
    assert!(h.sent.is_empty() && h.http.is_empty() && h.closed.is_empty());
    fire_timer(&mut h, old[0]);
    assert_eq!(
        h.http.len(),
        1,
        "existing peer still receives its exact snapshot"
    );
    accept_depth_reads(&mut h, &mut vec![]);
    h.clear_observations();
    command(
        &mut h,
        Command::SubscribeOrderBook {
            symbol: "NEW".into(),
        },
    );
    assert_eq!(h.sent.len(), 1);
    assert!(h.sent[0].1.contains("depth.NEW"));
    let current = current_wakes(&h);
    assert_eq!(
        current.len(),
        512,
        "remaining peers plus NEW get current rebased wakes"
    );
    assert!(current.iter().all(|id| !old.contains(id)));
    for timer in &old {
        fire_timer(&mut h, *timer);
    }
    assert!(h.http.is_empty());
    let mut urls = vec![];
    for timer in current {
        fire_timer(&mut h, timer);
        accept_depth_reads(&mut h, &mut urls);
    }
    assert_eq!(urls.len(), 512);
    assert_eq!(
        urls.iter()
            .filter(|url| url.ends_with("symbol=NEW"))
            .count(),
        1
    );
    h.clear_observations();
    command(
        &mut h,
        Command::UnsubscribeOrderBook {
            symbol: "NEW".into(),
        },
    );
    assert_eq!(
        h.sent,
        [(
            public,
            r#"{"method":"UNSUBSCRIBE","params":["depth.NEW"]}"#.into()
        )]
    );
    h.clear_observations();
    command(
        &mut h,
        Command::UnsubscribeOrderBook {
            symbol: "B0".into(),
        },
    );
    assert_eq!(
        h.sent,
        [(
            public,
            r#"{"method":"UNSUBSCRIBE","params":["depth.B0"]}"#.into()
        )]
    );
    h.message(public, TICKER);
    assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
    assert!(h.closed.is_empty());
}

#[test]
fn real_synchronous_byte_and_invalid_key_refusal_preserve_peers_and_rollback_demand() {
    use safe_cex_engine::Command;
    for (len, reason) in [(4 * 1024 * 1024, "Capacity"), (65_536, "InvalidInterest")] {
        let mut h = Harness::new(PublicFeedPolicy::Direct);
        let public = h.open(SocketChannel::Public);
        h.clear_observations();
        let oversized = "X".repeat(len);
        h.engine.handle(
            h.now,
            Event::Command(Command::SubscribeOrderBook {
                symbol: oversized.clone(),
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
        let notices: Vec<_> = actions
            .iter()
            .filter_map(|a| {
                if let Action::Notice(safe_cex_engine::Notice::Event(n)) = a {
                    Some(n)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].code,
            safe_cex_engine::log::code::SUBSCRIPTION_REJECTED
        );
        assert!(notices[0].message.contains(reason));
        assert!(actions.iter().all(|a| matches!(
            a,
            Action::Notice(_) | Action::ClearTimer { .. } | Action::SetTimer { .. }
        )));
        command(&mut h, Command::UnsubscribeOrderBook { symbol: oversized });
        assert!(
            h.sent.is_empty() && h.http.is_empty() && h.notices == 0,
            "refused addition left no local subscriber"
        );
        command(
            &mut h,
            Command::SubscribeOrderBook {
                symbol: SYMBOL.into(),
            },
        );
        assert_eq!(h.sent.len(), 1);
        assert_eq!(wake_current_interests(&mut h).len(), 1);
        assert_eq!(h.http.len(), 1);
        h.message(public, TICKER);
        assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
        assert!(h.closed.is_empty());
    }
}

fn long_candle_symbols(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!("C{n:03}{}", "X".repeat(990)))
        .collect()
}

#[test]
fn valid_key_batch_frame_capacity_refuses_only_last_addition_then_recovers_after_release() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let public = h.open(SocketChannel::Public);
    h.clear_observations();
    let symbols = long_candle_symbols(80);
    let count = (1..=symbols.len())
        .find(|count| {
            let params: Vec<_> = symbols[..*count]
                .iter()
                .map(|s| format!("kline.1m.{s}"))
                .collect();
            serde_json::to_string(&serde_json::json!({"method":"SUBSCRIBE","params":params}))
                .unwrap()
                .len()
                > 65_536
        })
        .unwrap();
    let symbols = &symbols[..count];
    for symbol in symbols {
        assert!(format!("kline.1m.{symbol}").len() < 1024);
        h.engine.handle(
            h.now,
            Event::Command(Command::SubscribeCandles {
                symbol: symbol.clone(),
                interval: Interval::M1,
            }),
        );
    }
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let notices: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Notice(safe_cex_engine::Notice::Event(n)) = a {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].message.contains("Capacity"));
    let sent: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::WsSend { socket, text } = a {
                assert_eq!(*socket, public);
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        sent.len(),
        1,
        "prior peer frame remains atomically releasable"
    );
    let params = serde_json::from_str::<serde_json::Value>(sent[0]).unwrap()["params"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(params.len(), count - 1);
    for symbol in &symbols[..count - 1] {
        assert!(params.iter().any(|p| p == &format!("kline.1m.{symbol}")));
    }
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a, Action::WsClose { .. } | Action::Http(_)))
    );
    command(
        &mut h,
        Command::SubscribeCandles {
            symbol: symbols.last().unwrap().clone(),
            interval: Interval::M1,
        },
    );
    assert_eq!(
        h.sent.len(),
        1,
        "fresh real request succeeds after peer frame release"
    );
    h.clear_observations();
    command(
        &mut h,
        Command::UnsubscribeCandles {
            symbol: symbols.last().unwrap().clone(),
            interval: Interval::M1,
        },
    );
    assert_eq!(h.sent.len(), 1, "one successful add needs only one remove");
    assert!(h.sent[0].1.contains("UNSUBSCRIBE"));
    h.message(public, TICKER);
    assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
    assert!(h.closed.is_empty());
}

#[test]
fn persistent_handoff_pressure_observes_cooldown_and_three_retry_exhaustion() {
    let mut h = pending_books(513);
    assert_eq!(
        h.notices, 1,
        "same-clock commit callbacks did not retry the first Capacity"
    );
    for n in 1..=3 {
        let timers: Vec<_> = h
            .timers
            .iter()
            .filter(|(_, at)| *at == 1000. + f64::from(n) * 1000.)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(timers.len(), 1);
        h.now = 1000. + f64::from(n) * 1000.;
        fire_timer(&mut h, timers[0]);
        assert!(h.http.is_empty());
        assert_eq!(
            h.notices,
            1 + n as usize + usize::from(n == 3),
            "one attempt per genuine observation, then one exhaustion Warn"
        );
    }
    let future: Vec<_> = h
        .timers
        .iter()
        .filter(|(_, at)| *at > 4000. && *at < 6000.)
        .collect();
    assert!(future.is_empty(), "no fourth recovery timer");
    let current: Vec<_> = h
        .timers
        .iter()
        .filter(|(_, at)| *at == 1000.)
        .map(|(timer, _)| *timer)
        .collect();
    assert_eq!(current.len(), 512);
    fire_timer(&mut h, current[0]);
    accept_depth_reads(&mut h, &mut vec![]);
    h.clear_observations();
    command(
        &mut h,
        safe_cex_engine::Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
        },
    );
    assert_eq!(
        h.sent.len(),
        1,
        "genuine peer proposal commits with the released credit"
    );
    assert!(
        h.http.is_empty() && h.notices == 0,
        "same-clock commit cannot restart exhausted snapshot recovery"
    );
    h.clear_observations();
    let public = h.socket(SocketChannel::Public);
    h.message(public, TICKER);
    assert_eq!(h.engine.store().ticker(SYMBOL).unwrap().last, 105.);
    assert!(
        h.http.is_empty() && h.notices == 0,
        "ordinary current ingress cannot restart exhausted recovery"
    );
}

#[test]
fn all_nonfinite_interest_retry_timer_deliveries_create_no_work_or_timers() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = pending_books(513);
        let retry = h.timers.iter().find(|(_, at)| *at == 2000.).unwrap().0;
        let before = store_frame(&h);
        h.engine.handle(now, Event::TimerFired { timer: retry });
        h.engine.prepare_dispatch(now);
        let actions: Vec<_> = h.engine.drain_actions().collect();
        assert_eq!(store_frame(&h), before);
        assert!(
            actions.iter().all(|action| match action {
                Action::ClearTimer { timer } => h.timers.iter().any(|(id, _)| id == timer),
                Action::WsClose { socket } => h.connects.iter().any(|(_, id)| id == socket),
                _ => false,
            }),
            "only exact issued cleanup, no proposal/HTTP/auth/frames/timers"
        );
    }
}

#[test]
fn inherited_engine_ready_replay_capacity_stops_only_its_feed_without_venue_retry() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new(PublicFeedPolicy::Direct);
    let public = h.open(SocketChannel::Public);
    let private = h.open(SocketChannel::Private);
    for symbol in long_candle_symbols(80) {
        command(
            &mut h,
            Command::SubscribeCandles {
                symbol,
                interval: Interval::M1,
            },
        );
    }
    h.now = 1001.;
    h.event(Event::WsClosed {
        socket: public,
        code: 1006,
    });
    h.advance(2000.);
    let new = h.socket(SocketChannel::Public);
    assert_ne!(new, public);
    h.clear_observations();
    h.open(SocketChannel::Public);
    assert!(
        h.closed.contains(&new),
        "inherited Ready replay Capacity remains fail-closed"
    );
    assert!(!h.closed.contains(&private));
    assert!(
        h.sent.is_empty(),
        "no partial replay or opening PING handoff"
    );
    assert_eq!(
        h.connects
            .iter()
            .filter(|(channel, _)| *channel == SocketChannel::Public)
            .count(),
        2
    );
    h.message(private, ORDER);
    assert!(h.engine.store().order("unexpected").is_some());
}

#[test]
fn removing_the_only_unarmed_demand_cancels_its_obsolete_recovery_timer() {
    use safe_cex_engine::Command;
    let mut h = pending_books(513);
    let retry = h.timers.iter().find(|(_, at)| *at == 2000.).unwrap().0;
    let mut urls = vec![];
    for timer in current_wakes(&h) {
        fire_timer(&mut h, timer);
        accept_depth_reads(&mut h, &mut urls);
    }
    assert_eq!(urls.len(), 512);
    let missing: Vec<_> = (0..513)
        .map(|n| format!("B{n}"))
        .filter(|symbol| {
            !urls
                .iter()
                .any(|url| url.ends_with(&format!("symbol={symbol}")))
        })
        .collect();
    assert_eq!(missing.len(), 1);
    h.clear_observations();
    command(
        &mut h,
        Command::UnsubscribeOrderBook {
            symbol: missing[0].clone(),
        },
    );
    assert!(
        !h.timers.iter().any(|(timer, _)| *timer == retry),
        "resolved demand cancels its exact genuine timer"
    );
    assert_eq!(h.sent.len(), 1);
    h.clear_observations();
    h.now = 2000.;
    fire_timer(&mut h, retry);
    assert!(
        h.sent.is_empty() && h.http.is_empty() && h.notices == 0,
        "obsolete wake cannot mint new work"
    );
}
