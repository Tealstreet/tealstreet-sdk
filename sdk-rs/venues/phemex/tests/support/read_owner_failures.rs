use super::*;
use safe_cex_engine::request_context::{Error, Preparation, Prepared, RoutingIntent};
use safe_cex_engine::signing::{SignError, SignFuture, SignRequest, Signer};
use safe_cex_engine::{TimerId, VenueError};

fn token(actions: &[Action], path: &str) -> Preparation {
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains(path) => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real shared preparation")
}
fn poll(actions: &[Action], now: f64) -> (TimerId, f64) {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > now && *at_ms < now + 10_000. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("native retry/poll")
}
fn identity_failure(actions: &[Action], kind: safe_cex_engine::ErrorKind) {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 42, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(kind));
    assert!(requests(actions).is_empty());
}
pub(super) struct Gated {
    pub(super) inner: safe_cex_engine::signing::Provider,
    pub(super) paused: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Signer for Gated {
    fn immediate(&self) -> bool {
        !self.paused.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        if self.immediate() {
            self.inner.sign(request)
        } else {
            Box::pin(std::future::pending())
        }
    }
    fn sign_ready(
        &self,
        request: &SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, SignError>> {
        if self.immediate() {
            self.inner.sign_ready(request)
        } else {
            None
        }
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algo: safe_cex_engine::signing::Algorithm,
    ) -> safe_cex_engine::signing::Capability {
        self.inner.can_sign(key, algo)
    }
}
fn account_poll(a: &[Action], now: f64) -> (TimerId, f64) {
    let filtered: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= now + 1500. && *at_ms < now + 10_000. =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(filtered.len(), 1, "one native account poll: {filtered:?}");
    filtered[0]
}
struct Parked;
impl Signer for Parked {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}
fn sign_attempt(e: &mut Engine<PhemexPerpetuals>, now: f64, preparation: Preparation) -> RequestId {
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, .. } => Some(attempt),
            _ => None,
        })
        .expect("real parked provider attempt")
}

#[test]
fn identity_preparation_refusal_releases_root_and_can_reuse_command_and_credits() {
    let (mut e, a) = identity_with(false, false);
    let p = token(&a, "accountPositions");
    e.http_preparation_failed(
        NOW,
        p,
        Error::Invalid,
        VenueError::new("fixture route refusal").kind(safe_cex_engine::ErrorKind::InvalidInput),
    );
    let a: Vec<_> = e.drain_actions().collect();
    identity_failure(&a, safe_cex_engine::ErrorKind::InvalidInput);
    for _ in 0..300 {
        e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let r = request(&a, "accountPositions");
        response(
            &mut e,
            NOW,
            r.id,
            200,
            r#"{"code":0,"data":{"account":{"userID":9007199254740993}}}"#,
        );
        let a: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            a.iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 42, .. })))
                .count(),
            1
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}
#[test]
fn parked_identity_provider_refusal_and_timeout_each_settle_once_then_recover() {
    for expire in [false, true] {
        let (mut e, a) = identity_with(false, false);
        let local = e.signer();
        e.set_signer(Some(std::sync::Arc::new(Parked))).unwrap();
        let p = token(&a, "accountPositions");
        let attempt = sign_attempt(&mut e, NOW, p);
        let at = if expire { NOW + 20_000. } else { NOW + 1. };
        e.handle(
            at,
            Event::Signed {
                attempt,
                result: Err(SignError::Refused),
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        identity_failure(
            &a,
            if expire {
                safe_cex_engine::ErrorKind::Timeout
            } else {
                safe_cex_engine::ErrorKind::InvalidInput
            },
        );
        e.handle(
            at + 1.,
            Event::Signed {
                attempt,
                result: Err(SignError::Refused),
            },
        );
        assert!(e.drain_actions().next().is_none());
        e.set_signer(local).unwrap();
        e.handle(
            at + 2.,
            Event::Command(Command::FetchAccount { command: 42 }),
        );
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, at + 2.);
        let r = request(&a, "accountPositions");
        response(
            &mut e,
            at + 3.,
            r.id,
            200,
            r#"{"code":0,"data":{"account":{"userID":"recovered"}}}"#,
        );
        assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone {command:42,result}) if result.contains("recovered"))));
    }
}
#[test]
fn one_currency_preparation_refusal_preserves_sibling_commit_and_next_cycle() {
    for first in ["USDT", "USDC"] {
        let (mut e, a, _) = started_with(false);
        let p = token(&a, &format!("currency={first}"));
        e.http_preparation_failed(
            NOW,
            p,
            Error::Invalid,
            VenueError::new("fixture route refusal"),
        );
        let mut out: Vec<_> = e.drain_actions().collect();
        out.extend(release(&mut e, NOW, a));
        let other = if first == "USDT" { "USDC" } else { "USDT" };
        let r = request(&out, &format!("currency={other}"));
        response(
            &mut e,
            NOW + 1.,
            r.id,
            200,
            &envelope(other, 200., 19., if other == "USDC" { ETH } else { BTC }),
        );
        assert_eq!(e.store().positions().len(), 1);
        let a: Vec<_> = e.drain_actions().collect();
        let (timer, at) = poll(&a, NOW + 1.);
        e.handle(at, Event::TimerFired { timer });
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        assert_eq!(
            requests(&a)
                .iter()
                .filter(|r| r.url.contains("accountPositions"))
                .count(),
            2
        );
    }
}
#[test]
fn both_currency_physical_timeouts_release_once_and_old_host_cannot_touch_new_cycle() {
    for reverse in [false, true] {
        let (mut e, a, _) = started_with(false);
        let mut ps = [token(&a, "currency=USDT"), token(&a, "currency=USDC")];
        if reverse {
            ps.reverse();
        }
        let mut out = Vec::new();
        for p in ps {
            e.http_prepared(
                NOW + 20_000.,
                p,
                Prepared {
                    id: p.attempt().request.0,
                    expires_at: None,
                },
            );
            out.extend(e.drain_actions());
        }
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().balance().total, 0.);
        assert!(requests(&out).is_empty());
        let (timer, at) = account_poll(&out, NOW + 20_000.);
        e.handle(at, Event::TimerFired { timer });
        let fresh = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        let a = request(&fresh, "currency=USDT");
        let b = request(&fresh, "currency=USDC");
        for p in ps {
            e.http_preparation_failed(at + 1., p, Error::Invalid, VenueError::new("old failure"));
        }
        assert!(e.drain_actions().next().is_none());
        response(
            &mut e,
            at + 2.,
            a.id,
            200,
            &envelope("USDT", 1000., 69., BTC),
        );
        response(
            &mut e,
            at + 3.,
            b.id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        assert_eq!(e.store().positions().len(), 2);
        assert_eq!(e.store().balance().total, 1200.);
    }
}
#[test]
fn parked_currency_provider_refusal_releases_both_slots_and_recovery_is_live() {
    for expire in [false, true] {
        let gate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (mut e, a, _) = started_config(false, Some(gate.clone()));
        gate.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut pending = Vec::new();
        for name in ["USDT", "USDC"] {
            let p = token(&a, &format!("currency={name}"));
            e.http_prepared(
                NOW,
                p,
                Prepared {
                    id: p.attempt().request.0,
                    expires_at: None,
                },
            );
            pending.extend(e.drain_actions());
        }
        let attempts: Vec<_> = pending
            .iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .collect();
        assert_eq!(attempts.len(), 2);
        let failed_at = if expire { NOW + 20_000. } else { NOW + 1. };
        let mut out = Vec::new();
        for attempt in &attempts {
            e.handle(
                failed_at,
                Event::Signed {
                    attempt: *attempt,
                    result: Err(SignError::Refused),
                },
            );
            out.extend(e.drain_actions());
        }
        assert!(
            requests(&out)
                .iter()
                .all(|r| !r.url.contains("accountPositions"))
        );
        assert!(e.store().positions().is_empty());
        let (timer, at) = account_poll(&out, failed_at);
        gate.store(false, std::sync::atomic::Ordering::SeqCst);
        for action in &out {
            if let Action::Sign { attempt, work, .. } = action {
                let result = e
                    .signer()
                    .unwrap()
                    .sign_ready(work)
                    .expect("unparked trusted provider");
                e.handle(
                    NOW + 2.,
                    Event::Signed {
                        attempt: *attempt,
                        result,
                    },
                );
            }
        }
        e.drain_actions().for_each(drop);
        e.handle(at, Event::TimerFired { timer });
        let fresh = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        assert_eq!(
            requests(&fresh)
                .iter()
                .filter(|r| r.url.contains("accountPositions"))
                .count(),
            2
        );
        for attempt in attempts {
            e.handle(
                at + 1.,
                Event::Signed {
                    attempt,
                    result: Err(SignError::Refused),
                },
            );
        }
        assert!(e.drain_actions().next().is_none());
        response(
            &mut e,
            at + 2.,
            request(&fresh, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., BTC),
        );
        response(
            &mut e,
            at + 3.,
            request(&fresh, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        assert_eq!(e.store().positions().len(), 2);
    }
}
#[test]
fn reconciliation_preparation_refusal_and_parked_provider_failure_retry_on_primary() {
    for (parked, expire) in [(false, false), (true, false), (true, true)] {
        let gate = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (mut e, a, wake) = uncertain_config(false, Some(gate.clone()));
        let p = token(&a, "activeList");
        let failed_at = if expire { wake + 20_000. } else { wake + 1. };
        if parked {
            gate.store(true, std::sync::atomic::Ordering::SeqCst);
            let attempt = sign_attempt(&mut e, wake, p);
            e.handle(
                failed_at,
                Event::Signed {
                    attempt,
                    result: Err(SignError::Refused),
                },
            );
        } else {
            e.http_preparation_failed(
                wake + 1.,
                p,
                Error::Invalid,
                VenueError::new("route failed"),
            );
        }
        let a: Vec<_> = e.drain_actions().collect();
        assert!(e.store().orders().is_empty());
        let (timer, at) = poll(&a, failed_at);
        gate.store(false, std::sync::atomic::Ordering::SeqCst);
        e.handle(at, Event::TimerFired { timer });
        let a: Vec<_> = e.drain_actions().collect();
        assert!(a.iter().any(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare {intent:RoutingIntent::Primary,request,..} if request.url.contains("activeList")))));
        let fresh = release(&mut e, at, a);
        let r = request(&fresh, "activeList");
        assert_ne!(r.id, p.attempt().request);
        e.http_preparation_failed(at + 1., p, Error::Invalid, VenueError::new("stale refusal"));
        assert!(e.drain_actions().next().is_none());
        response(&mut e, at + 2., r.id, 200, revealed());
        assert!(e.store().order("revealed").is_some());
    }
}

use safe_cex_engine::request_context::{
    Context as HttpContext, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, Venue};
use safe_cex_venue_phemex::Pending;
use std::sync::{Arc, Mutex};
struct Pressure {
    venue: PhemexPerpetuals,
    held: Vec<HttpContext>,
    requests: Arc<Mutex<Vec<RequestId>>>,
    identity: safe_cex_engine::signing::Identity,
    paused: Arc<std::sync::atomic::AtomicBool>,
}
impl Venue for Pressure {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        self.venue.capture_http_reconciliation(pending)
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.venue.on_http_owner_retired(owner);
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        pending: Vec<Pending>,
    ) {
        self.venue.read_root_retired(owner, pending);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: safe_cex_engine::Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response);
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.venue.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.venue.fence_uncertain_cancel_all()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            if let Some(count) = symbols
                .first()
                .and_then(|s| s.strip_prefix("peer-contexts:"))
                .map(|s| s.parse::<usize>().unwrap())
            {
                for _ in 0..count {
                    self.held.push(
                        cx.http_context(
                            Scope::Background,
                            Policy {
                                operation: Operation::Read,
                                credential: None,
                                timeout_ms: 120_000,
                                pacing: None,
                                retry: RetryPolicy::Never,
                            },
                        )
                        .unwrap(),
                    );
                }
                return;
            }
            if symbols == &["release-contexts"] {
                self.paused
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                for context in self.held.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
                return;
            }
            if symbols == &["fill-http"] {
                self.paused.store(true, std::sync::atomic::Ordering::SeqCst);
                let sentinel = cx
                    .http_context(
                        Scope::Background,
                        Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 120_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                cx.finish_http_context(&sentinel).unwrap();
                for _ in 0..256 {
                    let id = cx.request(
                        Method::Get,
                        "https://api.phemex.com/peer".into(),
                        vec![],
                        None,
                        Pending::PublicCandles {
                            context: sentinel.clone(),
                            rows: vec![],
                            invalid: true,
                        },
                    );
                    cx.sign_http(
                        id,
                        Box::new(safe_cex_venue_phemex::dispatch_signing::Http::new(
                            self.identity.clone(),
                            false,
                            Method::Get,
                            "/peer".into(),
                            String::new(),
                            None,
                        )),
                    );
                    self.requests.lock().unwrap().push(id);
                }
                return;
            }
        }
        self.venue.on_command(cx, command);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        if !matches!(p, Pending::PublicCandles { invalid: true, .. }) {
            self.venue.on_response(cx, p, r);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        self.venue.on_item(cx, p, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        if !matches!(p, Pending::PublicCandles { invalid: true, .. }) {
            self.venue.on_request_failed(cx, p, error);
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, p, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        if !matches!(p, Pending::PublicCandles { invalid: true, .. }) {
            self.venue.on_sign_error(cx, p, error);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        self.venue.on_timer(cx, t);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.venue.on_open(cx, s);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, c: u16) {
        self.venue.on_close(cx, s, c);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, text: &str) {
        self.venue.on_message(cx, s, text);
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue.on_socket_event(cx, event)
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        auth: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.venue.socket_control(store, feed, auth, text)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue
            .encode_socket_interests(store, feed, subscribe, interests, emit)
    }
    fn on_signing_release(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        r: safe_cex_engine::SigningRelease,
    ) {
        self.venue.on_signing_release(target, r);
    }
    fn on_signing_receipt(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        r: safe_cex_engine::signing::SignReceipt,
    ) {
        self.venue.on_signing_receipt(target, r);
    }
    fn book(&self, s: &str) -> Option<&safe_cex_engine::book::Book> {
        self.venue.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.venue.books()
    }
}
fn pressure_command(e: &mut Engine<Pressure>, now: f64, value: &str) {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![value.into()],
        }),
    );
}
fn pressure_answer(e: &mut Engine<Pressure>, now: f64, id: RequestId, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn pressure() -> (
    Engine<Pressure>,
    Arc<Mutex<Vec<RequestId>>>,
    TimerId,
    SocketId,
) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let (local, refs) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("key".into()),
            secret: Secret::new("secret".into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    let identity = refs.primary.unwrap();
    let venue = PhemexPerpetuals::new(safe_cex_venue_phemex::Options {
        credentials: Some(identity.clone()),
        ..Default::default()
    });
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signer: safe_cex_engine::signing::Provider = Arc::new(Gated {
        inner: Arc::new(local),
        paused: paused.clone(),
    });
    let ids = Arc::new(Mutex::new(vec![]));
    let mut e = safe_cex_engine::test_support::seeded_engine(Pressure {
        venue,
        held: vec![],
        requests: ids.clone(),
        identity,
        paused,
    });
    e.set_signer(Some(signer)).unwrap();
    e.start(NOW);
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let product = request(&a, "products");
    let a = pressure_answer(&mut e, NOW, product.id, PRODUCTS);
    let socket = a
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .expect("native private socket");
    let ticker = request(&a, "24hr/all");
    pressure_answer(&mut e, NOW, ticker.id, DAILY);
    pressure_answer(
        &mut e,
        NOW,
        request(&a, "currency=USDT").id,
        &envelope("USDT", 1000., 69., BTC),
    );
    let out = pressure_answer(
        &mut e,
        NOW,
        request(&a, "currency=USDC").id,
        &envelope("USDC", 200., 19., ETH),
    );
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    (e, ids, timer, socket)
}
#[test]
fn partial_context_admission_and_real_queue_refusal_release_slots_then_poll_recovers() {
    for queue_full in [false, true] {
        let (mut e, ids, timer, _) = pressure();
        let before = e.account_snapshot_frame(NOW + 2000.).to_vec();
        pressure_command(
            &mut e,
            NOW + 1.,
            if queue_full {
                "fill-http"
            } else {
                "peer-contexts:255"
            },
        );
        let held_actions: Vec<_> = e.drain_actions().collect();
        let mut peer_attempts: std::collections::VecDeque<_> = held_actions
            .iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .collect();
        if queue_full {
            let attempts: Vec<_> = held_actions
                .iter()
                .filter_map(|a| match a {
                    Action::Sign { attempt, .. } => Some(*attempt),
                    _ => None,
                })
                .collect();
            assert_eq!(
                attempts.len(),
                1,
                "shared signer FIFO announces its parked head only"
            );
        }
        e.handle(NOW + 2000., Event::TimerFired { timer });
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 2000.);
        let reads: Vec<_> = requests(&out)
            .into_iter()
            .filter(|r| r.url.contains("accountPositions"))
            .collect();
        assert_eq!(reads.len(), usize::from(!queue_full));
        assert_eq!(e.account_snapshot_frame(NOW + 2000.), before);
        let mut output = out;
        if !queue_full {
            output.extend(pressure_answer(
                &mut e,
                NOW + 2001.,
                reads[0].id,
                &envelope("USDT", 1000., 69., BTC),
            ));
        }
        let (retry, at) = account_poll(&output, NOW + 2001.);
        if queue_full {
            assert_eq!(
                ids.lock().unwrap().len(),
                256,
                "real peer requests fill the signing queue"
            );
            let mut retired = 0;
            while let Some(attempt) = peer_attempts.pop_front() {
                e.handle(
                    NOW + 2002.,
                    Event::Signed {
                        attempt,
                        result: Err(SignError::Refused),
                    },
                );
                for action in e.drain_actions() {
                    if let Action::Sign { attempt, .. } = action {
                        peer_attempts.push_back(attempt);
                    }
                }
                retired += 1;
                assert!(retired <= 256);
            }
            assert_eq!(
                retired, 256,
                "each real queued provider attempt retired once"
            );
        }
        pressure_command(&mut e, NOW + 2002., "release-contexts");
        e.drain_actions().for_each(drop);
        e.handle(at, Event::TimerFired { timer: retry });
        let fresh = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        assert_eq!(
            requests(&fresh)
                .iter()
                .filter(|r| r.url.contains("accountPositions"))
                .count(),
            2
        );
        pressure_answer(
            &mut e,
            at + 1.,
            request(&fresh, "currency=USDT").id,
            &envelope("USDT", 1000., 69., BTC),
        );
        pressure_answer(
            &mut e,
            at + 2.,
            request(&fresh, "currency=USDC").id,
            &envelope("USDC", 200., 19., ETH),
        );
        pressure_command(&mut e, at + 3., "peer-contexts:256");
        pressure_command(&mut e, at + 3., "release-contexts");
        assert_eq!(e.store().balance().total, 1090.);
    }
}

#[test]
fn reconciliation_capacity_refusal_releases_reading_and_recovers_on_primary() {
    for queue_full in [false, true] {
        let (mut e, _, _, socket) = pressure();
        e.handle(NOW, Event::WsOpened { socket });
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        e.handle(
            NOW,
            Event::WsMessage {
                socket,
                text: r#"{"id":999,"result":{"status":"success"}}"#,
            },
        );
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        e.handle(
            NOW,
            Event::WsMessage {
                socket,
                text: r#"{"type":"snapshot","orders_p":[]}"#,
            },
        );
        e.drain_actions().for_each(drop);
        assert!(e.store().loaded().orders);
        e.handle(
            NOW + 1000.,
            Event::Command(Command::PlaceOrders {
                command: 71,
                orders: vec![opts("uncertain")],
            }),
        );
        let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1_000.);
        let create = request(&a, "/g-orders/create");
        e.handle(
            NOW + 1500.,
            Event::HttpFailed {
                request: create.id,
                error: "transport outcome unknown",
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        let wake = NOW + 66_000.;
        let timer = a
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == wake => Some(*timer),
                _ => None,
            })
            .expect("native uncertainty fence");
        let refresh = release(&mut e, NOW + 1500., a);
        for (currency, total, used, rows) in [("USDT", 1000., 69., BTC), ("USDC", 200., 19., ETH)] {
            if let Some(r) = requests(&refresh)
                .into_iter()
                .find(|r| r.url.contains(&format!("currency={currency}")))
            {
                pressure_answer(
                    &mut e,
                    NOW + 1501.,
                    r.id,
                    &envelope(currency, total, used, rows),
                );
            }
        }
        pressure_command(
            &mut e,
            wake - 1.,
            if queue_full {
                "fill-http"
            } else {
                "peer-contexts:256"
            },
        );
        let held: Vec<_> = e.drain_actions().collect();
        let mut attempts: std::collections::VecDeque<_> = held
            .into_iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, .. } => Some(attempt),
                _ => None,
            })
            .collect();
        e.handle(wake, Event::TimerFired { timer });
        let a: Vec<_> = e.drain_actions().collect();
        assert!(requests(&a).is_empty());
        assert!(!a.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
        assert!(e.store().orders().is_empty());
        let (timer, at) = poll(&a, wake);
        if queue_full {
            let mut retired = 0;
            while let Some(attempt) = attempts.pop_front() {
                e.handle(
                    wake + 1.,
                    Event::Signed {
                        attempt,
                        result: Err(SignError::Refused),
                    },
                );
                attempts.extend(e.drain_actions().filter_map(|a| match a {
                    Action::Sign { attempt, .. } => Some(attempt),
                    _ => None,
                }));
                retired += 1;
                assert!(retired <= 256);
            }
            assert_eq!(retired, 256);
        }
        pressure_command(&mut e, wake + 1., "release-contexts");
        e.drain_actions().for_each(drop);
        e.handle(at, Event::TimerFired { timer });
        let a: Vec<_> = e.drain_actions().collect();
        assert!(a.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { intent: RoutingIntent::Primary, request, .. } if request.url.contains("activeList")))));
        let fresh = release(&mut e, at, a);
        let r = request(&fresh, "activeList");
        pressure_answer(&mut e, at + 1., r.id, revealed());
        assert!(e.store().order("revealed").is_some());
        pressure_command(&mut e, at + 2., "peer-contexts:256");
        pressure_command(&mut e, at + 2., "release-contexts");
    }
}
