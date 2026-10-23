//! Original behavioral controls for #1896, using only reviewed #1873 APIs.
#![cfg(feature = "test-support")]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Owner, Plan, PlanPolicy, Preflight,
    PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy,
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
struct NativeFifo {
    plans: std::collections::HashMap<safe_cex_engine::CommandId, Plan>,
    slots: std::collections::HashMap<safe_cex_engine::CommandId, Slot>,
    queued: Option<Preflight>,
    foreign_refusal: Option<Error>,
    retired: Vec<Owner>,
    active: bool,
}
fn admit_one(cx: &mut Cx<'_, Context>, preflight: &Preflight) -> Result<Plan, Error> {
    let calls = [CallBound {
        timeout_ms: 20_000,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &[],
    }];
    cx.admit_plan(
        preflight,
        &[StepBound {
            wait_ms: 0,
            calls: &calls,
        }],
        PlanPolicy {
            operation: Operation::Mutation,
            credential: Some(credential()),
            pacing: None,
            retry: RetryPolicy::Never,
        },
    )
}
impl NativeFifo {
    fn issue(&mut self, cx: &mut Cx<'_, Context>, plan: Plan) {
        let Owner::Command { id, .. } = plan.owner() else {
            panic!("command owner required");
        };
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_signed_managed(
            context.clone(),
            Method::Post,
            format!("https://venue.test/cancel/{id}"),
            None,
            Box::new(Recipe),
            context,
        )
        .unwrap();
        self.slots.insert(id, slot);
        self.plans.insert(id, plan);
        self.active = true;
    }
}
impl Venue for NativeFifo {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        assert!(matches!(command, Command::CancelOrders { .. }));
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential()),
            })
            .unwrap();
        if self.active {
            assert!(self.queued.is_none());
            self.queued = Some(preflight);
        } else {
            let plan = admit_one(cx, &preflight).unwrap();
            self.issue(cx, plan);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!((response.status, response.body), (200, "cancelled"));
        let Owner::Command { id, .. } = context.owner() else {
            panic!("command owner required");
        };
        let plan = self.plans[&id].clone();
        let slot = self.slots[&id].clone();
        cx.validate_plan_delivery(&plan, &context).unwrap();
        if let Some(queued) = self.queued.as_ref() {
            // This must stay forbidden after readiness is implemented.
            self.foreign_refusal = Some(admit_one(cx, queued).unwrap_err());
            assert_eq!(self.foreign_refusal, Some(Error::Revoked));
        }
        cx.http_write_fact(&context, WriteFact::Accepted { order_ids: vec![] }, None)
            .unwrap();
        cx.validate_http_write_effects(&context).unwrap();
        cx.finish_plan_call(&plan, &slot, &context).unwrap();
        cx.done_plan(&plan, "[]".into()).unwrap();
        self.active = false;
        // Parent has no own-Preflight ready callback or restricted retirement
        // wake. The adopter replaces this missing handoff, never the refusal.
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("fifo-native".into())
    }
    fn on_http_owner_retired(&mut self, owner: &HttpOwnerRetirement) {
        let plan = self
            .plans
            .values()
            .find(|plan| owner.matches_plan(plan))
            .expect("retirement must match the exact stored Plan");
        self.retired.push(plan.owner());
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
fn start() -> Engine<NativeFifo> {
    let mut e = support::seeded_engine(NativeFifo::default());
    e.set_signer(Some(Arc::new(Present))).unwrap();
    e.start(0.);
    command(&mut e, 1, 1.);
    e
}
fn command(e: &mut Engine<NativeFifo>, id: safe_cex_engine::CommandId, at: f64) {
    e.handle(
        at,
        Event::Command(Command::CancelOrders {
            command: id,
            ids: vec!["native".into()],
        }),
    );
}
fn released(e: &mut Engine<NativeFifo>, at: f64) -> Vec<HttpRequest> {
    prepared_actions(e, at)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
fn reply(e: &mut Engine<NativeFifo>, request: &HttpRequest, at: f64) {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "cancelled",
            },
        },
    );
}
fn terminals(actions: &[Action], id: safe_cex_engine::CommandId) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(action,
        Action::Notice(Notice::CommandDone { command, result }) if *command == id && result == "[]")
        })
        .count()
}
#[test]
#[ignore = "adoption #1896, owner codex-hcmq3f; runtime waits for reviewed #1873 feature pin"]
fn original_red_queued_owner_gets_own_activation_after_foreign_delivery() {
    let mut e = start();
    let first = released(&mut e, 1.).remove(0);
    command(&mut e, 2, 2.);
    assert!(
        released(&mut e, 2.).is_empty(),
        "native FIFO must retain B while A is active"
    );
    let queued_owner = e.venue().queued.as_ref().unwrap().owner();
    assert!(matches!(queued_owner, Owner::Command { id: 2, .. }));
    reply(&mut e, &first, 3.);
    let actions = prepared_actions(&mut e, 3.);
    assert_eq!(e.venue().foreign_refusal, Some(Error::Revoked));
    assert_eq!(terminals(&actions, 1), 1);
    let next: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(
        next.len(),
        1,
        "retained B must activate in its own scope after A unwinds"
    );
    assert!(next[0].url.ends_with("/cancel/2"));
    assert_eq!(e.venue().plans[&2].owner(), queued_owner);
    reply(&mut e, &next[0], 4.);
    assert_eq!(terminals(&prepared_actions(&mut e, 4.), 2), 1);
    reply(&mut e, &first, 5.);
    let late = prepared_actions(&mut e, 5.);
    assert_eq!(terminals(&late, 1) + terminals(&late, 2), 0);
    assert_eq!(e.venue().retired.len(), 2);
}
#[test]
fn immediate_native_owner_completes_once_before_next_genuine_entry() {
    let mut e = start();
    let first = released(&mut e, 1.).remove(0);
    reply(&mut e, &first, 2.);
    assert_eq!(terminals(&prepared_actions(&mut e, 2.), 1), 1);
    command(&mut e, 2, 3.);
    let next = released(&mut e, 3.).remove(0);
    reply(&mut e, &next, 4.);
    assert_eq!(terminals(&prepared_actions(&mut e, 4.), 2), 1);
    assert_eq!(e.venue().retired.len(), 2);
}
