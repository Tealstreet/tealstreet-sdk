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
    let mut identity = Identity::new(
        KeyRef("kraken-prerequisite/credential".into()),
        Some(KeyRef("kraken-prerequisite/signing".into())),
    );
    identity.stable_id = Some("kraken-prerequisite/account".into());
    identity
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
                key: identity().signing_ref()?.clone(),
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
    fn http_mutation_barriers() -> bool {
        true
    }
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

#[cfg(feature = "serial-plans")]
mod serial {
    use super::*;
    use safe_cex_engine::classify::{
        CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
    };
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, StopTriggerType};
    use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
    use safe_cex_engine::request_context::{
        Claim, Plan, Preparation, Prepared, ReconciliationData,
    };
    use safe_cex_engine::{ErrorKind, HttpRequest, Notice, Outcome, VenueError};

    const COMMAND: u32 = 74;
    const REFUSALS: CodeTable = CodeTable {
        rows: &[Row {
            key: Key::Text("rejected"),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        }],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    };
    fn call() -> SerialCall {
        SerialCall {
            operation: Operation::Mutation,
            timeout_ms: 20_000,
            attempts: 1,
        }
    }
    fn original() -> Order {
        Order {
            id: "native-7".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSD".into(),
            kind: OrderType::StopLoss,
            side: OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: true,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: Some(StopTriggerType::MarkPrice),
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }
    struct SerialProbe {
        identity: Identity,
        plan: Option<Plan>,
        slot: Option<Slot>,
        context: Option<Context>,
        original: Order,
        placed: bool,
        timer: Option<TimerId>,
        custody: Vec<Result<(), Error>>,
        facts: usize,
        responses: usize,
    }
    impl SerialProbe {
        fn issue(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.as_ref().unwrap();
            let slot = self.slot.as_ref().unwrap();
            let context = if self.placed {
                cx.plan_leg_context_with_intent(plan, slot, Intent::Place)
            } else {
                cx.plan_protection_cancel_leg_context(plan, slot)
            }
            .unwrap();
            let (_, _, claim) =
                Slots::new(Some(self.identity.clone()), None, SlotPolicy::default())
                    .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
                    .unwrap();
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                format!(
                    "https://nonce-prerequisite.test/{}",
                    if self.placed { "place" } else { "cancel" }
                ),
                None,
                Box::new(Recipe(claim)),
                context.clone(),
            )
            .unwrap();
            self.context = Some(context);
        }
        fn probe(&mut self, cx: &mut Cx<'_, Context>, context: &Context) {
            self.custody.push(
                cx.continue_serial_plan(
                    self.plan.as_ref().unwrap(),
                    self.slot.as_ref().unwrap(),
                    context,
                    call(),
                )
                .map(drop),
            );
        }
        fn fail(&self, cx: &mut Cx<'_, Context>, context: &Context) {
            let plan = self.plan.as_ref().unwrap();
            cx.finish_plan_call(plan, self.slot.as_ref().unwrap(), context)
                .unwrap();
            cx.fail_plan(
                plan,
                VenueError::new("protective replacement suppressed").outcome(Outcome::NotSent),
            )
            .unwrap();
        }
    }
    impl Venue for SerialProbe {
        fn http_mutation_barriers() -> bool {
            true
        }
        type Pending = Context;
        fn dispatch_nonce() -> bool {
            true
        }
        fn http_release() -> HttpRelease<Self> {
            HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
        }
        fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
            ReconciliationData::new("native-7".into())
        }
        fn start(&mut self, cx: &mut Cx<'_, Context>) {
            self.timer = Some(cx.set_timer(1.));
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            let slots = Slots::new(Some(self.identity.clone()), None, SlotPolicy::default());
            let (selection, identity, _) = slots
                .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
                .unwrap();
            let credential = selection.credential(identity.credential.clone());
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential.clone()),
                })
                .unwrap();
            let (plan, slot) = cx
                .admit_serial_plan(
                    &preflight,
                    call(),
                    SerialPolicy {
                        credential: Some(credential),
                        pacing: None,
                        refusals_complete: false,
                    },
                )
                .unwrap();
            self.plan = Some(plan);
            self.slot = Some(slot);
            self.issue(cx);
            self.probe(cx, &self.context.clone().unwrap());
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            let plan = self.plan.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            self.responses += 1;
            self.probe(cx, &context);
            let accepted = response.body == "accepted";
            let grant = if accepted {
                cx.http_write_fact(
                    &context,
                    WriteFact::Accepted {
                        order_ids: vec![
                            if self.placed {
                                "replacement-8"
                            } else {
                                "native-7"
                            }
                            .into(),
                        ],
                    },
                    None,
                )
                .unwrap()
            } else {
                let native = Native {
                    code: NativeCode::Text(response.body),
                    message: "cancel failed",
                };
                cx.http_write_failure(&context, &REFUSALS, Some(&native), "cancel failed", None)
                    .unwrap()
                    .1
            };
            assert!(
                grant.is_none(),
                "the native protective pair has no write resend"
            );
            self.facts += 1;
            let old = &self.original;
            let unchanged = cx.store.order(&old.id).is_some_and(|current| {
                current.filled == old.filled
                    && current.price == old.price
                    && current.amount == old.amount
                    && current.kind == old.kind
                    && current.trigger_type == old.trigger_type
                    && current.status != OrderStatus::Closed
            });
            if self.placed {
                cx.finish_plan_call(&plan, self.slot.as_ref().unwrap(), &context)
                    .unwrap();
                cx.done_plan(&plan, "[\"replacement-8\"]".into()).unwrap();
            } else if accepted && unchanged {
                let slot = cx
                    .continue_serial_plan(&plan, self.slot.as_ref().unwrap(), &context, call())
                    .unwrap();
                self.slot = Some(slot);
                self.placed = true;
                self.issue(cx);
            } else {
                self.fail(cx, &context);
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            let plan = self.plan.as_ref().unwrap();
            cx.finish_plan_failed_call(plan, self.slot.as_ref().unwrap(), &context)
                .unwrap();
            cx.fail_plan(plan, "cancel transport failed").unwrap();
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
            self.probe(cx, &self.context.clone().unwrap());
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn engine(scenario: &str) -> Engine<SerialProbe> {
        let mut installed = identity();
        installed.stable_id = Some(format!("kraken-prerequisite/{scenario}"));
        let mut e = safe_cex_engine::test_support::seeded_engine(SerialProbe {
            identity: installed.clone(),
            plan: None,
            slot: None,
            context: None,
            original: original(),
            placed: false,
            timer: None,
            custody: vec![],
            facts: 0,
            responses: 0,
        });
        e.set_identities(Identities {
            primary: Some(installed),
            secondary: None,
        })
        .unwrap();
        e.set_signer(Some(Arc::new(Provider))).unwrap();
        e.store_mut().upsert_order(original());
        e.start(1_000.);
        e.handle(
            1_001.,
            Event::Command(Command::AmendOrder {
                command: COMMAND,
                id: "native-7".into(),
                price: Some(2.),
                amount: None,
            }),
        );
        e
    }
    fn preparation(actions: &[Action]) -> Preparation {
        let routes = actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(routes.len(), 1, "exactly one serial call may prepare");
        routes[0]
    }
    fn no_egress(actions: &[Action]) {
        assert!(!actions.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(h) => matches!(
                h.as_ref(),
                HttpAction::Prepare { .. } | HttpAction::Bind { .. } | HttpAction::BindGroup { .. }
            ),
            _ => false,
        }));
        assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
    }
    fn release(e: &mut Engine<SerialProbe>, token: Preparation, now: f64) -> (u64, HttpRequest) {
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let work = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, work, .. } => Some((attempt, work)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(work.len(), 1);
        let (attempt, request) = work.into_iter().next().unwrap();
        let nonce = request.receipt.nonce.unwrap();
        e.handle(
            now,
            Event::Signed {
                attempt,
                result: Ok(SignResult::new(vec![vec![7; 32]], request.receipt)),
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let mut bound = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind {
                        permit, request, ..
                    } => Some((permit, request)),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(bound.len(), 1);
        let (permit, request) = bound.remove(0);
        assert!(
            matches!(e.claim_http(now, permit), Ok(Claim::Released(release)) if release.signature.nonce == Some(nonce))
        );
        (nonce, request)
    }
    fn reply(e: &mut Engine<SerialProbe>, token: Preparation, body: &str) -> Vec<Action> {
        e.handle(
            1_005.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        e.drain_actions().collect()
    }
    fn failed(actions: &[Action]) -> &VenueError {
        let failures = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: COMMAND,
                    error,
                }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 1);
        failures[0]
    }
    #[test]
    fn replacement_requires_its_cancel_data_and_a_recorded_native_fact() {
        let mut e = engine("serial-custody");
        let actions = e.drain_actions().collect::<Vec<_>>();
        let cancel = preparation(&actions);
        assert_eq!(e.venue().custody, vec![Err(Error::Revoked)]);
        let (_, request) = release(&mut e, cancel, 1_003.);
        assert!(request.url.ends_with("/cancel"));
        no_egress(&e.drain_actions().collect::<Vec<_>>());
        let timer = e.venue().timer.unwrap();
        e.handle(1_004., Event::TimerFired { timer });
        assert_eq!(e.venue().custody, vec![Err(Error::Revoked); 2]);
        no_egress(&e.drain_actions().collect::<Vec<_>>());
        let actions = reply(&mut e, cancel, "accepted");
        preparation(&actions);
        assert_eq!(e.venue().custody.last(), Some(&Err(Error::State)));
        assert_eq!(e.venue().facts, 1);
        assert!(e.venue().placed);
        no_egress(&reply(&mut e, cancel, "accepted"));
        assert_eq!(
            e.venue().responses,
            1,
            "a retired cancel reply cannot admit another replacement"
        );
    }
    #[test]
    fn replacement_reserves_only_after_cancel_data_and_uses_a_higher_nonce() {
        let mut e = engine("serial-nonce-order");
        let cancel = preparation(&e.drain_actions().collect::<Vec<_>>());
        let (cancel_nonce, _) = release(&mut e, cancel, 1_003.);
        no_egress(&e.drain_actions().collect::<Vec<_>>());
        let place = preparation(&reply(&mut e, cancel, "accepted"));
        assert_ne!(place.attempt(), cancel.attempt());
        let (place_nonce, request) = release(&mut e, place, 1_006.);
        assert!(place_nonce > cancel_nonce);
        assert!(request.url.ends_with("/place"));
    }
    #[test]
    fn rejected_and_unknown_cancel_seal_without_replacement_or_write_resend() {
        for (body, outcome) in [
            ("rejected", Outcome::Rejected),
            ("unknown", Outcome::Unknown),
        ] {
            let mut e = engine(body);
            let cancel = preparation(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, cancel, 1_003.);
            e.drain_actions().for_each(drop);
            let actions = reply(&mut e, cancel, body);
            no_egress(&actions);
            assert_eq!(failed(&actions).outcome, Some(outcome));
            assert!(!e.venue().placed);
            assert_eq!(e.venue().facts, 1);
        }
    }
    #[test]
    fn released_cancel_transport_failure_is_unknown_and_never_resends() {
        let mut e = engine("serial-transport-failure");
        let cancel = preparation(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, cancel, 1_003.);
        e.drain_actions().for_each(drop);
        e.handle(
            1_004.,
            Event::HttpFailed {
                request: cancel.attempt().request,
                error: "lost ACK",
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        no_egress(&actions);
        assert_eq!(failed(&actions).outcome, Some(Outcome::Unknown));
        assert!(!e.venue().placed);
        assert_eq!(
            e.venue().facts,
            0,
            "transport failure cannot invent a native answer"
        );
    }
    #[test]
    fn execution_or_changed_provenance_during_cancel_preserves_accepted_cancel_and_suppresses_place()
     {
        for field in ["filled", "price", "amount", "kind", "trigger", "closed"] {
            let mut e = engine(field);
            let cancel = preparation(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, cancel, 1_003.);
            e.drain_actions().for_each(drop);
            let mut changed = original();
            match field {
                "filled" => changed.filled = 0.5,
                "price" => changed.price = 2.,
                "amount" => changed.amount = 2.,
                "kind" => changed.kind = OrderType::Limit,
                "trigger" => changed.trigger_type = Some(StopTriggerType::LastPrice),
                "closed" => changed.status = OrderStatus::Closed,
                _ => unreachable!(),
            }
            e.store_mut().upsert_order(changed);
            let actions = reply(&mut e, cancel, "accepted");
            no_egress(&actions);
            let error = failed(&actions);
            assert_ne!(
                error.outcome,
                Some(Outcome::NotSent),
                "the accepted cancel cannot become NotSent"
            );
            assert_eq!(error.partial.as_ref().unwrap().order_ids, ["native-7"]);
            assert!(!e.venue().placed);
            assert_eq!(e.venue().facts, 1);
        }
    }
}
