use super::*;
use crate::request_context::{Claim, HttpAction, Prepared, RetryPolicy};
use crate::{Action, Engine, Event, Method, Notice, Response, SocketId, TimerId, Venue};

#[derive(Default)]
struct ReadPublicationProbe {
    wrong_leg: bool,
    root: Option<Context>,
    argument: Option<Context>,
    socket: Option<SocketId>,
    native_publications: usize,
    callbacks: usize,
    failure_authority: Option<Result<(), Error>>,
    failure_kind: Option<crate::ErrorKind>,
}

fn read_policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}

impl ReadPublicationProbe {
    fn publish(&mut self, cx: &mut Cx<'_, Context>) {
        let root = self.root.as_ref().unwrap();
        let leg = self.argument.as_ref().unwrap();
        let authority = cx.validate_read_http_delivery(root, leg);
        if authority.is_ok() {
            self.native_publications += 1;
            cx.store.set_hedged(true);
            cx.done_context(root, "{}".into()).unwrap();
        }
    }
}

impl Venue for ReadPublicationProbe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let root = cx
            .http_context(Scope::Command(7), read_policy(100))
            .unwrap();
        let actual = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let sibling = cx.http_leg_context(&root, read_policy(50)).unwrap();
        self.argument = Some(if self.wrong_leg {
            sibling
        } else {
            actual.clone()
        });
        self.socket = Some(cx.connect("wss://read-publication.test"));
        cx.request_managed(
            actual.clone(),
            Method::Get,
            "https://read-publication.test/settings".into(),
            None,
            actual,
        )
        .unwrap();
        self.root = Some(root);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        self.callbacks += 1;
        self.publish(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        self.callbacks += 1;
        self.publish(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, leg: Context, _: &str) {
        self.failure_authority =
            Some(cx.validate_read_http_delivery(self.root.as_ref().unwrap(), &leg));
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, leg: Context, error: VenueError) {
        self.failure_kind = error.kind;
        self.on_request_failed(cx, leg, &error.message);
    }
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: crate::Command) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released_read(wrong_leg: bool) -> (Engine<ReadPublicationProbe>, RequestId) {
    let mut engine = crate::test_support::seeded_engine(ReadPublicationProbe {
        wrong_leg,
        ..Default::default()
    });
    engine.start(0.);
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 55,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    (engine, preparation.attempt().request)
}

#[test]
fn read_publication_rejects_a_live_same_owner_sibling_before_native_and_store_effects() {
    for wrong_leg in [true, false] {
        let (mut engine, request) = released_read(wrong_leg);
        engine.handle(
            3.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert_eq!(engine.venue().callbacks, 1);
        assert_eq!(engine.venue().native_publications, usize::from(!wrong_leg));
        assert_eq!(engine.store().hedged(), !wrong_leg);
        let completions = engine
            .drain_actions()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            .count();
        assert_eq!(completions, usize::from(!wrong_leg));
    }
}

#[test]
fn read_publication_cannot_borrow_live_read_handles_from_a_socket_callback() {
    let (mut engine, _) = released_read(false);
    let socket = engine.venue().socket.unwrap();
    engine.handle(
        3.,
        Event::WsMessage {
            socket,
            text: "unrelated",
        },
    );
    assert_eq!(engine.venue().callbacks, 1);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

#[test]
fn read_publication_exact_physical_expiry_never_reaches_the_codec() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        50.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().callbacks, 0);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
}

#[test]
fn read_delivery_failure_ownership_does_not_prove_native_response_data() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(engine.venue().failure_authority, Some(Ok(())));
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

#[test]
fn released_deadline_expiry_is_a_typed_timeout_and_host_loss_stays_network() {
    let (mut engine, request) = released_read(false);
    // Managed identities are reserved as request, generation, timer.
    engine.handle(
        50.,
        Event::TimerFired {
            timer: TimerId(request.0 + 2),
        },
    );
    assert_eq!(engine.venue().failure_kind, Some(crate::ErrorKind::Timeout));

    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(engine.venue().failure_kind, Some(crate::ErrorKind::Network));
}

#[test]
fn a_delivered_body_that_is_not_utf8_is_a_typed_malformed_reply() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request,
            data: &[0xff, 0xfe],
        },
    );
    engine.handle(3., Event::HttpEnd { request });
    assert_eq!(
        engine.venue().failure_kind,
        Some(crate::ErrorKind::Internal)
    );
    assert_eq!(engine.venue().native_publications, 0);
}

#[test]
fn a_stream_that_breaks_mid_array_is_a_typed_malformed_reply() {
    let (mut engine, request) = released_read(false);
    {
        let mut cx = Cx {
            now: 2.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.stream_response(request, crate::json::ArrayAt::Root);
    }
    engine.handle(
        3.,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request,
            data: &[b'[', 0xff],
        },
    );
    engine.handle(3., Event::HttpEnd { request });
    assert_eq!(
        engine.venue().failure_kind,
        Some(crate::ErrorKind::Internal)
    );
    assert_eq!(engine.venue().native_publications, 0);
}

#[test]
fn read_publication_pair_authority_is_exact_and_read_only() {
    for case in 0..16 {
        let mut io = super::super::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let root = cx
            .http_context(Scope::Command(7), read_policy(100))
            .unwrap();
        let leg = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let sibling = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let mut root_argument = root.clone();
        let mut leg_argument = leg.clone();
        cx.io.http_delivery = Some(Delivery {
            context: leg.clone(),
            attempt: crate::request_context::Attempt {
                request: RequestId(99),
                generation: 100,
            },
        });
        let expected = match case {
            0 => {
                // The original root survives logical handle retirement.
                cx.finish_http_context(&root).unwrap();
                Ok(())
            }
            1 => {
                root_argument = sibling;
                Err(Error::Revoked)
            }
            2 => {
                root_argument = ContextRegistry::default()
                    .issue(1, Scope::Command(7), read_policy(100), 0.)
                    .unwrap();
                Err(Error::Revoked)
            }
            3 => {
                leg_argument = ContextRegistry::default()
                    .issue(2, Scope::Command(7), read_policy(50), 0.)
                    .unwrap();
                Err(Error::Revoked)
            }
            4 => {
                root_argument = cx
                    .http_context(Scope::Command(8), read_policy(100))
                    .unwrap();
                Err(Error::Revoked)
            }
            5 => {
                let mut mutation = read_policy(100);
                mutation.operation = Operation::Mutation;
                mutation.retry = RetryPolicy::Never;
                cx.http_context(Scope::Command(7), mutation).unwrap();
                Err(Error::Invalid)
            }
            6 => {
                cx.now = leg.deadline();
                Err(Error::TimedOut)
            }
            7 => {
                cx.io.http_delivery = None;
                Err(Error::Revoked)
            }
            8 => {
                cx.io
                    .request_contexts
                    .commands
                    .get_mut(&7)
                    .unwrap()
                    .generation += 1;
                Err(Error::Revoked)
            }
            9 => {
                root_argument = cx
                    .http_context(Scope::Background, read_policy(100))
                    .unwrap();
                Err(Error::Invalid)
            }
            10 => {
                leg_argument = root.clone();
                cx.io.http_delivery.as_mut().unwrap().context = root.clone();
                Ok(())
            }
            11 => {
                cx.finish_http_context(&leg).unwrap();
                Err(Error::Revoked)
            }
            12 => {
                leg_argument = cx
                    .http_context(Scope::Command(7), read_policy(200))
                    .unwrap();
                cx.io.http_delivery.as_mut().unwrap().context = leg_argument.clone();
                Err(Error::Revoked)
            }
            13 => {
                root_argument = root.clone().with_retry(RetryPolicy::Never).unwrap();
                Err(Error::Revoked)
            }
            14 => {
                let mut mutation = read_policy(100);
                mutation.operation = Operation::Mutation;
                mutation.retry = RetryPolicy::Never;
                let mutation = cx.http_context(Scope::Command(7), mutation).unwrap();
                cx.finish_http_context(&mutation).unwrap();
                Err(Error::Invalid)
            }
            15 => {
                cx.now = f64::NAN;
                Err(Error::Invalid)
            }
            _ => unreachable!(),
        };
        let contexts = cx.io.request_contexts.contexts.len();
        let commands = cx.io.request_contexts.commands.len();
        let delivery = cx
            .io
            .http_delivery
            .as_ref()
            .map(|d| (d.context.clone(), d.attempt));
        assert_eq!(
            cx.validate_read_http_delivery(&root_argument, &leg_argument),
            expected,
            "case {case}"
        );
        assert_eq!(
            cx.validate_read_http_delivery(&root_argument, &leg_argument),
            expected,
            "repeat {case}"
        );
        assert_eq!(cx.io.request_contexts.contexts.len(), contexts);
        assert_eq!(cx.io.request_contexts.commands.len(), commands);
        assert_eq!(
            cx.io
                .http_delivery
                .as_ref()
                .map(|d| (d.context.clone(), d.attempt)),
            delivery
        );
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.timers.is_empty());
        assert!(!cx.store.hedged());
    }
}

/// A command whose root is a Mutation and whose physical children include
/// Reads (WOO: Touch pre-read, writes, then Mode/Leverage verification).
#[derive(Default)]
struct MutationRootProbe {
    root: Option<Context>,
    delivered: Option<Context>,
    sibling: Option<Context>,
    foreign_root: Option<Context>,
    socket: Option<SocketId>,
    data: Vec<[Result<(), Error>; 7]>,
    after_terminal: Option<Result<(), Error>>,
    socket_authority: Option<Result<(), Error>>,
    failure_authority: Option<Result<(), Error>>,
    publications: usize,
}

fn mutation_policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

impl Venue for MutationRootProbe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let root = cx
            .http_context(Scope::Command(7), mutation_policy())
            .unwrap();
        let read = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let sibling = cx.http_leg_context(&root, read_policy(50)).unwrap();
        self.foreign_root = Some(
            cx.http_context(Scope::Command(8), read_policy(100))
                .unwrap(),
        );
        self.socket = Some(cx.connect("wss://mutation-root.test"));
        cx.request_managed(
            read.clone(),
            Method::Get,
            "https://mutation-root.test/mode".into(),
            None,
            read.clone(),
        )
        .unwrap();
        self.delivered = Some(read);
        self.sibling = Some(sibling);
        self.root = Some(root);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, leg: Context, _: Response<'_>) {
        let root = self.root.clone().unwrap();
        let checks = [
            cx.validate_read_http_delivery(&root, &leg),
            cx.validate_background_read_http_delivery(&leg),
            cx.validate_command_read_http_delivery(&root, &leg),
            cx.validate_command_read_http_delivery(&root, self.sibling.as_ref().unwrap()),
            cx.validate_command_read_http_delivery(self.foreign_root.as_ref().unwrap(), &leg),
            // A physical leg is never a root.
            cx.validate_command_read_http_delivery(&leg, &leg),
            {
                let sibling = self.sibling.clone().unwrap();
                cx.finish_http_context(&sibling).unwrap();
                cx.validate_command_read_http_delivery(&root, &sibling)
            },
        ];
        if checks[2].is_ok() {
            self.publications += 1;
            cx.store.set_hedged(true);
            cx.done_context(&root, "{}".into()).unwrap();
            self.after_terminal = Some(cx.validate_command_read_http_delivery(&root, &leg));
        }
        self.data.push(checks);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        let root = self.root.clone().unwrap();
        self.socket_authority =
            Some(cx.validate_command_read_http_delivery(&root, self.delivered.as_ref().unwrap()));
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, leg: Context, _: &str) {
        let root = self.root.clone().unwrap();
        self.failure_authority = Some(cx.validate_command_read_http_delivery(&root, &leg));
    }
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: crate::Command) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released_mutation_root_read() -> (Engine<MutationRootProbe>, RequestId) {
    let mut engine = crate::test_support::seeded_engine(MutationRootProbe::default());
    engine.start(0.);
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 77,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    (engine, preparation.attempt().request)
}

/// C1/C2 pin today's refusals; R1/R2/R6: the exact Read child's DATA under a
/// Mutation root validates, a sibling and a foreign root do not.
#[test]
fn a_read_child_of_a_mutation_root_has_exact_data_authority() {
    let (mut engine, request) = released_mutation_root_read();
    engine.handle(
        3.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    let checks = &engine.venue().data[0];
    assert_eq!(
        checks[0],
        Err(Error::Invalid),
        "C1: the read-only validator refuses a Mutation root"
    );
    assert_eq!(
        checks[1],
        Err(Error::Invalid),
        "C2: the Background validator refuses a command owner"
    );
    assert_eq!(checks[2], Ok(()), "R1: the exact Read child's DATA");
    assert_eq!(
        checks[3],
        Err(Error::Revoked),
        "R2: a live sibling is not this delivery"
    );
    assert_eq!(checks[4], Err(Error::Revoked), "R6: another command's root");
    assert_eq!(
        checks[5],
        Err(Error::Revoked),
        "R8: a physical leg is not a root"
    );
    assert_eq!(
        checks[6],
        Err(Error::Revoked),
        "R9: a retired sibling child"
    );
    assert_eq!(
        engine.venue().after_terminal,
        Some(Err(Error::Revoked)),
        "R10: a sealed root is no longer the current root"
    );
    assert_eq!(engine.venue().publications, 1);
    // The verification read grants no outcome: the root's terminal is still
    // the journal projection, and a Mutation root with no confirmed write
    // cannot become success through a Read child.
    let terminals = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, .. }) => Some(None),
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.outcome),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(terminals, [Some(crate::Outcome::NotSent)]);
}

/// R4/R5: a socket callback has no delivery; a failure callback proves only
/// ownership, never data.
#[test]
fn a_mutation_root_read_child_needs_its_own_delivery_and_failure_proves_no_data() {
    let (mut engine, request) = released_mutation_root_read();
    let socket = engine.venue().socket.unwrap();
    engine.handle(3., Event::WsMessage { socket, text: "{}" });
    assert_eq!(
        engine.venue().socket_authority,
        Some(Err(Error::Revoked)),
        "R4"
    );
    engine.handle(
        4.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(
        engine.venue().failure_authority,
        Some(Ok(())),
        "R5: ownership only"
    );
    assert_eq!(engine.venue().publications, 0);
}

/// R7: DATA at the child's exact physical deadline never reaches a callback,
/// so no validator can publish it.
#[test]
fn a_mutation_root_read_child_at_exact_expiry_never_reaches_the_codec() {
    let (mut engine, request) = released_mutation_root_read();
    let deadline = engine.venue().delivered.as_ref().unwrap().deadline();
    engine.handle(
        deadline,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert!(
        engine.venue().data.is_empty(),
        "no DATA callback at equality"
    );
    assert_eq!(engine.venue().publications, 0);
}

/// Owners outside the plain Context phase, and a write leg (R3, R11, R12).
#[cfg(feature = "http-plans")]
#[derive(Default)]
struct PhaseProbe {
    execution: Option<crate::engine::ExecutionHandle>,
    observer: Option<Context>,
    preflight_read: Option<Context>,
    plan_leg: Option<Context>,
    mutation_root: Option<Context>,
    write: Option<Context>,
}

#[cfg(feature = "http-plans")]
impl Venue for PhaseProbe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: crate::Command) {
        use crate::request_context::plan::{CallBound, CallTerms, RetryWindow, StepBound};
        use crate::request_context::{PlanPolicy, PreflightPolicy, PreflightPurpose};
        let preflight_policy = PreflightPolicy {
            purpose: PreflightPurpose::ReadOnly,
            read_credential: None,
            write_credential: None,
        };
        match command.completion_id() {
            Some(1) => {
                let founder = cx.admit_http_command(read_policy(100)).unwrap();
                self.execution = Some(
                    cx.begin_execution(
                        &founder,
                        read_policy(100),
                        crate::engine::Window::UntilTerminal,
                        "phase",
                    )
                    .unwrap(),
                );
            }
            Some(2) => {
                let observer = cx.admit_http_command(read_policy(100)).unwrap();
                cx.join_execution(self.execution.as_ref().unwrap(), &observer, "phase")
                    .unwrap();
                self.observer = Some(observer);
            }
            Some(3) => {
                let preflight = cx.begin_preflight(preflight_policy).unwrap();
                let terms = CallTerms {
                    wait_ms: 0,
                    timeout_ms: 100,
                    attempts: 1,
                    retry_window: RetryWindow::FreshAttempt,
                };
                self.preflight_read = Some(
                    cx.preflight_read_context(&preflight, read_policy(100), terms)
                        .unwrap(),
                );
            }
            Some(4) => {
                let preflight = cx.begin_preflight(preflight_policy).unwrap();
                let calls = [CallBound {
                    timeout_ms: 100,
                    attempts: 1,
                    retry_window: RetryWindow::FreshAttempt,
                    after_refusal: &[],
                }];
                let plan = cx
                    .admit_plan(
                        &preflight,
                        &[StepBound {
                            wait_ms: 0,
                            calls: &calls,
                        }],
                        PlanPolicy {
                            operation: Operation::Read,
                            credential: None,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                let slot = cx.plan_slots(&plan).unwrap().remove(0);
                self.plan_leg = Some(cx.plan_leg_context(&plan, &slot).unwrap());
            }
            Some(5) => {
                let root = cx.admit_http_command(mutation_policy()).unwrap();
                self.write = Some(cx.http_leg_context(&root, mutation_policy()).unwrap());
                self.mutation_root = Some(root);
            }
            _ => {}
        }
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// R3: a Mutation write leg is never a Read child, even as this callback's
/// exact delivery. R11: Preflight and Plan-phase handles are not plain roots.
/// R12: an execution observer re-dispatches only through its execution, and a
/// physical Execution owner is no command root.
#[cfg(feature = "http-plans")]
#[test]
fn read_child_authority_refuses_writes_and_non_context_owners() {
    let mut engine = crate::test_support::seeded_engine(PhaseProbe::default());
    engine.start(0.);
    for command in 1..=5 {
        engine.handle(
            0.,
            Event::Command(crate::Command::FetchCandles {
                command,
                symbol: "BTCUSD".into(),
                interval: crate::model::Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        engine.drain_actions().for_each(drop);
    }
    let probe = engine.venue();
    let (observer, execution) = (
        probe.observer.clone().unwrap(),
        probe.execution.as_ref().unwrap().context().clone(),
    );
    let (preflight_read, plan_leg) = (
        probe.preflight_read.clone().unwrap(),
        probe.plan_leg.clone().unwrap(),
    );
    let (root, write) = (
        probe.mutation_root.clone().unwrap(),
        probe.write.clone().unwrap(),
    );
    let cx = Cx {
        now: 1.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.io.http_delivery = Some(Delivery {
        context: write.clone(),
        attempt: Attempt {
            request: RequestId(u64::MAX),
            generation: u64::MAX,
        },
    });
    assert_eq!(
        cx.validate_command_read_http_delivery(&root, &write),
        Err(Error::Invalid),
        "R3"
    );
    cx.io.http_delivery = None;
    for phase in [&preflight_read, &plan_leg] {
        assert!(
            matches!(
                cx.validate_command_read_http_delivery(phase, phase),
                Err(Error::Invalid | Error::Revoked)
            ),
            "R11"
        );
    }
    assert_eq!(
        cx.validate_command_read_http_delivery(&observer, &observer),
        Err(Error::Revoked),
        "R12: observer"
    );
    assert_eq!(
        cx.validate_command_read_http_delivery(&execution, &execution),
        Err(Error::Invalid),
        "R12: physical Execution owner"
    );
}
