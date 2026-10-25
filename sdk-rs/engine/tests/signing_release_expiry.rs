//! Original release-metadata assertions plus compatibility and custody controls.
//! The original UTC-only adapter and its REDs are retained in Git history.
#![cfg(feature = "test-support")]

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Claim, Credential, Error, HttpAction, HttpRelease, Operation as HttpOperation,
    Permit, Policy, Prepared, ReleaseTime, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    Algorithm, Context, KeyRef, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    RequestId, Response, SignStep, SigningTarget, SocketId, TimerId, Venue, VenueError,
};

const START: f64 = 1_000.;
const WINDOW: f64 = 60_000.;
const KEY: &str = "account/primary";

struct Recipe {
    expiry: bool,
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
        request.url = format!("https://release.test/write?stamp={}", context.now_ms);
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(context.now_ms),
                expiry_ms: self.expiry.then_some(context.now_ms + WINDOW),
            },
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef(KEY.into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: context.now_ms.to_string().into_bytes(),
                }],
            }],
            context: Context::default(),
        })
    }
    fn apply(&mut self, action: &mut Action, _: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request.headers.push(("signature", "applied".into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}

struct Pending {
    owner: u32,
}
struct Observation {
    owner: u32,
    request: RequestId,
    receipt: SignReceipt,
    raw_expiry: Option<f64>,
    time: ReleaseTime,
}
struct Probe {
    offset: f64,
    managed: bool,
    expiry: bool,
    deny_admission: bool,
    admissions: usize,
    observed: Vec<Observation>,
    errors: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        assert!(cx.observe_protocol_time(cx.now + self.offset));
        let recipe = Box::new(Recipe {
            expiry: self.expiry,
        });
        let pending = Pending { owner: 7 };
        if self.managed {
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
            cx.request_signed_managed(
                context,
                Method::Post,
                "https://release.test/write".into(),
                None,
                recipe,
                pending,
            )
            .unwrap_or_else(|_| panic!("genuine managed request must prepare"));
        } else {
            let request = cx.request(
                Method::Post,
                "https://release.test/write".into(),
                vec![],
                None,
                pending,
            );
            cx.sign_http(request, recipe);
        }
    }
    fn on_signing_release(
        &mut self,
        target: SigningTarget<'_, Pending>,
        release: safe_cex_engine::SigningRelease,
    ) {
        let SigningTarget::Http { request, pending } = target else {
            panic!("fixture signs HTTP only");
        };
        self.observed.push(Observation {
            owner: pending.owner,
            request,
            receipt: release.receipt(),
            raw_expiry: release.raw_expiry_ms(),
            time: release.time(),
        });
    }
    fn on_signing_receipt(&mut self, _: SigningTarget<'_, Pending>, _: SignReceipt) {
        panic!("an overridden release hook must not receive a second legacy notification");
    }
    fn on_sign_error(&mut self, _: &mut Cx<'_, Pending>, _: Pending, error: VenueError) {
        self.errors.push(error);
    }
    fn on_http_release(&mut self, _: &mut Pending, _: &HttpRelease) -> Result<Admission, Error> {
        self.admissions += 1;
        Ok(if self.deny_admission {
            Admission::Denied
        } else {
            Admission::Unscheduled
        })
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::SetTickerSymbols { symbols } = command else {
            panic!("fixture uses only clock calibration commands");
        };
        let offset: f64 = symbols[0].parse().unwrap();
        assert!(cx.observe_protocol_time(cx.now + offset));
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("no fixture network failure");
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
        panic!("signing timers must remain engine-owned");
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(offset: f64, managed: bool, expiry: bool) -> Engine<Probe> {
    engine_with_admission(offset, managed, expiry, false)
}
fn engine_with_admission(
    offset: f64,
    managed: bool,
    expiry: bool,
    deny_admission: bool,
) -> Engine<Probe> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        offset,
        managed,
        expiry,
        deny_admission,
        admissions: 0,
        observed: vec![],
        errors: vec![],
    });
    engine.start(START);
    engine
}
fn signing(actions: Vec<Action>) -> (RequestId, SignRequest) {
    let mut work = actions.into_iter().filter_map(|action| match action {
        Action::Sign { attempt, work, .. } => Some((attempt, work)),
        Action::Http(_) => panic!("a provider must finish before release"),
        _ => None,
    });
    let first = work.next().expect("one genuine signing attempt");
    assert!(work.next().is_none());
    first
}
fn signed(engine: &mut Engine<Probe>, now: f64, attempt: RequestId, receipt: SignReceipt) {
    engine.handle(
        now,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(vec![vec![7; 32]], receipt)),
        },
    );
}
fn legacy(offset: f64, expiry: bool) -> (Engine<Probe>, HttpRequest) {
    let mut engine = engine(offset, false, expiry);
    let (attempt, work) = signing(engine.drain_actions().collect());
    assert!(engine.venue().observed.is_empty());
    signed(&mut engine, START + 100., attempt, work.receipt);
    let packets: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 1);
    (engine, packets.into_iter().next().unwrap())
}
fn managed() -> (Engine<Probe>, HttpRelease) {
    let (mut engine, permit) = managed_ready(false);
    let Claim::Released(release) = engine.claim_http(START + 100., permit).unwrap() else {
        panic!("fixture has no pacing");
    };
    (engine, release)
}
fn managed_ready(deny_admission: bool) -> (Engine<Probe>, Permit) {
    let mut engine = engine_with_admission(5_000., true, true, deny_admission);
    let preparations: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(preparations.len(), 1);
    engine.http_prepared(
        START + 1.,
        preparations[0],
        Prepared {
            id: 9,
            expires_at: None,
        },
    );
    let (attempt, work) = signing(engine.drain_actions().collect());
    assert!(engine.venue().observed.is_empty());
    signed(&mut engine, START + 100., attempt, work.receipt);
    let binds: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            Action::Http(_) => panic!("managed HTTP cannot bypass claim"),
            _ => None,
        })
        .collect();
    assert_eq!(binds.len(), 1);
    assert!(engine.venue().observed.is_empty(), "Bind is not release");
    (engine, binds[0])
}

#[test]
fn managed_denied_admission_observes_no_signing_release() {
    let (mut engine, permit) = managed_ready(true);
    assert_eq!(engine.claim_http(START + 100., permit), Err(Error::Revoked));
    assert_eq!(engine.venue().admissions, 1);
    assert!(engine.venue().observed.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
    assert!(
        !engine
            .drain_actions()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(engine.claim_http(START + 101., permit), Err(Error::Revoked));
    assert_eq!(engine.venue().admissions, 1);
    assert!(engine.venue().observed.is_empty());
}

#[test]
fn existing_protocol_receipt_and_exact_pending_target_remain_unchanged() {
    let (mut engine, packet) = legacy(5_000., true);
    let records = &engine.venue().observed;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].owner, 7);
    assert_eq!(records[0].request, packet.id);
    assert_eq!(records[0].receipt.stamp_ms, Some(6_000.));
    assert_eq!(records[0].receipt.expiry_ms, Some(66_000.));
    assert_eq!(records[0].time.raw_ms, START + 100.);
    assert_eq!(records[0].time.protocol_ms, START + 100. + 5_000.);
    assert!(packet.headers.contains(&("signature", "applied".into())));
    assert!(engine.drain_actions().next().is_none());
    assert_eq!(engine.venue().observed.len(), 1);
}

#[test]
fn managed_duplicate_claim_cannot_observe_a_second_release() {
    let (mut engine, permit) = managed_ready(false);
    assert!(matches!(
        engine.claim_http(START + 100., permit),
        Ok(Claim::Released(_))
    ));
    assert_eq!(engine.claim_http(START + 101., permit), Err(Error::Revoked));
    assert_eq!(engine.venue().admissions, 1);
    assert_eq!(engine.venue().observed.len(), 1);
    assert_eq!(engine.venue().observed[0].request, permit.attempt().request);
}

#[test]
fn missing_expiry_stays_none_instead_of_an_invented_raw_deadline() {
    let (engine, _) = legacy(5_000., false);
    assert_eq!(engine.venue().observed.len(), 1);
    assert_eq!(engine.venue().observed[0].receipt.expiry_ms, None);
    assert_eq!(engine.venue().observed[0].raw_expiry, None);
}

#[test]
fn provider_refusal_emits_no_release_observation_or_physical_packet() {
    let mut engine = engine(5_000., false, true);
    let (attempt, _) = signing(engine.drain_actions().collect());
    engine.handle(
        START + 100.,
        Event::Signed {
            attempt,
            result: Err(SignError::Unavailable),
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(engine.venue().observed.is_empty());
    assert_eq!(engine.venue().errors.len(), 1);
}

#[test]
fn managed_repair_bound_remains_capped_at_the_existing_lifecycle_deadline() {
    let (engine, release) = managed();
    assert_eq!(release.deadline, 21_000.);
    assert_eq!(release.signature_valid_until, Some(21_000.));
    assert_eq!(release.signature.expiry_ms, Some(66_001.));
    assert_eq!(engine.venue().observed.len(), 1);
}

#[test]
fn legacy_final_release_supplies_raw_expiry_with_positive_protocol_offset() {
    let (engine, _) = legacy(5_000., true);
    assert_eq!(engine.venue().observed[0].raw_expiry, Some(61_000.));
}

#[test]
fn legacy_final_release_supplies_raw_expiry_with_negative_protocol_offset() {
    let (engine, _) = legacy(-500., true);
    assert_eq!(engine.venue().observed[0].receipt.expiry_ms, Some(60_500.));
    assert_eq!(engine.venue().observed[0].raw_expiry, Some(61_000.));
}

#[test]
fn managed_final_release_supplies_uncapped_raw_signature_expiry() {
    let (engine, release) = managed();
    assert_eq!(release.signature_valid_until, Some(21_000.));
    assert_eq!(engine.venue().observed[0].raw_expiry, Some(61_001.));
}

#[test]
fn clock_reprepare_publishes_only_the_final_receipts_raw_expiry() {
    let mut engine = engine(5_000., false, true);
    let (old, old_work) = signing(engine.drain_actions().collect());
    engine.handle(
        1_100.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["9000".into()],
        }),
    );
    signed(&mut engine, 1_101., old, old_work.receipt);
    let (new, work) = signing(engine.drain_actions().collect());
    assert_ne!(old, new);
    assert!(engine.venue().observed.is_empty());
    signed(&mut engine, 1_102., old, old_work.receipt);
    assert!(engine.drain_actions().next().is_none());
    assert!(engine.venue().observed.is_empty());
    signed(&mut engine, 1_103., new, work.receipt);
    let packets = engine
        .drain_actions()
        .filter(|action| matches!(action, Action::Http(_)))
        .count();
    assert_eq!(packets, 1);
    assert_eq!(engine.venue().observed.len(), 1);
    assert_eq!(engine.venue().observed[0].receipt.expiry_ms, Some(70_101.));
    assert_eq!(engine.venue().observed[0].raw_expiry, Some(61_101.));
}
