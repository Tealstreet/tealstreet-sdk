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
    fn managed_sockets() -> bool {
        true
    }
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

#[test]
fn mixed_depth_and_tickers_keep_native_book_first_wire_order() {
    let (mut d, socket) = public();
    let out = d.subscribe(NOW + 1., B, false);
    let id = out.depth_requests()[0].id;
    let accepted = d.response(NOW + 2., id, 200, DEPTH);
    assert!(accepted.sent().is_empty());
    let opened = d.event(NOW + 3., Event::WsOpened { socket }, false);
    assert_eq!(
        opened.sent(),
        [(
            socket,
            r#"{"method":"SUBSCRIBE","params":["ETHUSDT@depth200","BTCUSDT@ticker","ETHUSDT@ticker"],"id":1}"#
        )]
    );
}

struct RetirementPressure {
    inner: Pressure,
    foreign: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    public: Option<safe_cex_engine::socket_lifecycle::FeedId>,
    foreign_socket: Option<SocketId>,
    armed: bool,
    interest_pressure: bool,
}
impl RetirementPressure {
    fn fill_proposals(&self, cx: &mut Cx<'_, Pending>) {
        for _ in 0..512 {
            cx.set_socket_interests(self.foreign.unwrap(), Vec::new())
                .expect("genuine pending proposal credit");
        }
    }
}
impl Venue for RetirementPressure {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
        use safe_cex_engine::socket_lifecycle::*;
        self.foreign = Some(
            cx.register_socket_feed(FeedPolicy {
                role: SocketChannel::Private,
                supervision: safe_cex_engine::socket::Config::default(),
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::Native,
                invalidation: Invalidation::Required,
                subscription_interval_ms: 0.,
            })
            .expect("genuine foreign feed registration"),
        );
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("fixture:interest-pressure") => {
                    self.interest_pressure = true;
                    return;
                }
                Some("fixture:interest-release") => {
                    self.interest_pressure = false;
                    return;
                }
                _ => {}
            }
        }
        if matches!(&command, Command::SetTickerSymbols { symbols } if symbols.first().is_some_and(|s| s == "fixture:retirement-pressure"))
        {
            self.armed = true;
            return;
        }
        if self.interest_pressure {
            self.fill_proposals(cx);
        }
        self.inner.on_command(cx, command);
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        use safe_cex_engine::socket_lifecycle::*;
        let feed = match &event {
            LifecycleEvent::Opened(epoch)
            | LifecycleEvent::Retired { epoch, .. }
            | LifecycleEvent::Ready { epoch, .. }
            | LifecycleEvent::Ping(epoch) => Some(epoch.feed()),
            LifecycleEvent::InterestsCommitted { feed, .. }
            | LifecycleEvent::InterestsRefused { feed, .. } => Some(*feed),
            _ => None,
        };
        if feed == self.foreign {
            return Ok(());
        }
        if let LifecycleEvent::Opened(epoch) = event
            && self.public.is_none()
        {
            self.public = Some(epoch.feed());
            let foreign =
                cx.connect_socket(self.foreign.unwrap(), "wss://fixture-foreign.invalid")?;
            self.foreign_socket = Some(foreign.socket());
        }
        let pressure = matches!(&event, LifecycleEvent::Retired { epoch, .. } if Some(epoch.feed()) == self.public)
            && self.armed;
        if pressure {
            self.fill_proposals(cx);
        }
        let result = self.inner.inner.on_socket_event(cx, event);
        if pressure {
            assert_eq!(
                result,
                Err(SocketError::Capacity),
                "the original shared proposal cap is genuinely saturated"
            );
        }
        result
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.inner
            .inner
            .encode_socket_interests(store, feed, subscribe, items, emit)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if Some(socket) == self.foreign_socket {
            cx.send(socket, "fixture:foreign-positive");
        } else {
            self.inner.on_message(cx, socket, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.interest_pressure && cx.now.is_finite() {
            self.fill_proposals(cx);
        }
        self.inner.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}

#[test]
fn refused_public_retirement_cleans_depth_before_refusing_reconnect() {
    let e = support::setup::with_venue(options(true, true), |inner| RetirementPressure {
        inner: Pressure {
            inner,
            peers: Vec::new(),
        },
        foreign: None,
        public: None,
        foreign_socket: None,
        armed: false,
        interest_pressure: false,
    });
    let mut d = Driver::new(e);
    let boot = bootstrap(&mut d);
    let public = boot.socket(SocketChannel::Public);
    let private = boot.socket(SocketChannel::Private);
    for request in &boot.requests {
        let body = if request.url.contains("accountConfig") {
            r#"{"dualSidePosition":false,"canTrade":true}"#
        } else {
            "[]"
        };
        let _ = d.response(NOW, request.id, 200, body);
    }
    let opened = d.event(NOW + 1., Event::WsOpened { socket: public }, false);
    let foreign = opened.socket(SocketChannel::Private);
    let foreign_connect = opened
        .actions
        .iter()
        .position(|action| matches!(action, Action::WsConnect { socket, .. } if *socket == foreign))
        .unwrap();
    let foreign_timer = opened.actions[..foreign_connect]
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .expect("captured foreign connect watchdog");
    let public_timers: Vec<_> = opened
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1. + 5_000. => Some(*timer),
            _ => None,
        })
        .filter(|timer| *timer != foreign_timer && !opened.actions.iter().any(|action| matches!(action, Action::ClearTimer { timer: cleared } if cleared == timer)))
        .collect();
    assert_eq!(
        public_timers.len(),
        1,
        "one captured public watchdog; the captured foreign connect watchdog is unrelated"
    );
    let _ = d.event(NOW + 2., Event::WsOpened { socket: foreign }, false);
    let private_open = d.event(NOW + 3., Event::WsOpened { socket: private }, false);
    for request in &private_open.requests {
        let body = if request.url.contains("accountConfig") {
            r#"{"dualSidePosition":false,"canTrade":true}"#
        } else {
            "[]"
        };
        let _ = d.response(NOW + 3., request.id, 200, body);
    }
    let held = prepared(&d.subscribe(NOW + 4., A, true));
    let released = prepared(&d.subscribe(NOW + 5., B, true));
    let wire = d.release(NOW + 6., &released);
    let query = d.event(
        NOW + 6.,
        Event::Command(Command::FetchOrderBook {
            command: 700,
            symbol: A.into(),
            limit: Some(25),
        }),
        false,
    );
    let query_request = query.depth_requests()[0].clone();
    let _ = d.event(
        NOW + 6.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["fixture:retirement-pressure".into()],
        }),
        false,
    );
    let before = store(&d);
    let closed = d.event(
        NOW + 7.,
        Event::WsClosed {
            socket: public,
            code: 1006,
        },
        true,
    );
    assert!(
        !d.e.http_preparation_pending(held.token),
        "refused invalidation still retires the held Depth credit"
    );
    let mut timer_seen = Vec::new();
    let mut cancel_seen = Vec::new();
    let mut closes = 0;
    let mut refusals = 0;
    for action in &closed.actions {
        match action {
            Action::ClearTimer { timer }
                if public_timers.contains(timer)
                    || [held.timer, released.timer].contains(timer) =>
            {
                assert!(!timer_seen.contains(timer));
                timer_seen.push(*timer);
            }
            Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { preparation, signing: None } if [held.token, released.token].contains(preparation)) =>
            {
                let HttpAction::Cancel { preparation, .. } = a.as_ref() else {
                    unreachable!()
                };
                assert!(!cancel_seen.contains(preparation));
                cancel_seen.push(*preparation);
            }
            Action::WsClose { socket } if *socket == public => closes += 1,
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::SUBSCRIPTION_REJECTED
                    && event.message == "Managed socket invalidation refused: Capacity" =>
            {
                refusals += 1
            }
            _ => panic!("unowned retirement effect: {action:?}"),
        }
    }
    assert!(timer_seen.contains(&held.timer) && timer_seen.contains(&released.timer));
    assert!(public_timers.iter().all(|timer| timer_seen.contains(timer)));
    assert_eq!(timer_seen.len(), public_timers.len() + 2);
    assert_eq!(cancel_seen.len(), 2);
    assert_eq!((closes, refusals), (1, 1));
    assert_eq!(store(&d), before);
    assert!(d.e.book(A).is_none() && d.e.book(B).is_none());
    let refusal = closed.actions.iter().position(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message == "Managed socket invalidation refused: Capacity")).unwrap();
    assert!(
        closed
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, Action::ManagedHttp(_)))
            .all(|(at, _)| at < refusal)
    );
    for (id, status, body) in [
        (wire.id, 200, DEPTH),
        (wire.id, 429, r#"{"bad":"old"}"#),
        (held.request.id, 200, DEPTH),
    ] {
        assert!(d.response(NOW + 8., id, status, body).actions.is_empty());
        assert_eq!(store(&d), before);
        assert!(d.e.book(A).is_none() && d.e.book(B).is_none());
    }
    d.e.http_prepared(
        NOW + 8.,
        held.token,
        Prepared {
            id: held.request.id.0,
            expires_at: None,
        },
    );
    assert!(d.take(NOW + 8., false).actions.is_empty());
    let completed = d.response(NOW + 8., query_request.id, 200, DEPTH);
    assert!(completed.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { command: 700, result }) if result == r#"{"bids":[{"price":99.0,"amount":2.0,"total":2.0}],"asks":[{"price":101.0,"amount":1.0,"total":1.0}],"lastUpdateId":"160"}"#)));
    let pong = d.event(
        NOW + 9.,
        Event::WsMessage {
            socket: private,
            text: r#"{"type":"ping"}"#,
        },
        false,
    );
    assert_eq!(pong.sent(), [(private, r#"{"method":"PONG","id":2}"#)]);
    let peer = d.event(
        NOW + 9.,
        Event::WsMessage {
            socket: foreign,
            text: "fixture:probe",
        },
        false,
    );
    assert_eq!(peer.sent(), [(foreign, "fixture:foreign-positive")]);
    for name in ["fixture:fill", "fixture:release"] {
        let out = d.event(
            NOW + 10.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec![name.into()],
            }),
            true,
        );
        assert!(
            out.actions.is_empty(),
            "all256 Context credits remain reusable without public reconnect"
        );
    }
    assert!(
        d.event(
            NOW + 10_000.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec![A.into()]
            }),
            false
        )
        .actions
        .iter()
        .all(|a| !matches!(
            a,
            Action::WsConnect {
                channel: Some(SocketChannel::Public),
                ..
            }
        ))
    );
}

fn physical_depth_timeout(held: bool) {
    let (mut d, _) = public();
    let out = d.subscribe(NOW + 1., A, held);
    let issued = out.prepares[0].clone();
    assert_eq!(issued.deadline, NOW + 1. + LIMIT);
    assert_eq!(out.depth_requests().len(), usize::from(!held));
    let before = store(&d);
    let expired = d.event(
        issued.deadline,
        Event::TimerFired {
            timer: issued.timer,
        },
        true,
    );
    assert_eq!(store(&d), before);
    let mut cleanup = Trace::default();
    let mut retry = None;
    let mut diagnostics = 0;
    for action in &expired.actions {
        match action {
            Action::ClearTimer { .. } | Action::ManagedHttp(_) => {
                cleanup.actions.push(action.clone())
            }
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::REQUEST_FAILED
                    && event.message == "WEEX: HTTP request timed out" =>
            {
                diagnostics += 1
            }
            Action::SetTimer { timer, at_ms }
                if *at_ms >= issued.deadline + 750. && *at_ms <= issued.deadline + 1_250. =>
            {
                assert!(retry.replace((*timer, *at_ms)).is_none());
            }
            _ => panic!("unexpected physical-timeout effect: {action:?}"),
        }
    }
    exact_cleanup(&cleanup, Some(&issued));
    assert_eq!(diagnostics, 1);
    assert!(
        d.event(
            issued.deadline,
            Event::TimerFired {
                timer: issued.timer
            },
            true
        )
        .actions
        .is_empty()
    );
    d.e.http_prepared(
        issued.deadline,
        issued.token,
        Prepared {
            id: issued.request.id.0,
            expires_at: None,
        },
    );
    assert!(d.take(issued.deadline, true).actions.is_empty());
    let (timer, at) =
        retry.expect("one native finite backoff after retiring the expired observation");
    let fresh = prepared(&d.event(at, Event::TimerFired { timer }, true));
    assert_ne!(fresh.token, issued.token);
    let sent = d.release(at, &fresh);
    let _ = d.response(at, sent.id, 200, DEPTH);
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
}
#[test]
fn released_depth_physical_timeout_retires_then_recovers_once() {
    physical_depth_timeout(false);
}
#[test]
fn held_depth_physical_timeout_retires_then_recovers_once() {
    physical_depth_timeout(true);
}

fn invalid_preparation_failure(now: f64) {
    let (mut d, _) = public();
    let current = prepared(&d.subscribe(NOW + 1., A, true));
    let sibling = prepared(&d.subscribe(NOW + 2., B, true));
    let before = store(&d);
    d.e.http_preparation_failed(
        now,
        current.token,
        safe_cex_engine::request_context::Error::Invalid,
        safe_cex_engine::VenueError::new("invalid host observation")
            .kind(safe_cex_engine::ErrorKind::Internal),
    );
    let failed = d.take(now, true);
    exact_cleanup(&failed, Some(&current));
    assert_eq!(store(&d), before);
    assert!(!d.e.http_preparation_pending(current.token));
    assert!(d.e.http_preparation_pending(sibling.token));
    let sent = d.release(NOW + 3., &sibling);
    let _ = d.response(NOW + 3., sent.id, 200, DEPTH);
    assert_eq!(d.e.book(B).unwrap().bids(), [(99., 2.)]);
    let _ = d.unsubscribe(NOW + 4., A);
    let fresh = prepared(&d.subscribe(NOW + 5., A, true));
    assert_ne!(fresh.token, current.token);
    let sent = d.release(NOW + 5., &fresh);
    let _ = d.response(NOW + 5., sent.id, 200, DEPTH);
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
}
#[test]
fn nan_depth_preparation_failure_only_retires_its_exact_observation() {
    invalid_preparation_failure(f64::NAN);
}
#[test]
fn infinity_depth_preparation_failure_only_retires_its_exact_observation() {
    invalid_preparation_failure(f64::INFINITY);
}
#[test]
fn negative_infinity_depth_preparation_failure_only_retires_its_exact_observation() {
    invalid_preparation_failure(f64::NEG_INFINITY);
}

#[test]
fn retained_last_is_a_real_intervening_snapshot_and_reconnect_preserves_native_order() {
    let (mut d, socket) = public();
    let _ = d.event(NOW, Event::WsOpened { socket }, false);
    let initial = d.subscribe(NOW + 1., A, false).depth_requests()[0].clone();
    let failed = d.event(
        NOW + 2.,
        Event::HttpFailed {
            request: initial.id,
            error: "native depth loss",
        },
        false,
    );
    let book_retry = failed
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("real native book retry");
    let closed = d.event(NOW + 700., Event::WsClosed { socket, code: 1006 }, true);
    let socket_retry = closed
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("real native socket retry");
    assert!(book_retry.1 > NOW + 700. && book_retry.1 < socket_retry.1);
    let intervening = prepared(&d.event(
        book_retry.1,
        Event::TimerFired {
            timer: book_retry.0,
        },
        true,
    ));
    let sent = d.release(book_retry.1, &intervening);
    let accepted = d.response(book_retry.1, sent.id, 200, DEPTH);
    assert!(accepted.sent().is_empty());
    assert_eq!(d.e.book(A).unwrap().bids(), [(99., 2.)]);
    let reconnect = d.event(
        socket_retry.1,
        Event::TimerFired {
            timer: socket_retry.0,
        },
        true,
    );
    let next_socket = reconnect.socket(SocketChannel::Public);
    let next_snapshot = prepared(&reconnect);
    let connect_at = reconnect
        .actions
        .iter()
        .position(|a| matches!(a, Action::WsConnect { socket, .. } if *socket == next_socket))
        .unwrap();
    let prepare_at = reconnect.actions.iter().position(|a| matches!(a, Action::ManagedHttp(work) if matches!(work.as_ref(), HttpAction::Prepare { preparation, .. } if *preparation == next_snapshot.token))).unwrap();
    assert!(
        connect_at < prepare_at,
        "native reconnect queues connect before its pre-open snapshot"
    );
    assert_eq!(
        d.e.book(A).unwrap().bids(),
        [(99., 2.)],
        "connect adds no new clear-last gate"
    );
    let opened = d.event(
        socket_retry.1 + 1.,
        Event::WsOpened {
            socket: next_socket,
        },
        true,
    );
    assert!(
        opened
            .sent()
            .iter()
            .any(|(_, text)| text.contains("BTCUSDT@depth200")),
        "genuine intervening last remains topic-eligible before the replacement response"
    );
    assert!(d.e.http_preparation_pending(next_snapshot.token));
    let sent = d.release(socket_retry.1 + 2., &next_snapshot);
    let _ = d.response(
        socket_retry.1 + 2.,
        sent.id,
        200,
        r#"{"lastUpdateId":161,"bids":[["100","3"]],"asks":[]}"#,
    );
    assert_eq!(d.e.book(A).unwrap().bids(), [(100., 3.)]);
}

#[test]
fn gap_heal_observes_depth_without_new_topic_handoff() {
    let (mut d, socket) = public();
    let _ = d.event(NOW + 1., Event::WsOpened { socket }, false);
    let initial = d.subscribe(NOW + 2., A, false).depth_requests()[0].clone();
    let _ = d.response(NOW + 3., initial.id, 200, DEPTH);
    let gap = d.event(
        NOW + 4.,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":162,"u":162,"b":[["99","0"]],"a":[]}"#,
        },
        true,
    );
    let healing = prepared(&gap);
    assert!(gap.sent().is_empty());
    assert!(
        !gap.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsClose { .. }))
    );
    let sent = d.release(NOW + 5., &healing);
    let _ = d.response(
        NOW + 5.,
        sent.id,
        200,
        r#"{"lastUpdateId":161,"bids":[["99","2"]],"asks":[["101","1"]]}"#,
    );
    assert!(
        d.e.book(A).unwrap().bids().is_empty(),
        "the genuinely buffered gap delta joins the accepted snapshot"
    );
}

#[test]
fn malformed_public_text_keeps_native_any_text_liveness_and_ping_codec() {
    let (mut d, socket) = public();
    let opened = d.event(NOW + 1., Event::WsOpened { socket }, false);
    let mut watchdog = opened
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5_001. => Some(*timer),
            _ => None,
        })
        .unwrap();
    let before = store(&d);
    let malformed = d.event(NOW + 1_000., Event::WsMessage { socket, text: "{" }, false);
    assert_eq!(store(&d), before);
    assert!(malformed.actions.is_empty());
    for tick in 1..=6 {
        let at = NOW + 1. + f64::from(tick) * 5_000.;
        let out = d.event(at, Event::TimerFired { timer: watchdog }, false);
        assert!(
            !out.actions
                .iter()
                .any(|action| matches!(action, Action::WsClose { .. } | Action::WsConnect { .. }))
        );
        assert_eq!(
            out.sent(),
            [(
                socket,
                format!(r#"{{"method":"PING","id":{}}}"#, tick + 1).as_str()
            )]
        );
        let next = out
            .actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("actual next supervised watchdog");
        if tick == 6 {
            assert_eq!(
                next.1,
                NOW + 31_000.,
                "malformed text moved the idle bound beyond the original deadline"
            );
        }
        watchdog = next.0;
    }
    let pong = d.event(
        NOW + 30_002.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"ping"}"#,
        },
        false,
    );
    assert_eq!(pong.sent(), [(socket, r#"{"method":"PONG","id":8}"#)]);
}

#[test]
fn upgrade_header_incapability_preserves_native_rest_only_notice_and_reads() {
    let mut d = Driver::new(support::setup::engine(options(true, false)));
    let boot = bootstrap(&mut d);
    assert_eq!(boot.requests.len(), 6);
    assert!(!boot.actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        }
    )));
    assert!(boot.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code == safe_cex_engine::log::code::STREAM_UNAVAILABLE && event.message == "WEEX private WebSocket disabled: transport cannot attach upgrade headers; continuing REST safety loop")));
    assert!(boot.requests.iter().all(|request| {
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "ACCESS-SIGN" && !value.is_empty())
    }));
}

#[test]
fn paused_depth_has_no_context_until_the_actual_native_wake() {
    let e = support::setup::with_venue(options(false, false), |inner| Pressure {
        inner,
        peers: Vec::new(),
    });
    let mut d = Driver::new(e);
    let _ = bootstrap(&mut d);
    let sent = d.subscribe(NOW + 1., A, false).depth_requests()[0].clone();
    let _ = d.event(
        NOW + 2.,
        Event::HttpResponse {
            request: sent.id,
            response: Response {
                status: 429,
                headers: "retry-after: 2",
                body: "native rate limit",
            },
        },
        true,
    );
    let waiting = d.subscribe(NOW + 3., B, true);
    assert!(waiting.prepares.is_empty() && waiting.requests.is_empty());
    let wake = waiting
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2_002. => Some(*timer),
            _ => None,
        })
        .expect("actual native pause wake");
    for name in ["fixture:fill", "fixture:release"] {
        assert!(
            d.event(
                NOW + 4.,
                Event::Command(Command::SetTickerSymbols {
                    symbols: vec![name.into()]
                }),
                true
            )
            .actions
            .is_empty(),
            "all256 genuine credits prove the paused book holds no Context"
        );
    }
    let eligible = prepared(&d.event(NOW + 2_002., Event::TimerFired { timer: wake }, true));
    let sent = d.release(NOW + 2_002., &eligible);
    let _ = d.response(NOW + 2_002., sent.id, 200, DEPTH);
    assert_eq!(d.e.book(B).unwrap().bids(), [(99., 2.)]);
}

fn interest_pressure() -> (Driver<RetirementPressure>, SocketId, SocketId) {
    let e = support::setup::with_venue(options(false, false), |inner| RetirementPressure {
        inner: Pressure {
            inner,
            peers: Vec::new(),
        },
        foreign: None,
        public: None,
        foreign_socket: None,
        armed: false,
        interest_pressure: false,
    });
    let mut d = Driver::new(e);
    let socket = bootstrap(&mut d).socket(SocketChannel::Public);
    let foreign = d
        .event(NOW, Event::WsOpened { socket }, false)
        .socket(SocketChannel::Private);
    let _ = d.event(NOW + 1., Event::WsOpened { socket: foreign }, false);
    let _ = ticker_demand(&mut d, NOW + 2., vec!["fixture:interest-pressure"]);
    (d, socket, foreign)
}
fn ticker_demand<V: Venue>(d: &mut Driver<V>, now: f64, symbols: Vec<&str>) -> Trace {
    d.event(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: symbols.into_iter().map(str::to_owned).collect(),
        }),
        false,
    )
}
fn sole_timer(out: &Trace) -> (TimerId, f64) {
    let timers: Vec<_> = out
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        timers.len(),
        1,
        "one actual observation timer: {:?}",
        out.actions
    );
    timers[0]
}
#[test]
fn interest_pressure_obeys_cooldown_three_observations_and_resolved_demand_recovery() {
    let (mut d, socket, foreign) = interest_pressure();
    let initial = ticker_demand(&mut d, NOW + 3., vec![A]);
    assert!(initial.sent().is_empty());
    let (mut timer, mut bound) = sole_timer(&initial);
    assert_eq!(bound, NOW + 1_003.);
    for _ in 0..8 {
        assert!(
            ticker_demand(&mut d, NOW + 3., vec![A]).actions.is_empty(),
            "same-clock commits cannot bypass pending cooldown"
        );
    }
    let early = d.event(NOW + 4., Event::TimerFired { timer }, false);
    let (replacement, same_bound) = sole_timer(&early);
    assert_ne!(replacement, timer);
    assert_eq!(same_bound, bound);
    assert!(early.sent().is_empty());
    assert!(
        d.event(NOW + 4., Event::TimerFired { timer }, false)
            .actions
            .is_empty()
    );
    timer = replacement;
    for observation in 1..=3 {
        let now = bound + if observation == 2 { 1. } else { 0. };
        let observed = d.event(now, Event::TimerFired { timer }, false);
        assert!(observed.sent().is_empty());
        assert!(
            d.event(now, Event::TimerFired { timer }, false)
                .actions
                .is_empty()
        );
        if observation < 3 {
            let next = sole_timer(&observed);
            assert_eq!(next.1, now + 1_000.);
            timer = next.0;
            bound = next.1;
        } else {
            assert!(
                !observed
                    .actions
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer { .. }))
            );
        }
    }
    let now = bound + 1.;
    let _ = ticker_demand(&mut d, now, vec!["fixture:interest-release"]);
    assert!(
        ticker_demand(&mut d, now, vec![A]).actions.is_empty(),
        "pressure release does not renew the exhausted epoch budget"
    );
    assert!(
        ticker_demand(&mut d, now, Vec::new()).actions.is_empty(),
        "returning to the committed native demand resolves the observation"
    );
    let accepted = ticker_demand(&mut d, now + 1., vec![A]);
    assert_eq!(
        accepted.sent(),
        [(
            socket,
            r#"{"method":"UNSUBSCRIBE","params":["ETHUSDT@ticker"],"id":2}"#
        )]
    );
    let peer = d.event(
        now + 2.,
        Event::WsMessage {
            socket: foreign,
            text: "fixture:probe",
        },
        false,
    );
    assert_eq!(peer.sent(), [(foreign, "fixture:foreign-positive")]);
}
fn nonfinite_interest_observation(now: f64) {
    let (mut d, socket, foreign) = interest_pressure();
    let initial = ticker_demand(&mut d, NOW + 3., vec![A]);
    let (timer, bound) = sole_timer(&initial);
    let before = store(&d);
    assert!(
        d.event(now, Event::TimerFired { timer }, false)
            .actions
            .is_empty()
    );
    assert_eq!(store(&d), before);
    let _ = ticker_demand(&mut d, NOW + 4., vec!["fixture:interest-release"]);
    let resumed = ticker_demand(&mut d, NOW + 5., vec![A]);
    assert!(
        resumed.sent().is_empty(),
        "a nonfinite timer cannot erase the retained not-before bound"
    );
    let (next, at) = sole_timer(&resumed);
    assert_ne!(next, timer);
    assert_eq!(at, bound);
    assert!(
        d.event(NOW + 5., Event::TimerFired { timer }, false)
            .actions
            .is_empty()
    );
    let accepted = d.event(bound, Event::TimerFired { timer: next }, false);
    assert_eq!(
        accepted.sent(),
        [(
            socket,
            r#"{"method":"UNSUBSCRIBE","params":["ETHUSDT@ticker"],"id":2}"#
        )]
    );
    let peer = d.event(
        bound + 1.,
        Event::WsMessage {
            socket: foreign,
            text: "fixture:probe",
        },
        false,
    );
    assert_eq!(peer.sent(), [(foreign, "fixture:foreign-positive")]);
}
#[test]
fn nan_interest_observation_retains_cooldown_without_new_work() {
    nonfinite_interest_observation(f64::NAN);
}
#[test]
fn infinity_interest_observation_retains_cooldown_without_new_work() {
    nonfinite_interest_observation(f64::INFINITY);
}
#[test]
fn negative_infinity_interest_observation_retains_cooldown_without_new_work() {
    nonfinite_interest_observation(f64::NEG_INFINITY);
}

#[test]
fn hourly_quota_refusal_preserves_wire_ids_and_recovers_at_its_real_floor() {
    let (mut d, socket) = public();
    let _ = d.event(NOW, Event::WsOpened { socket }, false);
    let first = ticker_demand(&mut d, NOW + 1., vec![A]);
    assert_eq!(first.sent().len(), 1);
    let mut current = A;
    for step in 0..119 {
        current = if current == A { B } else { A };
        let accepted = ticker_demand(&mut d, NOW + 2. + f64::from(step), vec![current]);
        assert_eq!(
            accepted.sent().len(),
            2,
            "two genuine native subscription operations"
        );
        let last: serde_json::Value = serde_json::from_str(accepted.sent()[1].1).unwrap();
        assert_eq!(last["id"], 4 + step * 2);
    }
    let desired = if current == A { B } else { A };
    let refused = ticker_demand(&mut d, NOW + 121., vec![desired]);
    assert!(refused.sent().is_empty());
    assert!(refused.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message == "WEEX subscription operation limit reached")));
    let (timer, bound) = sole_timer(&refused);
    assert_eq!(bound, NOW + 3_600_000.);
    let early = d.event(NOW + 122., Event::TimerFired { timer }, false);
    let (mut timer, at) = sole_timer(&early);
    assert_eq!(at, bound);
    assert!(early.sent().is_empty());
    let before = store(&d);
    for step in 1..180 {
        let observed = d.event(
            NOW + f64::from(step) * 20_000.,
            Event::WsMessage { socket, text: "{" },
            false,
        );
        assert!(observed.sent().is_empty());
    }
    assert_eq!(store(&d), before);
    let first_floor = d.event(bound, Event::TimerFired { timer }, false);
    assert!(
        first_floor.sent().is_empty(),
        "one freed operation cannot partially commit the two-operation replacement"
    );
    let (next, at) = sole_timer(&first_floor);
    assert!(at > bound && at <= bound + 1_000.);
    timer = next;
    let recovered = d.event(at, Event::TimerFired { timer }, false);
    assert_eq!(recovered.sent().len(), 2);
    let messages: Vec<serde_json::Value> = recovered
        .sent()
        .iter()
        .map(|(_, text)| serde_json::from_str(text).unwrap())
        .collect();
    assert_eq!(
        messages[0],
        serde_json::json!({"method":"UNSUBSCRIBE","params":[format!("{current}@ticker")],"id":241})
    );
    assert_eq!(
        messages[1],
        serde_json::json!({"method":"SUBSCRIBE","params":[format!("{desired}@ticker")],"id":242})
    );
    assert_eq!(store(&d), before);
}

fn frame_limit_fixture(bytes: usize) -> (Driver<WeexFutures>, SocketId, Vec<String>, String) {
    let mut symbols: Vec<_> = (0..50)
        .map(|i| format!("{i:02}{}", "\\".repeat(600)))
        .collect();
    let frame = |symbols: &[String]| {
        let params: Vec<_> = symbols.iter().map(|s| format!("{s}@ticker")).collect();
        format!(
            r#"{{"method":"SUBSCRIBE","params":{},"id":3}}"#,
            serde_json::to_string(&params).unwrap()
        )
    };
    let extra = bytes
        .checked_sub(frame(&symbols).len())
        .expect("base frame is below the boundary");
    for (i, symbol) in symbols.iter_mut().enumerate() {
        symbol.extend(std::iter::repeat_n(
            'x',
            extra / 50 + usize::from(i < extra % 50),
        ));
        assert!(symbol.len() + "@ticker".len() <= 1_024);
    }
    let expected = frame(&symbols);
    assert_eq!(expected.len(), bytes);
    let rows: Vec<_> = [A.to_owned(), B.to_owned()].into_iter().chain(symbols.iter().cloned()).map(|symbol| serde_json::json!({"symbol":symbol,"baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":3})).collect();
    let info = serde_json::json!({"symbols":rows}).to_string();
    let mut d = Driver::new(support::setup::engine(options(false, false)));
    d.e.start(NOW);
    let initial = d.take(NOW, false);
    let tickers = d.response(NOW, request(&initial, "exchangeInfo"), 200, &info);
    let _ = d.response(NOW, request(&tickers, "24hr"), 200, DAILY);
    let boot = d.response(NOW, request(&tickers, "bookTicker"), 200, TICKERS);
    let socket = boot.socket(SocketChannel::Public);
    let _ = ticker_demand(&mut d, NOW, vec![A]);
    let opened = d.event(NOW, Event::WsOpened { socket }, false);
    assert_eq!(
        opened.sent(),
        [(
            socket,
            r#"{"method":"SUBSCRIBE","params":["BTCUSDT@ticker"],"id":1}"#
        )]
    );
    (d, socket, symbols, expected)
}
#[test]
fn fifty_valid_interest_keys_accept_the_exact_native_frame_boundary() {
    let (mut d, socket, symbols, expected) = frame_limit_fixture(65_536);
    let accepted = ticker_demand(
        &mut d,
        NOW + 1.,
        symbols.iter().map(String::as_str).collect(),
    );
    assert_eq!(
        accepted.sent(),
        [
            (
                socket,
                r#"{"method":"UNSUBSCRIBE","params":["BTCUSDT@ticker"],"id":2}"#
            ),
            (socket, expected.as_str())
        ]
    );
}
#[test]
fn oversized_encoded_frame_refuses_atomically_and_preserves_healthy_peer_and_ids() {
    let (mut d, socket, symbols, _) = frame_limit_fixture(65_537);
    let before = store(&d);
    let refused = ticker_demand(
        &mut d,
        NOW + 1.,
        symbols.iter().map(String::as_str).collect(),
    );
    assert!(
        refused.sent().is_empty(),
        "serialized frame capacity cannot partially unsubscribe the healthy peer"
    );
    let (timer, _) = sole_timer(&refused);
    assert_eq!(store(&d), before);
    let resolved = ticker_demand(&mut d, NOW + 2., vec![A]);
    assert!(resolved.sent().is_empty());
    assert!(
        resolved
            .actions
            .iter()
            .any(|action| matches!(action, Action::ClearTimer { timer: old } if *old == timer))
    );
    assert!(
        d.event(NOW + 2., Event::TimerFired { timer }, false)
            .actions
            .is_empty()
    );
    let recovered = ticker_demand(&mut d, NOW + 3., vec![B]);
    assert_eq!(
        recovered.sent(),
        [
            (
                socket,
                r#"{"method":"UNSUBSCRIBE","params":["BTCUSDT@ticker"],"id":2}"#
            ),
            (
                socket,
                r#"{"method":"SUBSCRIBE","params":["ETHUSDT@ticker"],"id":3}"#
            )
        ]
    );
    let pong = d.event(
        NOW + 4.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"ping"}"#,
        },
        false,
    );
    assert_eq!(pong.sent(), [(socket, r#"{"method":"PONG","id":4}"#)]);
    assert_eq!(store(&d), before);
}

#[test]
fn removing_multiple_tickers_preserves_their_committed_native_wire_order() {
    let mut info: serde_json::Value = serde_json::from_str(INFO).unwrap();
    info["symbols"].as_array_mut().unwrap().reverse();
    let mut d = Driver::new(support::setup::engine(options(false, false)));
    d.e.start(NOW);
    let initial = d.take(NOW, false);
    let tickers = d.response(
        NOW,
        request(&initial, "exchangeInfo"),
        200,
        &info.to_string(),
    );
    let _ = d.response(NOW, request(&tickers, "24hr"), 200, DAILY);
    let boot = d.response(NOW, request(&tickers, "bookTicker"), 200, TICKERS);
    assert_eq!(
        d.e.store()
            .markets()
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        [B, A]
    );
    let socket = boot.socket(SocketChannel::Public);
    let opened = d.event(NOW, Event::WsOpened { socket }, false);
    assert_eq!(
        opened.sent(),
        [(
            socket,
            r#"{"method":"SUBSCRIBE","params":["ETHUSDT@ticker","BTCUSDT@ticker"],"id":1}"#
        )]
    );
    let removed = ticker_demand(&mut d, NOW + 1., vec!["unavailable-native-symbol"]);
    assert_eq!(
        removed.sent(),
        [(
            socket,
            r#"{"method":"UNSUBSCRIBE","params":["ETHUSDT@ticker","BTCUSDT@ticker"],"id":2}"#
        )]
    );
}
