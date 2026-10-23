use super::*;
use crate::book::Book;
use crate::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};

fn read_policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Default)]
struct GroupProbe {
    execution: Option<ExecutionHandle>,
    roots: HashMap<CommandId, Context>,
    timeouts: HashMap<CommandId, u32>,
    complete: bool,
    publications: usize,
    failures: usize,
    join_errors: Vec<crate::request_context::Error>,
    independent_leg: Option<CommandId>,
    unentered: Option<CommandId>,
    /// Admit this command's root but hold it instead of beginning or joining.
    hold: Option<CommandId>,
    cross_errors: Vec<crate::request_context::Error>,
    abort: bool,
    joined: Vec<(CommandId, Joined)>,
}

impl Venue for GroupProbe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let id = command.completion_id().unwrap();
        let timeout = self.timeouts.get(&id).copied().unwrap_or(1_000);
        let root = if self.unentered == Some(id) {
            cx.http_context(Scope::Command(id), read_policy(timeout))
                .unwrap()
        } else {
            cx.admit_http_command(read_policy(timeout)).unwrap()
        };
        if self.independent_leg == Some(id) {
            cx.http_leg_context(&root, read_policy(timeout)).unwrap();
        }
        if self.hold == Some(id) {
            self.roots.insert(id, root);
            return;
        }
        if let Some(held) = self.hold.and_then(|held| self.roots.get(&held)) {
            // Another command's genuine callback cannot carry the held root.
            let held = held.clone();
            if let Err(error) =
                cx.begin_execution(&held, read_policy(1_000), Window::UntilTerminal, "x")
            {
                self.cross_errors.push(error);
            }
        }
        let key = format!("k{id}");
        if let Some(execution) = &self.execution {
            match cx.join_execution(execution, &root, &key) {
                Ok(joined) => self.joined.push((id, joined)),
                Err(error) => self.join_errors.push(error),
            }
        } else {
            let execution = cx
                .begin_execution(&root, read_policy(1_000), Window::UntilTerminal, &key)
                .unwrap();
            let leg = cx
                .http_leg_context(execution.context(), read_policy(1_000))
                .unwrap();
            cx.request_managed(
                leg.clone(),
                Method::Get,
                "https://group.test/read".into(),
                None,
                leg,
            )
            .unwrap();
            self.execution = Some(execution);
        }
        self.roots.insert(id, root);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, leg: Context, _: Response<'_>) {
        let execution = self.execution.clone().unwrap();
        if cx
            .validate_execution_read_delivery(&execution, &leg)
            .is_ok()
        {
            self.publications += 1;
            if self.abort {
                cx.abort_execution(&execution, VenueError::new("read failed"))
                    .unwrap();
            } else if self.complete {
                cx.complete_execution(&execution, |key| Ok(format!("\"{key}\"")))
                    .unwrap();
            }
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        self.failures += 1;
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn fetch(command: CommandId) -> Event<'static> {
    Event::Command(Command::FetchCandles {
        command,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    })
}

struct Run {
    engine: Engine<GroupProbe>,
    actions: Vec<Action>,
}

impl Run {
    fn new(probe: GroupProbe) -> Self {
        let mut engine = crate::test_support::seeded_engine(probe);
        engine.start(0.);
        engine.drain_actions().for_each(drop);
        Self {
            engine,
            actions: Vec::new(),
        }
    }
    fn handle(&mut self, at: f64, event: Event<'_>) {
        self.engine.handle(at, event);
        self.actions.extend(self.engine.drain_actions());
    }
    fn prepares(&self) -> Vec<crate::request_context::Preparation> {
        self.actions
            .iter()
            .filter_map(|action| match action {
                Action::ManagedHttp(action) => match **action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
    /// Prepare and claim the newest physical request; returns its id.
    fn release(&mut self, at: f64) -> RequestId {
        let preparation = *self.prepares().last().expect("a physical request");
        self.engine.http_prepared(
            at,
            preparation,
            Prepared {
                id: 9,
                expires_at: None,
            },
        );
        let permit = self
            .engine
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
            self.engine.claim_http(at, permit),
            Ok(Claim::Released(_))
        ));
        preparation.attempt().request
    }
    fn respond(&mut self, at: f64, request: RequestId) {
        self.handle(
            at,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
    }
    fn done(&self) -> Vec<(CommandId, String)> {
        self.actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandDone { command, result }) => {
                    Some((*command, result.clone()))
                }
                _ => None,
            })
            .collect()
    }
    fn failed(&self) -> Vec<(CommandId, Option<ErrorKind>)> {
        self.actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    Some((*command, error.kind))
                }
                _ => None,
            })
            .collect()
    }
    fn fire_execution_timer(&mut self, at: f64) {
        let timer = self.engine.io.execution_timer.expect("armed").0;
        self.handle(at, Event::TimerFired { timer });
    }
}

#[test]
fn joined_commands_share_one_request_and_each_completes_once() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    assert_eq!(run.prepares().len(), 1, "one shared physical request");
    let request = run.release(3.);
    run.respond(4., request);
    assert_eq!(run.engine.venue().publications, 1);
    assert_eq!(
        run.done(),
        [(1, "\"k1\"".to_owned()), (2, "\"k2\"".to_owned())]
    );
    assert!(run.failed().is_empty());
    assert_eq!(run.engine.io.execution_timer, None);
}

#[test]
fn an_expired_follower_fails_alone_and_the_founder_still_completes() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        timeouts: HashMap::from([(2, 100)]),
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    let request = run.release(3.);
    assert_eq!(run.engine.io.execution_timer.map(|(_, at)| at), Some(102.));
    run.fire_execution_timer(102.);
    assert_eq!(run.failed(), [(2, Some(ErrorKind::Timeout))]);
    run.respond(110., request);
    assert_eq!(run.done(), [(1, "\"k1\"".to_owned())]);
    assert_eq!(run.failed().len(), 1);
}

#[test]
fn execution_deadline_with_a_leg_in_flight_seals_through_the_physical_timeout_once() {
    let mut run = Run::new(GroupProbe::default());
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    let request = run.release(3.);
    // The leg owns the execution deadline; no separate execution timer.
    assert_eq!(run.engine.io.execution_timer, None);
    let timer = run.engine.io.managed_http[&request].timer;
    run.handle(1_001., Event::TimerFired { timer });
    assert_eq!(run.engine.venue().failures, 1);
    assert_eq!(
        run.failed(),
        [(1, Some(ErrorKind::Timeout)), (2, Some(ErrorKind::Timeout))]
    );
    run.handle(1_002., Event::TimerFired { timer });
    assert_eq!(run.failed().len(), 2);
}

#[test]
fn a_late_or_second_response_cannot_complete_or_publish_again() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    run.handle(1., fetch(1));
    let request = run.release(2.);
    run.respond(3., request);
    run.respond(4., request);
    assert_eq!(run.engine.venue().publications, 1);
    assert_eq!(run.done().len(), 1);
}

#[test]
fn observers_cannot_be_finished_numerically_or_through_their_own_root() {
    let mut run = Run::new(GroupProbe::default());
    run.handle(1., fetch(1));
    let root = run.engine.venue().roots[&1].clone();
    let mut cx = Cx {
        now: 2.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    assert!(cx.done_context(&root, "direct".into()).is_err());
    cx.done(1, "numeric".into());
    run.actions.extend(run.engine.drain_actions());
    assert!(run.done().is_empty());
    assert!(run.failed().is_empty());
}

#[test]
fn only_exclusive_entry_roots_can_join() {
    let mut run = Run::new(GroupProbe {
        independent_leg: Some(2),
        unentered: Some(3),
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    run.handle(3., fetch(3));
    assert_eq!(
        run.engine.venue().join_errors,
        [
            crate::request_context::Error::State,
            crate::request_context::Error::Revoked
        ]
    );
    assert_eq!(run.prepares().len(), 1);
}

#[test]
fn an_execution_with_no_live_observer_never_releases_its_leg() {
    let mut run = Run::new(GroupProbe::default());
    run.handle(1., fetch(1));
    let root = run.engine.venue().roots[&1].clone();
    let mut cx = Cx {
        now: 2.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    cx.detach_execution_observer(&root, VenueError::new("cancelled"))
        .unwrap();
    run.actions.extend(run.engine.drain_actions());
    assert_eq!(run.failed(), [(1, None)]);
    let [preparation] = run.prepares()[..] else {
        panic!("one request")
    };
    run.engine.http_prepared(
        3.,
        preparation,
        Prepared {
            id: 9,
            expires_at: None,
        },
    );
    let bound = run.engine.drain_actions().find_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Bind { permit, .. } => Some(permit),
            _ => None,
        },
        _ => None,
    });
    if let Some(permit) = bound {
        assert!(run.engine.claim_http(3., permit).is_err());
    }
    assert!(run.done().is_empty());
}

#[test]
fn a_member_cannot_issue_its_own_physical_request() {
    let mut run = Run::new(GroupProbe::default());
    run.handle(1., fetch(1));
    let root = run.engine.venue().roots[&1].clone();
    let mut cx = Cx {
        now: 2.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    assert_eq!(
        cx.http_leg_context(&root, read_policy(500)),
        Err(crate::request_context::Error::Revoked)
    );
    assert_eq!(
        cx.http_context(Scope::Command(1), read_policy(500)),
        Err(crate::request_context::Error::Revoked)
    );
    // Nor may it send on the entry root it already holds.
    let managed = cx.io.managed_http.len();
    let sent = cx.request_managed(
        root.clone(),
        crate::Method::Get,
        "https://venue.test/own-root".into(),
        None,
        root.clone(),
    );
    assert_eq!(
        sent.map_err(|(_, error)| error),
        Err(crate::request_context::Error::Revoked)
    );
    assert_eq!(cx.io.managed_http.len(), managed);
}

#[test]
fn a_response_after_an_observer_deadline_times_it_out_before_projection() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        timeouts: HashMap::from([(2, 100)]),
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    let request = run.release(3.);
    // The host has not delivered the execution timer yet.
    run.respond(150., request);
    assert_eq!(run.failed(), [(2, Some(ErrorKind::Timeout))]);
    assert_eq!(run.done(), [(1, "\"k1\"".to_owned())]);
}

#[test]
fn abort_before_release_cancels_the_unsent_leg_immediately() {
    let mut run = Run::new(GroupProbe::default());
    run.handle(1., fetch(1));
    let execution = run.engine.venue().execution.clone().unwrap();
    let mut cx = Cx {
        now: 2.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    cx.abort_execution(&execution, VenueError::new("stop"))
        .unwrap();
    assert!(run.engine.io.managed_http.is_empty());
    run.actions.extend(run.engine.drain_actions());
    assert!(run.actions.iter().any(|action| matches!(
        action,
        Action::ManagedHttp(action) if matches!(**action, HttpAction::Cancel { .. })
    )));
    assert_eq!(run.failed(), [(1, None)]);
}

#[test]
fn a_detached_founder_leaves_the_shared_request_to_the_joiner() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    let root = run.engine.venue().roots[&1].clone();
    let mut cx = Cx {
        now: 3.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    cx.detach_execution_observer(&root, VenueError::new("cancelled"))
        .unwrap();
    assert_eq!(run.engine.io.managed_http.len(), 1);
    let request = run.release(4.);
    run.respond(5., request);
    assert_eq!(run.done(), [(2, "\"k2\"".to_owned())]);
    assert_eq!(run.engine.venue().publications, 1);
}

#[test]
fn begin_and_join_need_the_observer_commands_own_genuine_callback() {
    let mut run = Run::new(GroupProbe {
        hold: Some(1),
        ..Default::default()
    });
    run.handle(1., fetch(1));
    run.handle(2., fetch(2));
    assert_eq!(
        run.engine.venue().cross_errors,
        [crate::request_context::Error::Revoked]
    );
    // The genuine callback of command 2 began the execution.
    assert_eq!(run.prepares().len(), 1);
    let held = run.engine.venue().roots[&1].clone();
    let execution = run.engine.venue().execution.clone().unwrap();
    // A stored valid root outside any command callback (timer, start, response).
    let mut cx = Cx {
        now: 3.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    assert_eq!(
        cx.join_execution(&execution, &held, "k1"),
        Err(crate::request_context::Error::Revoked)
    );
    assert_eq!(
        cx.begin_execution(&held, read_policy(1_000), Window::UntilTerminal, "k1")
            .map(|_| ()),
        Err(crate::request_context::Error::Revoked)
    );
}

/// Begin a background-rooted read (or the successor of `previous`) with one
/// leg; returns the background root.
fn background_read(run: &mut Run, at: f64, previous: Option<ExecutionHandle>) -> Context {
    let mut cx = Cx {
        now: at,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    let background = cx
        .http_context(Scope::Background, read_policy(1_000))
        .unwrap();
    let execution = match previous {
        None => cx
            .begin_background_execution(&background, read_policy(1_000), Window::BeforeRelease)
            .unwrap(),
        Some(previous) => cx
            .begin_successor_execution(&previous, &background, read_policy(1_000))
            .unwrap(),
    };
    let leg = cx
        .http_leg_context(execution.context(), read_policy(1_000))
        .unwrap();
    cx.request_managed(
        leg.clone(),
        Method::Get,
        "https://group.test/account".into(),
        None,
        leg,
    )
    .unwrap();
    run.engine.venue.execution = Some(execution);
    run.actions.extend(run.engine.drain_actions());
    background
}

fn cx_at(run: &mut Run, now: f64) -> Cx<'_, Context> {
    Cx {
        now,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    }
}

#[test]
fn a_background_root_publishes_and_completes_without_command_observers() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    background_read(&mut run, 1., None);
    let request = run.release(2.);
    run.respond(3., request);
    assert_eq!(run.engine.venue().publications, 1);
    assert!(run.done().is_empty() && run.failed().is_empty());
}

#[test]
fn owners_joining_after_release_wait_for_the_next_background_read() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    background_read(&mut run, 1., None);
    run.handle(2., fetch(1));
    let first = run.release(3.);
    run.handle(4., fetch(2));
    assert_eq!(
        run.engine.venue().joined,
        [(1, Joined::Live(vec![])), (2, Joined::Pending)]
    );
    run.respond(5., first);
    assert_eq!(run.done(), [(1, "\"k1\"".to_owned())]);
    let previous = run.engine.venue().execution.clone();
    background_read(&mut run, 6., previous);
    let second = run.release(7.);
    run.respond(8., second);
    assert_eq!(
        run.done(),
        [(1, "\"k1\"".to_owned()), (2, "\"k2\"".to_owned())]
    );
}

#[test]
fn a_failed_read_fails_only_owners_that_joined_before_its_release() {
    let mut run = Run::new(GroupProbe {
        abort: true,
        ..Default::default()
    });
    background_read(&mut run, 1., None);
    run.handle(2., fetch(1));
    let first = run.release(3.);
    run.handle(4., fetch(2));
    run.respond(5., first);
    assert_eq!(run.failed(), [(1, None)]);
    run.engine.venue.abort = false;
    run.engine.venue.complete = true;
    let previous = run.engine.venue().execution.clone();
    background_read(&mut run, 6., previous);
    let second = run.release(7.);
    run.respond(8., second);
    assert_eq!(run.done(), [(2, "\"k2\"".to_owned())]);
}

#[test]
fn a_queued_owner_times_out_once_when_no_next_read_arrives() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        timeouts: HashMap::from([(2, 100)]),
        ..Default::default()
    });
    background_read(&mut run, 1., None);
    let first = run.release(2.);
    run.handle(3., fetch(2));
    run.respond(4., first);
    assert!(run.done().is_empty());
    assert_eq!(run.engine.io.execution_timer.map(|(_, at)| at), Some(103.));
    run.fire_execution_timer(103.);
    assert_eq!(run.failed(), [(2, Some(ErrorKind::Timeout))]);
    assert_eq!(run.engine.io.execution_timer, None);
}

#[test]
fn background_roots_refuse_command_roots_and_writes() {
    let mut run = Run::new(GroupProbe::default());
    let mut cx = Cx {
        now: 1.,
        store: &mut run.engine.store,
        io: &mut run.engine.io,
    };
    let command = cx
        .http_context(Scope::Command(9), read_policy(1_000))
        .unwrap();
    let background = cx
        .http_context(Scope::Background, read_policy(1_000))
        .unwrap();
    let mut write = read_policy(1_000);
    write.operation = Operation::Mutation;
    for (root, policy) in [(&command, read_policy(1_000)), (&background, write)] {
        assert_eq!(
            cx.begin_background_execution(root, policy, Window::BeforeRelease)
                .map(|_| ()),
            Err(crate::request_context::Error::Invalid)
        );
    }
}

#[test]
fn a_finished_background_root_publishes_only_while_a_command_observer_survives() {
    for observed in [false, true] {
        let mut run = Run::new(GroupProbe {
            complete: true,
            ..Default::default()
        });
        let background = background_read(&mut run, 1., None);
        if observed {
            run.handle(2., fetch(1));
        }
        let request = run.release(3.);
        cx_at(&mut run, 4.)
            .finish_http_context(&background)
            .unwrap();
        run.respond(5., request);
        assert_eq!(run.engine.venue().publications, usize::from(observed));
        assert_eq!(run.done().len(), usize::from(observed));
    }
}

#[test]
fn a_successor_needs_the_same_background_credential() {
    let mut run = Run::new(GroupProbe::default());
    background_read(&mut run, 1., None);
    run.release(2.);
    let previous = run.engine.venue().execution.clone().unwrap();
    let mut cx = cx_at(&mut run, 3.);
    let mut other = read_policy(1_000);
    other.credential = Some(crate::request_context::Credential {
        slot: CredentialSlot::Primary,
        reference: crate::signing::KeyRef("other-account".into()),
    });
    let foreign = cx.http_context(Scope::Background, other).unwrap();
    assert_eq!(
        cx.begin_successor_execution(&previous, &foreign, read_policy(1_000))
            .map(|_| ()),
        Err(crate::request_context::Error::Invalid)
    );
}

#[test]
fn one_background_context_roots_at_most_one_live_execution() {
    let mut run = Run::new(GroupProbe::default());
    let background = background_read(&mut run, 1., None);
    assert_eq!(
        cx_at(&mut run, 2.)
            .begin_background_execution(&background, read_policy(1_000), Window::BeforeRelease)
            .map(|_| ()),
        Err(crate::request_context::Error::State)
    );
}

#[test]
fn a_succeeded_execution_refuses_joins_and_a_second_successor() {
    let mut run = Run::new(GroupProbe::default());
    background_read(&mut run, 1., None);
    run.release(2.);
    run.handle(3., fetch(1));
    let previous = run.engine.venue().execution.clone().unwrap();
    background_read(&mut run, 4., Some(previous.clone()));
    // A caller still holding the old handle cannot strand an owner on it.
    run.engine.venue.execution = Some(previous.clone());
    run.handle(5., fetch(2));
    assert_eq!(
        run.engine.venue().join_errors,
        [crate::request_context::Error::State]
    );
    let mut cx = cx_at(&mut run, 6.);
    let background = cx
        .http_context(Scope::Background, read_policy(1_000))
        .unwrap();
    assert_eq!(
        cx.begin_successor_execution(&previous, &background, read_policy(1_000))
            .map(|_| ()),
        Err(crate::request_context::Error::State)
    );
}

#[test]
fn a_retired_previous_execution_has_no_successor() {
    let mut run = Run::new(GroupProbe {
        complete: true,
        ..Default::default()
    });
    background_read(&mut run, 1., None);
    let request = run.release(2.);
    run.respond(3., request);
    let previous = run.engine.venue().execution.clone().unwrap();
    let mut cx = cx_at(&mut run, 4.);
    let background = cx
        .http_context(Scope::Background, read_policy(1_000))
        .unwrap();
    assert_eq!(
        cx.begin_successor_execution(&previous, &background, read_policy(1_000))
            .map(|_| ()),
        Err(crate::request_context::Error::Revoked)
    );
}

#[test]
fn a_group_retired_before_its_leg_is_sent_leaves_no_stale_root() {
    let mut run = Run::new(GroupProbe::default());
    for round in 0..3 {
        let command = 10 + round;
        run.engine.venue.execution = None;
        run.handle(f64::from(round) * 10. + 1., fetch(command));
        let root = run.engine.venue().roots[&command].clone();
        // The sole observer leaves before its leg is released.
        let mut cx = Cx {
            now: f64::from(round) * 10. + 2.,
            store: &mut run.engine.store,
            io: &mut run.engine.io,
        };
        cx.detach_execution_observer(&root, VenueError::new("cancelled"))
            .unwrap();
        assert!(run.engine.io.managed_http.is_empty());
        assert!(!run.engine.io.request_contexts.has_executions());
        // Any later event retires the stale root.
        run.handle(
            f64::from(round) * 10. + 3.,
            Event::TimerFired { timer: TimerId(1) },
        );
        assert_eq!(run.engine.io.request_contexts.root_count(), 0);
    }
}
