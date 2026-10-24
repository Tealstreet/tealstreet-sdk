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
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
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
struct Recipe(KeyRef);
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 5_000.),
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
                Box::new(Recipe(reference)),
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
                    independent_calls: false,
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
        cx.http_write_fact(&context, WriteFact::Accepted { order_ids: vec![] }, None)
            .unwrap();
        cx.finish_plan_call(&plan, &self.slots[&context.leg()], &context)
            .unwrap();
        self.replies += 1;
        if self.pair {
            if self.replies == 2 {
                cx.done_plan(&plan, "[]".into()).unwrap();
            }
        } else {
            self.issue(cx);
        }
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
    let mut engine = support::seeded_engine(Consumer {
        pair,
        ..Default::default()
    });
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
#[ignore = "G2b group preparation runtime adoption: codex-hcmq3f, feat/rust-plan-mutation-prepare-barrier"]
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
#[ignore = "G2b group preparation runtime adoption: codex-hcmq3f, feat/rust-plan-mutation-prepare-barrier"]
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
