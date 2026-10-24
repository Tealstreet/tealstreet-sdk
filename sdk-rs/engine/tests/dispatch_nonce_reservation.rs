//! Compiling original evidence; no prospective nonce API is mocked into the engine.
//! QueueTime uses Cx today. PrepareProbe uses the legacy allocator at prepare to
//! isolate the missing fence; it is not trusted subject or raw-clock evidence.
#![cfg(feature = "test-support")]

use safe_cex_engine::book::Book;
use safe_cex_engine::nonce::{HYPERLIQUID_AGENT, NonceKey, reserve_kraken_legacy};
use safe_cex_engine::request_context::{
    Claim, Credential, Error, HttpAction, Operation as HttpOperation, Permit, Policy, Preparation,
    Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    Algorithm, Context, KeyRef, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    RequestId, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const START: f64 = 1_000.;
const KEY: &str = "nonce-fixture/primary";

#[derive(Clone, Copy)]
enum Allocation {
    QueueTime,
    PrepareProbe,
}
#[derive(Default)]
struct Trace {
    queued: Vec<(u8, u64)>,
    prepared: Vec<(u8, u64)>,
    applied: Vec<u8>,
}
struct Recipe {
    owner: u8,
    queued: Option<u64>,
    subject: [u8; 32],
    trace: Arc<Mutex<Trace>>,
}
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        // This probe deliberately has no release fence. Replace the adapter,
        // retaining the assertions, once the reviewed DispatchContext API exists.
        let nonce = if let Some(nonce) = self.queued {
            nonce
        } else {
            let floor = (context.now_ms.floor() as u64)
                .checked_mul(1000)
                .ok_or(SignError::InvalidRequest)?;
            reserve_kraken_legacy(self.subject, floor).map_err(|_| SignError::Unavailable)?
        };
        self.trace
            .lock()
            .unwrap()
            .prepared
            .push((self.owner, nonce));
        let body = format!("owner={}&nonce={nonce}", self.owner);
        request.body = Some(body.clone());
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(context.stamp_ms()),
                expiry_ms: None,
            },
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef(KEY.into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: body.into_bytes(),
                }],
            }],
            context: Context::default(),
        })
    }
    fn apply(&mut self, action: &mut Action, _: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        self.trace.lock().unwrap().applied.push(self.owner);
        request.headers.push(("signature", "applied".into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}
struct Probe {
    allocation: Allocation,
    subject: &'static str,
    count: u8,
    trace: Arc<Mutex<Trace>>,
    errors: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = u8;
    fn start(&mut self, cx: &mut Cx<'_, u8>) {
        for owner in 0..self.count {
            let queued = match self.allocation {
                Allocation::QueueTime => Some(
                    cx.reserve_nonce(&HYPERLIQUID_AGENT, &NonceKey::stable_id(self.subject))
                        .unwrap(),
                ),
                Allocation::PrepareProbe => None,
            };
            if let Some(nonce) = queued {
                self.trace.lock().unwrap().queued.push((owner, nonce));
            }
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: HttpOperation::Read,
                        credential: Some(Credential {
                            slot: CredentialSlot::Primary,
                            reference: KeyRef(KEY.into()),
                        }),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let recipe = Recipe {
                owner,
                queued,
                subject: Sha256::digest(self.subject.as_bytes()).into(),
                trace: self.trace.clone(),
            };
            cx.request_signed_managed(
                context,
                Method::Post,
                format!("https://nonce.test/{owner}"),
                None,
                Box::new(recipe),
                owner,
            )
            .unwrap_or_else(|_| panic!("fixture request must queue"));
        }
    }
    fn on_sign_error(&mut self, _: &mut Cx<'_, u8>, _: u8, error: VenueError) {
        self.errors.push(error);
    }
    fn on_http_failed(&mut self, _: &mut Cx<'_, u8>, _: u8, error: VenueError) {
        self.errors.push(error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, u8>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, u8>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, u8>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, u8>, _: u8, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, u8>, _: u8, _: &str) {
        panic!("fixture expects typed failure");
    }
    fn on_timer(&mut self, _: &mut Cx<'_, u8>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, u8>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
struct Route {
    owner: u8,
    token: Preparation,
}
struct Work {
    attempt: RequestId,
    work: SignRequest,
}
struct Bound {
    owner: u8,
    permit: Permit,
    request: HttpRequest,
}
impl Bound {
    fn nonce(&self) -> u64 {
        self.request
            .body
            .as_ref()
            .unwrap()
            .split("&nonce=")
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }
}
#[derive(Default)]
struct Batch {
    routes: Vec<Route>,
    work: Vec<Work>,
    bound: Vec<Bound>,
}
fn drain(engine: &mut Engine<Probe>) -> Batch {
    let mut batch = Batch::default();
    for action in engine.drain_actions() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => batch.routes.push(Route {
                    owner: request.url.rsplit('/').next().unwrap().parse().unwrap(),
                    token: preparation,
                }),
                HttpAction::Bind {
                    permit, request, ..
                } => batch.bound.push(Bound {
                    owner: request.url.rsplit('/').next().unwrap().parse().unwrap(),
                    permit,
                    request,
                }),
                HttpAction::Cancel { .. } => {}
            },
            Action::Sign { attempt, work, .. } => batch.work.push(Work { attempt, work }),
            Action::Http(_) => panic!("managed packet escaped final Claim"),
            _ => {}
        }
    }
    batch
}
fn fixture(
    subject: &'static str,
    allocation: Allocation,
    count: u8,
) -> (Engine<Probe>, Vec<Route>) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        allocation,
        subject,
        count,
        trace: Arc::default(),
        errors: vec![],
    });
    engine.start(START);
    let mut batch = drain(&mut engine);
    assert_eq!(batch.routes.len(), usize::from(count));
    assert!(batch.work.is_empty() && batch.bound.is_empty());
    batch.routes.sort_by_key(|r| r.owner);
    (engine, batch.routes)
}
fn ready(engine: &mut Engine<Probe>, route: &Route, now: f64) -> Work {
    engine.http_prepared(
        now,
        route.token,
        Prepared {
            id: route.token.attempt().request.0,
            expires_at: None,
        },
    );
    let mut batch = drain(engine);
    assert!(batch.routes.is_empty() && batch.bound.is_empty());
    assert_eq!(
        batch.work.len(),
        1,
        "exact route must reach real provider work"
    );
    batch.work.remove(0)
}
fn reply(engine: &mut Engine<Probe>, work: Work, now: f64) -> Batch {
    engine.handle(
        now,
        Event::Signed {
            attempt: work.attempt,
            result: Ok(SignResult::new(vec![vec![7; 32]], work.work.receipt)),
        },
    );
    drain(engine)
}
fn bind(engine: &mut Engine<Probe>, work: Work, now: f64) -> Bound {
    let mut batch = reply(engine, work, now);
    assert!(batch.routes.is_empty() && batch.work.is_empty());
    assert_eq!(batch.bound.len(), 1);
    batch.bound.remove(0)
}
fn send(engine: &mut Engine<Probe>, packet: &Bound, now: f64) -> u64 {
    assert!(
        packet
            .request
            .headers
            .contains(&("signature", "applied".into()))
    );
    let Claim::Released(release) = engine.claim_http(now, packet.permit).unwrap() else {
        panic!("fixture has no pacing");
    };
    assert_eq!(release.attempt.request, packet.request.id);
    packet.nonce()
}

#[test]
fn queue_time_reservation_cannot_reverse_nonce_order_when_routes_resume_backwards() {
    let (mut engine, routes) = fixture("nonce-original/reverse-route", Allocation::QueueTime, 2);
    let queue = engine.venue().trace.lock().unwrap().queued.clone();
    assert!(
        queue[0].1 < queue[1].1,
        "native queue reservation is monotonic"
    );
    let b = ready(&mut engine, &routes[1], 1_001.);
    let b = bind(&mut engine, b, 1_002.);
    assert_eq!(b.owner, 1);
    let first = send(&mut engine, &b, 1_003.);
    let a = ready(&mut engine, &routes[0], 1_004.);
    let a = bind(&mut engine, a, 1_005.);
    let second = send(&mut engine, &a, 1_006.);
    assert!(
        second > first,
        "release order must increase; got {first} then {second}"
    );
}

#[test]
fn prepare_reservation_alone_cannot_apply_a_provider_result_behind_released_floor() {
    let (mut engine, routes) = fixture("nonce-original/provider-park", Allocation::PrepareProbe, 2);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let b = ready(&mut engine, &routes[1], 1_002.);
    let b = bind(&mut engine, b, 1_003.);
    send(&mut engine, &b, 1_004.);
    assert_eq!(engine.venue().trace.lock().unwrap().applied, vec![1]);
    let old = reply(&mut engine, a, 1_005.);
    assert_eq!(
        engine.venue().trace.lock().unwrap().applied,
        vec![1],
        "superseded provider result must be refused before recipe.apply"
    );
    assert!(old.bound.is_empty(), "stale nonce must never bind");
}

#[test]
fn two_bound_packets_recheck_nonce_floor_at_final_claim() {
    let (mut engine, routes) = fixture("nonce-original/reverse-claim", Allocation::PrepareProbe, 2);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let a = bind(&mut engine, a, 1_002.);
    let b = ready(&mut engine, &routes[1], 1_003.);
    let b = bind(&mut engine, b, 1_004.);
    assert!(a.nonce() < b.nonce());
    send(&mut engine, &b, 1_005.);
    assert_eq!(
        engine.claim_http(1_006., a.permit),
        Err(Error::Stale),
        "a still-fresh permit cannot release below a later Claim"
    );
}

#[test]
fn released_floor_must_cross_engine_boundaries() {
    let subject = "nonce-original/cross-engine-fence";
    let (mut a, ar) = fixture(subject, Allocation::PrepareProbe, 1);
    let aw = ready(&mut a, &ar[0], 1_001.);
    let (mut b, br) = fixture(subject, Allocation::PrepareProbe, 1);
    let bw = ready(&mut b, &br[0], 1_002.);
    let bound = bind(&mut b, bw, 1_003.);
    send(&mut b, &bound, 1_004.);
    let old = reply(&mut a, aw, 1_005.);
    assert!(
        a.venue().trace.lock().unwrap().applied.is_empty(),
        "a process-global sequence requires a process-global release fence"
    );
    assert!(old.bound.is_empty());
}

#[test]
fn new_contract_nonce_receipt_is_an_exact_decimal_string_separate_from_stamp() {
    let wire = r#"{"stampMs":1000.0,"nonce":"18446744073709551615"}"#;
    let receipt = serde_json::from_str::<SignReceipt>(wire);
    assert!(
        receipt.is_ok(),
        "new receipt must carry exact u64 without f64: {receipt:?}"
    );
    let receipt = receipt.unwrap();
    assert_eq!(receipt.stamp_ms, Some(1_000.));
    assert_eq!(serde_json::to_string(&receipt).unwrap(), wire);
}

#[test]
fn in_order_binds_release_both_without_spurious_repair() {
    let (mut engine, routes) = fixture("nonce-control/in-order", Allocation::PrepareProbe, 2);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let a = bind(&mut engine, a, 1_002.);
    let b = ready(&mut engine, &routes[1], 1_003.);
    let b = bind(&mut engine, b, 1_004.);
    let first = send(&mut engine, &a, 1_005.);
    let second = send(&mut engine, &b, 1_006.);
    assert!(second > first);
    assert_eq!(engine.venue().trace.lock().unwrap().prepared.len(), 2);
    assert!(engine.venue().errors.is_empty());
}

#[test]
fn abandoned_unreleased_reservation_does_not_block_earlier_packet() {
    let (mut engine, routes) = fixture("nonce-control/abandoned", Allocation::PrepareProbe, 2);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let a = bind(&mut engine, a, 1_002.);
    let _abandoned = ready(&mut engine, &routes[1], 1_003.);
    assert_eq!(send(&mut engine, &a, 1_004.), a.nonce());
    assert_eq!(engine.venue().trace.lock().unwrap().applied, vec![0]);
}

#[test]
fn provider_refusal_does_not_release_a_packet() {
    let (mut engine, routes) =
        fixture("nonce-control/provider-denial", Allocation::PrepareProbe, 1);
    let a = ready(&mut engine, &routes[0], 1_001.);
    engine.handle(
        1_002.,
        Event::Signed {
            attempt: a.attempt,
            result: Err(SignError::Unavailable),
        },
    );
    let output = drain(&mut engine);
    assert!(output.bound.is_empty() && output.work.is_empty() && output.routes.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert!(engine.venue().trace.lock().unwrap().applied.is_empty());
}

#[test]
fn duplicate_claim_and_late_provider_are_inert() {
    let (mut engine, routes) = fixture("nonce-control/duplicate", Allocation::PrepareProbe, 1);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let old_attempt = a.attempt;
    let receipt = a.work.receipt;
    let a = bind(&mut engine, a, 1_002.);
    send(&mut engine, &a, 1_003.);
    assert_eq!(engine.claim_http(1_004., a.permit), Err(Error::Revoked));
    let late = reply(
        &mut engine,
        Work {
            attempt: old_attempt,
            work: SignRequest {
                receipt,
                operations: vec![],
                context: Context::default(),
            },
        },
        1_005.,
    );
    assert!(late.bound.is_empty() && late.work.is_empty() && late.routes.is_empty());
    assert_eq!(engine.venue().trace.lock().unwrap().applied, vec![0]);
}

#[test]
fn queue_store_continues_across_engines_and_recreation() {
    let subject = "nonce-control/queue-continuity";
    let (a, _) = fixture(subject, Allocation::QueueTime, 1);
    let first = a.venue().trace.lock().unwrap().queued[0].1;
    let (b, _) = fixture(subject, Allocation::QueueTime, 1);
    let second = b.venue().trace.lock().unwrap().queued[0].1;
    drop(a);
    let (a, _) = fixture(subject, Allocation::QueueTime, 1);
    let third = a.venue().trace.lock().unwrap().queued[0].1;
    assert!(first < second && second < third);
}

#[test]
fn distinct_subjects_have_independent_sequences() {
    let (a, _) = fixture("nonce-control/independent-a", Allocation::QueueTime, 2);
    let (b, _) = fixture("nonce-control/independent-b", Allocation::QueueTime, 1);
    assert_eq!(
        a.venue().trace.lock().unwrap().queued,
        vec![(0, 1000), (1, 1001)]
    );
    assert_eq!(b.venue().trace.lock().unwrap().queued, vec![(0, 1000)]);
}

#[test]
fn existing_counter_is_exact_for_regression_more_than_one_thousand_and_exhaustion() {
    let key: [u8; 32] = Sha256::digest(b"nonce-control/regression-overflow").into();
    let mut previous = reserve_kraken_legacy(key, 1_000_000).unwrap();
    for _ in 0..2002 {
        let nonce = reserve_kraken_legacy(key, 999_000).unwrap();
        assert_eq!(nonce, previous + 1);
        previous = nonce;
    }
    let large = reserve_kraken_legacy(key, 9_007_199_254_740_993).unwrap();
    assert_eq!(large.to_string(), "9007199254740993");
    assert_eq!(reserve_kraken_legacy(key, u64::MAX).unwrap(), u64::MAX);
    assert!(reserve_kraken_legacy(key, 0).is_err());
}

#[test]
fn existing_timestamp_receipt_stays_safe_integer_and_without_nonce() {
    let receipt = SignReceipt {
        stamp_ms: Some(1_000.),
        expiry_ms: Some(2_000.),
    };
    assert!(receipt.validate().is_ok());
    assert_eq!(
        serde_json::to_string(&receipt).unwrap(),
        r#"{"stampMs":1000.0,"expiryMs":2000.0}"#
    );
    for stamp in [f64::NAN, f64::INFINITY, -1., 1.5, 9_007_199_254_740_992.] {
        assert!(
            SignReceipt {
                stamp_ms: Some(stamp),
                expiry_ms: None
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn malformed_nonce_receipts_fail_closed_on_original_schema() {
    // These currently fail at unknown-field validation, not a new nonce parser.
    for value in [
        r#"1"#,
        r#""01""#,
        r#""+1""#,
        r#""-1""#,
        r#"" 1""#,
        r#""1 ""#,
        r#""1.0""#,
        r#""1e3""#,
        r#""18446744073709551616""#,
    ] {
        assert!(serde_json::from_str::<SignReceipt>(&format!("{{\"nonce\":{value}}}")).is_err());
    }
}
