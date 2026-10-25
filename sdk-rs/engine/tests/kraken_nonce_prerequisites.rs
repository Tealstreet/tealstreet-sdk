//! Baseline controls: serial protective custody grants no parallel nonce authority.
#![cfg(all(
    feature = "test-support",
    feature = "dispatch-nonce",
    feature = "http-mutation-barriers"
))]

use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{KRAKEN, NonceClaim};
use safe_cex_engine::request_context::{
    Context, Error, HttpAction, Operation, PlanPolicy, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, MutationBarrierBound, RetryWindow, StepBound},
};
use safe_cex_engine::signing::{
    Algorithm, Identities, Identity, KeyRef, Operation as SignOperation, PayloadPart, SignError,
    SignFuture, SignReceipt, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, Cx, DispatchContext, Engine, Event, HttpRelease, Method, Response, SignStep,
    SocketId, TimerId, Venue,
};
use std::sync::Arc;

fn identity() -> Identity {
    Identity {
        credential: KeyRef("kraken-prerequisite/credential".into()),
        signing: KeyRef("kraken-prerequisite/signing".into()),
        passphrase: None,
        stable_id: Some("kraken-prerequisite/account".into()),
        extra: Default::default(),
    }
}

struct Recipe(NonceClaim);
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        Some(&self.0)
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = dispatch
            .reserve_nonce(&self.0)
            .map_err(|_| SignError::Unavailable)?;
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        let body = format!("nonce={nonce}");
        request.body = Some(body.clone());
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: Some(nonce),
                stamp_ms: Some(dispatch.stamp_ms()),
                expiry_ms: None,
            },
            context: Default::default(),
            operations: vec![SignOperation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: identity().signing,
                payload: vec![PayloadPart::Bytes {
                    bytes: body.into_bytes(),
                }],
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
}
struct Provider;
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}

struct Probe<const NONCE: bool> {
    barrier: bool,
    entered: bool,
    admission: Option<Result<(), Error>>,
    queued: usize,
}
impl<const NONCE: bool> Venue for Probe<NONCE> {
    type Pending = Context;
    fn dispatch_nonce() -> bool {
        NONCE
    }
    fn http_release() -> HttpRelease<Self> {
        HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        self.entered = true;
        let slots = Slots::new(Some(identity()), None, SlotPolicy::default());
        let (selection, selected, _) = slots
            .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
            .unwrap();
        let credential = selection.credential(selected.credential.clone());
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential.clone()),
            })
            .unwrap();
        let call = || CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        };
        let calls = [call(), call()];
        let steps = [StepBound {
            wait_ms: 0,
            calls: &calls,
        }];
        let result = cx.admit_plan_with_barriers(
            &preflight,
            &steps,
            PlanPolicy {
                operation: Operation::Mutation,
                credential: Some(credential),
                pacing: None,
                retry: RetryPolicy::Never,
                independent_calls: false,
            },
            if self.barrier {
                &[MutationBarrierBound { step: 0 }]
            } else {
                &[]
            },
        );
        match result {
            Err(error) => {
                self.admission = Some(Err(error));
                cx.fail_preflight(&preflight, error.venue_error()).unwrap();
            }
            Ok(plan) => {
                self.admission = Some(Ok(()));
                for slot in cx.plan_slots(&plan).unwrap() {
                    let context = cx.plan_leg_context(&plan, &slot).unwrap();
                    let (_, _, claim) = slots
                        .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
                        .unwrap();
                    cx.request_signed_managed(
                        context.clone(),
                        None,
                        Method::Post,
                        "https://nonce-prerequisite.test/protective-member".into(),
                        None,
                        Box::new(Recipe(claim)),
                        context,
                    )
                    .unwrap();
                    self.queued += 1;
                }
            }
        }
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn run<const NONCE: bool>(barrier: bool) -> Engine<Probe<NONCE>> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        barrier,
        entered: false,
        admission: None,
        queued: 0,
    });
    engine
        .set_identities(Identities {
            primary: Some(identity()),
            secondary: None,
        })
        .unwrap();
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(1_000.);
    engine.handle(
        1_001.,
        Event::Command(Command::SetLeverage {
            command: 73,
            symbol: "BTC-USD".into(),
            leverage: 5.,
            side: None,
        }),
    );
    assert!(
        engine.venue().entered,
        "the genuine engine command must reach its venue"
    );
    engine
}
#[test]
fn parallel_nonce_barriers_remain_refused_pending_a_separate_shared_contract() {
    let mut engine = run::<true>(true);
    assert_eq!(engine.venue().admission, Some(Err(Error::Invalid)));
    assert_eq!(engine.venue().queued, 0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
}
#[test]
fn default_nonce_unscheduled_without_permission_keeps_its_barrier_refusal() {
    let mut engine = run::<false>(true);
    assert_eq!(engine.venue().admission, Some(Err(Error::Invalid)));
    assert_eq!(engine.venue().queued, 0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
}
#[test]
fn the_existing_nonce_plan_without_a_barrier_remains_a_working_control() {
    let mut engine = run::<true>(false);
    assert_eq!(engine.venue().admission, Some(Ok(())));
    assert_eq!(engine.venue().queued, 2);
    assert_eq!(engine.drain_actions().filter(|a| matches!(a, Action::ManagedHttp(h) if matches!(h.as_ref(), HttpAction::Prepare { .. }))).count(), 2);
}
