//! Credential-only setup custody (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md C4, C4p):
//! a signing call whose ref is absent refuses in preparation, before any provider
//! work, and releases everything it held.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Plan, PlanPolicy, Policy,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy, Scope,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    Identity, KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, ErrorKind, Event, HttpRequest,
    Method, Notice, Outcome, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Takes its key through `Identity::signing_ref()`: an absent ref refuses in
/// preparation, before any provider work.
struct Recipe(Identity);
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        3_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let key = self.0.signing_ref()?.clone();
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 3_000.),
            },
            context: Default::default(),
            operations: vec![SignOperation::Sign {
                algorithm: safe_cex_engine::signing::Algorithm::HmacSha256,
                key,
                payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                    bytes: b"preimage".to_vec(),
                }],
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
}

fn credential() -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: KeyRef("primary.credential".into()),
    }
}
fn signing(signed: bool) -> Identity {
    Identity::new(
        KeyRef("primary.credential".into()),
        signed.then(|| KeyRef("primary.signing".into())),
    )
}

#[derive(Default)]
struct Probe {
    /// Commands whose single signed Mutation has a signing ref.
    signed: Vec<u32>,
    /// C4p: a two-step Mutation Plan whose second call has no signing ref.
    plan: Option<(Plan, Slot)>,
    plan_mode: bool,
    failures: Vec<(Error, VenueError)>,
}
impl Probe {
    fn plan_call(&mut self, cx: &mut Cx<'_, Context>, signed: bool) {
        let plan = self.plan.as_ref().unwrap().0.clone();
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            "https://venue.test/write".into(),
            None,
            Box::new(Recipe(signing(signed))),
            context,
        )
        .map_err(|(_, error)| error)
        .unwrap();
        self.plan = Some((plan, slot));
    }
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let Command::AmendOrder { command, .. } = command else {
            panic!("fixture issues amends only")
        };
        if self.plan_mode {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential()),
                })
                .unwrap();
            let calls = [CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            let steps = [
                StepBound {
                    wait_ms: 0,
                    calls: &calls,
                },
                StepBound {
                    wait_ms: 0,
                    calls: &calls,
                },
            ];
            let plan = cx
                .admit_plan(
                    &preflight,
                    &steps,
                    PlanPolicy {
                        independent_calls: false,
                        operation: Operation::Mutation,
                        credential: Some(credential()),
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            self.plan = Some((plan, slot));
            self.plan_call(cx, true);
            return;
        }
        let context = cx
            .http_context(
                Scope::Command(command),
                Policy {
                    operation: Operation::Mutation,
                    credential: Some(credential()),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            format!("https://venue.test/amend-{command}"),
            None,
            Box::new(Recipe(signing(self.signed.contains(&command)))),
            context,
        )
        .map_err(|(_, error)| error)
        .unwrap();
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        pending: Context,
        reason: Error,
        error: VenueError,
    ) {
        self.failures.push((reason, error.clone()));
        match &self.plan {
            Some((plan, _)) => cx.fail_plan(plan, error).unwrap(),
            None => cx.fail_context(&pending, error).unwrap(),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.status, 200);
        let (plan, slot) = self
            .plan
            .clone()
            .expect("only the Plan's first call is answered");
        cx.validate_plan_delivery(&plan, &context).unwrap();
        cx.http_write_fact(
            &context,
            WriteFact::Accepted {
                order_ids: vec![response.body.into()],
            },
            None,
        )
        .unwrap();
        cx.validate_http_write_effects(&context).unwrap();
        cx.finish_plan_call(&plan, &slot, &context).unwrap();
        self.plan_call(cx, false);
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("custody-native".into())
    }
    fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {}
    fn on_sign_error(&mut self, _: &mut Cx<'_, Context>, _: Context, error: VenueError) {
        panic!("managed preparation failures use their own hook: {error:?}")
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("typed fixture failure required: {error}")
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {
        panic!("no fixture socket")
    }
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        panic!("no fixture socket")
    }
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {
        panic!("no fixture socket")
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

const NOW: f64 = 1_000.;

/// Immediate provider; counts every request it is asked to sign.
struct Provider(Arc<AtomicUsize>);
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(Ok(SignResult::new(vec![vec![7; 32]], work.receipt)))
    }
}

fn engine(probe: Probe) -> (Engine<Probe>, Arc<AtomicUsize>) {
    let provided = Arc::new(AtomicUsize::new(0));
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine
        .set_signer(Some(Arc::new(Provider(provided.clone()))))
        .unwrap();
    engine.start(NOW);
    engine.drain_actions().for_each(drop);
    (engine, provided)
}
fn amend(engine: &mut Engine<Probe>, at: f64, command: u32) -> Vec<Action> {
    engine.handle(
        at,
        Event::Command(Command::AmendOrder {
            command,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    prepared_actions(engine, at)
}
fn http(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
fn failed(actions: &[Action], command: u32) -> Option<VenueError> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
            Some(error.clone())
        }
        _ => None,
    })
}

/// C4: the refused prepare reaches no provider and releases no HTTP, fails its
/// command InvalidInput / NotSent, and releases its reservation and Context: a
/// peer signed command afterwards still signs and releases.
#[test]
fn c4_absent_signing_ref_refuses_in_preparation_and_releases_custody() {
    let (mut e, provided) = engine(Probe {
        signed: vec![2],
        ..Default::default()
    });
    let out = amend(&mut e, NOW, 1);
    assert_eq!(provided.load(Ordering::SeqCst), 0, "no provider work");
    assert!(http(&out).is_empty(), "no HTTP for an absent ref: {out:?}");
    let error = failed(&out, 1).expect("command 1 terminal");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
    assert_eq!(e.venue().failures.len(), 1);

    let peer = amend(&mut e, NOW + 1., 2);
    assert_eq!(
        provided.load(Ordering::SeqCst),
        1,
        "the peer reaches the provider"
    );
    assert_eq!(http(&peer).len(), 1, "the peer releases: {peer:?}");
    assert_eq!(failed(&peer, 2), None);
}

/// C4p: in a sequential Mutation Plan, the first signed call settles Accepted,
/// then a second call without a signing ref refuses NotSent in preparation. The
/// command projection keeps the settled call's facts: not NotSent, not Rejected.
#[test]
fn c4p_plan_keeps_a_settled_call_when_a_later_call_lacks_a_signing_ref() {
    let (mut e, provided) = engine(Probe {
        plan_mode: true,
        ..Default::default()
    });
    let out = amend(&mut e, NOW, 1);
    let [request] = http(&out)[..] else {
        panic!("the first call releases: {out:?}")
    };
    let request = request.clone();
    assert_eq!(provided.load(Ordering::SeqCst), 1);
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "settled-native-id",
            },
        },
    );
    let out = prepared_actions(&mut e, NOW + 1.);
    assert_eq!(
        provided.load(Ordering::SeqCst),
        1,
        "no provider work for an absent ref"
    );
    assert!(http(&out).is_empty(), "no HTTP for an absent ref: {out:?}");
    assert_eq!(e.venue().failures.len(), 1);
    let error = failed(&out, 1).expect("command 1 terminal");
    assert_ne!(
        error.outcome,
        Some(Outcome::NotSent),
        "a settled Accepted call is not NotSent: {error:?}"
    );
    assert_ne!(
        error.outcome,
        Some(Outcome::Rejected),
        "a settled Accepted call is not Rejected: {error:?}"
    );
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["settled-native-id".to_string()]),
        "the settled call's native id survives: {error:?}"
    );
}
