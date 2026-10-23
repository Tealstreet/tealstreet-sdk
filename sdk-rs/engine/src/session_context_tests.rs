use super::super::super::*;
use super::SESSIONS;
use crate::request_context::{
    Credential, Error, HttpAction, Operation, Owner, Policy, Preparation, Prepared, RetryPolicy,
    Scope, Session, SessionPolicy, WriteResult,
};

#[derive(Debug)]
struct Pending(crate::request_context::Context);

#[derive(Clone, Copy, PartialEq)]
enum Fact {
    None,
    Accepted,
    Rejected,
}

struct Probe {
    fact: Fact,
    responses: Vec<(Owner, String)>,
    failures: Vec<Owner>,
    preparation_failures: Vec<(Error, Option<crate::Outcome>)>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            fact: Fact::None,
            responses: vec![],
            failures: vec![],
            preparation_failures: vec![],
        }
    }
}

impl Venue for Probe {
    type Pending = Pending;
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<crate::request_context::ReconciliationData, Error> {
        assert!(
            matches!(pending.0.owner(), Owner::Command { .. }),
            "session writes never capture reconciliation"
        );
        crate::request_context::ReconciliationData::new("command".into())
    }
    fn on_http_preparation_failed(
        &mut self,
        _: &mut Cx<'_, Pending>,
        _: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.preparation_failures.push((reason, error.outcome));
    }
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.responses
            .push((pending.0.owner(), response.body.to_owned()));
        if pending.0.operation() == Operation::Mutation
            && matches!(pending.0.owner(), Owner::Session { .. })
        {
            let fact = match self.fact {
                Fact::None => None,
                Fact::Accepted => Some(WriteResult::Accepted { order_ids: vec![] }),
                Fact::Rejected => {
                    Some(WriteResult::Rejected(VenueError::new("listen key refused")))
                }
            };
            if let Some(fact) = fact {
                cx.http_write_result(&pending.0, fact).unwrap();
            }
        }
        if matches!(pending.0.owner(), Owner::Session { .. }) {
            cx.finish_http_context(&pending.0).unwrap();
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, pending: Pending, _: &str) {
        self.failures.push(pending.0.owner());
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
        panic!("engine timer leaked to venue");
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine() -> Engine<Probe> {
    let mut engine = crate::test_support::seeded_engine(Probe::default());
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine
}

fn with<R>(engine: &mut Engine<Probe>, now: f64, f: impl FnOnce(&mut Cx<'_, Pending>) -> R) -> R {
    f(&mut Cx {
        now,
        store: &mut engine.store,
        io: &mut engine.io,
    })
}

fn policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

fn open(engine: &mut Engine<Probe>, repeat_safe: bool) -> Session {
    with(engine, 0., |cx| {
        cx.open_session(SessionPolicy {
            credential: None,
            repeat_safe,
        })
    })
    .unwrap()
}

fn queue(
    engine: &mut Engine<Probe>,
    context: &crate::request_context::Context,
    now: f64,
) -> Preparation {
    with(engine, now, |cx| {
        cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/listenKey".into(),
            None,
            Pending(context.clone()),
        )
    })
    .unwrap();
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("preparation for queued session work")
}

fn session_write(engine: &mut Engine<Probe>, session: &Session, now: f64) -> Preparation {
    let context = with(engine, now, |cx| {
        cx.session_context(session, policy(Operation::Mutation))
    })
    .unwrap();
    queue(engine, &context, now)
}

fn release(engine: &mut Engine<Probe>, token: Preparation, at: f64) -> RequestId {
    engine.http_prepared(
        at,
        token,
        Prepared {
            id: token.attempt().request.0,
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
        .expect("bound session packet");
    assert!(matches!(
        engine.claim_http(at, permit),
        Ok(crate::request_context::Claim::Released(_))
    ));
    engine.drain_actions().for_each(drop);
    token.attempt().request
}

fn respond(engine: &mut Engine<Probe>, request: RequestId, at: f64, body: &str) {
    engine.handle(
        at,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    engine.drain_actions().for_each(drop);
}

fn lose(engine: &mut Engine<Probe>, request: RequestId, at: f64) {
    engine.handle(
        at,
        Event::HttpFailed {
            request,
            error: "connection reset",
        },
    );
    engine.drain_actions().for_each(drop);
}

#[test]
fn session_mutation_releases_without_command_journal_or_reconciliation() {
    let mut engine = engine();
    assert_eq!(
        with(&mut engine, 0., |cx| cx
            .http_context(Scope::Background, policy(Operation::Mutation))),
        Err(Error::Invalid),
        "plain background still cannot own a mutation"
    );
    let session = open(&mut engine, true);
    let revision = engine.io.http_write_revision;
    let request = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 2.)
    };
    assert!(!engine.io.http_responses.contains(request));
    assert_eq!(engine.io.http_write_revision, revision);
    respond(&mut engine, request, 3., "{\"listenKey\":\"k\"}");
    assert_eq!(
        engine.venue.responses,
        [(session.owner(), "{\"listenKey\":\"k\"}".to_owned())]
    );
}

#[test]
fn session_release_does_not_fence_command_reconciliation() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let command = with(&mut engine, 1., |cx| {
        cx.http_context(Scope::Command(7), policy(Operation::Mutation))
    })
    .unwrap();
    let write = {
        let token = queue(&mut engine, &command, 1.);
        release(&mut engine, token, 2.)
    };
    let fence = engine
        .io
        .http_responses
        .get_mut(write)
        .unwrap()
        .write_revision;
    {
        let token = session_write(&mut engine, &session, 3.);
        release(&mut engine, token, 4.)
    };
    assert_eq!(
        engine.io.http_write_revision, fence,
        "a keepalive must not supersede an unrelated command write"
    );
}

#[test]
fn retirement_before_release_emits_no_http_and_no_callback() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let token = session_write(&mut engine, &session, 1.);
    with(&mut engine, 2., |cx| cx.retire_session(&session)).unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Cancel { preparation, .. } if *preparation == token))));
    engine.http_prepared(
        3.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(engine.drain_actions().all(|action| !matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. }))));
    assert!(engine.venue.responses.is_empty());
    assert!(engine.venue.failures.is_empty());
    assert!(engine.venue.preparation_failures.is_empty());
}

#[test]
fn late_response_after_retirement_never_reaches_the_venue() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let request = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 2.)
    };
    with(&mut engine, 3., |cx| cx.retire_session(&session)).unwrap();
    respond(&mut engine, request, 4., "{\"listenKey\":\"stale\"}");
    lose(&mut engine, request, 5.);
    assert!(engine.venue.responses.is_empty());
    assert!(engine.venue.failures.is_empty());
}

#[test]
fn retired_and_foreign_sessions_issue_nothing() {
    let mut engine = engine();
    let mut other = self::engine();
    let foreign = open(&mut other, true);
    assert_eq!(
        with(&mut engine, 0., |cx| cx
            .session_context(&foreign, policy(Operation::Read))),
        Err(Error::Revoked)
    );
    assert_eq!(
        with(&mut engine, 0., |cx| cx.retire_session(&foreign)),
        Err(Error::Revoked)
    );
    let session = open(&mut engine, true);
    let context = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Read))
    })
    .unwrap();
    with(&mut engine, 2., |cx| cx.retire_session(&session)).unwrap();
    assert!(!with(&mut engine, 2., |cx| cx.session_live(&session)));
    assert_eq!(
        with(&mut engine, 2., |cx| cx
            .session_context(&session, policy(Operation::Read))),
        Err(Error::Revoked)
    );
    let queued = with(&mut engine, 2., |cx| {
        cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/bullet".into(),
            None,
            Pending(context.clone()),
        )
    });
    assert!(matches!(queued, Err((_, Error::Revoked))));
    assert_ne!(
        open(&mut engine, true).owner(),
        session.owner(),
        "a reopened session is a new generation"
    );
}

#[test]
fn one_physical_attempt_per_session_mutation() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let context = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Mutation))
    })
    .unwrap();
    let request = {
        let token = queue(&mut engine, &context, 1.);
        release(&mut engine, token, 2.)
    };
    lose(&mut engine, request, 3.);
    assert_eq!(engine.venue.failures, [session.owner()]);
    let resend = with(&mut engine, 4., |cx| {
        cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/listenKey".into(),
            None,
            Pending(context.clone()),
        )
    });
    assert!(matches!(resend, Err((_, Error::RetryForbidden))));
    assert!(
        with(&mut engine, 4., |cx| cx.session_context(
            &session,
            Policy {
                retry: RetryPolicy::ReadFailure,
                ..policy(Operation::Mutation)
            }
        ))
        .is_err(),
        "read-failure retry never applies to a session write"
    );
}

#[test]
fn unresolved_write_blocks_a_session_that_is_not_repeat_safe() {
    let mut engine = engine();
    let session = open(&mut engine, false);
    let request = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 2.)
    };
    // In flight: not yet proven.
    assert_eq!(
        with(&mut engine, 3., |cx| cx
            .session_context(&session, policy(Operation::Mutation))),
        Err(Error::State)
    );
    lose(&mut engine, request, 3.);
    assert_eq!(
        with(&mut engine, 4., |cx| cx
            .session_context(&session, policy(Operation::Mutation))),
        Err(Error::State)
    );
    assert!(
        with(&mut engine, 4., |cx| cx
            .session_context(&session, policy(Operation::Read)))
        .is_ok()
    );
    with(&mut engine, 5., |cx| cx.retire_session(&session)).unwrap();
    let fresh = open(&mut engine, false);
    session_write(&mut engine, &fresh, 6.);
}

#[test]
fn unclassified_response_is_unresolved_but_codec_facts_resolve() {
    for (fact, resolved) in [
        (Fact::None, false),
        (Fact::Accepted, true),
        (Fact::Rejected, true),
    ] {
        let mut engine = engine();
        engine.venue.fact = fact;
        let session = open(&mut engine, false);
        let request = {
            let token = session_write(&mut engine, &session, 1.);
            release(&mut engine, token, 2.)
        };
        respond(&mut engine, request, 3., "<html>gateway</html>");
        let next = with(&mut engine, 4., |cx| {
            cx.session_context(&session, policy(Operation::Mutation))
        });
        assert_eq!(next.is_ok(), resolved, "{:?}", next);
    }
}

#[test]
fn repeat_safe_session_admits_recovery_after_unknown() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let request = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 2.)
    };
    lose(&mut engine, request, 3.);
    let next = {
        let token = session_write(&mut engine, &session, 4.);
        release(&mut engine, token, 5.)
    };
    respond(&mut engine, next, 6., "{\"listenKey\":\"k\"}");
    assert_eq!(engine.venue.responses.len(), 1);
}

#[test]
fn unreleased_failure_is_not_sent_and_leaves_the_session_resolved() {
    let mut engine = engine();
    let session = open(&mut engine, false);
    let token = session_write(&mut engine, &session, 1.);
    engine.http_preparation_failed(
        2.,
        token,
        Error::State,
        VenueError::new("route unavailable"),
    );
    assert_eq!(
        engine.venue.preparation_failures,
        [(Error::State, Some(crate::Outcome::NotSent))]
    );
    session_write(&mut engine, &session, 3.);
}

#[test]
fn session_contexts_require_the_session_credential_and_keep_their_owner() {
    let mut engine = engine();
    let credential = Credential {
        slot: CredentialSlot::Primary,
        reference: crate::signing::KeyRef("primary".into()),
    };
    let session = with(&mut engine, 0., |cx| {
        cx.open_session(SessionPolicy {
            credential: Some(credential.clone()),
            repeat_safe: true,
        })
    })
    .unwrap();
    assert_eq!(
        with(&mut engine, 1., |cx| cx
            .session_context(&session, policy(Operation::Mutation))),
        Err(Error::Invalid),
        "an unsigned packet cannot speak for a keyed session"
    );
    let keyed = Policy {
        credential: Some(credential),
        ..policy(Operation::Read)
    };
    let read = with(&mut engine, 1., |cx| {
        cx.session_context(&session, keyed.clone())
    })
    .unwrap();
    let leg = with(&mut engine, 2., |cx| {
        cx.http_leg_context(&read, keyed.clone())
    })
    .unwrap();
    assert_eq!(leg.owner(), session.owner());
    assert_eq!(leg.deadline(), read.deadline());
    assert_eq!(
        with(&mut engine, 2., |cx| cx.http_leg_context(
            &read,
            Policy {
                operation: Operation::Mutation,
                ..keyed
            }
        )),
        Err(Error::Invalid)
    );
    assert!(
        with(&mut engine, 2., |cx| cx.done_context(&read, "null".into())).is_err(),
        "a session is not command completion authority"
    );
}

#[test]
fn bounded_sessions_and_operation_records() {
    let mut engine = engine();
    let sessions = (0..SESSIONS)
        .map(|_| open(&mut engine, true))
        .collect::<Vec<_>>();
    assert_eq!(
        with(&mut engine, 0., |cx| cx.open_session(SessionPolicy {
            credential: None,
            repeat_safe: true,
        })),
        Err(Error::Capacity)
    );
    with(&mut engine, 0., |cx| cx.retire_session(&sessions[0])).unwrap();
    open(&mut engine, true);
    let session = &sessions[1];
    for step in 0..300 {
        let at = f64::from(step) * 10.;
        let token = session_write(&mut engine, session, at);
        let request = release(&mut engine, token, at + 1.);
        engine.venue.fact = Fact::Accepted;
        respond(&mut engine, request, at + 2., "{}");
        assert_eq!(engine.venue.responses.len(), step as usize + 1);
    }
    let Owner::Session { generation } = session.owner() else {
        unreachable!()
    };
    assert!(
        engine.io.request_contexts.sessions.live[&generation]
            .writes
            .len()
            <= super::super::CAPACITY,
        "finished operations must not grow session state without bound"
    );
}

#[test]
fn session_handles_carry_no_credential_material() {
    let mut engine = engine();
    let session = with(&mut engine, 0., |cx| {
        cx.open_session(SessionPolicy {
            credential: Some(Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("primary".into()),
            }),
            repeat_safe: true,
        })
    })
    .unwrap();
    let debug = format!("{session:?}");
    assert!(!debug.contains("primary"), "{debug}");
}
