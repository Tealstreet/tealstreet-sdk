//! Original transport oracles driven through exact background Plan activation.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::request_context::plan::Slot;
use safe_cex_engine::request_context::{
    BackgroundActivation, BackgroundTask, BackgroundTaskPolicy, CommandEntryError, Context, Error,
    Plan, PreflightPolicy, PreflightPurpose, SessionPolicy,
};
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::selected_read::{ReadRequest, ReadRole, SelectedRead};
use safe_cex_engine::signing::{
    Identity, KeyRef, SignError, SignReceipt, SignRequest, SignResult, hmac_sha256_work,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Notice, Response, SocketId, TimerId,
    Venue,
};
use safe_cex_engine::{DispatchContext, SignStep};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
struct Recipe(Identity);
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &self.0,
            Vec::new(),
            Default::default(),
            SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
enum Control {
    Pause(f64),
    Cancel,
    Refresh,
    Invalidate,
    RetireParent,
}
#[derive(Default)]
struct Cycle {
    root: Option<Plan>,
    session: Option<safe_cex_engine::request_context::Session>,
    task: Option<BackgroundTask>,
    control: Arc<Mutex<Option<Control>>>,
    activations: usize,
    stage: usize,
    failures: usize,
    last_failure: Option<safe_cex_engine::VenueError>,
    retry_errors: bool,
    calls: u16,
    idle: bool,
    second_task: bool,
    admission_errors: Vec<Error>,
    command_contexts: Vec<Context>,
    panic_ready: bool,
    panic_retire: bool,
    contexts: Vec<Context>,
    unrelated: Option<TimerId>,
    replies: usize,
    outside_refusals: usize,
    attempted: Option<Result<(), Error>>,
}
impl Cycle {
    fn command_denied(&mut self, cx: &mut Cx<'_, (Context, Slot)>) {
        assert!(matches!(
            cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            }),
            Err(CommandEntryError::Revoked)
        ));
        self.outside_refusals += 1;
    }
    fn issue(&mut self, cx: &mut Cx<'_, (Context, Slot)>, path: &str) -> Result<(), Error> {
        let plan = self.root.as_ref().ok_or(Error::Revoked)?;
        let slot = cx
            .plan_slots(plan)?
            .into_iter()
            .next()
            .ok_or(Error::Revoked)?;
        let context = cx.plan_leg_context(plan, &slot)?;
        self.contexts.push(context.clone());
        cx.queue_background_read(
            plan,
            context.clone(),
            ReadRequest {
                method: Method::Get,
                url: format!("https://background.test/{path}"),
                body: None,
                stream: None,
            },
            |identity| Box::new(Recipe(identity.clone())),
            (context, slot),
        )
        .map_err(|(_, error)| error)?;
        Ok(())
    }
}
impl Venue for Cycle {
    type Pending = (Context, Slot);
    fn start(&mut self, cx: &mut Cx<'_, (Context, Slot)>) {
        self.command_denied(cx);
        self.unrelated = Some(cx.set_timer(1.));
        let session = cx
            .open_session(SessionPolicy {
                credential: None,
                repeat_safe: true,
            })
            .unwrap();
        let slots = Slots::new(
            Some(Identity {
                credential: KeyRef("background-credential".into()),
                signing: KeyRef("background-signing".into()),
                passphrase: None,
                stable_id: None,
                extra: Default::default(),
            }),
            None,
            SlotPolicy::default(),
        );
        let selected = SelectedRead::select(&slots, ReadRole::Read { scope: None }).unwrap();
        let task = cx
            .register_background_task(
                &session,
                selected,
                BackgroundTaskPolicy {
                    calls: if self.calls == 0 { 2 } else { self.calls },
                    debounce_ms: 0.,
                    refresh_debounce_ms: 0.,
                },
            )
            .unwrap();
        if !self.idle {
            cx.request_background_task(&task, safe_cex_engine::read_scheduler::Demand::Refresh)
                .unwrap();
        }
        if self.second_task {
            let second = cx
                .register_background_task(
                    &session,
                    SelectedRead::select(&slots, ReadRole::Read { scope: None }).unwrap(),
                    BackgroundTaskPolicy {
                        calls: self.calls,
                        debounce_ms: 0.,
                        refresh_debounce_ms: 0.,
                    },
                )
                .unwrap();
            cx.request_background_task(&second, safe_cex_engine::read_scheduler::Demand::Refresh)
                .unwrap();
        }
        self.session = Some(session);
        self.task = Some(task);
    }
    fn on_background_task_ready(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        _: &BackgroundTask,
        activation: &BackgroundActivation,
    ) {
        assert!(!self.panic_ready, "adversarial ready callback panic");
        self.activations += 1;
        self.stage = 0;
        match cx.admit_background_plan(activation) {
            Ok(plan) => self.root = Some(plan),
            Err(error) => {
                self.admission_errors.push(error);
                cx.decline_background_task(activation, error).unwrap();
                return;
            }
        }
        self.issue(cx, "margin-mode").unwrap();
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, (Context, Slot)>,
        pending: (Context, Slot),
        response: Response<'_>,
    ) {
        let (context, slot) = pending;
        cx.validate_http_context(&context).unwrap();
        assert_eq!(response.body, "validated native settings");
        self.replies += 1;
        self.stage += 1;
        if self.stage == 1 {
            cx.finish_plan_call(self.root.as_ref().unwrap(), &slot, &context)
                .unwrap();
            self.issue(cx, "position-mode").unwrap();
        } else {
            cx.finish_background_plan_after_data(self.root.as_ref().unwrap(), &slot, &context)
                .unwrap();
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, (Context, Slot)>, timer: TimerId) {
        if self.unrelated == Some(timer) {
            let control = self.control.lock().unwrap().take();
            if let Some(control) = control {
                match control {
                    Control::Pause(until) => cx
                        .pause_background_tasks(self.session.as_ref().unwrap(), until)
                        .unwrap(),
                    Control::Cancel => cx
                        .cancel_background_task(self.task.as_ref().unwrap())
                        .unwrap(),
                    Control::Refresh => cx
                        .request_background_task(
                            self.task.as_ref().unwrap(),
                            safe_cex_engine::read_scheduler::Demand::Refresh,
                        )
                        .unwrap(),
                    Control::Invalidate => cx
                        .request_background_task(
                            self.task.as_ref().unwrap(),
                            safe_cex_engine::read_scheduler::Demand::Invalidate,
                        )
                        .unwrap(),
                    Control::RetireParent => {
                        cx.retire_session(self.session.as_ref().unwrap()).unwrap();
                    }
                }
                self.unrelated = Some(cx.set_timer(1.));
                return;
            }
            self.command_denied(cx);
            // Adversarial callback has a live handle, not the task activation loan.
            self.attempted = Some(self.issue(cx, "unrelated-timer"));
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        _: Self::Pending,
        error: safe_cex_engine::VenueError,
    ) {
        cx.fail_background_plan(self.root.as_ref().unwrap(), error, self.retry_errors)
            .unwrap();
    }
    fn on_http_owner_retired(&mut self, _: &safe_cex_engine::request_context::HttpOwnerRetirement) {
        assert!(!self.panic_retire, "adversarial cleanup callback panic");
    }
    fn on_background_task_failed(
        &mut self,
        _: &BackgroundTask,
        error: safe_cex_engine::VenueError,
    ) {
        self.failures += 1;
        self.last_failure = Some(error);
    }
    fn on_request_failed(
        &mut self,
        _: &mut Cx<'_, (Context, Slot)>,
        _: (Context, Slot),
        message: &str,
    ) {
        panic!("{message}")
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        use safe_cex_engine::request_context::{Credential, Operation, Policy, RetryPolicy};
        let mutation = matches!(command, Command::SetLeverage { .. });
        let policy = Policy {
            operation: if mutation {
                Operation::Mutation
            } else {
                Operation::Read
            },
            credential: mutation.then(|| Credential {
                slot: safe_cex_engine::CredentialSlot::Primary,
                reference: KeyRef("foreground".into()),
            }),
            timeout_ms: 100_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        match cx.admit_http_command(policy) {
            Ok(context) => self.command_contexts.push(context),
            Err(CommandEntryError::Rejected(_)) => {}
            Err(CommandEntryError::Revoked) => panic!("lost genuine command entry"),
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, (Context, Slot)>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, (Context, Slot)>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, (Context, Slot)>, _: SocketId, _: u16) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn start() -> (Engine<Cycle>, HttpRequest) {
    let mut engine = support::seeded_engine(Cycle::default());
    engine.start(0.);
    let first = requests(&mut engine, 0.);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].url, "https://background.test/margin-mode");
    (engine, first.into_iter().next().unwrap())
}
fn requests(engine: &mut Engine<Cycle>, now: f64) -> Vec<HttpRequest> {
    drain(engine, now).0
}
fn drain(engine: &mut Engine<Cycle>, now: f64) -> (Vec<HttpRequest>, Vec<(TimerId, f64)>) {
    engine.prepare_dispatch(now);
    let mut queue = VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut resources = HashMap::new();
    let mut requests = Vec::new();
    let mut timers = Vec::new();
    let mut steps = 0;
    while let Some(action) = queue.pop_front() {
        steps += 1;
        assert!(steps < 1000);
        match action {
            Action::Sign { attempt, work, .. } => engine.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Ok(SignResult {
                        values: vec![b"test-key".to_vec(), vec![7; 32]],
                        receipt: work.receipt,
                        headers: Vec::new(),
                    }),
                },
            ),
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    if engine.http_preparation_pending(preparation) {
                        assert!(resources.insert(request.id, preparation).is_none());
                        engine.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert!(resources.remove(&request.id) == Some(preparation));
                    match engine.claim_http(now, permit) {
                        Ok(Claim::Released(_)) => requests.push(request),
                        Ok(Claim::WaitUntil(_)) => panic!("unexpected pacing"),
                        Err(_) => {}
                    }
                }
                HttpAction::Cancel { preparation, .. } => {
                    resources.remove(&preparation.attempt().request);
                }
            },
            Action::Notice(Notice::CommandFailed { .. }) => panic!("unexpected command failure"),
            Action::Http(_) => panic!("legacy HTTP bypassed managed custody"),
            Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
            _ => {}
        }
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    assert!(resources.is_empty());
    (requests, timers)
}

fn reply(engine: &mut Engine<Cycle>, request: &HttpRequest, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "validated native settings",
            },
        },
    );
}
#[test]
fn existing_context_chain_can_span_two_physical_windows_without_fake_entry() {
    let (mut engine, first) = start();
    assert_eq!(engine.venue().root.as_ref().unwrap().deadline(), 40_000.);
    assert_eq!(engine.venue().contexts[0].deadline(), 20_000.);
    reply(&mut engine, &first, 19_000.);
    let second = requests(&mut engine, 19_000.);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].url, "https://background.test/position-mode");
    assert_eq!(engine.venue().contexts[1].deadline(), 39_000.);
    reply(&mut engine, &second[0], 38_000.);
    assert!(requests(&mut engine, 38_000.).is_empty());
    assert_eq!(engine.venue().replies, 2);
    assert_eq!(engine.venue().outside_refusals, 1);
}
#[test]
fn original_red_unrelated_timer_cannot_borrow_background_cycle_custody() {
    let (mut engine, _) = start();
    let timer = engine.venue().unrelated.unwrap();
    engine.handle(1., Event::TimerFired { timer });
    let extra = requests(&mut engine, 1.);
    assert_eq!(engine.venue().outside_refusals, 2);
    assert!(
        extra.is_empty(),
        "a live background handle is not an exact task activation loan"
    );
    assert_eq!(engine.venue().attempted, Some(Err(Error::Revoked)));
}
#[test]
fn original_red_completed_two_call_cycle_cannot_release_an_undeclared_third_call() {
    let (mut engine, first) = start();
    reply(&mut engine, &first, 19_000.);
    let second = requests(&mut engine, 19_000.);
    reply(&mut engine, &second[0], 38_000.);
    assert!(requests(&mut engine, 38_000.).is_empty());
    let timer = engine.venue().unrelated.unwrap();
    engine.handle(38_001., Event::TimerFired { timer });
    assert!(
        requests(&mut engine, 38_001.).is_empty(),
        "two declared calls exhaust the finite cycle; no third structural credit"
    );
}

fn controlled() -> (Engine<Cycle>, HttpRequest, Arc<Mutex<Option<Control>>>) {
    let control = Arc::new(Mutex::new(None));
    let mut engine = support::seeded_engine(Cycle {
        control: control.clone(),
        ..Default::default()
    });
    engine.start(0.);
    let first = requests(&mut engine, 0.).pop().unwrap();
    (engine, first, control)
}
fn control(engine: &mut Engine<Cycle>, handle: &Mutex<Option<Control>>, now: f64, action: Control) {
    *handle.lock().unwrap() = Some(action);
    engine.handle(
        now,
        Event::TimerFired {
            timer: engine.venue().unrelated.unwrap(),
        },
    );
}
#[test]
fn successful_dirty_cycles_start_with_a_fresh_three_admission_budget() {
    let (mut engine, mut first, handle) = controlled();
    for cycle in 0..5 {
        let now = (cycle * 10) as f64;
        control(&mut engine, &handle, now + 1., Control::Invalidate);
        reply(&mut engine, &first, now + 2.);
        let second = requests(&mut engine, now + 2.).pop().unwrap();
        reply(&mut engine, &second, now + 3.);
        first = requests(&mut engine, now + 3.).pop().unwrap();
        assert_eq!(engine.venue().activations, cycle + 2);
        assert_eq!(first.url, "https://background.test/margin-mode");
    }
}
#[test]
fn cancelled_cycle_followed_by_new_demand_has_three_independent_admissions() {
    let control_handle = Arc::new(Mutex::new(None));
    let mut engine = support::seeded_engine(Cycle {
        control: control_handle.clone(),
        retry_errors: true,
        ..Default::default()
    });
    engine.start(0.);
    assert_eq!(requests(&mut engine, 0.).len(), 1);
    let handle = control_handle;
    control(&mut engine, &handle, 1., Control::Cancel);
    assert!(requests(&mut engine, 1.).is_empty());
    control(&mut engine, &handle, 2., Control::Refresh);
    let mut released = requests(&mut engine, 2.).pop().unwrap();
    for (now, delay) in [(3., 1000.), (1004., 2000.)] {
        engine.handle(
            now,
            Event::HttpFailed {
                request: released.id,
                error: "native network failure",
            },
        );
        let (http, timers) = drain(&mut engine, now);
        assert!(http.is_empty());
        let (timer, at) = timers
            .into_iter()
            .find(|(_, at)| *at == now + delay)
            .unwrap();
        assert_eq!(at, now + delay);
        engine.handle(at - 1., Event::TimerFired { timer });
        assert!(requests(&mut engine, at - 1.).is_empty());
        engine.handle(at, Event::TimerFired { timer });
        released = requests(&mut engine, at).pop().unwrap();
    }
    assert_eq!(engine.venue().activations, 4);
    engine.handle(
        3005.,
        Event::HttpFailed {
            request: released.id,
            error: "native network failure",
        },
    );
    assert!(drain(&mut engine, 3005.).1.is_empty());
}
#[test]
fn parent_retirement_cancels_the_child_and_late_transport_is_inert() {
    let (mut engine, first, handle) = controlled();
    control(&mut engine, &handle, 1., Control::RetireParent);
    assert!(requests(&mut engine, 1.).is_empty());
    reply(&mut engine, &first, 2.);
    assert!(requests(&mut engine, 2.).is_empty());
    assert_eq!(engine.venue().replies, 0);
    assert_eq!(engine.venue().activations, 1);
}
#[test]
fn pause_retires_an_admitted_child_and_waits_for_the_shared_floor() {
    let (mut engine, first, handle) = controlled();
    control(&mut engine, &handle, 1., Control::Pause(100.));
    let (http, timers) = drain(&mut engine, 1.);
    assert!(http.is_empty());
    let (timer, _) = timers.into_iter().find(|(_, at)| *at == 100.).unwrap();
    reply(&mut engine, &first, 2.);
    assert!(requests(&mut engine, 2.).is_empty());
    assert_eq!(engine.venue().replies, 0);
    engine.handle(99., Event::TimerFired { timer });
    assert!(requests(&mut engine, 99.).is_empty());
    engine.handle(100., Event::TimerFired { timer });
    assert_eq!(requests(&mut engine, 100.).len(), 1);
    assert_eq!(engine.venue().activations, 2);
}

fn foreground(engine: &mut Engine<Cycle>, command: u32, mutation: bool) {
    engine.handle(
        0.,
        Event::Command(if mutation {
            Command::SetLeverage {
                command,
                symbol: "BTC".into(),
                leverage: 2.,
                side: None,
            }
        } else {
            Command::FetchOrderBook {
                command,
                symbol: "BTC".into(),
                limit: None,
            }
        }),
    );
}
#[test]
fn background_share_is_a_cap_inside_the_pool_and_foreground_refusal_is_retryable() {
    let control_handle = Arc::new(Mutex::new(None));
    let mut engine = support::seeded_engine(Cycle {
        calls: 150,
        control: control_handle.clone(),
        ..Default::default()
    });
    engine.start(0.);
    assert_eq!(requests(&mut engine, 0.).len(), 1);
    for command in 1..=106 {
        foreground(&mut engine, command, command == 106);
    }
    assert_eq!(engine.venue().command_contexts.len(), 106);
    assert!(
        engine
            .drain_actions()
            .all(|action| !matches!(action, Action::Notice(Notice::CommandFailed { .. })))
    );
    for mutation in [false, true] {
        foreground(&mut engine, if mutation { 108 } else { 107 }, mutation);
        let errors = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].kind,
            Some(safe_cex_engine::ErrorKind::RateLimited)
        );
        assert_eq!(errors[0].retry_after_ms, Some(1000.));
        assert_eq!(
            errors[0].outcome,
            mutation.then_some(safe_cex_engine::Outcome::NotSent)
        );
        assert_eq!(engine.venue().command_contexts.len(), 106);
    }
    control(&mut engine, &control_handle, 1., Control::RetireParent);
    let _ = engine.drain_actions().collect::<Vec<_>>();
    for command in 107..=256 {
        foreground(&mut engine, command, false);
    }
    assert_eq!(engine.venue().command_contexts.len(), 256);
}
#[test]
fn idle_registration_holds_no_context_slots_and_pure_foreground_pressure_stays_capacity() {
    let mut engine = support::seeded_engine(Cycle {
        idle: true,
        ..Default::default()
    });
    engine.start(0.);
    let _ = engine.drain_actions().collect::<Vec<_>>();
    for command in 1..=256 {
        foreground(&mut engine, command, false);
    }
    assert_eq!(engine.venue().command_contexts.len(), 256);
    assert!(
        engine
            .drain_actions()
            .all(|action| !matches!(action, Action::Notice(Notice::CommandFailed { .. })))
    );
    foreground(&mut engine, 257, true);
    let errors = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(errors[0].retry_after_ms.is_none());
}
#[test]
fn second_child_cannot_exceed_the_single_aggregate_background_share() {
    let mut engine = support::seeded_engine(Cycle {
        calls: 150,
        second_task: true,
        ..Default::default()
    });
    engine.start(0.);
    assert_eq!(requests(&mut engine, 0.).len(), 1);
    assert_eq!(engine.venue().admission_errors, [Error::Capacity]);
    assert_eq!(engine.venue().contexts.len(), 1);
    assert_eq!(engine.venue().failures, 1);
    assert_eq!(engine.venue().activations, 2);
}

#[test]
fn pause_fences_prepare_sign_and_bound_claim_without_replacement_before_cleanup() {
    for stage in 0..3 {
        let handle = Arc::new(Mutex::new(None));
        let mut engine = support::seeded_engine(Cycle {
            control: handle.clone(),
            ..Default::default()
        });
        engine.start(0.);
        engine.prepare_dispatch(0.);
        let prepare = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => Some((preparation, request.id)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let mut sign = None;
        let mut bind = None;
        if stage >= 1 {
            engine.http_prepared(
                0.,
                prepare.0,
                Prepared {
                    id: prepare.1.0,
                    expires_at: None,
                },
            );
            sign = engine.drain_actions().find_map(|action| match action {
                Action::Sign { attempt, work, .. } => Some((attempt, work.receipt)),
                _ => None,
            });
            assert!(sign.is_some());
        }
        if stage == 2 {
            let (attempt, receipt) = sign.unwrap();
            engine.handle(
                0.,
                Event::Signed {
                    attempt,
                    result: Ok(SignResult {
                        values: vec![b"test-key".to_vec(), vec![7; 32]],
                        receipt,
                        headers: Vec::new(),
                    }),
                },
            );
            bind = engine.drain_actions().find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            });
            assert!(bind.is_some());
        }
        control(&mut engine, &handle, 1., Control::Pause(100.));
        match stage {
            0 => engine.http_prepared(
                1.,
                prepare.0,
                Prepared {
                    id: prepare.1.0,
                    expires_at: None,
                },
            ),
            1 => {
                let (attempt, receipt) = sign.unwrap();
                engine.handle(
                    1.,
                    Event::Signed {
                        attempt,
                        result: Ok(SignResult {
                            values: vec![b"test-key".to_vec(), vec![7; 32]],
                            receipt,
                            headers: Vec::new(),
                        }),
                    },
                );
            }
            _ => {
                assert!(engine.claim_http(1., bind.unwrap()).is_err());
            }
        }
        assert!(requests(&mut engine, 1.).is_empty());
        assert_eq!(engine.venue().activations, 1);
    }
}
#[test]
fn root_expiry_defers_the_background_seal_until_all_native_cleanup_finishes() {
    let mut engine = support::seeded_engine(Cycle {
        retry_errors: true,
        ..Default::default()
    });
    engine.start(0.);
    let first = requests(&mut engine, 0.).pop().unwrap();
    engine.handle(
        40_000.,
        Event::HttpFailed {
            request: first.id,
            error: "late network event",
        },
    );
    let (http, timers) = drain(&mut engine, 40_000.);
    assert!(http.is_empty());
    assert_eq!(engine.venue().failures, 1);
    let (timer, _) = timers.into_iter().find(|(_, at)| *at == 41_000.).unwrap();
    engine.handle(41_000., Event::TimerFired { timer });
    assert_eq!(requests(&mut engine, 41_000.).len(), 1);
    assert_eq!(engine.venue().activations, 2);
}

#[test]
fn a_caught_activation_panic_leaves_the_engine_permanently_fenced() {
    let mut engine = support::seeded_engine(Cycle {
        panic_ready: true,
        ..Default::default()
    });
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.start(0.))).is_err());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.prepare_dispatch(1.)))
            .is_err()
    );
}
#[test]
fn a_caught_native_cleanup_panic_never_acknowledges_or_replaces_the_child() {
    let handle = Arc::new(Mutex::new(None));
    let mut engine = support::seeded_engine(Cycle {
        panic_retire: true,
        control: handle.clone(),
        ..Default::default()
    });
    engine.start(0.);
    assert_eq!(requests(&mut engine, 0.).len(), 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| control(
            &mut engine,
            &handle,
            1.,
            Control::Cancel
        )))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.prepare_dispatch(2.)))
            .is_err()
    );
}

#[test]
fn pause_during_the_third_admission_emits_budget_exhausted_without_a_fourth() {
    let handle = Arc::new(Mutex::new(None));
    let mut engine = support::seeded_engine(Cycle {
        retry_errors: true,
        control: handle.clone(),
        ..Default::default()
    });
    engine.start(0.);
    let mut released = requests(&mut engine, 0.).pop().unwrap();
    for (now, delay) in [(1., 1000.), (1002., 2000.)] {
        engine.handle(
            now,
            Event::HttpFailed {
                request: released.id,
                error: "network",
            },
        );
        let (_, timers) = drain(&mut engine, now);
        let (timer, at) = timers
            .into_iter()
            .find(|(_, at)| *at == now + delay)
            .unwrap();
        engine.handle(at, Event::TimerFired { timer });
        released = requests(&mut engine, at).pop().unwrap();
    }
    assert_eq!(engine.venue().activations, 3);
    control(&mut engine, &handle, 3003., Control::Pause(4000.));
    let (http, timers) = drain(&mut engine, 3003.);
    assert!(http.is_empty());
    assert!(!timers.iter().any(|(_, at)| *at == 4000.));
    let error = engine.venue().last_failure.as_ref().unwrap();
    assert_eq!(
        error.message,
        "Background traversal admission budget exhausted"
    );
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert!(error.outcome.is_none());
    engine.handle(
        4000.,
        Event::HttpFailed {
            request: released.id,
            error: "late old request",
        },
    );
    assert!(requests(&mut engine, 4000.).is_empty());
    assert_eq!(engine.venue().activations, 3);
}
