#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::{MARKET, TICKER, envelope};
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_okx::Okx;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const LOGIN_OK: &str = r#"{"event":"login","code":"0","msg":""}"#;

struct Harness {
    engine: Engine<Okx>,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    connects: Vec<(SocketId, String)>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    requests: Vec<Action>,
    signs: Vec<(RequestId, SignRequest)>,
}

/// Holds signing work until released, like a slow host provider.
struct Gate {
    provider: Provider,
    parked: Arc<AtomicBool>,
}
impl Signer for Gate {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.provider.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.parked.load(Ordering::SeqCst) {
            None
        } else {
            self.provider.sign_ready(request)
        }
    }
}

impl Harness {
    fn new(options: Options, policy: PublicFeedPolicy) -> Self {
        Self::new_with(options, policy, |_| {})
    }

    fn new_with(
        options: Options,
        policy: PublicFeedPolicy,
        configure: impl FnOnce(&mut Engine<Okx>),
    ) -> Self {
        let mut engine = crate::support::engine(options);
        engine.set_public_feed_policy(policy).unwrap();
        configure(&mut engine);
        let mut h = Self {
            engine,
            now: 0.0,
            timers: vec![],
            connects: vec![],
            sent: vec![],
            closed: vec![],
            requests: vec![],
            signs: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h.reply_next(&envelope(MARKET));
        h.reply_next(&envelope(TICKER));
        h
    }

    fn public() -> Self {
        Self::new(Options::default(), PublicFeedPolicy::Direct)
    }

    /// Credentials make business and private sockets log in; HTTP is answered empty.
    fn private() -> Self {
        Self::private_with(|_| {})
    }

    /// `private`, with the provider behind a gate that parks signing work.
    fn private_gated(parked: Arc<AtomicBool>) -> Self {
        Self::private_with(|engine| {
            let provider = engine.signer().unwrap();
            engine
                .set_signer(Some(Arc::new(Gate { provider, parked })))
                .unwrap();
        })
    }

    fn private_with(configure: impl FnOnce(&mut Engine<Okx>)) -> Self {
        let mut h = Self::new_with(
            Options {
                key: Some("test-key".into()),
                secret: Some("test-secret".into()),
                passphrase: Some("test-passphrase".into()),
                ..Options::default()
            },
            PublicFeedPolicy::Direct,
            configure,
        );
        let config = h
            .requests
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.url.ends_with("/api/v5/account/config")))
            .unwrap();
        let config = h.requests.remove(config);
        h.reply(&config, &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#));
        h.requests.clear();
        h
    }

    fn collect(&mut self) {
        for action in crate::support::prepared(&mut self.engine, self.now) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { socket, url, .. } => self.connects.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::Http(_) => self.requests.push(action),
                Action::Sign { attempt, work, .. } => self.signs.push((attempt, work)),
                _ => {}
            }
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn reply(&mut self, action: &Action, body: &str) {
        let Action::Http(r) = action else {
            panic!("expected request")
        };
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }

    fn reply_next(&mut self, body: &str) {
        let request = self.requests.remove(0);
        self.reply(&request, body);
    }

    fn socket(&self, path: &str) -> SocketId {
        self.connects
            .iter()
            .rev()
            .find(|(_, url)| url.ends_with(path))
            .unwrap_or_else(|| panic!("no {path} socket"))
            .0
    }

    fn connect_count(&self, path: &str) -> usize {
        self.connects
            .iter()
            .filter(|(_, url)| url.ends_with(path))
            .count()
    }

    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }

    fn open(&mut self, path: &str) -> SocketId {
        let socket = self.socket(path);
        self.event(Event::WsOpened { socket });
        if path != "/public"
            && self
                .sent
                .iter()
                .any(|(s, t)| *s == socket && t.contains("login"))
        {
            self.message(socket, LOGIN_OK);
        }
        socket
    }

    /// Fires every timer due by `until`, answering pings on `answer` sockets.
    fn advance_answering(&mut self, until: f64, answer: &[SocketId]) -> usize {
        let mut fired = 0;
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            let pings = self.sent.len();
            self.event(Event::TimerFired { timer });
            let answered: Vec<_> = self.sent[pings..]
                .iter()
                .filter(|(s, t)| t == "ping" && answer.contains(s))
                .map(|(s, _)| *s)
                .collect();
            for socket in answered {
                self.message(socket, "pong");
            }
            fired += 1;
        }
        self.now = until;
        fired
    }

    fn advance(&mut self, until: f64) -> usize {
        self.advance_answering(until, &[])
    }

    fn pings(&self, socket: SocketId) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t == "ping")
            .count()
    }

    /// Closes `path` and returns the delay of the single retry timer it armed.
    fn close(&mut self, path: &str) -> f64 {
        let socket = self.socket(path);
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

    fn reconnect(&mut self, path: &str, delay: f64) {
        let count = self.connect_count(path);
        let until = self.now + delay;
        self.advance(until);
        assert_eq!(
            self.connect_count(path),
            count + 1,
            "one reconnect per close"
        );
    }
}

fn harness(path: &str) -> Harness {
    if path == "/public" {
        Harness::public()
    } else {
        Harness::private()
    }
}

const SOCKETS: [&str; 3] = ["/public", "/business", "/private"];

#[test]
fn flapping_sockets_back_off_although_each_attempt_opens() {
    for path in SOCKETS {
        let mut h = harness(path);
        for attempt in 0..7 {
            h.open(path);
            let delay = h.close(path);
            let nominal = (1_000.0 * 2f64.powi(attempt)).min(30_000.0);
            assert!(
                (nominal * 0.75..=nominal).contains(&delay),
                "{path} attempt {attempt}: {delay}"
            );
            h.reconnect(path, delay);
        }
    }
}

#[test]
fn reconnected_public_socket_resubscribes_and_ignores_retired_frames() {
    let mut h = Harness::public();
    h.open("/public");
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let old = h.socket("/public");
    let delay = h.close("/public");
    h.reconnect("/public", delay);
    h.sent.clear();
    let socket = h.open("/public");
    assert_ne!(socket, old);
    let sent: Vec<_> = h.sent.iter().filter(|(s, _)| *s == socket).collect();
    for topic in [
        r#""channel":"tickers""#,
        r#""channel":"mark-price""#,
        r#""channel":"books""#,
    ] {
        assert!(sent.iter().any(|(_, t)| t.contains(topic)), "{topic}");
    }
    assert!(sent.iter().any(|(_, t)| t == "ping"));
    let mark = |px: &str| {
        format!(
            r#"{{"arg":{{"channel":"mark-price"}},"data":[{{"instId":"BTC-USDT-SWAP","markPx":"{px}"}}]}}"#
        )
    };
    h.message(old, &mark("42000"));
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().mark, 40000.0);
    h.message(socket, &mark("41000"));
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().mark, 41000.0);
}

#[test]
fn reconnected_business_socket_resubscribes_candles() {
    let mut h = Harness::public();
    h.open("/business");
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    let delay = h.close("/business");
    h.reconnect("/business", delay);
    h.sent.clear();
    let socket = h.open("/business");
    assert!(
        h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains(r#""channel":"candle1m""#))
    );
}

#[test]
fn reconnected_private_sockets_log_in_before_resubscribing() {
    for (path, topic) in [("/private", "positions"), ("/business", "orders-algo")] {
        let mut h = Harness::private();
        h.open(path);
        let delay = h.close(path);
        h.reconnect(path, delay);
        let socket = h.socket(path);
        h.sent.clear();
        h.event(Event::WsOpened { socket });
        assert!(
            h.sent
                .iter()
                .any(|(s, t)| *s == socket && t.contains(r#""op":"login""#))
        );
        assert!(!h.sent.iter().any(|(_, t)| t.contains(topic)), "{path}");
        h.message(socket, LOGIN_OK);
        assert!(
            h.sent
                .iter()
                .any(|(s, t)| *s == socket && t.contains(topic)),
            "{path}"
        );
    }
}

#[test]
fn open_sockets_ping_every_ten_seconds_and_answered_pings_keep_them_alive() {
    for path in SOCKETS {
        let mut h = harness(path);
        let socket = h.open(path);
        let opened = h.now;
        assert_eq!(h.pings(socket), 1, "{path} pings at open");
        h.message(socket, "pong");
        h.advance_answering(opened + 9_999.0, &[socket]);
        assert_eq!(h.pings(socket), 1, "{path}");
        h.advance_answering(opened + 10_000.0, &[socket]);
        assert_eq!(h.pings(socket), 2, "{path}");
        h.advance_answering(opened + 120_000.0, &[socket]);
        assert_eq!(h.pings(socket), 13, "{path}");
        assert!(!h.closed.contains(&socket), "{path}");
    }
}

#[test]
fn unanswered_pings_retire_socket_after_twenty_seconds() {
    for path in SOCKETS {
        let mut h = harness(path);
        let socket = h.open(path);
        let opened = h.now;
        // Data frames are not pongs and do not prove liveness.
        for step in 1..20 {
            h.advance(opened + f64::from(step) * 1_000.0);
            h.message(socket, r#"{"arg":{"channel":"tickers"},"data":[]}"#);
        }
        h.advance(opened + 19_999.0);
        assert!(!h.closed.contains(&socket), "{path}");
        h.advance(opened + 20_000.0);
        assert!(h.closed.contains(&socket), "{path}");
        let count = h.connect_count(path);
        let until = h.now + 1_000.0;
        h.advance(until);
        assert_eq!(h.connect_count(path), count + 1, "{path}");
    }
}

#[test]
fn never_opened_sockets_expire_after_ten_seconds() {
    for path in SOCKETS {
        let mut h = harness(path);
        let socket = h.socket(path);
        let connecting = h.now;
        h.advance(connecting + 9_999.0);
        assert!(!h.closed.contains(&socket), "{path}");
        h.advance(connecting + 10_000.0);
        assert!(h.closed.contains(&socket), "{path}");
    }
}

#[test]
fn unacknowledged_login_expires_after_ten_seconds_despite_pongs() {
    for path in ["/business", "/private"] {
        let mut h = Harness::private();
        let socket = h.socket(path);
        h.event(Event::WsOpened { socket });
        let opened = h.now;
        h.advance_answering(opened + 9_999.0, &[socket]);
        assert!(!h.closed.contains(&socket), "{path}");
        h.advance_answering(opened + 10_000.0, &[socket]);
        assert!(h.closed.contains(&socket), "{path}");
    }
}

#[test]
fn stable_traffic_resets_backoff() {
    for path in SOCKETS {
        let mut h = harness(path);
        for _ in 0..3 {
            h.open(path);
            let delay = h.close(path);
            h.reconnect(path, delay);
        }
        let socket = h.open(path);
        h.message(socket, "pong");
        let until = h.now + 45_000.0;
        h.advance_answering(until, &[socket]);
        let delay = h.close(path);
        assert!((750.0..=1_000.0).contains(&delay), "{path}: {delay}");
    }
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::new(Options::default(), PublicFeedPolicy::Shared);
    assert_eq!(h.connect_count("/public"), 0);
    let business = h.open("/business");
    h.message(business, "pong");
    let until = h.now + 600_000.0;
    h.advance_answering(until, &[business]);
    assert_eq!(h.connect_count("/public"), 0);
    assert!(h.closed.is_empty(), "no closes: {:?}", h.closed);
    // Only the live business socket's watchdog remains armed.
    assert_eq!(h.timers.len(), 1, "{:?}", h.timers);
    assert_eq!(h.pings(business), 61);
}

#[test]
fn managed_duplicate_open_does_not_repeat_login_or_health() {
    let mut h = Harness::private();
    let socket = h.open("/private");
    let sends = h.sent.len();
    let opened = h.engine.health().ws_opened;
    h.event(Event::WsOpened { socket });
    assert_eq!(
        h.sent.len(),
        sends,
        "duplicate open cannot resend auth/ping"
    );
    assert_eq!(
        h.engine.health().ws_opened,
        opened,
        "duplicate open is not traffic"
    );
}

#[test]
fn managed_duplicate_login_ack_does_not_replay_subscriptions() {
    let mut h = Harness::private();
    let socket = h.open("/private");
    let sends = h.sent.len();
    let requests = h.requests.len();
    h.message(socket, LOGIN_OK);
    assert_eq!(h.sent.len(), sends, "one subscription replay per epoch");
    assert_eq!(
        h.requests.len(),
        requests,
        "duplicate ACK cannot restart reconciliation"
    );
}

#[test]
fn managed_expired_auth_cannot_be_rescued_before_watchdog_delivery() {
    let mut h = Harness::private();
    let socket = h.socket("/private");
    h.event(Event::WsOpened { socket });
    h.sent.clear();
    // Deliberately do not deliver timers. Ingress must enforce the same bound.
    h.now = 10_000.;
    h.message(socket, LOGIN_OK);
    assert!(
        h.closed.contains(&socket),
        "expired epoch must retire before ACK"
    );
    assert!(
        !h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains("subscribe"))
    );
}

#[test]
fn malformed_login_codes_never_authenticate_or_deactivate_credentials() {
    for text in [
        r#"{"event":"login"}"#,
        r#"{"event":"login","code":0}"#,
        r#"{"event":"login","code":null}"#,
    ] {
        let mut h = Harness::private();
        let socket = h.socket("/private");
        h.event(Event::WsOpened { socket });
        h.sent.clear();
        h.message(socket, text);
        assert!(h.closed.contains(&socket));
        assert!(!h.sent.iter().any(|(_, s)| s.contains("subscribe")));
        assert_eq!(h.engine.health().auth_failures, 0);
    }
}

#[test]
fn array_shaped_login_is_neither_acceptance_nor_auth_recovery_evidence() {
    for text in [r#"["login"]"#, r#"["login","0"]"#, r#"["login",null]"#] {
        let mut h = Harness::private();
        let socket = h.socket("/private");
        h.event(Event::WsOpened { socket });
        h.sent.clear();
        h.message(socket, text);
        assert!(!h.closed.contains(&socket));
        assert!(!h.sent.iter().any(|(_, s)| s.contains("subscribe")));
        assert_eq!(h.engine.health().auth_failures, 0);
        h.message(socket, LOGIN_OK);
        assert!(h.sent.iter().any(|(_, s)| s.contains("subscribe")));
    }
}

#[test]
fn retirement_before_first_depth_does_not_materialize_an_empty_consumer_book() {
    let mut h = Harness::public();
    h.open("/public");
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.engine.take_frame(h.now);
    h.close("/public");
    assert!(
        h.engine.take_market_frame(h.now).is_none(),
        "a never-published subscription must not create a BOOK clear frame"
    );
}

#[test]
fn retirement_of_a_loaded_book_still_publishes_an_immediate_clear() {
    let mut h = Harness::public();
    let socket = h.open("/public");
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.message(socket, r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{"seqId":100,"prevSeqId":-1,"bids":[["39999","5","0","1"]],"asks":[["40001","7","0","1"]]}]}"#);
    h.engine.take_frame(h.now);
    h.close("/public");
    let frame = h
        .engine
        .take_market_frame(h.now)
        .expect("published book retirement must reach its consumer");
    assert!(
        safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::BOOK)
    );
}

/// A login signed after its socket closed, or after its freshness window, never
/// reaches a socket: the engine's signing gate rechecks the epoch and expiry.
fn parked_login(h: &mut Harness, parked: &Arc<AtomicBool>) -> (SocketId, RequestId, SignResult) {
    h.open("/private");
    let delay = h.close("/private");
    h.reconnect("/private", delay);
    let socket = h.socket("/private");
    h.sent.clear();
    h.signs.clear();
    parked.store(true, Ordering::SeqCst);
    h.event(Event::WsOpened { socket });
    assert!(
        !h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains("login")),
        "login waits for the provider"
    );
    let (attempt, work) = h
        .signs
        .iter()
        .find(|(_, work)| work.context.method == "WS")
        .cloned()
        .expect("staged WS login work");
    parked.store(false, Ordering::SeqCst);
    let result = h
        .engine
        .signer()
        .unwrap()
        .sign_ready(&work)
        .unwrap()
        .unwrap();
    parked.store(true, Ordering::SeqCst);
    (socket, attempt, result)
}

#[test]
fn delayed_login_signature_after_socket_retirement_is_inert() {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = Harness::private_gated(parked.clone());
    let (socket, attempt, result) = parked_login(&mut h, &parked);
    h.event(Event::WsClosed { socket, code: 1006 });
    h.sent.clear();
    h.event(Event::Signed {
        attempt,
        result: Ok(result),
    });
    assert!(
        !h.sent.iter().any(|(s, _)| *s == socket),
        "late signature cannot send on a retired socket"
    );
}

#[test]
fn expired_login_signature_never_sends_its_stale_stamp() {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = Harness::private_gated(parked.clone());
    let (socket, attempt, result) = parked_login(&mut h, &parked);
    let stale = format!(r#""timestamp":"{}""#, (h.now / 1000.0).floor());
    h.now += 6_000.0;
    h.sent.clear();
    h.signs.clear();
    h.event(Event::Signed {
        attempt,
        result: Ok(result),
    });
    assert!(
        !h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains(&stale)),
        "an expired login signature is never released"
    );
}

#[test]
fn prompt_login_signature_is_released_on_the_live_socket() {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = Harness::private_gated(parked.clone());
    let (socket, attempt, result) = parked_login(&mut h, &parked);
    let stamp = format!(r#""timestamp":"{}""#, (h.now / 1000.0).floor());
    h.sent.clear();
    h.event(Event::Signed {
        attempt,
        result: Ok(result),
    });
    assert!(
        h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains(r#""op":"login""#) && t.contains(&stamp)),
        "control: a live, fresh signature sends the login: {:?}",
        h.sent
    );
}

#[test]
fn reconnect_subscription_precedes_its_account_refresh() {
    let mut h = Harness::private();
    let first = h.open("/private");
    h.requests.clear();
    h.event(Event::WsClosed {
        socket: first,
        code: 1006,
    });
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= h.now + 1000.)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("reconnect timer");
    h.now = at;
    h.event(Event::TimerFired { timer });
    let replacement = h.socket("/private");
    assert_ne!(first, replacement);
    h.now += 1.;
    h.event(Event::WsOpened {
        socket: replacement,
    });
    h.engine.handle(
        h.now + 1.,
        Event::WsMessage {
            socket: replacement,
            text: LOGIN_OK,
        },
    );
    let mut out: Vec<_> = h.engine.drain_actions().collect();
    let wakeups: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1. => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    for (timer, at) in wakeups {
        h.engine.handle(at, Event::TimerFired { timer });
        out.extend(h.engine.drain_actions());
    }
    let subscribe = out.iter().position(|action| matches!(action,
        Action::WsSend { socket, text } if *socket == replacement && text.contains("subscribe")
    )).expect("private subscription");
    let refresh = out
        .iter()
        .position(|action| match action {
            Action::Http(request) => request.url.contains("/api/v5/account/"),
            // Account-cycle reads are managed: their Prepare is the earliest staging.
            Action::ManagedHttp(http) => matches!(
                &**http,
                safe_cex_engine::request_context::HttpAction::Prepare { request, .. }
                    if request.url.contains("/api/v5/account/")
            ),
            _ => false,
        })
        .expect("account HTTP refresh");
    assert!(
        subscribe < refresh,
        "HTTP refresh preceded private subscription release: subscribe={subscribe}, refresh={refresh}"
    );
}
