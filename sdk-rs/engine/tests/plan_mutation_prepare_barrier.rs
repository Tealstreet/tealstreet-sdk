//! G2b original asynchronous preparation-barrier controls.
#![cfg(all(feature = "test-support", feature = "http-mutation-barriers"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, MutationBarrierBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt, SignRequest,
    SignResult, Signer,
};

use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    Notice, Response, SignStep, SocketId, TimerId, Venue,
};
use std::{collections::HashMap, sync::Arc};

fn credential(slot: CredentialSlot) -> Credential {
    Credential {
        slot,
        reference: KeyRef(
            match slot {
                CredentialSlot::Primary => "primary-ref",
                CredentialSlot::Secondary => "secondary-ref",
            }
            .into(),
        ),
    }
}
fn prepared_actions(engine: &mut Engine<Consumer>, now: f64) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, GroupClaim, HttpAction, Prepared};
    let mut queue = std::collections::VecDeque::new();
    engine.prepare_dispatch(now);
    queue.extend(engine.drain_actions());
    let mut actions = Vec::new();
    let mut steps = 0;
    while let Some(action) = queue.pop_front() {
        steps += 1;
        assert!(steps < 1024);
        if let Action::ManagedHttp(action) = action {
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert_eq!(request.id, preparation.attempt().request);
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(request.id, preparation.attempt().request);
                    assert_eq!(permit.prepared_id(), request.id.0);
                    match engine.claim_http(now, permit) {
                        Ok(Claim::Released(_)) => actions.push(Action::Http(request)),
                        Ok(Claim::WaitUntil(_)) => panic!("fixture has no pacing"),
                        Err(_) => {}
                    }
                }
                HttpAction::BindGroup { permit, members } => {
                    if !engine.http_group_pending(&permit) {
                        continue;
                    }
                    let requests: Vec<_> = members
                        .into_iter()
                        .map(|mut member| member.request.take().expect("initial group packet"))
                        .collect();
                    match engine.claim_http_group(now, permit) {
                        Ok(GroupClaim::Released(_)) => {
                            actions.extend(requests.into_iter().map(Action::Http))
                        }
                        Ok(GroupClaim::WaitUntil(_)) => panic!("fixture has no group pacing"),
                        Err(_) => {}
                    }
                }
                HttpAction::Cancel { .. } => {}
            }
        } else {
            actions.push(action);
        }
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    actions
}
struct Provider;
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}
type EntropyLog = Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;
struct Recipe(KeyRef, f64, Option<EntropyLog>);
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        self.1
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if let Some(log) = &self.2 {
            let Action::Http(request) = outbound else {
                return Err(SignError::InvalidRequest);
            };
            let mut nonce = vec![0; 21];
            cx.signing_bytes(&mut nonce)?;
            log.lock()
                .unwrap()
                .push((request.body.clone().unwrap(), nonce));
        }
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + self.1),
            },
            context: Default::default(),
            operations: vec![
                SignOperation::Present {
                    key: self.0.clone(),
                },
                SignOperation::Sign {
                    algorithm: safe_cex_engine::signing::Algorithm::HmacSha256,
                    key: KeyRef("signing-ref".into()),
                    payload: vec![safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: b"group-preimage".to_vec(),
                    }],
                },
            ],
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.signer_headers.push((
            "x-selected-ref".into(),
            String::from_utf8(result.values[0].clone()).map_err(|_| SignError::InvalidResult)?,
        ));
        Ok(())
    }
}
#[derive(Default)]
struct Consumer {
    pair: bool,
    plan: Option<Plan>,
    slots: HashMap<u64, Slot>,
    contexts: Vec<Context>,
    replies: usize,
    independent: bool,
    preparation_attack: bool,
    attack_results: Vec<safe_cex_engine::request_context::Error>,
    failures: usize,
    reconciles: usize,
    warmup: bool,
    placing: bool,
    stagger_freshness: bool,
    entropy_log: Option<EntropyLog>,
    /// The first group member binds this Session (refused until R11).
    bind_session: bool,
    session: Option<safe_cex_engine::request_context::Session>,
    issue_errors: Vec<safe_cex_engine::request_context::Error>,
    /// The pending each refused issue returned, and whether the context matched.
    returned_own_pending: Vec<bool>,
    /// A following non-group request bound to the same Session.
    follow_up: Option<Result<safe_cex_engine::RequestId, safe_cex_engine::request_context::Error>>,
}
impl Consumer {
    fn issue(&mut self, cx: &mut Cx<'_, Context>) {
        let plan = self.plan.as_ref().unwrap();
        for slot in cx.plan_slots(plan).unwrap() {
            let context = cx.plan_leg_context(plan, &slot).unwrap();
            let (method, path, body) = if self.pair {
                (
                    Method::Post,
                    "/api/v1/account/set-leverage",
                    Some(format!(
                        "{{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"{}\"}}",
                        if self.contexts.is_empty() {
                            "long"
                        } else {
                            "short"
                        }
                    )),
                )
            } else if self.replies == 0 {
                (
                    Method::Post,
                    "/api/v1/trade/cancel-batch-orders",
                    Some("[{\"instId\":\"BTC-USDT\",\"orderId\":\"native-7\"}]".into()),
                )
            } else {
                (
                    Method::Get,
                    "/api/v1/trade/order-detail?instId=BTC-USDT&orderId=native-7",
                    None,
                )
            };
            let reference = context.credential().unwrap().reference.clone();
            self.slots.insert(context.leg(), slot);
            self.contexts.push(context.clone());
            let bearer = self.session.clone().filter(|_| self.bind_session);
            let issued = cx.request_signed_managed(
                context.clone(),
                bearer.as_ref(),
                method,
                format!("https://openapi.blofin.com{path}"),
                body,
                Box::new(Recipe(
                    reference,
                    if self.stagger_freshness && self.contexts.len() == 2 {
                        10_000.
                    } else {
                        5_000.
                    },
                    self.entropy_log.clone(),
                )),
                context,
            );
            if let Err((returned, error)) = issued {
                assert!(self.bind_session, "unexpected issue refusal: {error:?}");
                self.issue_errors.push(error);
                self.returned_own_pending
                    .push(self.contexts.last() == Some(&returned));
                // The refusal left the Session's index usable: an ordinary
                // bound read on it issues.
                let read = cx
                    .http_context(
                        safe_cex_engine::request_context::Scope::Background,
                        safe_cex_engine::request_context::Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                self.follow_up = Some(
                    cx.request_managed(
                        read.clone(),
                        self.session.as_ref(),
                        Method::Get,
                        "https://venue.test/after".into(),
                        None,
                        read,
                    )
                    .map_err(|(_, error)| error),
                );
                return;
            }
        }
    }
}
impl Venue for Consumer {
    type Pending = Context;
    fn http_pacing(&self) -> &[safe_cex_engine::request_context::Pacing] {
        &[safe_cex_engine::request_context::Pacing {
            group: 77,
            minimum_gap_ms: 0.,
        }]
    }
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        if self.bind_session {
            self.session = Some(
                cx.open_session(safe_cex_engine::request_context::SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                    private_egress: None,
                })
                .unwrap(),
            );
        }
        if self.warmup {
            let context = cx
                .http_context(
                    safe_cex_engine::request_context::Scope::Background,
                    safe_cex_engine::request_context::Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: Some(safe_cex_engine::request_context::Pacing {
                            group: 77,
                            minimum_gap_ms: 100.,
                        }),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context.clone(),
                None,
                Method::Get,
                "https://venue.test/clock".into(),
                None,
                context,
            )
            .unwrap();
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: Some(credential(CredentialSlot::Secondary)),
                write_credential: Some(credential(CredentialSlot::Primary)),
            })
            .unwrap();
        let call = || CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        };
        let parallel = [call(), call()];
        let serial = [call()];
        let steps = if self.pair {
            vec![StepBound {
                wait_ms: 0,
                calls: &parallel,
            }]
        } else {
            (0..3)
                .map(|_| StepBound {
                    wait_ms: 0,
                    calls: &serial,
                })
                .collect()
        };
        self.plan = Some(
            cx.admit_plan_with_barriers(
                &preflight,
                &steps,
                PlanPolicy {
                    independent_calls: self.independent,
                    operation: Operation::Mutation,
                    credential: Some(credential(CredentialSlot::Primary)),
                    pacing: self
                        .warmup
                        .then_some(safe_cex_engine::request_context::Pacing {
                            group: 77,
                            minimum_gap_ms: 0.,
                        }),
                    retry: RetryPolicy::Never,
                },
                if self.pair {
                    &[MutationBarrierBound { step: 0 }]
                } else {
                    &[]
                },
            )
            .unwrap(),
        );
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.status, 200);
        let plan = self.plan.as_ref().unwrap().clone();
        cx.validate_plan_delivery(&plan, &context).unwrap();
        // The baseline stops at Verify: never fabricate a money fact for its GET.
        assert!(self.pair || self.replies == 0);
        cx.http_write_fact(
            &context,
            WriteFact::Accepted {
                order_ids: if self.independent {
                    vec!["accepted-short".into()]
                } else {
                    vec![]
                },
            },
            None,
        )
        .unwrap();
        cx.finish_plan_call(&plan, &self.slots[&context.leg()], &context)
            .unwrap();
        self.replies += 1;
        if self.pair {
            if self.replies + self.failures == 2 {
                cx.done_plan(&plan, "[]".into()).unwrap();
            }
        } else {
            self.issue(cx);
        }
    }
    fn prepare_group_http_release(
        &self,
        _: &Context,
        _: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<safe_cex_engine::request_context::Admission, safe_cex_engine::request_context::Error>
    {
        Ok(safe_cex_engine::request_context::Admission::Unscheduled)
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("native-7".into())
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        _: safe_cex_engine::request_context::Error,
        _: safe_cex_engine::VenueError,
    ) {
        if self.preparation_attack {
            self.attack_results.push(
                cx.finish_plan_failed_call(
                    self.plan.as_ref().unwrap(),
                    &self.slots[&context.leg()],
                    &context,
                )
                .unwrap_err(),
            );
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        error: safe_cex_engine::VenueError,
    ) {
        assert!(self.independent);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        self.failures += 1;
        cx.finish_plan_failed_call(
            self.plan.as_ref().unwrap(),
            &self.slots[&context.leg()],
            &context,
        )
        .unwrap();
        if self.replies + self.failures == 2 {
            cx.done_plan(self.plan.as_ref().unwrap(), "[]".into())
                .unwrap();
        }
    }
    fn reconcile_uncertain(&mut self, _: &mut Cx<'_, Context>, _: Option<&[String]>) {
        self.reconciles += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, message: &str) {
        let _ = message;
    }
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
fn start(pair: bool) -> Engine<Consumer> {
    start_consumer(Consumer {
        pair,
        ..Default::default()
    })
}
fn start_consumer(consumer: Consumer) -> Engine<Consumer> {
    let pair = consumer.pair;
    let placing = consumer.placing;
    let mut engine = support::seeded_engine(consumer);
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    let command = if placing {
        Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }
    } else if pair {
        Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }
    } else {
        Command::AmendOrder {
            command: 7,
            id: "native-7".into(),
            price: Some(1.),
            amount: None,
        }
    };
    engine.handle(1., Event::Command(command));
    engine
}
fn reply(engine: &mut Engine<Consumer>, request: &HttpRequest, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "{\"code\":\"0\",\"data\":[]}",
            },
        },
    );
}

fn work() -> (
    Engine<Consumer>,
    Vec<(safe_cex_engine::RequestId, SignRequest)>,
) {
    let mut engine = start(true);
    let actions = prepared_actions(&mut engine, 1.);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let work = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(work.len(), 2);
    (engine, work)
}
fn signed(
    engine: &mut Engine<Consumer>,
    work: &(safe_cex_engine::RequestId, SignRequest),
    now: f64,
) {
    engine.handle(
        now,
        Event::Signed {
            attempt: work.0,
            result: Ok(SignResult::new(
                vec![b"remote-only-primary".to_vec(), vec![7; 32]],
                work.1.receipt,
            )),
        },
    );
}
fn http(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
#[test]
fn original_red_first_signature_cannot_release_while_sibling_is_pending() {
    let (mut engine, work) = work();
    signed(&mut engine, &work[0], 2.);
    let actions = prepared_actions(&mut engine, 2.);
    assert!(
        http(&actions).is_empty(),
        "all parallel Mutation preparations must succeed before either HTTP"
    );
}
#[test]
fn original_red_second_provider_refusal_leaves_zero_http_released() {
    let (mut engine, work) = work();
    signed(&mut engine, &work[0], 2.);
    let auth_failures = engine.health().auth_failures;
    let mut actions = prepared_actions(&mut engine, 2.);
    engine.handle(
        3.,
        Event::Signed {
            attempt: work[1].0,
            result: Err(SignError::Unavailable),
        },
    );
    actions.extend(prepared_actions(&mut engine, 3.));
    assert_eq!(engine.health().auth_failures, auth_failures);
    assert!(
        http(&actions).is_empty(),
        "second preparation refusal must not create one-sided leverage"
    );
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
}
#[test]
fn both_provider_results_release_both_and_reverse_replies_complete_once() {
    let (mut engine, work) = work();
    signed(&mut engine, &work[0], 2.);
    let mut actions = prepared_actions(&mut engine, 2.);
    signed(&mut engine, &work[1], 3.);
    actions.extend(prepared_actions(&mut engine, 3.));
    let requests = http(&actions);
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].body.as_deref(),
        Some("{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"long\"}")
    );
    assert_eq!(
        requests[1].body.as_deref(),
        Some("{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"short\"}")
    );
    for request in &requests {
        assert_eq!(
            request.signer_headers,
            [("x-selected-ref".into(), "remote-only-primary".into())]
        );
    }
    reply(&mut engine, requests[1], 4.);
    assert!(!prepared_actions(&mut engine, 4.).iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    reply(&mut engine, requests[0], 5.);
    let done = prepared_actions(&mut engine, 5.);
    assert_eq!(
        done.iter()
            .filter(|a| matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                )
            ))
            .count(),
        1
    );
    assert!(done.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")));
}

fn announced(
    consumer: Consumer,
    second_at: f64,
) -> (
    Engine<Consumer>,
    safe_cex_engine::request_context::GroupPermit,
    Vec<safe_cex_engine::request_context::GroupPacket>,
) {
    let mut engine = start_consumer(consumer);
    let actions = prepared_actions(&mut engine, 1.);
    let work: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    signed(&mut engine, &work[0], 2.);
    signed(&mut engine, &work[1], second_at);
    engine.prepare_dispatch(second_at);
    let mut group = None;
    for action in engine.drain_actions() {
        match action {
            Action::ManagedHttp(action) => {
                if let safe_cex_engine::request_context::HttpAction::BindGroup { permit, members } =
                    *action
                {
                    assert!(group.replace((permit, members)).is_none());
                }
            }
            Action::Http(_) => panic!("individual group release"),
            _ => {}
        }
    }
    let (permit, members) = group.unwrap();
    (engine, permit, members)
}
fn failed_once(
    actions: &[Action],
    outcome: safe_cex_engine::Outcome,
) -> &safe_cex_engine::VenueError {
    assert!(http(actions).is_empty());
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(outcome));
    errors[0]
}
#[test]
fn stale_announced_group_refuses_partial_completion_and_seals_once() {
    use safe_cex_engine::request_context::{Error, HttpAction};
    let (mut engine, permit, members) = announced(
        Consumer {
            pair: true,
            preparation_attack: true,
            ..Default::default()
        },
        3.,
    );
    engine.http_preparation_failed(
        4.,
        members[0].preparation,
        Error::Stale,
        safe_cex_engine::VenueError::new("stale bound packet"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    failed_once(&actions, safe_cex_engine::Outcome::NotSent);
    assert_eq!(engine.venue().attack_results, [Error::State]);
    assert_eq!(engine.venue().reconciles, 0);
    assert!(!engine.http_group_pending(&permit));
    assert!(matches!(
        engine.claim_http_group(5., permit),
        Err(Error::Revoked)
    ));
    assert!(engine.drain_actions().all(|a| !matches!(a, Action::ManagedHttp(action) if matches!(*action, HttpAction::Prepare { .. } | HttpAction::BindGroup { .. }))));
}
#[test]
fn invalid_time_and_individual_claims_are_inert_before_whole_release() {
    use safe_cex_engine::request_context::{Error, GroupClaim};
    let (mut engine, permit, _) = announced(
        Consumer {
            pair: true,
            ..Default::default()
        },
        3.,
    );
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            engine.claim_http_group(now, permit.clone()),
            Err(Error::Invalid)
        ));
        assert!(engine.http_group_pending(&permit));
        assert_eq!(engine.drain_actions().count(), 0);
    }
    for member in permit.members() {
        assert!(matches!(
            engine.claim_http(4., *member),
            Err(Error::Revoked)
        ));
        assert_eq!(engine.drain_actions().count(), 0);
    }
    assert!(
        matches!(engine.claim_http_group(4., permit.clone()), Ok(GroupClaim::Released(releases)) if releases.len() == 2)
    );
    assert!(matches!(
        engine.claim_http_group(4., permit),
        Err(Error::Revoked)
    ));
}
#[test]
fn precommit_deadline_is_timeout_not_sent_with_no_reconciliation() {
    let (mut engine, permit, _) = announced(
        Consumer {
            pair: true,
            ..Default::default()
        },
        3.,
    );
    assert!(engine.claim_http_group(20_001., permit).is_err());
    let actions: Vec<_> = engine.drain_actions().collect();
    let error = failed_once(&actions, safe_cex_engine::Outcome::NotSent);
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert_eq!(engine.venue().reconciles, 0);
}
#[test]
fn committed_independent_503_keeps_the_accepted_sibling_id() {
    let mut engine = start_consumer(Consumer {
        pair: true,
        independent: true,
        ..Default::default()
    });
    let work: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    signed(&mut engine, &work[0], 2.);
    signed(&mut engine, &work[1], 3.);
    let actions = prepared_actions(&mut engine, 3.);
    let requests = http(&actions);
    assert_eq!(requests.len(), 2);
    engine.handle(
        4.,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 503,
                headers: "",
                body: "unavailable",
            },
        },
    );
    let interim = prepared_actions(&mut engine, 4.);
    assert!(!interim.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
    reply(&mut engine, requests[1], 5.);
    let terminal = prepared_actions(&mut engine, 5.);
    let error = failed_once(&terminal, safe_cex_engine::Outcome::Unknown);
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["accepted-short"]
    );
    assert_eq!(engine.venue().failures, 1);
    assert_eq!(engine.venue().replies, 1);
}

#[test]
fn a_bound_stale_member_reprepares_without_replacing_its_sibling() {
    use safe_cex_engine::request_context::{Error, GroupClaim, HttpAction, Prepared};
    let (mut engine, old, mut retained) = announced(
        Consumer {
            pair: true,
            stagger_freshness: true,
            ..Default::default()
        },
        4_999.,
    );
    assert!(matches!(
        engine.claim_http_group(5_003., old.clone()),
        Err(Error::Stale)
    ));
    let mut preparations = Vec::new();
    for action in engine.drain_actions() {
        if let Action::ManagedHttp(action) = action {
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert_eq!(request.id, old.members()[0].attempt().request);
                    preparations.push(preparation);
                }
                HttpAction::BindGroup { .. } => panic!("reannounce before stale member prepares"),
                _ => {}
            }
        }
    }
    assert_eq!(preparations.len(), 1);
    let token = preparations[0];
    assert!(!engine.http_group_pending(&old));
    engine.http_prepared(
        5_003.,
        token,
        Prepared {
            id: 100,
            expires_at: None,
        },
    );
    engine.prepare_dispatch(5_003.);
    let work: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            Action::Http(_) => panic!("stale partial release"),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 1);
    signed(&mut engine, &work[0], 5_004.);
    engine.prepare_dispatch(5_004.);
    let mut groups = Vec::new();
    for action in engine.drain_actions() {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::BindGroup { permit, members } = *action
        {
            groups.push((permit, members));
        }
    }
    assert_eq!(groups.len(), 1);
    let (permit, mut members) = groups.pop().unwrap();
    assert!(members[0].request.is_some());
    assert!(members[1].request.is_none());
    assert_eq!(members[1].preparation, retained[1].preparation);
    assert_eq!(permit.members()[0].attempt(), old.members()[0].attempt());
    assert_eq!(permit.members()[1].attempt(), old.members()[1].attempt());
    let sibling = retained[1].request.take().unwrap();
    let fresh = members[0].request.take().unwrap();
    assert_eq!(sibling.signer_headers, fresh.signer_headers);
    assert!(
        matches!(engine.claim_http_group(5_005., permit), Ok(GroupClaim::Released(releases)) if releases.len() == 2)
    );
    assert!(matches!(
        engine.claim_http_group(5_005., old),
        Err(Error::Revoked)
    ));
    assert_eq!(engine.venue().replies, 0);
}

#[test]
fn reverse_routes_and_providers_preserve_declaration_order_entropy() {
    use safe_cex_engine::request_context::{GroupClaim, HttpAction, Prepared};
    let log = EntropyLog::default();
    let mut engine = start_consumer(Consumer {
        pair: true,
        entropy_log: Some(log.clone()),
        ..Default::default()
    });
    let routes: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(routes.len(), 2);
    engine.http_prepared(
        2.,
        routes[1],
        Prepared {
            id: 102,
            expires_at: None,
        },
    );
    engine.prepare_dispatch(2.);
    assert!(log.lock().unwrap().is_empty());
    assert!(
        engine
            .drain_actions()
            .all(|a| !matches!(a, Action::Sign { .. } | Action::Http(_)))
    );
    engine.http_prepared(
        3.,
        routes[0],
        Prepared {
            id: 101,
            expires_at: None,
        },
    );
    engine.prepare_dispatch(3.);
    let work: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    let entries = log.lock().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].0,
        "{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"long\"}"
    );
    assert_eq!(
        entries[1].0,
        "{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"short\"}"
    );
    let mut state = 29_u64;
    for (_, nonce) in entries.iter() {
        let expected: Vec<_> = (0..21)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        assert_eq!(*nonce, expected);
    }
    drop(entries);
    signed(&mut engine, &work[1], 4.);
    engine.prepare_dispatch(4.);
    assert!(
        engine
            .drain_actions()
            .all(|a| !matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    signed(&mut engine, &work[0], 5.);
    engine.prepare_dispatch(5.);
    let groups: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::BindGroup { permit, members } => Some((permit, members)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1);
    assert!(
        matches!(engine.claim_http_group(5., groups[0].0.clone()), Ok(GroupClaim::Released(releases)) if releases.len() == 2)
    );
    assert_eq!(log.lock().unwrap().len(), 2);
}

#[cfg(feature = "order-locks")]
#[test]
fn provider_refusal_leaves_no_placing_fence_or_reconciliation() {
    let mut engine = start_consumer(Consumer {
        pair: true,
        placing: true,
        ..Default::default()
    });
    let work: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    signed(&mut engine, &work[0], 2.);
    let mut actions = prepared_actions(&mut engine, 2.);
    let auth = engine.health().auth_failures;
    engine.handle(
        3.,
        Event::Signed {
            attempt: work[1].0,
            result: Err(SignError::Refused),
        },
    );
    actions.extend(prepared_actions(&mut engine, 3.));
    failed_once(&actions, safe_cex_engine::Outcome::NotSent);
    assert_eq!(engine.health().auth_failures, auth);
    assert_eq!(engine.venue().reconciles, 0);
    assert_eq!(engine.venue().contexts.len(), 2);
    // An uncertain placing scope would fence this global cancel before venue entry.
    engine.handle(
        4.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: String::new(),
        }),
    );
    assert_eq!(engine.venue().contexts.len(), 4);
    assert_eq!(engine.venue().reconciles, 0);
}

#[test]
fn zero_floor_group_waits_for_an_existing_shared_clock_before_signing() {
    let log = EntropyLog::default();
    let mut engine = support::seeded_engine(Consumer {
        pair: true,
        warmup: true,
        entropy_log: Some(log.clone()),
        ..Default::default()
    });
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    let warmup = prepared_actions(&mut engine, 0.);
    assert_eq!(http(&warmup).len(), 1);
    assert_eq!(http(&warmup)[0].url, "https://venue.test/clock");
    engine.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let waiting = prepared_actions(&mut engine, 1.);
    assert!(
        waiting
            .iter()
            .all(|action| !matches!(action, Action::Sign { .. } | Action::Http(_))),
        "zero floor cannot bypass a shared clock charged by an earlier outer gap"
    );
    assert!(log.lock().unwrap().is_empty());
    let timers: Vec<_> = waiting
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 100. => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1);
    engine.handle(100., Event::TimerFired { timer: timers[0] });
    let ready = prepared_actions(&mut engine, 100.);
    let work: Vec<_> = ready
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    assert!(
        work.iter()
            .all(|(_, work)| work.receipt.stamp_ms == Some(100.))
    );
    assert_eq!(log.lock().unwrap().len(), 2);
    signed(&mut engine, &work[1], 101.);
    assert!(http(&prepared_actions(&mut engine, 101.)).is_empty());
    signed(&mut engine, &work[0], 102.);
    assert_eq!(http(&prepared_actions(&mut engine, 102.)).len(), 2);
}

#[test]
fn group_pacing_wake_reprepares_an_expired_route_without_renewing_bounds() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    let log = EntropyLog::default();
    let mut engine = support::seeded_engine(Consumer {
        pair: true,
        warmup: true,
        entropy_log: Some(log.clone()),
        ..Default::default()
    });
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    assert_eq!(http(&prepared_actions(&mut engine, 0.)).len(), 1);
    engine.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let routes: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(routes.len(), 2);
    for (ordinal, token) in routes.iter().enumerate() {
        engine.http_prepared(
            1.,
            *token,
            Prepared {
                id: 300 + ordinal as u64,
                expires_at: (ordinal == 0).then_some(50.),
            },
        );
    }
    engine.prepare_dispatch(1.);
    let waiting: Vec<_> = engine.drain_actions().collect();
    assert!(log.lock().unwrap().is_empty());
    assert!(
        !waiting
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::Http(_)))
    );
    let timers: Vec<_> = waiting
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 50. => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1);
    engine.handle(25., Event::TimerFired { timer: timers[0] });
    let early: Vec<_> = engine.drain_actions().collect();
    assert!(log.lock().unwrap().is_empty());
    assert!(early.iter().any(
        |a| matches!(a, Action::SetTimer { timer, at_ms } if *timer == timers[0] && *at_ms == 50.)
    ));
    engine.handle(50., Event::TimerFired { timer: timers[0] });
    let expired: Vec<_> = engine.drain_actions().collect();
    assert!(log.lock().unwrap().is_empty());
    let fresh: Vec<_> = expired
        .into_iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(fresh.len(), 1);
    assert_ne!(fresh[0], routes[0]);
    assert_eq!(fresh[0].attempt(), routes[0].attempt());
    engine.http_prepared(
        60.,
        fresh[0],
        Prepared {
            id: 400,
            expires_at: None,
        },
    );
    let waiting: Vec<_> = engine.drain_actions().collect();
    assert!(log.lock().unwrap().is_empty());
    assert!(waiting.iter().any(
        |a| matches!(a, Action::SetTimer { timer, at_ms } if *timer == timers[0] && *at_ms == 100.)
    ));
    engine.handle(100., Event::TimerFired { timer: timers[0] });
    engine.prepare_dispatch(100.);
    let work: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    assert!(
        work.iter()
            .all(|(_, work)| work.receipt.stamp_ms == Some(100.))
    );
    signed(&mut engine, &work[0], 101.);
    signed(&mut engine, &work[1], 102.);
    engine.prepare_dispatch(102.);
    let groups: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::BindGroup { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1);
    match engine
        .claim_http_group(102., groups.into_iter().next().unwrap())
        .unwrap()
    {
        safe_cex_engine::request_context::GroupClaim::Released(releases) => {
            assert_eq!(releases.len(), 2);
            assert!(
                releases
                    .iter()
                    .all(|release| release.release.deadline == 20_001.)
            );
        }
        _ => panic!("eligible group did not release"),
    }
}

#[test]
fn review_clock_advanced_while_group_provider_parked_does_not_strand_bind() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    let mut engine = support::seeded_engine(Consumer {
        pair: true,
        warmup: true,
        ..Default::default()
    });
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    let warm = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine independent background preparation");
    engine.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let work: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(
        work.len(),
        2,
        "unblocked group must start both real providers"
    );
    engine.http_prepared(
        50.,
        warm,
        Prepared {
            id: warm.attempt().request.0,
            expires_at: None,
        },
    );
    let released = prepared_actions(&mut engine, 50.);
    assert_eq!(http(&released).len(), 1);
    assert_eq!(http(&released)[0].url, "https://venue.test/clock");
    signed(&mut engine, &work[0], 51.);
    signed(&mut engine, &work[1], 52.);
    let waiting: Vec<_> = engine.drain_actions().collect();
    let timer = waiting
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 150. => Some(*timer),
            _ => None,
        })
        .expect("actual shared clock now blocks group until 150");
    assert!(!waiting.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(**action, HttpAction::BindGroup { .. }))));
    engine.handle(150., Event::TimerFired { timer });
    engine.prepare_dispatch(150.);
    let ready: Vec<_> = engine.drain_actions().collect();
    assert!(ready.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(**action, HttpAction::BindGroup { .. }))), "group wake must resume announcement even when members are already signed");
}

#[test]
fn non_first_reprepare_wakes_with_the_first_member_already_permitted() {
    use safe_cex_engine::request_context::{Error, GroupClaim, HttpAction, Prepared};
    let log = EntropyLog::default();
    let mut engine = support::seeded_engine(Consumer {
        pair: true,
        warmup: true,
        entropy_log: Some(log.clone()),
        ..Default::default()
    });
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    let warmup = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let routes: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(routes.len(), 2);
    for (ordinal, token) in routes.iter().enumerate() {
        engine.http_prepared(
            1.,
            *token,
            Prepared {
                id: 800 + ordinal as u64,
                expires_at: (ordinal == 1).then_some(10.),
            },
        );
    }
    engine.prepare_dispatch(1.);
    let work: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 2);
    signed(&mut engine, &work[0], 2.);
    signed(&mut engine, &work[1], 3.);
    engine.prepare_dispatch(3.);
    let mut groups: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::BindGroup { permit, members } => Some((permit, members)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1);
    let (old, retained) = groups.pop().unwrap();
    engine.http_prepared(
        50.,
        warmup,
        Prepared {
            id: warmup.attempt().request.0,
            expires_at: None,
        },
    );
    let released = prepared_actions(&mut engine, 50.);
    assert_eq!(http(&released).len(), 1);
    assert_eq!(http(&released)[0].url, "https://venue.test/clock");
    assert!(matches!(
        engine.claim_http_group(60., old.clone()),
        Err(Error::Stale)
    ));
    let fresh: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].attempt(), routes[1].attempt());
    engine.http_prepared(
        70.,
        fresh[0],
        Prepared {
            id: 900,
            expires_at: None,
        },
    );
    engine.prepare_dispatch(70.);
    let waiting: Vec<_> = engine.drain_actions().collect();
    assert_eq!(log.lock().unwrap().len(), 2);
    assert!(!waiting.iter().any(|a| matches!(a, Action::Sign { .. })));
    let timers: Vec<_> = waiting
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 150. => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1);
    engine.handle(150., Event::TimerFired { timer: timers[0] });
    engine.prepare_dispatch(150.);
    let fresh_work: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(
        fresh_work.len(),
        1,
        "the first member's wake must activate its stale sibling"
    );
    assert_eq!(fresh_work[0].1.receipt.stamp_ms, Some(150.));
    signed(&mut engine, &fresh_work[0], 151.);
    engine.prepare_dispatch(151.);
    let mut groups: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::BindGroup { permit, members } => Some((permit, members)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1);
    let (permit, members) = groups.pop().unwrap();
    assert!(members[0].request.is_none());
    assert!(members[1].request.is_some());
    assert_eq!(members[0].preparation, retained[0].preparation);
    assert_eq!(permit.members()[0], old.members()[0]);
    assert_eq!(permit.members()[1].attempt(), old.members()[1].attempt());
    assert!(
        matches!(engine.claim_http_group(152., permit), Ok(GroupClaim::Released(releases)) if releases.len() == 2 && releases.iter().all(|release| release.release.deadline == 20_001.))
    );
    assert_eq!(log.lock().unwrap().len(), 3);
    assert!(matches!(
        engine.claim_http_group(152., old),
        Err(Error::Revoked)
    ));
}

/// #2069 R11 deferral (overseer ruling): a Session-bound request cannot join a
/// Mutation group. It is refused at issue, before any effect: nothing reaches
/// the host, and the command fails once, NotSent, with a message naming the
/// restriction.
#[test]
fn a_session_bound_group_member_is_refused_not_sent_before_any_effect() {
    let mut engine = start_consumer(Consumer {
        pair: true,
        bind_session: true,
        ..Default::default()
    });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        engine.venue().issue_errors,
        [safe_cex_engine::request_context::Error::Invalid]
    );
    assert_eq!(
        engine.venue().returned_own_pending,
        [true],
        "its own pending back"
    );
    assert!(
        matches!(engine.venue().follow_up, Some(Ok(_))),
        "a following bound issue on the same Session succeeds"
    );
    // Only the follow-up read prepares; nothing of the group member does.
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Sign { .. })
            || matches!(a, Action::ManagedHttp(managed) if matches!(
                managed.as_ref(),
                safe_cex_engine::request_context::HttpAction::Prepare { request, .. }
                    if request.url.contains("blofin")
            ))),
        "a refused group member reached the host"
    );
    let error = failed_once(&actions, safe_cex_engine::Outcome::NotSent);
    assert_eq!(
        error.message,
        "Session-bound requests cannot join a Mutation group"
    );
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
}
