//! Original ordering assertions ported to trusted selected send-time claims.
//! Queue reservation and arithmetic probe ports are disclosed in the plan.
#![cfg(all(feature = "test-support", feature = "dispatch-nonce"))]

use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{KRAKEN, NonceClaim};
use safe_cex_engine::request_context::{
    Claim, Credential, Error, HttpAction, Operation as HttpOperation, Permit, Policy, Preparation,
    Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::signing::{
    Algorithm, Context, KeyRef, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    RequestId, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
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
    prepared: Vec<(u8, u64)>,
    applied: Vec<u8>,
    deny_release: bool,
    reserve_twice: bool,
    stamp_floor: Option<safe_cex_engine::StampFloor>,
    releases: Vec<safe_cex_engine::SigningRelease>,
    command_root: Option<safe_cex_engine::request_context::Context>,
    #[cfg(feature = "session-bearer")]
    bearer_revision: Option<u64>,
    #[cfg(feature = "session-bearer")]
    bearer_session: Option<safe_cex_engine::request_context::Session>,
}
struct Recipe {
    owner: u8,
    claim: NonceClaim,
    trace: Arc<Mutex<Trace>>,
}
impl SignStep for Recipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        Some(&self.claim)
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        let nonce = context
            .reserve_nonce(&self.claim)
            .map_err(|error| match error {
                safe_cex_engine::nonce::NonceError::InvalidTime
                | safe_cex_engine::nonce::NonceError::InvalidClaim => SignError::InvalidRequest,
                _ => SignError::Unavailable,
            })?;
        if self.trace.lock().unwrap().reserve_twice {
            let _ = context.reserve_nonce(&self.claim);
        }
        self.trace
            .lock()
            .unwrap()
            .prepared
            .push((self.owner, nonce));
        let body = format!("owner={}&nonce={nonce}", self.owner);
        request.body = Some(body.clone());
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: Some(nonce),
                stamp_ms: Some(context.stamp_after(self.trace.lock().unwrap().stamp_floor)?),
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
static NONCE_SELECTOR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

struct NonceProbe<const SUPPORTED: bool, const CAPABILITY: u8 = 0> {
    subject: &'static str,
    count: u8,
    trace: Arc<Mutex<Trace>>,
    errors: Vec<VenueError>,
    pacing: Option<safe_cex_engine::request_context::Pacing>,
}
type Probe = NonceProbe<true>;
impl<const SUPPORTED: bool, const CAPABILITY: u8> Venue for NonceProbe<SUPPORTED, CAPABILITY> {
    #[inline]
    fn dispatch_nonce() -> bool {
        match CAPABILITY {
            0 => true,
            1 => false,
            2 => NONCE_SELECTOR.load(std::sync::atomic::Ordering::SeqCst),
            _ => unreachable!(),
        }
    }

    type Pending = u8;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }

    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        if SUPPORTED {
            safe_cex_engine::HttpRelease::NonceUnscheduled(|this, _, _| {
                if this.trace.lock().unwrap().deny_release {
                    Err(Error::Invalid)
                } else {
                    Ok(())
                }
            })
        } else {
            safe_cex_engine::HttpRelease::Mutable(|_, _, _| {
                Ok(safe_cex_engine::request_context::Admission::Unscheduled)
            })
        }
    }
    fn on_signing_release(
        &mut self,
        _: safe_cex_engine::SigningTarget<'_, u8>,
        release: safe_cex_engine::SigningRelease,
    ) {
        self.trace.lock().unwrap().releases.push(release);
    }
    fn http_pacing(&self) -> &[safe_cex_engine::request_context::Pacing] {
        self.pacing.as_slice()
    }
    #[cfg(feature = "session-bearer")]
    fn session_bearer(&self, _: u64) -> safe_cex_engine::request_context::SessionBearer {
        use safe_cex_engine::request_context::{SecretHeaderValue, SessionBearer, SessionHeaders};
        match self.trace.lock().unwrap().bearer_revision {
            Some(revision) => SessionBearer::Ready {
                headers: SessionHeaders::authorization(
                    SecretHeaderValue::new(format!("Bearer revision-{revision}")).unwrap(),
                ),
                revision,
            },
            None => SessionBearer::NotReady,
        }
    }
    #[cfg(feature = "session-bearer")]
    fn session_bearer_revision(&self, _: u64) -> Option<u64> {
        self.trace.lock().unwrap().bearer_revision
    }
    fn start(&mut self, cx: &mut Cx<'_, u8>) {
        #[cfg(feature = "session-bearer")]
        let bearer = if self.trace.lock().unwrap().bearer_revision.is_some() {
            let session = cx
                .open_session(safe_cex_engine::request_context::SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                    private_egress: None,
                })
                .unwrap();
            self.trace.lock().unwrap().bearer_session = Some(session.clone());
            Some(session)
        } else {
            None
        };
        #[cfg(not(feature = "session-bearer"))]
        let bearer = None;
        for owner in 0..self.count {
            let slots = Slots::new(
                Some(Identity {
                    credential: KeyRef(KEY.into()),
                    signing: KeyRef(KEY.into()),
                    passphrase: None,
                    stable_id: Some(self.subject.into()),
                    extra: Default::default(),
                }),
                None,
                SlotPolicy::default(),
            );
            let (_, _, claim) = slots
                .select_with_nonce(CredentialRole::Read { scope: None }, &KRAKEN)
                .unwrap();
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
                        pacing: self.pacing,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let recipe = Recipe {
                owner,
                claim,
                trace: self.trace.clone(),
            };
            if let Err((_, error)) = cx.request_signed_managed(
                context,
                bearer.as_ref(),
                Method::Post,
                format!("https://nonce.test/{owner}"),
                None,
                Box::new(recipe),
                owner,
            ) {
                self.errors.push(error.venue_error());
            }
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, u8>, _: u8, error: VenueError) {
        let root = self.trace.lock().unwrap().command_root.clone();
        if let Some(root) = root {
            cx.fail_context(&root, error.clone()).unwrap();
        }
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
    fn on_command(&mut self, cx: &mut Cx<'_, u8>, command: Command) {
        let Command::FetchAccount { .. } = command else {
            panic!("fixture command");
        };
        let slots = Slots::new(
            Some(Identity {
                credential: KeyRef(KEY.into()),
                signing: KeyRef(KEY.into()),
                passphrase: None,
                stable_id: Some(self.subject.into()),
                extra: Default::default(),
            }),
            None,
            SlotPolicy::default(),
        );
        let (_, _, claim) = slots
            .select_with_nonce(CredentialRole::Read { scope: None }, &KRAKEN)
            .unwrap();
        let root = cx
            .admit_http_command(Policy {
                operation: HttpOperation::Read,
                credential: Some(Credential {
                    slot: CredentialSlot::Primary,
                    reference: KeyRef(KEY.into()),
                }),
                timeout_ms: 20000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        self.trace.lock().unwrap().command_root = Some(root.clone());
        cx.request_signed_managed(
            root,
            None,
            Method::Post,
            "https://nonce.test/0".into(),
            None,
            Box::new(Recipe {
                owner: 0,
                claim,
                trace: self.trace.clone(),
            }),
            0,
        )
        .unwrap();
    }
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
    canceled: Vec<Preparation>,
    work: Vec<Work>,
    bound: Vec<Bound>,
    failed: Vec<(u32, VenueError)>,
    done: Vec<u32>,
}
fn drain<const CAPABILITY: u8>(engine: &mut Engine<NonceProbe<true, CAPABILITY>>) -> Batch {
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
                HttpAction::Cancel { preparation, .. } => batch.canceled.push(preparation),
                HttpAction::BindGroup { .. } => panic!("nonce fixture declares no barriers"),
            },
            Action::Sign { attempt, work, .. } => batch.work.push(Work { attempt, work }),
            Action::Http(_) => panic!("managed packet escaped final Claim"),
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error }) => {
                batch.failed.push((command, error))
            }
            Action::Notice(safe_cex_engine::Notice::CommandDone { command, .. }) => {
                batch.done.push(command)
            }
            _ => {}
        }
    }
    batch
}
fn fixture(
    subject: &'static str,
    _allocation: Allocation,
    count: u8,
) -> (Engine<Probe>, Vec<Route>) {
    fixture_with_pacing(subject, count, None)
}
fn fixture_with_pacing(
    subject: &'static str,
    count: u8,
    pacing: Option<safe_cex_engine::request_context::Pacing>,
) -> (Engine<Probe>, Vec<Route>) {
    fixture_with_capability::<0>(subject, count, pacing)
}
fn fixture_with_capability<const CAPABILITY: u8>(
    subject: &'static str,
    count: u8,
    pacing: Option<safe_cex_engine::request_context::Pacing>,
) -> (Engine<NonceProbe<true, CAPABILITY>>, Vec<Route>) {
    let mut engine = unstarted_fixture::<CAPABILITY>(subject, count, pacing);
    engine.start(START);
    let mut batch = drain(&mut engine);
    assert_eq!(
        batch.routes.len(),
        if CAPABILITY == 1 {
            0
        } else {
            usize::from(count)
        }
    );
    assert!(batch.work.is_empty() && batch.bound.is_empty());
    batch.routes.sort_by_key(|r| r.owner);
    (engine, batch.routes)
}
fn unstarted_fixture<const CAPABILITY: u8>(
    subject: &'static str,
    count: u8,
    pacing: Option<safe_cex_engine::request_context::Pacing>,
) -> Engine<NonceProbe<true, CAPABILITY>> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(NonceProbe::<true, CAPABILITY> {
        subject,
        count,
        trace: Arc::default(),
        errors: vec![],
        pacing,
    });
    engine
        .set_identities(safe_cex_engine::signing::Identities {
            primary: Some(Identity {
                credential: KeyRef(KEY.into()),
                signing: KeyRef(KEY.into()),
                passphrase: None,
                stable_id: Some(subject.into()),
                extra: Default::default(),
            }),
            secondary: None,
        })
        .unwrap();
    engine
}

fn ready<const CAPABILITY: u8>(
    engine: &mut Engine<NonceProbe<true, CAPABILITY>>,
    route: &Route,
    now: f64,
) -> Work {
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
fn reply<const CAPABILITY: u8>(
    engine: &mut Engine<NonceProbe<true, CAPABILITY>>,
    work: Work,
    now: f64,
) -> Batch {
    engine.handle(
        now,
        Event::Signed {
            attempt: work.attempt,
            result: Ok(SignResult::new(vec![vec![7; 32]], work.work.receipt)),
        },
    );
    drain(engine)
}
fn bind<const CAPABILITY: u8>(
    engine: &mut Engine<NonceProbe<true, CAPABILITY>>,
    work: Work,
    now: f64,
) -> Bound {
    let mut batch = reply(engine, work, now);
    assert!(batch.routes.is_empty() && batch.work.is_empty());
    assert_eq!(batch.bound.len(), 1);
    batch.bound.remove(0)
}
fn send<const CAPABILITY: u8>(
    engine: &mut Engine<NonceProbe<true, CAPABILITY>>,
    packet: &Bound,
    now: f64,
) -> u64 {
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
    assert!(
        engine.venue().trace.lock().unwrap().prepared.is_empty(),
        "parking reserves nothing"
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
    let (mut a, ar) = fixture(subject, Allocation::QueueTime, 1);
    let _ = ready(&mut a, &ar[0], START);
    let first = a.venue().trace.lock().unwrap().prepared[0].1;
    let (mut b, br) = fixture(subject, Allocation::QueueTime, 1);
    let _ = ready(&mut b, &br[0], START);
    let second = b.venue().trace.lock().unwrap().prepared[0].1;
    drop(a);
    let (mut a, ar) = fixture(subject, Allocation::QueueTime, 1);
    let _ = ready(&mut a, &ar[0], START);
    let third = a.venue().trace.lock().unwrap().prepared[0].1;
    assert!(first < second && second < third);
}

#[test]
fn distinct_subjects_have_independent_sequences() {
    let (mut a, ar) = fixture("nonce-control/independent-a", Allocation::QueueTime, 2);
    let (mut b, br) = fixture("nonce-control/independent-b", Allocation::QueueTime, 1);
    let _ = ready(&mut a, &ar[0], START);
    let _ = ready(&mut a, &ar[1], START);
    let _ = ready(&mut b, &br[0], START);
    assert_eq!(
        a.venue().trace.lock().unwrap().prepared,
        vec![(0, 1000000), (1, 1000001)]
    );
    assert_eq!(b.venue().trace.lock().unwrap().prepared, vec![(0, 1000000)]);
}

#[test]
fn existing_timestamp_receipt_stays_safe_integer_and_without_nonce() {
    let receipt = SignReceipt {
        nonce: None,
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
                nonce: None,
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

fn repair(engine: &mut Engine<Probe>, mut batch: Batch, now: f64) -> Bound {
    assert_eq!(
        batch.routes.len(),
        1,
        "one fresh preparation for same physical attempt"
    );
    assert!(batch.bound.is_empty() && batch.work.is_empty());
    let route = batch.routes.remove(0);
    let work = ready(engine, &route, now);
    bind(engine, work, now)
}
#[test]
fn reversed_claim_repairs_same_physical_attempt_once_and_releases_above_floor() {
    let (mut engine, routes) = fixture("nonce-source/repair", Allocation::PrepareProbe, 2);
    let work = ready(&mut engine, &routes[0], START);
    let a = bind(&mut engine, work, START);
    let work = ready(&mut engine, &routes[1], START);
    let b = bind(&mut engine, work, START);
    let floor = send(&mut engine, &b, START);
    assert_eq!(engine.claim_http(START, a.permit), Err(Error::Stale));
    let batch = drain(&mut engine);
    let fresh = repair(&mut engine, batch, START);
    assert_eq!(fresh.request.id, a.request.id);
    assert!(send(&mut engine, &fresh, START) > floor);
    assert_eq!(engine.venue().trace.lock().unwrap().prepared.len(), 3);
    assert!(engine.venue().errors.is_empty());
}
#[test]
fn thirty_two_reverse_bound_claims_each_repair_at_most_once_without_exhaustion() {
    let (mut engine, routes) = fixture("nonce-source/burst32", Allocation::PrepareProbe, 32);
    let mut packets = Vec::new();
    for route in &routes {
        let work = ready(&mut engine, route, START);
        packets.push(bind(&mut engine, work, START));
    }
    let mut previous = 0;
    let mut repairs = 0;
    for packet in packets.iter().rev() {
        let nonce = match engine.claim_http(START, packet.permit) {
            Ok(Claim::Released(_)) => packet.nonce(),
            Err(Error::Stale) => {
                repairs += 1;
                let batch = drain(&mut engine);
                let fresh = repair(&mut engine, batch, START);
                assert_eq!(fresh.request.id, packet.request.id);
                send(&mut engine, &fresh, START)
            }
            other => panic!("unexpected claim: {other:?}"),
        };
        assert!(nonce > previous);
        previous = nonce;
    }
    assert_eq!(repairs, 31);
    assert_eq!(engine.venue().trace.lock().unwrap().prepared.len(), 63);
    assert!(engine.venue().errors.is_empty());
}
#[test]
fn thirty_two_paced_fifo_claims_do_not_repair() {
    let (mut engine, routes) = fixture_with_pacing(
        "nonce-source/fifo32",
        32,
        Some(safe_cex_engine::request_context::Pacing {
            group: 741,
            minimum_gap_ms: 1.,
        }),
    );
    let mut packets = Vec::new();
    for route in &routes {
        let work = ready(&mut engine, route, START);
        packets.push(bind(&mut engine, work, START));
    }
    let mut previous = 0;
    for (index, packet) in packets.iter().enumerate() {
        if index > 0 {
            assert_eq!(
                engine.claim_http(START + index as f64 - 1., packet.permit),
                Ok(Claim::WaitUntil(START + index as f64))
            );
        }
        let nonce = send(&mut engine, packet, START + index as f64);
        assert!(nonce > previous);
        previous = nonce;
    }
    assert_eq!(engine.venue().trace.lock().unwrap().prepared.len(), 32);
    assert!(engine.venue().errors.is_empty());
}
#[test]
fn nine_hostile_supersessions_exhaust_only_the_eight_nonce_repairs() {
    let subject = "nonce-source/exhaust8";
    let (mut a, ar) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut a, &ar[0], START);
    let mut packet = bind(&mut a, work, START);
    for iteration in 0..9 {
        let (mut b, br) = fixture(subject, Allocation::PrepareProbe, 1);
        let work = ready(&mut b, &br[0], START);
        let higher = bind(&mut b, work, START);
        send(&mut b, &higher, START);
        if iteration == 8 {
            assert_eq!(a.claim_http(START, packet.permit), Err(Error::State));
            let batch = drain(&mut a);
            assert!(batch.routes.is_empty() && batch.bound.is_empty() && batch.work.is_empty());
        } else {
            assert_eq!(a.claim_http(START, packet.permit), Err(Error::Stale));
            let batch = drain(&mut a);
            packet = repair(&mut a, batch, START);
        }
    }
    assert_eq!(a.venue().trace.lock().unwrap().prepared.len(), 9);
    assert_eq!(a.venue().errors.len(), 1);
    assert_eq!(
        a.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::Internal)
    );
    assert_eq!(
        a.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

#[test]
fn missing_or_substituted_provider_nonce_refuses_before_apply() {
    for (subject, changed) in [
        ("nonce-source/missing-result", None),
        ("nonce-source/changed-result", Some(7)),
    ] {
        let (mut engine, routes) = fixture(subject, Allocation::PrepareProbe, 1);
        let work = ready(&mut engine, &routes[0], START);
        let mut receipt = work.work.receipt;
        receipt.nonce = changed;
        engine.handle(
            START,
            Event::Signed {
                attempt: work.attempt,
                result: Ok(SignResult::new(vec![vec![7; 32]], receipt)),
            },
        );
        let batch = drain(&mut engine);
        assert!(batch.bound.is_empty() && batch.routes.is_empty() && batch.work.is_empty());
        assert!(engine.venue().trace.lock().unwrap().applied.is_empty());
        assert_eq!(engine.venue().errors.len(), 1);
        assert_eq!(engine.venue().errors[0].message, "Invalid signing result");
        assert_eq!(
            engine.venue().errors[0].outcome,
            Some(safe_cex_engine::Outcome::NotSent)
        );
    }
}

#[test]
fn unsupported_release_registration_refuses_before_reservation_or_provider() {
    let subject = "nonce-source/unsupported";
    let mut engine = safe_cex_engine::test_support::seeded_engine(NonceProbe::<false> {
        subject,
        count: 1,
        trace: Arc::default(),
        errors: vec![],
        pacing: None,
    });
    engine
        .set_identities(safe_cex_engine::signing::Identities {
            primary: Some(Identity {
                credential: KeyRef(KEY.into()),
                signing: KeyRef(KEY.into()),
                passphrase: None,
                stable_id: Some(subject.into()),
                extra: Default::default(),
            }),
            secondary: None,
        })
        .unwrap();
    engine.start(START);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Sign { .. } | Action::ManagedHttp(_) | Action::Http(_)
    )));
    assert!(engine.venue().trace.lock().unwrap().prepared.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert_eq!(
        engine.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        engine.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    let (mut supported, routes) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut supported, &routes[0], START);
    assert_eq!(
        work.work.receipt.nonce,
        Some(1000000),
        "refusal reserved nothing"
    );
}
#[test]
fn forged_selected_subject_refuses_against_the_installed_identity() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        subject: "nonce-source/forged",
        count: 1,
        trace: Arc::default(),
        errors: vec![],
        pacing: None,
    });
    engine
        .set_identities(safe_cex_engine::signing::Identities {
            primary: Some(Identity {
                credential: KeyRef(KEY.into()),
                signing: KeyRef(KEY.into()),
                passphrase: None,
                stable_id: Some("nonce-source/installed".into()),
                extra: Default::default(),
            }),
            secondary: None,
        })
        .unwrap();
    engine.start(START);
    let batch = drain(&mut engine);
    assert!(batch.bound.is_empty() && batch.work.is_empty() && batch.routes.is_empty());
    assert!(engine.venue().trace.lock().unwrap().prepared.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert_eq!(
        engine.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
}
#[test]
fn fallible_pure_admission_does_not_advance_the_released_floor() {
    let subject = "nonce-source/admission-refusal";
    let (mut a, ar) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut a, &ar[0], START);
    let low = bind(&mut a, work, START);
    let (mut b, br) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut b, &br[0], START);
    let high = bind(&mut b, work, START);
    b.venue().trace.lock().unwrap().deny_release = true;
    assert_eq!(b.claim_http(START, high.permit), Err(Error::Invalid));
    assert_eq!(send(&mut a, &low, START), low.nonce());
    assert_eq!(b.venue().errors.len(), 1);
    assert_eq!(
        b.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

#[test]
fn exhausted_command_has_one_terminal_and_late_provider_cannot_revive_it() {
    let subject = "nonce-source/command-exhaust8";
    let (mut a, _) = fixture(subject, Allocation::PrepareProbe, 0);
    a.handle(START, Event::Command(Command::FetchAccount { command: 71 }));
    let mut batch = drain(&mut a);
    assert_eq!(batch.routes.len(), 1);
    let work = ready(&mut a, &batch.routes.remove(0), START);
    let mut packet = bind(&mut a, work, START);
    for iteration in 0..9 {
        let (mut b, br) = fixture(subject, Allocation::PrepareProbe, 1);
        let work = ready(&mut b, &br[0], START);
        let higher = bind(&mut b, work, START);
        send(&mut b, &higher, START);
        if iteration == 8 {
            assert_eq!(a.claim_http(START, packet.permit), Err(Error::State));
            let batch = drain(&mut a);
            assert_eq!(batch.failed.len(), 1);
            assert_eq!(batch.failed[0].0, 71);
            assert_eq!(
                batch.failed[0].1.kind,
                Some(safe_cex_engine::ErrorKind::Internal)
            );
            assert_eq!(
                batch.failed[0].1.outcome,
                Some(safe_cex_engine::Outcome::NotSent)
            );
            assert!(
                batch.done.is_empty()
                    && batch.routes.is_empty()
                    && batch.bound.is_empty()
                    && batch.work.is_empty()
            );
        } else {
            assert_eq!(a.claim_http(START, packet.permit), Err(Error::Stale));
            let batch = drain(&mut a);
            packet = repair(&mut a, batch, START);
        }
    }
    assert_eq!(a.venue().trace.lock().unwrap().prepared.len(), 9);
    assert_eq!(a.venue().errors.len(), 1);
    assert_eq!(a.claim_http(START, packet.permit), Err(Error::Revoked));
    let late = drain(&mut a);
    assert!(late.failed.is_empty() && late.done.is_empty() && late.bound.is_empty());
}

#[test]
fn missing_nonce_remains_invalid_result_even_after_another_engine_releases() {
    let subject = "nonce-source/stale-missing-result";
    let (mut a, ar) = fixture(subject, Allocation::PrepareProbe, 1);
    let old = ready(&mut a, &ar[0], START);
    let (mut b, br) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut b, &br[0], START);
    let higher = bind(&mut b, work, START);
    send(&mut b, &higher, START);
    let mut receipt = old.work.receipt;
    receipt.nonce = None;
    a.handle(
        START,
        Event::Signed {
            attempt: old.attempt,
            result: Ok(SignResult::new(vec![vec![7; 32]], receipt)),
        },
    );
    let batch = drain(&mut a);
    assert!(batch.bound.is_empty() && batch.routes.is_empty() && batch.work.is_empty());
    assert!(a.venue().trace.lock().unwrap().applied.is_empty());
    assert_eq!(a.venue().errors.len(), 1);
    assert_eq!(a.venue().errors[0].message, "Invalid signing result");
}

#[test]
fn ignored_second_reserve_refuses_before_provider_or_handoff() {
    let (mut engine, routes) = fixture("nonce-source/double-reserve", Allocation::PrepareProbe, 1);
    engine.venue().trace.lock().unwrap().reserve_twice = true;
    engine.http_prepared(
        START,
        routes[0].token,
        Prepared {
            id: routes[0].token.attempt().request.0,
            expires_at: None,
        },
    );
    let batch = drain(&mut engine);
    assert!(
        batch.work.is_empty(),
        "a failed reservation poisons the whole prepare"
    );
    assert!(batch.bound.is_empty() && batch.routes.is_empty());
    let trace = engine.venue().trace.lock().unwrap();
    assert_eq!(trace.prepared.len(), 1, "first reserve succeeded");
    assert!(trace.applied.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert_eq!(
        engine.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        engine.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

#[test]
fn actual_stamp_floor_and_nonce_counter_remain_independent_at_dispatch() {
    let subject = "nonce-source/actual-stamp-floor";
    let (mut engine, routes) = fixture(subject, Allocation::PrepareProbe, 2);
    let first = ready(&mut engine, &routes[0], START);
    assert_eq!(first.work.receipt.stamp_ms, Some(1000.));
    assert_eq!(first.work.receipt.nonce, Some(1000000));
    let first = bind(&mut engine, first, START);
    send(&mut engine, &first, START);
    let floor = engine.venue().trace.lock().unwrap().releases[0].stamp_floor();
    assert!(floor.is_some());
    engine.venue().trace.lock().unwrap().stamp_floor = floor;
    let second = ready(&mut engine, &routes[1], START);
    assert_eq!(second.work.receipt.stamp_ms, Some(1001.));
    assert_eq!(
        second.work.receipt.nonce,
        Some(1000001),
        "stamp floor never scales the nonce floor"
    );
    let second = bind(&mut engine, second, START);
    send(&mut engine, &second, START);
    let (mut peer, routes) = fixture(subject, Allocation::PrepareProbe, 1);
    let third = ready(&mut peer, &routes[0], START);
    assert_eq!(
        third.work.receipt.stamp_ms,
        Some(1000.),
        "nonce release never creates a StampFloor"
    );
    assert_eq!(third.work.receipt.nonce, Some(1000002));
}

#[test]
fn disabled_nonce_capability_refuses_before_preparation_or_provider_work() {
    let (mut engine, routes) = fixture_with_capability::<1>("nonce-capability/disabled", 1, None);
    assert!(routes.is_empty());
    assert!(engine.venue().trace.lock().unwrap().prepared.is_empty());
    assert!(engine.venue().trace.lock().unwrap().releases.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert_eq!(
        engine.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        engine.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    assert!(drain(&mut engine).work.is_empty());
}
#[test]
fn a_changed_nonce_selector_refuses_before_release_without_advancing_the_floor() {
    use std::sync::atomic::Ordering;
    struct RestoreSelector;
    impl Drop for RestoreSelector {
        fn drop(&mut self) {
            NONCE_SELECTOR.store(true, Ordering::SeqCst);
        }
    }
    let _restore = RestoreSelector;
    NONCE_SELECTOR.store(false, Ordering::SeqCst);
    let mut initially_disabled =
        unstarted_fixture::<2>("nonce-capability/capture-ceiling", 1, None);
    NONCE_SELECTOR.store(true, Ordering::SeqCst);
    initially_disabled.start(START);
    let denied = drain(&mut initially_disabled);
    assert!(denied.routes.is_empty() && denied.work.is_empty() && denied.bound.is_empty());
    assert!(
        initially_disabled
            .venue()
            .trace
            .lock()
            .unwrap()
            .prepared
            .is_empty()
    );
    assert_eq!(initially_disabled.venue().errors.len(), 1);
    assert_eq!(
        initially_disabled.venue().errors[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        initially_disabled.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    let subject = "nonce-capability/changed-selector";
    let (mut earlier, routes) = fixture(subject, Allocation::PrepareProbe, 1);
    let work = ready(&mut earlier, &routes[0], START);
    let earlier_packet = bind(&mut earlier, work, START);
    let (mut changed, routes) = fixture_with_capability::<2>(subject, 1, None);
    let work = ready(&mut changed, &routes[0], START);
    let later_packet = bind(&mut changed, work, START);
    assert!(later_packet.nonce() > earlier_packet.nonce());
    NONCE_SELECTOR.store(false, Ordering::SeqCst);
    assert_eq!(
        changed.claim_http(START, later_packet.permit),
        Err(Error::State)
    );
    assert!(changed.venue().trace.lock().unwrap().releases.is_empty());
    assert_eq!(changed.venue().errors.len(), 1);
    assert_eq!(
        changed.venue().errors[0].message,
        "HTTP nonce capability is unavailable"
    );
    assert_eq!(
        changed.venue().errors[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    assert_eq!(
        send(&mut earlier, &earlier_packet, START),
        earlier_packet.nonce()
    );
    NONCE_SELECTOR.store(true, Ordering::SeqCst);
}

#[cfg(feature = "session-bearer")]
#[test]
fn bearer_revision_fence_precedes_nonce_floor_and_preserves_nonce_repair_credits() {
    let mut engine = unstarted_fixture::<0>("nonce-bearer/revision-floor", 2, None);
    engine.venue().trace.lock().unwrap().bearer_revision = Some(1);
    engine.start(START);
    let mut routes = drain(&mut engine).routes;
    routes.sort_by_key(|route| route.owner);
    let a = ready(&mut engine, &routes[0], 1_001.);
    let a = bind(&mut engine, a, 1_002.);
    let b = ready(&mut engine, &routes[1], 1_003.);
    let b = bind(&mut engine, b, 1_004.);
    assert!(b.nonce() > a.nonce());
    assert_eq!(send(&mut engine, &b, 1_005.), b.nonce());
    let mut stale = a;
    let mut stale_preparation = routes[0].token;
    for revision in 2..=10 {
        let now = 1_006. + (revision - 2) as f64 * 3.;
        engine.venue().trace.lock().unwrap().bearer_revision = Some(revision);
        assert_eq!(engine.claim_http(now, stale.permit), Err(Error::Stale));
        assert_eq!(engine.venue().trace.lock().unwrap().releases.len(), 1);
        let mut batch = drain(&mut engine);
        assert!(batch.work.is_empty() && batch.bound.is_empty() && batch.failed.is_empty());
        assert_eq!(batch.canceled, [stale_preparation]);
        assert_eq!(batch.routes.len(), 1);
        let route = batch.routes.remove(0);
        assert_eq!(route.token.attempt(), stale.permit.attempt());
        assert_ne!(route.token, stale_preparation);
        stale_preparation = route.token;
        let work = ready(&mut engine, &route, now + 1.);
        stale = bind(&mut engine, work, now + 2.);
        assert!(stale.nonce() > b.nonce());
        assert!(
            stale
                .request
                .headers
                .iter()
                .any(|(name, value)| *name == "Authorization"
                    && value == &format!("Bearer revision-{revision}"))
        );
    }
    let repaired_nonce = send(&mut engine, &stale, 1_033.);
    assert!(repaired_nonce > b.nonce());
    assert_eq!(engine.venue().trace.lock().unwrap().releases.len(), 2);
    assert!(engine.venue().errors.is_empty());
}
