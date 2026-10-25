//! Original runtime boundaries; all authority comes from real engine admission.
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, PublicFeedPolicy, RequestId, Response,
    SocketChannel, SocketId, TimerId, Venue,
};
use safe_cex_venue_weex::{Pending, WeexFutures};
use std::collections::{HashMap, VecDeque};
use support::setup::Options;

const NOW: f64 = 1_000.;
const LIMIT: f64 = 20_000.;
const A: &str = "BTCUSDT";
const B: &str = "ETHUSDT";
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":3},{"symbol":"ETHUSDT","baseAsset":"ETH","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":2,"quantityPrecision":3}]}"#;
const DAILY: &str =
    r#"[{"symbol":"BTCUSDT","lastPrice":"100"},{"symbol":"ETHUSDT","lastPrice":"100"}]"#;
const TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"99","askPrice":"101"},{"symbol":"ETHUSDT","bidPrice":"99","askPrice":"101"}]"#;
const DEPTH: &str = r#"{"lastUpdateId":160,"bids":[["99","2"]],"asks":[["101","1"]]}"#;
const PRIMARY_KEY: &str = "fixture-weex-primary";
const PRIMARY_MATERIAL: &str = "weex-fixture-primary-material";
const PRIMARY_PASS: &str = "fixture-weex-primary-pass";

#[derive(Clone)]
struct Issued {
    token: Preparation,
    request: HttpRequest,
    intent: RoutingIntent,
    timer: TimerId,
    deadline: f64,
}
#[derive(Default)]
struct Trace {
    actions: Vec<Action>,
    requests: Vec<HttpRequest>,
    prepares: Vec<Issued>,
}
impl Trace {
    fn depth_requests(&self) -> Vec<&HttpRequest> {
        self.requests.iter().filter(|r| depth(r)).collect()
    }
    fn socket(&self, role: SocketChannel) -> SocketId {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket, channel, ..
                } if channel.as_ref() == Some(&role) => Some(*socket),
                _ => None,
            })
            .expect("genuine native socket connect")
    }
    fn sent(&self) -> Vec<(SocketId, &str)> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { socket, text } => Some((*socket, text.as_str())),
                _ => None,
            })
            .collect()
    }
}
fn depth(r: &HttpRequest) -> bool {
    r.url.contains("/depth?")
}
struct Driver<V: Venue> {
    e: Engine<V>,
    routes: HashMap<RequestId, Preparation>,
}
impl<V: Venue> Driver<V> {
    fn new(e: Engine<V>) -> Self {
        Self {
            e,
            routes: HashMap::new(),
        }
    }
    fn take(&mut self, now: f64, hold_depth: bool) -> Trace {
        self.e.prepare_dispatch(now);
        let mut queue: VecDeque<_> = self.e.drain_actions().collect();
        let mut out = Trace::default();
        let mut armed = None;
        let mut count = 0;
        while let Some(action) = queue.pop_front() {
            count += 1;
            assert!(count < 4096, "fixture progress is bounded");
            match &action {
                Action::SetTimer { timer, at_ms } => armed = Some((*timer, *at_ms)),
                Action::Http(r) => out.requests.push(r.clone()),
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        intent,
                        ..
                    } => {
                        assert!(self.routes.insert(request.id, *preparation).is_none());
                        if depth(request) {
                            let (timer, deadline) = armed.expect("atomic timer before Prepare");
                            out.prepares.push(Issued {
                                token: *preparation,
                                request: request.clone(),
                                intent: *intent,
                                timer,
                                deadline,
                            });
                        }
                        if !hold_depth || !depth(request) {
                            self.e.http_prepared(
                                now,
                                *preparation,
                                Prepared {
                                    id: request.id.0,
                                    expires_at: None,
                                },
                            );
                        }
                    }
                    HttpAction::Bind {
                        preparation,
                        permit,
                        request,
                    } => {
                        assert_eq!(self.routes.remove(&request.id), Some(*preparation));
                        assert!(matches!(
                            self.e.claim_http(now, *permit),
                            Ok(Claim::Released(_))
                        ));
                        out.requests.push(request.clone());
                    }
                    HttpAction::Cancel { preparation, .. } => {
                        self.routes.remove(&preparation.attempt().request);
                    }
                    HttpAction::BindGroup { .. } => {
                        panic!("this fixture declares no mutation group")
                    }
                },
                _ => {}
            }
            out.actions.push(action);
            for next in self.e.drain_actions().collect::<Vec<_>>().into_iter().rev() {
                queue.push_front(next);
            }
        }
        out
    }
    fn event(&mut self, now: f64, event: Event<'_>, hold_depth: bool) -> Trace {
        self.e.handle(now, event);
        self.take(now, hold_depth)
    }
    fn response(&mut self, now: f64, id: RequestId, status: u16, body: &str) -> Trace {
        self.event(
            now,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
            false,
        )
    }
    fn subscribe(&mut self, now: f64, symbol: &str, hold: bool) -> Trace {
        self.event(
            now,
            Event::Command(Command::SubscribeOrderBook {
                symbol: symbol.into(),
            }),
            hold,
        )
    }
    fn unsubscribe(&mut self, now: f64, symbol: &str) -> Trace {
        self.event(
            now,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: symbol.into(),
            }),
            true,
        )
    }
    fn release(&mut self, now: f64, issued: &Issued) -> HttpRequest {
        assert!(self.e.http_preparation_pending(issued.token));
        self.e.http_prepared(
            now,
            issued.token,
            Prepared {
                id: issued.request.id.0,
                expires_at: None,
            },
        );
        let out = self.take(now, true);
        assert_eq!(
            out.depth_requests().len(),
            1,
            "exact prepared snapshot released"
        );
        out.depth_requests()[0].clone()
    }
}
fn request(out: &Trace, path: &str) -> RequestId {
    out.requests
        .iter()
        .find(|r| r.url.contains(path))
        .expect("real bootstrap request")
        .id
}
fn bootstrap<V: Venue>(d: &mut Driver<V>) -> Trace {
    d.e.start(NOW);
    let initial = d.take(NOW, false);
    let tickers = d.response(NOW, request(&initial, "exchangeInfo"), 200, INFO);
    let _ = d.response(NOW, request(&tickers, "24hr"), 200, DAILY);
    d.response(NOW, request(&tickers, "bookTicker"), 200, TICKERS)
}
fn options(private: bool, headers: bool) -> Options {
    Options {
        public_data_only: !private,
        key: private.then(|| PRIMARY_KEY.into()),
        secret: private.then(|| PRIMARY_MATERIAL.into()),
        passphrase: private.then(|| PRIMARY_PASS.into()),
        private_ws_headers: headers,
        read_credentials: private.then(|| safe_cex_engine::ReadCredentials {
            key: "fixture-weex-read".into(),
            secret: "weex-fixture-read-material".into(),
            passphrase: Some("fixture-weex-read-pass".into()),
        }),
        ..Default::default()
    }
}
fn public() -> (Driver<WeexFutures>, SocketId) {
    let mut d = Driver::new(support::setup::engine(options(false, false)));
    let out = bootstrap(&mut d);
    let socket = out.socket(SocketChannel::Public);
    (d, socket)
}
fn store<V: Venue>(d: &Driver<V>) -> Vec<u8> {
    let s = d.e.store();
    let mut frame = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.);
    s.write(&s.everything(), &mut frame);
    frame.finish()
}
fn prepared(out: &Trace) -> Issued {
    assert!(
        out.depth_requests().is_empty(),
        "Depth cannot escape before host preparation"
    );
    assert_eq!(
        out.prepares.len(),
        1,
        "one genuine managed Depth preparation"
    );
    out.prepares[0].clone()
}
fn exact_cleanup(out: &Trace, issued: Option<&Issued>) {
    let mut timer = false;
    let mut preparation = false;
    for action in &out.actions {
        match (action, issued) {
            (Action::ClearTimer { timer: id }, Some(p)) if *id == p.timer && !timer => timer = true,
            (Action::ManagedHttp(a), Some(p))
                if matches!(a.as_ref(), HttpAction::Cancel {
                preparation: token, signing: None,
            } if *token == p.token)
                    && !preparation =>
            {
                preparation = true
            }
            _ => panic!(
                "unexpected effect beyond exact issued cleanup: {:?}",
                out.actions
            ),
        }
    }
    if issued.is_some() {
        assert!(
            timer && preparation,
            "both captured physical resources retire exactly once"
        );
    }
}

#[test]
fn depth_waits_for_prepare_with_primary_unsigned_twenty_second_bound() {
    let (mut d, _) = public();
    let out = d.subscribe(NOW + 1., A, true);
    let issued = prepared(&out);
    assert_eq!(issued.intent, RoutingIntent::Primary);
    assert_eq!(issued.deadline, NOW + 1. + LIMIT);
    assert_eq!(issued.request.timeout_ms, Some(20_000));
    assert_eq!(
        issued.request.url,
        "https://api-contract.weex.com/capi/v3/market/depth?limit=200&symbol=BTCUSDT"
    );
    assert!(issued.request.headers.is_empty());
    assert!(issued.request.body.is_none());
    let sent = d.release(NOW + 2., &issued);
    let _ = d.response(NOW + 3., sent.id, 200, DEPTH);
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
}
#[test]
fn native_initial_snapshot_precedes_eligible_depth_topic() {
    let (mut d, socket) = public();
    let out = d.subscribe(NOW + 1., A, false);
    assert_eq!(out.depth_requests().len(), 1);
    assert!(out.sent().is_empty());
    let open = d.event(NOW + 2., Event::WsOpened { socket }, false);
    assert!(
        open.sent()
            .iter()
            .all(|(_, text)| !text.contains("@depth200"))
    );
    let accepted = d.response(NOW + 3., out.depth_requests()[0].id, 200, DEPTH);
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
    assert!(
        accepted
            .sent()
            .iter()
            .any(|(_, text)| text.contains("BTCUSDT@depth200"))
    );
}
#[test]
fn managed_unsubscribe_retires_only_its_preopen_snapshot_and_peer_survives() {
    let (mut d, socket) = public();
    let a = prepared(&d.subscribe(NOW + 1., A, true));
    let b = prepared(&d.subscribe(NOW + 2., B, true));
    let removed = d.unsubscribe(NOW + 3., A);
    exact_cleanup(&removed, Some(&a));
    assert!(!d.e.http_preparation_pending(a.token));
    assert!(d.e.http_preparation_pending(b.token));
    let sent = d.release(NOW + 4., &b);
    let accepted = d.response(NOW + 5., sent.id, 200, DEPTH);
    assert!(
        accepted.sent().is_empty(),
        "pre-open snapshot cannot send an interest frame"
    );
    assert!(d.e.book(A).is_none());
    assert_eq!(d.e.book(B).unwrap().bids(), [(99., 2.)]);
    let before = store(&d);
    assert!(
        d.event(NOW + 6., Event::TimerFired { timer: a.timer }, false)
            .actions
            .is_empty()
    );
    assert!(
        d.response(NOW + 6., a.request.id, 200, DEPTH)
            .actions
            .is_empty()
    );
    assert_eq!(store(&d), before);
    let opened = d.event(NOW + 7., Event::WsOpened { socket }, false);
    assert!(
        opened
            .sent()
            .iter()
            .any(|(_, s)| s.contains("ETHUSDT@depth200"))
    );
    assert!(
        opened
            .sent()
            .iter()
            .all(|(_, s)| !s.contains("BTCUSDT@depth200"))
    );
    let fresh = prepared(&d.subscribe(NOW + 8., A, true));
    assert_ne!(fresh.token, a.token);
    assert!(d.e.http_preparation_pending(fresh.token));
}
#[test]
fn native_two_preopen_books_keep_independent_publication_after_removal() {
    let (mut d, _) = public();
    let a = d.subscribe(NOW + 1., A, false).depth_requests()[0].clone();
    let b = d.subscribe(NOW + 2., B, false).depth_requests()[0].clone();
    let _ = d.unsubscribe(NOW + 3., A);
    let _ = d.response(NOW + 4., b.id, 200, DEPTH);
    let before = store(&d);
    let old = d.response(NOW + 5., a.id, 200, DEPTH);
    exact_cleanup(&old, None);
    assert_eq!(store(&d), before);
    assert!(d.e.book(A).is_none());
    assert_eq!(d.e.book(B).unwrap().bids(), [(99., 2.)]);
}
#[test]
fn expired_depth_response_cannot_publish_at_exact_physical_deadline() {
    let (mut d, _) = public();
    let out = d.subscribe(NOW + 1., A, false);
    assert_eq!(out.depth_requests().len(), 1);
    let _ = d.response(NOW + 1. + LIMIT, out.depth_requests()[0].id, 200, DEPTH);
    assert!(
        d.e.book(A).is_none_or(|book| book.bids().is_empty()),
        "expired snapshot cannot publish"
    );
}
#[test]
fn old_depth_429_after_unsubscribe_cannot_pause_or_emit_diagnostics() {
    let (mut d, _) = public();
    let out = d.subscribe(NOW + 1., A, false);
    let r = out.depth_requests()[0].clone();
    let _ = d.unsubscribe(NOW + 2., A);
    let before = store(&d);
    let late = d.response(NOW + 3., r.id, 429, r#"{"malformed":"old response"}"#);
    assert!(
        late.actions.is_empty(),
        "stale Depth status must not pause or diagnose"
    );
    assert_eq!(store(&d), before);
    let next = d.subscribe(NOW + 4., B, true);
    let _ = prepared(&next);
}
fn nonfinite_admission(now: f64) {
    let (mut d, _) = public();
    let before = store(&d);
    let out = d.subscribe(now, A, true);
    assert!(
        out.actions.is_empty(),
        "invalid clock admits no snapshot, timer or work"
    );
    assert_eq!(store(&d), before);
    let fresh = d.subscribe(NOW + 1., B, true);
    let _ = prepared(&fresh);
}
#[test]
fn nan_depth_admission_is_inert() {
    nonfinite_admission(f64::NAN);
}
#[test]
fn positive_infinity_depth_admission_is_inert() {
    nonfinite_admission(f64::INFINITY);
}
#[test]
fn negative_infinity_depth_admission_is_inert() {
    nonfinite_admission(f64::NEG_INFINITY);
}
fn nonfinite_returned_failure(now: f64) {
    let (mut d, _) = public();
    let out = d.subscribe(NOW + 1., A, false);
    let r = out.depth_requests()[0].clone();
    let issued = out.prepares.iter().find(|p| p.request.id == r.id);
    let before = store(&d);
    let failed = d.event(
        now,
        Event::HttpFailed {
            request: r.id,
            error: "fixture network loss",
        },
        false,
    );
    exact_cleanup(&failed, issued);
    assert_eq!(store(&d), before);
}
#[test]
fn nan_returned_depth_failure_only_cleans_issued_work() {
    nonfinite_returned_failure(f64::NAN);
}
#[test]
fn positive_infinity_returned_depth_failure_only_cleans_issued_work() {
    nonfinite_returned_failure(f64::INFINITY);
}
#[test]
fn negative_infinity_returned_depth_failure_only_cleans_issued_work() {
    nonfinite_returned_failure(f64::NEG_INFINITY);
}
#[test]
fn public_duplicate_opened_cannot_resubscribe_or_reset_native_quota() {
    let (mut d, socket) = public();
    let first = d.event(NOW + 1., Event::WsOpened { socket }, false);
    assert_eq!(
        first.sent(),
        [(
            socket,
            r#"{"method":"SUBSCRIBE","params":["BTCUSDT@ticker","ETHUSDT@ticker"],"id":1}"#
        )]
    );
    let before = store(&d);
    let duplicate = d.event(NOW + 2., Event::WsOpened { socket }, false);
    assert!(
        duplicate.actions.is_empty(),
        "duplicate Opened must not repeat native effects"
    );
    assert_eq!(store(&d), before);
}
#[test]
fn public_closed_epoch_text_open_and_close_are_inert() {
    let (mut d, socket) = public();
    let _ = d.event(NOW + 1., Event::WsOpened { socket }, false);
    let _ = d.event(NOW + 2., Event::WsClosed { socket, code: 1006 }, false);
    let before = store(&d);
    for event in [
        Event::WsMessage {
            socket,
            text: r#"{"e":"ticker","s":"BTCUSDT","d":[{"c":"999"}]}"#,
        },
        Event::WsOpened { socket },
        Event::WsClosed { socket, code: 1006 },
    ] {
        assert!(d.event(NOW + 3., event, false).actions.is_empty());
        assert_eq!(store(&d), before);
    }
}
#[test]
fn private_upgrade_keeps_exact_primary_provider_bytes_without_read_reselection() {
    let mut d = Driver::new(support::setup::engine(options(true, true)));
    let boot = bootstrap(&mut d);
    let socket = boot.socket(SocketChannel::Private);
    let headers = boot
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket: id,
                headers,
                ..
            } if *id == socket => Some(headers),
            _ => None,
        })
        .unwrap();
    let payload = format!("{NOW:.0}/v3/ws/private");
    let mac = safe_cex_crypto::hmac_sha256(
        PRIMARY_MATERIAL.as_bytes(),
        payload.as_bytes(),
        safe_cex_crypto::Encoding::Base64,
    );
    assert_eq!(
        *headers,
        vec![
            ("ACCESS-KEY", PRIMARY_KEY.into()),
            ("ACCESS-PASSPHRASE", PRIMARY_PASS.into()),
            ("ACCESS-TIMESTAMP", format!("{NOW:.0}")),
            ("ACCESS-SIGN", mac),
            ("Content-Type", "application/json".into()),
            ("User-Agent", "Tealstreet".into()),
        ]
    );
    let opened = d.event(NOW + 1., Event::WsOpened { socket }, false);
    assert_eq!(
        opened.sent(),
        [(
            socket,
            r#"{"method":"SUBSCRIBE","params":["account","positions","orders","fill"],"id":1}"#
        )]
    );
}
#[test]
fn private_duplicate_opened_never_repeats_native_subscription() {
    let mut d = Driver::new(support::setup::engine(options(true, true)));
    let boot = bootstrap(&mut d);
    let socket = boot.socket(SocketChannel::Private);
    let first = d.event(NOW + 1., Event::WsOpened { socket }, false);
    assert_eq!(first.sent().len(), 1);
    let before = store(&d);
    let duplicate = d.event(NOW + 2., Event::WsOpened { socket }, false);
    assert!(
        duplicate.actions.is_empty(),
        "private duplicate Opened repeats no frame/refresh"
    );
    assert_eq!(store(&d), before);
}
#[test]
fn disabled_public_feed_keeps_signed_private_connect_and_no_public_watchdog() {
    let mut e = support::setup::engine(options(true, true));
    e.set_public_feed_policy(PublicFeedPolicy::Disabled)
        .unwrap();
    let mut d = Driver::new(e);
    let boot = bootstrap(&mut d);
    assert!(!boot.actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Public),
            ..
        }
    )));
    let private = boot.socket(SocketChannel::Private);
    let opened = d.event(NOW + 1., Event::WsOpened { socket: private }, false);
    assert_eq!(opened.sent().len(), 1);
    assert!(opened.sent()[0].1.contains("account"));
}
#[test]
fn public_close_does_not_retire_private_ping_or_account_reducers() {
    let mut d = Driver::new(support::setup::engine(options(true, true)));
    let boot = bootstrap(&mut d);
    let public = boot.socket(SocketChannel::Public);
    let private = boot.socket(SocketChannel::Private);
    let _ = d.event(NOW + 1., Event::WsOpened { socket: private }, false);
    let _ = d.event(NOW + 2., Event::WsOpened { socket: public }, false);
    let _ = d.event(
        NOW + 3.,
        Event::WsClosed {
            socket: public,
            code: 1006,
        },
        false,
    );
    let pong = d.event(
        NOW + 4.,
        Event::WsMessage {
            socket: private,
            text: r#"{"type":"ping"}"#,
        },
        false,
    );
    assert_eq!(pong.sent(), [(private, r#"{"method":"PONG","id":2}"#)]);
}

struct Pressure {
    inner: WeexFutures,
    peers: Vec<Context>,
}
impl Venue for Pressure {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("fixture:fill") => {
                    for _ in 0..256 {
                        self.peers.push(
                            cx.http_context(
                                Scope::Background,
                                Policy {
                                    operation: Operation::Read,
                                    credential: None,
                                    timeout_ms: 100_000,
                                    pacing: None,
                                    retry: RetryPolicy::Never,
                                },
                            )
                            .expect("genuine Background peer admission"),
                        );
                    }
                    return;
                }
                Some("fixture:release") => {
                    for context in self.peers.drain(..) {
                        cx.finish_http_context(&context).unwrap();
                    }
                    return;
                }
                _ => {}
            }
        }
        self.inner.on_command(cx, command);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, text: &str) {
        self.inner.on_item(cx, p, text);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.inner.on_response(cx, p, r);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.inner.on_request_failed(cx, p, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}
#[test]
fn full_registry_refuses_depth_then_recovers_with_all_credits() {
    let e = support::setup::with_venue(options(false, false), |inner| Pressure {
        inner,
        peers: Vec::new(),
    });
    let mut d = Driver::new(e);
    let _ = bootstrap(&mut d);
    let control = |name: &str| {
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![format!("fixture:{name}")],
        })
    };
    let _ = d.event(NOW + 1., control("fill"), true);
    let refused = d.subscribe(NOW + 2., A, true);
    assert!(
        refused.depth_requests().is_empty(),
        "capacity denies raw Depth wire"
    );
    assert!(
        refused.prepares.is_empty(),
        "capacity denies Prepare before physical IDs"
    );
    let retry = refused
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms.is_finite() && *at_ms > NOW + 2. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native bounded book retry");
    let _ = d.event(NOW + 3., control("release"), true);
    let recovered = d.event(retry.1, Event::TimerFired { timer: retry.0 }, true);
    let p = prepared(&recovered);
    let r = d.release(retry.1, &p);
    let _ = d.response(retry.1, r.id, 200, DEPTH);
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
    let _ = d.event(retry.1 + 1., control("fill"), true);
    let _ = d.event(retry.1 + 2., control("release"), true);
}
