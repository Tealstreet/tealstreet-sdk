//! Original behavioral controls for #1894, using only reviewed #1873 APIs.
#![cfg(feature = "test-support")]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::{Intent, WriteFact};
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, ReconciliationData, RetryPolicy,
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
use std::sync::Arc;

// Local presentation fixture only: this does not prove remote family signing.
struct Present;
impl Signer for Present {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Ok(SignResult::new(vec![b"value".to_vec()], work.receipt)))
    }
}
struct Recipe;
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        3_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 3_000.),
            },
            context: Default::default(),
            operations: vec![SignOperation::Present {
                key: KeyRef("fixture".into()),
            }],
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.signer_headers.push((
            "x-credential".into(),
            String::from_utf8(result.values[0].clone()).map_err(|_| SignError::InvalidResult)?,
        ));
        Ok(())
    }
}
fn credential() -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: KeyRef("fixture".into()),
    }
}
#[derive(Default)]
struct NativeAmend {
    omit: bool,
    plan: Option<Plan>,
    slot: Option<Slot>,
    replies: usize,
    retired: usize,
    original_deadline: f64,
    completion: Option<Result<(), Error>>,
}
impl NativeAmend {
    fn issue(&mut self, cx: &mut Cx<'_, Context>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().remove(0);
        let context = if self.replies == 0 {
            cx.plan_leg_context(plan, &slot).unwrap()
        } else {
            cx.plan_leg_context_with_intent(plan, &slot, Intent::Place)
                .unwrap()
        };
        cx.request_signed_managed(
            context.clone(),
            Method::Post,
            format!(
                "https://venue.test/{}",
                if self.replies == 0 {
                    "cancel"
                } else {
                    "replace"
                }
            ),
            None,
            Box::new(Recipe),
            context,
        )
        .unwrap();
        self.slot = Some(slot);
    }
}
impl Venue for NativeAmend {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        assert!(matches!(command, Command::AmendOrder { .. }));
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
                    operation: Operation::Mutation,
                    credential: Some(credential()),
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        self.original_deadline = plan.deadline();
        self.plan = Some(plan);
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body,
            if self.replies == 0 {
                "cancelled"
            } else {
                "replacement-native-id"
            }
        );
        let plan = self.plan.as_ref().unwrap().clone();
        let slot = self.slot.as_ref().unwrap().clone();
        assert_eq!(plan.deadline(), self.original_deadline);
        cx.validate_plan_delivery(&plan, &context).unwrap();
        cx.http_write_fact(
            &context,
            WriteFact::Accepted {
                order_ids: if self.replies == 0 {
                    vec![]
                } else {
                    vec![response.body.into()]
                },
            },
            None,
        )
        .unwrap();
        cx.validate_http_write_effects(&context).unwrap();
        self.replies += 1;
        if self.omit {
            // Keep the parent's structural prohibition after the new API lands.
            assert_eq!(cx.done_plan(&plan, "[]".into()), Err(Error::State));
            // Parent-only terminal attempt; implementation replaces this pair
            // with declared exact-DATA conditional completion, not a slot skip.
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            self.completion = Some(cx.done_plan(&plan, "[]".into()));
        } else {
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            if self.replies == 2 {
                self.completion = Some(cx.done_plan(&plan, "[\"replacement-native-id\"]".into()));
            } else {
                self.issue(cx);
            }
        }
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("conditional-native".into())
    }
    fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {
        self.retired += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("unexpected fixture failure: {error}");
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
fn start(omit: bool) -> Engine<NativeAmend> {
    let mut e = support::seeded_engine(NativeAmend {
        omit,
        ..Default::default()
    });
    e.set_signer(Some(Arc::new(Present))).unwrap();
    e.start(0.);
    e.handle(
        1.,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    e
}
fn request(e: &mut Engine<NativeAmend>, now: f64) -> HttpRequest {
    let mut requests = prepared_actions(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        });
    let request = requests
        .next()
        .expect("managed request released through final claim");
    assert!(requests.next().is_none());
    request
}
fn reply(e: &mut Engine<NativeAmend>, request: &HttpRequest, at: f64, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn done(actions: &[Action], result: &str) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(action,
        Action::Notice(Notice::CommandDone { command: 7, result: got }) if got == result)
        })
        .count()
}
#[test]
fn original_red_cancel_success_can_retire_unissued_replacement_once() {
    let mut e = start(true);
    let cancel = request(&mut e, 1.);
    assert!(cancel.url.ends_with("/cancel"));
    reply(&mut e, &cancel, 2., "cancelled");
    let actions: Vec<_> = prepared_actions(&mut e, 2.);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "no replacement when native remainder is zero"
    );
    assert_eq!(
        done(&actions, "[]"),
        1,
        "declared native conditional terminal must complete once"
    );
    assert_eq!(e.venue().completion, Some(Ok(())));
    assert_eq!(e.venue().retired, 1);
    reply(&mut e, &cancel, 3., "cancelled");
    assert_eq!(done(&prepared_actions(&mut e, 3.), "[]"), 0);
    assert_eq!(e.venue().replies, 1);
}
#[test]
fn ordinary_full_amend_preserves_native_identity_and_root() {
    let mut e = start(false);
    let cancel = request(&mut e, 1.);
    reply(&mut e, &cancel, 2., "cancelled");
    let replacement = request(&mut e, 2.);
    assert!(replacement.url.ends_with("/replace"));
    reply(&mut e, &replacement, 3., "replacement-native-id");
    assert_eq!(
        done(&prepared_actions(&mut e, 3.), "[\"replacement-native-id\"]"),
        1
    );
    assert_eq!(e.venue().completion, Some(Ok(())));
    assert_eq!(e.venue().retired, 1);
}
