//! G2 original-source controls; binding admission changes later, transport stays.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
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
struct Provider;
impl Signer for Provider {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Ok(SignResult::new(
            work.operations
                .iter()
                .map(|op| match op {
                    SignOperation::Present { key } => key.0.as_bytes().to_vec(),
                    _ => panic!("presentation only"),
                })
                .collect(),
            work.receipt,
        )))
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
            operations: vec![SignOperation::Present {
                key: self.0.clone(),
            }],
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
            cx.admit_plan(
                &preflight,
                &steps,
                PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: Some(credential(CredentialSlot::Primary)),
                    pacing: None,
                    retry: RetryPolicy::Never,
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
        panic!("{message}")
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
fn requests(engine: &mut Engine<Consumer>, now: f64) -> Vec<HttpRequest> {
    prepared_actions(engine, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
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
fn verify() -> (Engine<Consumer>, HttpRequest) {
    let mut engine = start(false);
    let cancel = requests(&mut engine, 1.);
    assert_eq!(cancel.len(), 1);
    assert_eq!(
        cancel[0].url,
        "https://openapi.blofin.com/api/v1/trade/cancel-batch-orders"
    );
    reply(&mut engine, &cancel[0], 2.);
    let verify = requests(&mut engine, 2.);
    assert_eq!(verify.len(), 1);
    assert_eq!(verify[0].method, Method::Get);
    assert_eq!(
        verify[0].url,
        "https://openapi.blofin.com/api/v1/trade/order-detail?instId=BTC-USDT&orderId=native-7"
    );
    assert_eq!(verify[0].body, None);
    (engine, verify.into_iter().next().unwrap())
}
#[test]
#[ignore = "G2 mixed-leg runtime adoption: codex-hcmq3f, feat/rust-plan-mixed-legs"]
fn original_red_verify_leg_has_read_authority_under_mutation_root() {
    let (engine, _) = verify();
    assert_eq!(engine.venue().contexts[0].operation(), Operation::Mutation);
    assert_eq!(
        engine.venue().contexts[1].operation(),
        Operation::Read,
        "Verify is a Read even though its genuine owner is a mutation Plan"
    );
}
#[test]
#[ignore = "G2 mixed-leg runtime adoption: codex-hcmq3f, feat/rust-plan-mixed-legs"]
fn original_red_verify_leg_uses_immutable_secondary_selection() {
    let (engine, verify) = verify();
    let context = &engine.venue().contexts[1];
    assert_eq!(
        context.credential(),
        Some(&credential(CredentialSlot::Secondary)),
        "Verify must retain the declared read identity, not the root write identity"
    );
    assert_eq!(
        verify.signer_headers,
        [("x-selected-ref".into(), "secondary-ref".into())]
    );
}
#[test]
fn two_primary_mutations_remain_parallel_and_complete_once_in_reverse_order() {
    let mut engine = start(true);
    let root = engine.venue().plan.as_ref().unwrap().deadline();
    let pair = requests(&mut engine, 1.);
    assert_eq!(pair.len(), 2);
    assert_eq!(
        pair[0].body.as_deref(),
        Some("{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"long\"}")
    );
    assert_eq!(
        pair[1].body.as_deref(),
        Some("{\"instId\":\"BTC-USDT\",\"leverage\":\"5\",\"positionSide\":\"short\"}")
    );
    for context in &engine.venue().contexts {
        assert_eq!(context.operation(), Operation::Mutation);
        assert_eq!(
            context.credential(),
            Some(&credential(CredentialSlot::Primary))
        );
    }
    reply(&mut engine, &pair[1], 2.);
    assert!(!prepared_actions(&mut engine, 2.).iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    reply(&mut engine, &pair[0], 3.);
    assert_eq!(engine.venue().plan.as_ref().unwrap().deadline(), root);
    let done = prepared_actions(&mut engine, 3.);
    assert_eq!(
        done.iter()
            .filter(|action| matches!(
                action,
                Action::Notice(
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                )
            ))
            .count(),
        1
    );
    assert_eq!(done.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")).count(), 1);
    reply(&mut engine, &pair[0], 4.);
    assert!(prepared_actions(&mut engine, 4.).is_empty());
}
