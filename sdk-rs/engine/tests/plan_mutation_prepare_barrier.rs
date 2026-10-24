//! G2b original asynchronous preparation-barrier controls.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
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
    stagger_freshness: bool,
    entropy_log: Option<EntropyLog>,
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
            cx.request_signed_managed(
                context.clone(),
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
            )
            .unwrap();
        }
    }
}
impl Venue for Consumer {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
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
                    pacing: None,
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
    let mut engine = support::seeded_engine(consumer);
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    engine.start(0.);
    let command = if pair {
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
        if let Action::ManagedHttp(action) = action {
            if let HttpAction::BindGroup { permit, members } = *action {
                groups.push((permit, members));
            }
        }
    }
    assert_eq!(groups.len(), 1);
    let (permit, mut members) = groups.pop().unwrap();
    assert!(members[0].request.is_some());
    assert!(members[1].request.is_none());
    assert_eq!(members[1].preparation, retained[1].preparation);
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
