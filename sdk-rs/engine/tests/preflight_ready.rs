//! Original FIFO regression and exact activation controls for #1896.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::WriteFact;
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Owner, Plan, PlanPolicy, Policy,
    Preflight, PreflightPolicy, PreflightPurpose, ReconciliationData, RetryPolicy,
    plan::{CallBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, ErrorKind, Event, HttpRequest,
    Method, Notice, Outcome, Response, SignStep, SocketId, TimerId, Venue, VenueError,
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
                nonce: None,
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
#[derive(Clone, Copy, Default)]
enum ReadyBehavior {
    #[default]
    Plan,
    Empty,
    Failure,
    Cached,
    NoProgress,
    SelectOnly,
    ReadThenPlan,
    Panic,
}
#[derive(Default)]
struct NativeFifo {
    plans: std::collections::HashMap<safe_cex_engine::CommandId, Plan>,
    slots: std::collections::HashMap<safe_cex_engine::CommandId, Slot>,
    queued: Option<Preflight>,
    foreign_refusal: Option<Error>,
    retired: Vec<Owner>,
    active: bool,
    in_response: bool,
    ready_count: usize,
    behavior: ReadyBehavior,
    read_only: bool,
    handles: std::collections::HashMap<safe_cex_engine::CommandId, Preflight>,
    tail: std::collections::VecDeque<Preflight>,
    retain_context: bool,
    foreign_context: Option<Context>,
    foreign_submission: Option<Error>,
    duplicate_wake: bool,
    wake_timer: Option<TimerId>,
    panic_retirement: bool,
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
            independent_calls: false,
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
                purpose: if self.read_only
                    && !matches!(command, Command::CancelOrders { command: 1, .. })
                {
                    PreflightPurpose::ReadOnly
                } else {
                    PreflightPurpose::WriteAfterReads
                },
                read_credential: None,
                write_credential: if self.read_only
                    && !matches!(command, Command::CancelOrders { command: 1, .. })
                {
                    None
                } else {
                    Some(credential())
                },
            })
            .unwrap();
        let Owner::Command { id, .. } = preflight.owner() else {
            unreachable!()
        };
        self.handles.insert(id, preflight.clone());
        if self.retain_context && id == 3 {
            self.foreign_context = Some(
                cx.preflight_read_context(&preflight, read_policy(), read_terms())
                    .unwrap(),
            );
            return;
        }
        if self.active {
            cx.park_preflight(&preflight).unwrap();
            if self.duplicate_wake {
                self.wake_timer = Some(cx.set_timer(1.));
            }
            if self.queued.is_none() {
                self.queued = Some(preflight);
            } else {
                self.tail.push_back(preflight);
            }
        } else {
            let plan = admit_one(cx, &preflight).unwrap();
            self.issue(cx, plan);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        self.in_response = true;
        if context.operation() == Operation::Read {
            let Owner::Command { id, .. } = context.owner() else {
                unreachable!()
            };
            let handle = self.handles[&id].clone();
            cx.validate_preflight_delivery(&handle, &context).unwrap();
            cx.finish_preflight_call(&handle, &context).unwrap();
            let plan = admit_one(cx, &handle).unwrap();
            self.issue(cx, plan);
            self.in_response = false;
            return;
        }
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
        self.in_response = false;
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("fifo-native".into())
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Context>, preflight: &Preflight) {
        assert!(!self.in_response, "ready must run after the outer callback");
        self.ready_count += 1;
        assert!(
            cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None
            })
            .is_err()
        );
        assert_eq!(cx.park_preflight(preflight), Err(Error::Revoked));
        assert_eq!(cx.request_preflight_ready(preflight), Err(Error::Revoked));
        if let Some(context) = self.foreign_context.clone() {
            assert!(context.deadline() > cx.now);
            self.foreign_submission = Some(
                cx.request_managed(
                    context.clone(),
                    Method::Get,
                    "https://venue.test/foreign".into(),
                    None,
                    context,
                )
                .unwrap_err()
                .1,
            );
            assert_eq!(self.foreign_submission, Some(Error::Revoked));
            let foreign = self.handles[&3].clone();
            assert_eq!(admit_one(cx, &foreign).unwrap_err(), Error::Revoked);
            assert_eq!(
                cx.fail_preflight(&foreign, "borrowed ready failure"),
                Err(Error::Revoked)
            );
        }
        match self.behavior {
            ReadyBehavior::Plan => {
                let plan = admit_one(cx, preflight).unwrap();
                self.issue(cx, plan);
            }
            ReadyBehavior::Empty => cx.done_empty_plan(preflight, "[]".into()).unwrap(),
            ReadyBehavior::Failure => cx
                .fail_preflight(
                    preflight,
                    VenueError::new("native refusal").kind(ErrorKind::InvalidInput),
                )
                .unwrap(),
            ReadyBehavior::Cached => cx.done_preflight(preflight, "[]".into()).unwrap(),
            ReadyBehavior::NoProgress => {}
            ReadyBehavior::SelectOnly | ReadyBehavior::ReadThenPlan => {
                let context = cx
                    .preflight_read_context(preflight, read_policy(), read_terms())
                    .unwrap();
                if matches!(self.behavior, ReadyBehavior::ReadThenPlan) {
                    cx.request_managed(
                        context.clone(),
                        Method::Get,
                        "https://venue.test/prerequisite".into(),
                        None,
                        context,
                    )
                    .unwrap();
                }
            }
            ReadyBehavior::Panic => panic!("ready panic control"),
        }
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.on_http_owner_retired(owner);
        self.active = false;
        let next = self.queued.take();
        self.queued = self.tail.pop_front();
        next
    }
    fn on_http_owner_retired(&mut self, owner: &HttpOwnerRetirement) {
        assert!(!self.panic_retirement, "retirement panic control");
        let handle = self
            .handles
            .values()
            .find(|handle| owner.matches_preflight(handle))
            .expect("retirement must match exact native owner");
        self.retired.push(handle.owner());
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        _: Error,
        error: VenueError,
    ) {
        let Owner::Command { id, .. } = context.owner() else {
            unreachable!()
        };
        cx.fail_plan(&self.plans[&id], error).unwrap();
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        let Owner::Command { id, .. } = context.owner() else {
            unreachable!()
        };
        cx.fail_plan(&self.plans[&id], error).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("unexpected fixture failure: {error}");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
        if self.duplicate_wake {
            let handle = self.queued.as_ref().unwrap();
            cx.request_preflight_ready(handle).unwrap();
            cx.request_preflight_ready(handle).unwrap();
            assert_eq!(admit_one(cx, handle).unwrap_err(), Error::Revoked);
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn start_with(venue: NativeFifo) -> Engine<NativeFifo> {
    let mut e = support::seeded_engine(venue);
    e.set_signer(Some(Arc::new(Present))).unwrap();
    e.start(0.);
    command(&mut e, 1, 1.);
    e
}
fn start() -> Engine<NativeFifo> {
    start_with(NativeFifo::default())
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
fn total_terminals(actions: &[Action], id: safe_cex_engine::CommandId) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(action,
        Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. })
            if *command == id)
        })
        .count()
}
#[test]
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
    assert_eq!(total_terminals(&actions, 1), 1);
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
    let completed = prepared_actions(&mut e, 4.);
    assert_eq!(terminals(&completed, 2), 1);
    assert_eq!(total_terminals(&completed, 2), 1);
    reply(&mut e, &first, 5.);
    let late = prepared_actions(&mut e, 5.);
    assert_eq!(total_terminals(&late, 1) + total_terminals(&late, 2), 0);
    assert!(!late.iter().any(|action| matches!(action, Action::Http(_))));
    assert_eq!(e.venue().retired.len(), 2);
}
#[test]
fn immediate_native_owner_completes_once_before_next_genuine_entry() {
    let mut e = start();
    let first = released(&mut e, 1.).remove(0);
    reply(&mut e, &first, 2.);
    let completed = prepared_actions(&mut e, 2.);
    assert_eq!(terminals(&completed, 1), 1);
    assert_eq!(total_terminals(&completed, 1), 1);
    command(&mut e, 2, 3.);
    let next = released(&mut e, 3.).remove(0);
    reply(&mut e, &next, 4.);
    let completed = prepared_actions(&mut e, 4.);
    assert_eq!(terminals(&completed, 2), 1);
    assert_eq!(total_terminals(&completed, 2), 1);
    assert_eq!(e.venue().retired.len(), 2);
}

fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn read_terms() -> safe_cex_engine::request_context::plan::CallTerms {
    safe_cex_engine::request_context::plan::CallTerms {
        timeout_ms: 100,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        wait_ms: 0,
    }
}
#[test]
fn default_selected_only_and_explicit_terminals_do_not_strand_parked_callers() {
    for behavior in [
        ReadyBehavior::NoProgress,
        ReadyBehavior::SelectOnly,
        ReadyBehavior::Failure,
        ReadyBehavior::Empty,
        ReadyBehavior::Cached,
    ] {
        for read_only in [false, true] {
            if matches!(behavior, ReadyBehavior::Cached) && !read_only {
                continue;
            }
            if matches!(behavior, ReadyBehavior::Empty) && read_only {
                continue;
            }
            let mut e = start_with(NativeFifo {
                behavior,
                read_only,
                ..Default::default()
            });
            let first = released(&mut e, 1.).remove(0);
            command(&mut e, 2, 2.);
            reply(&mut e, &first, 3.);
            let actions = prepared_actions(&mut e, 3.);
            assert_eq!(total_terminals(&actions, 1), 1);
            assert_eq!(total_terminals(&actions, 2), 1);
            assert_eq!(e.venue().ready_count, 1);
            assert_eq!(e.venue().retired.len(), 2);
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            if !matches!(behavior, ReadyBehavior::Empty | ReadyBehavior::Cached) {
                let error = actions
                    .iter()
                    .find_map(|a| match a {
                        Action::Notice(Notice::CommandFailed { command: 2, error }) => Some(error),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    error.outcome,
                    if read_only {
                        None
                    } else {
                        Some(Outcome::NotSent)
                    }
                );
                assert_eq!(
                    error.kind,
                    Some(if matches!(behavior, ReadyBehavior::Failure) {
                        ErrorKind::InvalidInput
                    } else {
                        ErrorKind::Internal
                    })
                );
            }
        }
    }
}
#[test]
fn exact_ready_read_handoff_can_admit_plan_without_a_second_activation() {
    let mut e = start_with(NativeFifo {
        behavior: ReadyBehavior::ReadThenPlan,
        ..Default::default()
    });
    let first = released(&mut e, 1.).remove(0);
    command(&mut e, 2, 2.);
    reply(&mut e, &first, 3.);
    let prerequisite = released(&mut e, 3.).remove(0);
    assert!(prerequisite.url.ends_with("/prerequisite"));
    reply(&mut e, &prerequisite, 4.);
    let next = released(&mut e, 4.).remove(0);
    assert!(next.url.ends_with("/cancel/2"));
    reply(&mut e, &next, 5.);
    assert_eq!(total_terminals(&prepared_actions(&mut e, 5.), 2), 1);
    assert_eq!(e.venue().ready_count, 1);
}
#[test]
fn retained_logically_valid_foreign_context_cannot_be_physically_submitted_in_ready() {
    let mut e = start_with(NativeFifo {
        retain_context: true,
        ..Default::default()
    });
    let first = released(&mut e, 1.).remove(0);
    command(&mut e, 2, 2.);
    command(&mut e, 3, 2.5);
    assert!(released(&mut e, 2.5).is_empty());
    reply(&mut e, &first, 3.);
    let next = released(&mut e, 3.).remove(0);
    assert_eq!(e.venue().foreign_submission, Some(Error::Revoked));
    assert!(next.url.ends_with("/cancel/2"));
}
#[test]
fn fifo_successors_keep_order_and_terminal_precedes_their_prepare() {
    let mut e = start();
    let mut current = released(&mut e, 1.).remove(0);
    command(&mut e, 2, 2.);
    command(&mut e, 3, 2.5);
    for id in 1..=3 {
        reply(&mut e, &current, 3. + f64::from(id));
        let actions = prepared_actions(&mut e, 3. + f64::from(id));
        assert_eq!(total_terminals(&actions, id), 1);
        if id < 3 {
            let terminal = actions.iter().position(|a| matches!(a, Action::Notice(Notice::CommandDone{command,..}) if *command==id)).unwrap();
            let next = actions
                .iter()
                .position(|a| matches!(a, Action::Http(_)))
                .unwrap();
            assert!(terminal < next);
            let Action::Http(request) = &actions[next] else {
                unreachable!()
            };
            assert!(request.url.ends_with(&format!("/cancel/{}", id + 1)));
            current = request.clone();
        }
    }
    assert_eq!(e.venue().ready_count, 2);
    assert_eq!(e.venue().retired.len(), 3);
}
#[test]
fn ready_and_restricted_retirement_panics_poison_the_engine_after_catch() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    for retirement in [false, true] {
        let mut e = start_with(NativeFifo {
            panic_retirement: retirement,
            behavior: if retirement {
                ReadyBehavior::Plan
            } else {
                ReadyBehavior::Panic
            },
            ..Default::default()
        });
        let first = released(&mut e, 1.).remove(0);
        command(&mut e, 2, 2.);
        assert!(catch_unwind(AssertUnwindSafe(|| reply(&mut e, &first, 3.))).is_err());
        assert!(e.failed());
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                e.drain_actions().for_each(drop);
            }))
            .is_err()
        );
    }
}

#[test]
fn engine_root_expiry_wakes_the_next_owner_in_the_same_event() {
    let mut e = start();
    let first_actions = prepared_actions(&mut e, 1.);
    let root_deadline = e.venue().plans[&1].deadline();
    let root_timer = first_actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == root_deadline => Some(*timer),
            _ => None,
        })
        .unwrap();
    assert!(first_actions.iter().any(|a| matches!(a, Action::Http(_))));
    command(&mut e, 2, 2.);
    e.handle(root_deadline, Event::TimerFired { timer: root_timer });
    let actions = prepared_actions(&mut e, root_deadline);
    assert_eq!(total_terminals(&actions, 1), 1);
    assert_eq!(total_terminals(&actions, 2), 0);
    assert_eq!(e.venue().ready_count, 1);
    assert_eq!(e.venue().plans[&2].deadline(), root_deadline + 20_000.);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/cancel/2")))
    );
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed{command:1,error})
        if error.kind==Some(ErrorKind::Timeout) && error.outcome==Some(Outcome::Unknown))
    ));
}
#[test]
fn preparation_refusal_retires_the_exact_owner_and_wakes_its_successor() {
    use safe_cex_engine::request_context::HttpAction;
    let mut e = start();
    let initial = e.drain_actions().collect::<Vec<_>>();
    let token = initial
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    command(&mut e, 2, 2.);
    e.http_preparation_failed(
        3.,
        token,
        Error::Invalid,
        VenueError::new("preparation refused"),
    );
    let actions = prepared_actions(&mut e, 3.);
    assert_eq!(total_terminals(&actions, 1), 1);
    assert_eq!(e.venue().ready_count, 1);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/cancel/2")))
    );
    assert!(actions.iter().any(
        |a| matches!(a,Action::Notice(Notice::CommandFailed{command:1,error})
        if error.outcome==Some(Outcome::NotSent))
    ));
    e.http_preparation_failed(4., token, Error::Invalid, VenueError::new("duplicate"));
    assert_eq!(total_terminals(&prepared_actions(&mut e, 4.), 1), 0);
    assert_eq!(e.venue().ready_count, 1);
}
#[test]
fn duplicate_wake_intents_activate_once_without_lending_timer_custody() {
    let mut e = start_with(NativeFifo {
        duplicate_wake: true,
        ..Default::default()
    });
    let first = released(&mut e, 1.).remove(0);
    command(&mut e, 2, 2.);
    let queued = e.venue().queued.as_ref().unwrap().owner();
    e.handle(
        3.,
        Event::TimerFired {
            timer: e.venue().wake_timer.unwrap(),
        },
    );
    let next = released(&mut e, 3.);
    assert_eq!(next.len(), 1);
    assert_eq!(e.venue().ready_count, 1);
    assert_eq!(e.venue().plans[&2].owner(), queued);
    reply(&mut e, &first, 4.);
    assert!(released(&mut e, 4.).is_empty());
    assert_eq!(e.venue().ready_count, 1);
}
