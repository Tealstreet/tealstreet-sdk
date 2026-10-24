// The probe pending is a whole Context, returned by refused managed requests.
#![allow(clippy::result_large_err)]
use super::super::super::*;
use super::SESSIONS;
use crate::request_context::{
    Credential, Error, HttpAction, Operation, Owner, Policy, Preparation, Prepared, RetryPolicy,
    Scope, Session, SessionPolicy, Waiter, WriteResult,
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
    fact_on_failure: Fact,
    command_fact: Fact,
    classified: bool,
    refusal: Option<&'static crate::classify::CodeTable>,
    finish_first: bool,
    retire_in_response: Option<Session>,
    command_write_fact: bool,
    items: usize,
    settle_in_response: Option<(crate::request_context::Context, u32)>,
    refuse_in_response: Option<u32>,
    facts: Vec<Result<(), Error>>,
    completions: Vec<Result<(), Error>>,
    responses: Vec<(Owner, String)>,
    failures: Vec<Owner>,
    preparation_failures: Vec<(Error, Option<crate::Outcome>)>,
    grants: Vec<bool>,
    reconciles: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            fact: Fact::None,
            fact_on_failure: Fact::None,
            command_fact: Fact::None,
            classified: false,
            refusal: None,
            finish_first: false,
            retire_in_response: None,
            command_write_fact: false,
            items: 0,
            settle_in_response: None,
            refuse_in_response: None,
            facts: vec![],
            completions: vec![],
            responses: vec![],
            failures: vec![],
            preparation_failures: vec![],
            grants: vec![],
            reconciles: 0,
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
    fn on_session_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        if matches!(pending.0.owner(), Owner::Command { .. }) {
            self.completions
                .push(cx.fail_context(&pending.0, error.clone()));
        }
        self.on_http_preparation_failed(cx, pending, Error::TimedOut, error);
    }
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.responses
            .push((pending.0.owner(), response.body.to_owned()));
        if let Some(session) = self.retire_in_response.take() {
            cx.retire_session(&session).unwrap();
            return;
        }
        let session = matches!(pending.0.owner(), Owner::Session { .. });
        if session && let Some((waiter, legacy)) = self.settle_in_response.take() {
            self.facts
                .push(cx.fail_context(&waiter, VenueError::new("login failed")));
            cx.fail(legacy, VenueError::new("login failed"));
        }
        if session && let Some(managed) = self.refuse_in_response.take() {
            self.facts
                .push(cx.fail_context(&pending.0, VenueError::new("not a command")));
            cx.fail(
                managed,
                VenueError::new("numeric completion of a managed write"),
            );
        }
        if !session && self.command_write_fact && pending.0.operation() == Operation::Mutation {
            self.facts.push(
                cx.http_write_fact(
                    &pending.0,
                    crate::classify::WriteFact::Accepted { order_ids: vec![] },
                    None,
                )
                .map(drop),
            );
        }
        if session && self.finish_first {
            cx.finish_http_context(&pending.0).unwrap();
        }
        if pending.0.operation() == Operation::Mutation
            && session
            && let Some(table) = self.refusal
        {
            let native = crate::classify::Native {
                code: crate::classify::NativeCode::Int(-2015),
                message: "invalid API key",
            };
            let failure = cx.http_write_failure(&pending.0, table, Some(&native), "refused", None);
            if let Ok((_, grant)) = &failure {
                self.grants.push(grant.is_some());
            }
            self.facts.push(failure.map(drop));
        } else if pending.0.operation() == Operation::Mutation && session && self.classified {
            self.facts.push(
                cx.http_write_fact(
                    &pending.0,
                    crate::classify::WriteFact::Accepted { order_ids: vec![] },
                    None,
                )
                .map(drop),
            );
        } else if pending.0.operation() == Operation::Mutation
            && session
            && let Some(fact) = fact(self.fact)
        {
            self.facts.push(cx.http_write_result(&pending.0, fact));
        }
        if matches!(pending.0.owner(), Owner::Command { .. })
            && pending.0.operation() == Operation::Mutation
        {
            // A resendable native refusal goes through the classifier, which
            // alone grants the one resend a parked owner may later send.
            if self.command_fact == Fact::Rejected {
                let native = crate::classify::Native {
                    code: crate::classify::NativeCode::Int(-1021),
                    message: "timestamp outside recvWindow",
                };
                let failed =
                    cx.http_write_failure(&pending.0, &RESEND, Some(&native), "refused", None);
                self.facts.push(failed.map(drop));
            } else if let Some(fact) = fact(self.command_fact) {
                self.facts.push(cx.http_write_result(&pending.0, fact));
            }
        }
        if session && !self.finish_first {
            cx.finish_http_context(&pending.0).unwrap();
        }
    }
    fn on_item(&mut self, _: &mut Cx<'_, Pending>, _: &mut Pending, _: &str) {
        self.items += 1;
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: &str) {
        self.failures.push(pending.0.owner());
        if let Some(fact) = fact(self.fact_on_failure) {
            self.facts.push(cx.http_write_result(&pending.0, fact));
        }
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
        panic!("engine timer leaked to venue");
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn reconcile_uncertain(&mut self, _: &mut Cx<'_, Pending>, _: Option<&[String]>) {
        self.reconciles += 1;
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

const RESEND: crate::classify::CodeTable = crate::classify::CodeTable {
    rows: &[crate::classify::Row {
        key: crate::classify::Key::Int(-1021),
        verdict: crate::classify::Verdict::RefusedResend(crate::ErrorKind::AuthInvalid),
        only: None,
    }],
    success: &[],
    unlisted: crate::classify::Verdict::Unknown(None),
    status: &[],
};

fn fact(fact: Fact) -> Option<WriteResult> {
    match fact {
        Fact::None => None,
        Fact::Accepted => Some(WriteResult::Accepted { order_ids: vec![] }),
        Fact::Rejected => Some(WriteResult::Rejected(VenueError::new("listen key refused"))),
    }
}

const fn refusing(verdict: crate::classify::Verdict) -> crate::classify::CodeTable {
    crate::classify::CodeTable {
        rows: &[],
        success: &[],
        unlisted: verdict,
        status: &[],
    }
}
const REFUSED: crate::classify::CodeTable = refusing(crate::classify::Verdict::Refused(
    crate::ErrorKind::AuthInvalid,
));
const REFUSED_RESEND: crate::classify::CodeTable = refusing(
    crate::classify::Verdict::RefusedResend(crate::ErrorKind::AuthInvalid),
);

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
            private_egress: None,
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
        with(&mut engine, 0., |cx| cx.retire_session(&foreign)).err(),
        Some(Error::Revoked)
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
            private_egress: None,
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
            private_egress: None,
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
            private_egress: None,
        })
    })
    .unwrap();
    let debug = format!("{session:?}");
    assert!(!debug.contains("primary"), "{debug}");
}

fn mutation(
    engine: &mut Engine<Probe>,
    session: &Session,
    now: f64,
) -> Result<crate::request_context::Context, Error> {
    with(engine, now, |cx| {
        cx.session_context(session, policy(Operation::Mutation))
    })
}

fn queue_result(
    engine: &mut Engine<Probe>,
    context: &crate::request_context::Context,
    now: f64,
) -> Result<RequestId, Error> {
    with(engine, now, |cx| {
        cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/login".into(),
            None,
            Pending(context.clone()),
        )
    })
    .map_err(|(_, error)| error)
}

#[test]
fn a_context_issued_earlier_cannot_bypass_the_unresolved_gate() {
    let mut engine = engine();
    let session = open(&mut engine, false);
    let first = mutation(&mut engine, &session, 1.).unwrap();
    let second = mutation(&mut engine, &session, 1.).unwrap();
    let token = queue(&mut engine, &first, 1.);
    let request = release(&mut engine, token, 2.);
    lose(&mut engine, request, 3.);
    assert_eq!(queue_result(&mut engine, &second, 4.), Err(Error::State));
}

#[test]
fn a_session_that_is_not_repeat_safe_sends_one_write_at_a_time() {
    let mut engine = engine();
    let session = open(&mut engine, false);
    let first = mutation(&mut engine, &session, 1.).unwrap();
    let second = mutation(&mut engine, &session, 1.).unwrap();
    queue(&mut engine, &first, 1.);
    assert_eq!(mutation(&mut engine, &session, 2.), Err(Error::State));
    assert_eq!(
        with(&mut engine, 2., |cx| cx
            .http_leg_context(&first, policy(Operation::Mutation))),
        Err(Error::State)
    );
    assert_eq!(queue_result(&mut engine, &second, 2.), Err(Error::State));
}

#[test]
fn an_engine_recorded_failure_cannot_be_overwritten_by_a_later_fact() {
    for fact in [Fact::Accepted, Fact::Rejected] {
        let mut engine = engine();
        engine.venue.fact_on_failure = fact;
        let session = open(&mut engine, false);
        let token = session_write(&mut engine, &session, 1.);
        let request = release(&mut engine, token, 2.);
        lose(&mut engine, request, 3.);
        assert_eq!(engine.venue.facts, [Err(Error::State)]);
        assert_eq!(mutation(&mut engine, &session, 4.), Err(Error::State));
    }
}

#[test]
fn a_released_write_that_times_out_is_unknown() {
    let mut engine = engine();
    let session = open(&mut engine, false);
    let context = mutation(&mut engine, &session, 1.).unwrap();
    queue_result(&mut engine, &context, 1.).unwrap();
    let mut timer = None;
    let mut token = None;
    for action in engine.drain_actions() {
        match action {
            Action::SetTimer { timer: id, .. } => timer = Some(id),
            Action::ManagedHttp(action) => {
                if let HttpAction::Prepare { preparation, .. } = *action {
                    token = Some(preparation);
                }
            }
            _ => {}
        }
    }
    release(&mut engine, token.unwrap(), 2.);
    engine.handle(
        context.deadline(),
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.venue.failures, [session.owner()]);
    assert_eq!(
        mutation(&mut engine, &session, context.deadline() + 1.),
        Err(Error::State)
    );
}

#[test]
fn finishing_before_recording_the_fact_leaves_the_write_unknown() {
    let mut engine = engine();
    engine.venue.fact = Fact::Accepted;
    engine.venue.finish_first = true;
    let session = open(&mut engine, false);
    let token = session_write(&mut engine, &session, 1.);
    let request = release(&mut engine, token, 2.);
    respond(&mut engine, request, 3., "{}");
    assert_eq!(engine.venue.facts.len(), 1);
    assert!(engine.venue.facts[0].is_err());
    assert_eq!(mutation(&mut engine, &session, 4.), Err(Error::State));
}

#[test]
fn a_session_can_retire_itself_from_its_own_response() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let first = session_write(&mut engine, &session, 1.);
    let second = session_write(&mut engine, &session, 1.);
    let first = release(&mut engine, first, 2.);
    let second = release(&mut engine, second, 2.);
    engine.venue.retire_in_response = Some(session.clone());
    respond(&mut engine, first, 3., "{\"code\":-1125}");
    respond(&mut engine, second, 4., "{}");
    lose(&mut engine, second, 5.);
    assert_eq!(engine.venue.responses.len(), 1);
    assert!(engine.venue.failures.is_empty());
    assert!(!with(&mut engine, 6., |cx| cx.session_live(&session)));
}

#[cfg(feature = "order-locks")]
#[test]
fn session_writes_supply_no_legacy_placement_evidence() {
    let mut engine = engine();
    engine
        .io
        .order_locks
        .acquire(
            &Command::PlaceOrders {
                command: 7,
                orders: vec![],
            },
            &engine.store,
            true,
        )
        .unwrap();
    let session = open(&mut engine, true);
    let token = session_write(&mut engine, &session, 1.);
    release(&mut engine, token, 2.);
    engine.io.order_locks.finish(7, true);
    assert!(
        !engine.io.order_locks.uncertain(""),
        "a keepalive is not a placement write"
    );
}

#[cfg(feature = "order-locks")]
#[test]
fn an_absent_order_answer_to_a_session_write_never_fences_or_reconciles() {
    use crate::classify::{Key, Row, SatisfiedIntent, Verdict};
    // Even an unrestricted absent-order row cannot make a setting a cancel.
    const ABSENT: crate::classify::CodeTable = crate::classify::CodeTable {
        rows: &[Row {
            key: Key::Int(-2015),
            verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
            only: None,
        }],
        ..refusing(Verdict::Refused(crate::ErrorKind::Rejected))
    };
    let mut engine = engine();
    engine.venue.refusal = Some(&ABSENT);
    let session = open(&mut engine, false);
    let token = session_write(&mut engine, &session, 1.);
    let request = release(&mut engine, token, 2.);
    respond(&mut engine, request, 3., "{\"code\":-2015}");
    assert_eq!(engine.venue.facts.len(), 1);
    assert!(!engine.io.order_locks.uncertain(""), "no order fence");
    assert_eq!(engine.venue.reconciles, 0, "no reconcile queued");
}

#[test]
fn context_capacity_refusal_commits_nothing() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    for _ in 0..super::super::CAPACITY {
        with(&mut engine, 1., |cx| {
            cx.session_context(&session, policy(Operation::Read))
        })
        .unwrap();
    }
    assert_eq!(mutation(&mut engine, &session, 1.), Err(Error::Capacity));
    let Owner::Session { generation } = session.owner() else {
        unreachable!()
    };
    assert!(
        engine.io.request_contexts.sessions.live[&generation]
            .writes
            .is_empty()
    );
    with(&mut engine, 1., |cx| cx.retire_session(&session)).unwrap();
    let fresh = open(&mut engine, true);
    session_write(&mut engine, &fresh, 2.);
}

#[test]
fn queued_session_packets_redact_the_listen_key() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let context = mutation(&mut engine, &session, 1.).unwrap();
    with(&mut engine, 1., |cx| {
        cx.request_managed(
            context.clone(),
            Method::Put,
            "https://venue.test/listenKey?listenKey=secret-key".into(),
            None,
            Pending(context.clone()),
        )
    })
    .unwrap();
    let debug = format!("{:?}", engine.drain_actions().collect::<Vec<_>>());
    assert!(!debug.contains("secret-key"), "{debug}");
}

fn owner(
    engine: &mut Engine<Probe>,
    scope: Scope,
    timeout_ms: u32,
    now: f64,
) -> crate::request_context::Context {
    with(engine, now, |cx| {
        cx.http_context(
            scope,
            Policy {
                timeout_ms,
                ..policy(Operation::Read)
            },
        )
    })
    .unwrap()
}

fn wait(
    engine: &mut Engine<Probe>,
    session: &Session,
    owner: &crate::request_context::Context,
    now: f64,
) -> Result<(), Error> {
    with(engine, now, |cx| {
        cx.session_wait(session, owner, Pending(owner.clone()))
    })
    .map_err(|(pending, error)| {
        assert_eq!(&pending.0, owner, "refusal returns the continuation");
        error
    })
}

fn timers(engine: &mut Engine<Probe>) -> Vec<Action> {
    engine
        .drain_actions()
        .filter(|action| matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect()
}

#[test]
fn ready_returns_live_waiters_in_arrival_order_and_clears_their_timers() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let owners = [
        owner(&mut engine, Scope::Background, 20_000, 1.),
        owner(&mut engine, Scope::Command(7), 20_000, 1.),
        owner(&mut engine, Scope::Background, 20_000, 1.),
    ];
    for owner in &owners {
        wait(&mut engine, &session, owner, 1.).unwrap();
    }
    let armed = timers(&mut engine);
    assert_eq!(armed.len(), 3);
    assert!(armed.iter().all(|action| matches!(action,
        Action::SetTimer { at_ms, .. } if *at_ms == owners[0].deadline())));
    let ready = with(&mut engine, 2., |cx| cx.session_ready(&session));
    let order = ready
        .into_iter()
        .map(|waiter| match waiter {
            Waiter::Ready(context, pending) => {
                assert_eq!(pending.0, context);
                context
            }
            Waiter::Failed(..) => panic!("live owner failed"),
        })
        .collect::<Vec<_>>();
    assert_eq!(order, owners);
    assert_eq!(
        timers(&mut engine)
            .iter()
            .filter(|action| matches!(action, Action::ClearTimer { .. }))
            .count(),
        3
    );
    assert!(with(&mut engine, 3., |cx| cx.session_ready(&session)).is_empty());
}

#[test]
fn an_expired_waiter_fails_alone_at_its_unchanged_deadline() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let short = owner(&mut engine, Scope::Background, 1_000, 1.);
    let long = owner(&mut engine, Scope::Background, 20_000, 1.);
    wait(&mut engine, &session, &short, 1.).unwrap();
    wait(&mut engine, &session, &long, 1.).unwrap();
    let timer = timers(&mut engine)
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == short.deadline() => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(500., Event::TimerFired { timer });
    assert_eq!(
        timers(&mut engine),
        [Action::SetTimer {
            timer,
            at_ms: short.deadline()
        }],
        "early delivery re-arms without extending the bound"
    );
    assert!(engine.venue.preparation_failures.is_empty());
    engine.handle(short.deadline(), Event::TimerFired { timer });
    assert_eq!(
        engine.venue.preparation_failures,
        [(Error::TimedOut, Some(crate::Outcome::NotSent))]
    );
    let ready = with(&mut engine, short.deadline(), |cx| {
        cx.session_ready(&session)
    });
    assert!(matches!(ready.as_slice(), [Waiter::Ready(context, _)] if *context == long));
}

#[test]
fn prerequisite_failure_and_retirement_fan_out_not_sent() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let first = owner(&mut engine, Scope::Background, 20_000, 1.);
    let second = owner(&mut engine, Scope::Command(7), 20_000, 1.);
    wait(&mut engine, &session, &first, 1.).unwrap();
    wait(&mut engine, &session, &second, 1.).unwrap();
    let failed = with(&mut engine, 2., |cx| {
        cx.session_failed(
            &session,
            VenueError::new("login refused").kind(crate::ErrorKind::AuthInvalid),
        )
    });
    assert_eq!(failed.len(), 2);
    for (_, _, error) in &failed {
        assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
        assert_eq!(error.kind, Some(crate::ErrorKind::AuthInvalid));
    }
    wait(&mut engine, &session, &first, 3.).unwrap();
    let retired = with(&mut engine, 4., |cx| cx.retire_session(&session)).unwrap();
    assert!(matches!(retired.as_slice(), [(context, _, error)]
        if *context == first && error.outcome == Some(crate::Outcome::NotSent)));
}

#[test]
fn expired_or_revoked_owners_come_back_failed_never_dropped() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let expired = owner(&mut engine, Scope::Background, 1_000, 1.);
    let finished = owner(&mut engine, Scope::Background, 20_000, 1.);
    wait(&mut engine, &session, &expired, 1.).unwrap();
    wait(&mut engine, &session, &finished, 1.).unwrap();
    with(&mut engine, 2., |cx| cx.finish_http_context(&finished)).unwrap();
    let ready = with(&mut engine, expired.deadline() + 1., |cx| {
        cx.session_ready(&session)
    });
    let kinds = ready
        .iter()
        .map(|waiter| match waiter {
            Waiter::Failed(_, _, error) => {
                assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
                error.kind
            }
            Waiter::Ready(..) => panic!("inactive owner re-dispatched"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            Some(crate::ErrorKind::Timeout),
            Some(crate::ErrorKind::InvalidInput)
        ]
    );
}

#[test]
fn waits_refuse_invalid_owners_and_return_the_continuation() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let own = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Read))
    })
    .unwrap();
    assert_eq!(wait(&mut engine, &session, &own, 1.), Err(Error::Invalid));
    let background = owner(&mut engine, Scope::Background, 1_000, 1.);
    wait(&mut engine, &session, &background, 1.).unwrap();
    assert_eq!(
        wait(&mut engine, &session, &background, 1.),
        Err(Error::Duplicate)
    );
    let late = owner(&mut engine, Scope::Background, 1_000, 1.);
    assert_eq!(
        wait(&mut engine, &session, &late, late.deadline()),
        Err(Error::TimedOut)
    );
    let mut other = self::engine();
    let foreign = open(&mut other, true);
    let fresh = owner(&mut engine, Scope::Background, 20_000, 1.);
    assert_eq!(wait(&mut engine, &foreign, &fresh, 1.), Err(Error::Revoked));
    let retired = open(&mut engine, true);
    let _ = with(&mut engine, 1., |cx| cx.retire_session(&retired)).unwrap();
    assert_eq!(wait(&mut engine, &retired, &fresh, 1.), Err(Error::Revoked));
}

#[test]
fn a_write_with_an_unproven_release_cannot_park_for_a_resend() {
    for (lost, fact, retry, parks) in [
        (true, Fact::None, RetryPolicy::DefinitiveRefusal, false),
        (false, Fact::Accepted, RetryPolicy::DefinitiveRefusal, false),
        (false, Fact::Rejected, RetryPolicy::Never, false),
        (false, Fact::Rejected, RetryPolicy::DefinitiveRefusal, true),
    ] {
        let mut engine = engine();
        engine.venue.command_fact = fact;
        let session = open(&mut engine, false);
        // Issued as during command dispatch, so the classifier knows its intent.
        engine.io.request_contexts.dispatch = Some((7, crate::classify::Intent::Place));
        let write = with(&mut engine, 1., |cx| {
            cx.http_context(
                Scope::Command(7),
                Policy {
                    retry,
                    ..policy(Operation::Mutation)
                },
            )
        })
        .unwrap();
        engine.io.request_contexts.dispatch = None;
        let token = queue(&mut engine, &write, 1.);
        let request = release(&mut engine, token, 2.);
        if lost {
            lose(&mut engine, request, 3.);
        } else {
            respond(&mut engine, request, 3., "{\"code\":401}");
        }
        assert_eq!(
            wait(&mut engine, &session, &write, 4.).is_ok(),
            parks,
            "lost={lost} retry={retry:?}"
        );
    }
}

#[test]
fn a_parked_command_keeps_completion_authority_past_its_deadline() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let drained = owner(&mut engine, Scope::Command(7), 1_000, 1.);
    let expiring = owner(&mut engine, Scope::Command(8), 1_000, 1.);
    wait(&mut engine, &session, &drained, 1.).unwrap();
    wait(&mut engine, &session, &expiring, 1.).unwrap();
    timers(&mut engine);
    let late = drained.deadline() + 1.;
    // Unrelated work prunes expired owners that hold nothing.
    owner(&mut engine, Scope::Background, 20_000, late);
    let ready = with(&mut engine, late, |cx| cx.session_ready(&session));
    let [Waiter::Failed(context, _, error), Waiter::Failed(..)] = ready.as_slice() else {
        panic!("expired owners must fail");
    };
    assert_eq!(
        with(&mut engine, late, |cx| cx
            .fail_context(context, error.clone())),
        Ok(())
    );
    // The expiry path: a parked owner whose timer fires after pruning work.
    let mut engine = self::engine();
    let session = open(&mut engine, true);
    let parked = owner(&mut engine, Scope::Command(9), 1_000, 1.);
    wait(&mut engine, &session, &parked, 1.).unwrap();
    let timer = timers(&mut engine)
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    owner(
        &mut engine,
        Scope::Background,
        20_000,
        parked.deadline() + 1.,
    );
    engine.handle(parked.deadline() + 1., Event::TimerFired { timer });
    assert_eq!(engine.venue.completions, [Ok(())]);
}

#[test]
fn command_entry_cannot_reclaim_a_command_with_a_parked_waiter() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let parked = owner(&mut engine, Scope::Command(7), 1_000, 1.);
    wait(&mut engine, &session, &parked, 1.).unwrap();
    let late = parked.deadline() + 1.;
    assert!(!engine.io.request_contexts.prepare_command_entry(7, late));
    assert!(engine.io.request_contexts.command_live(7, late));
    let ready = with(&mut engine, late, |cx| cx.session_ready(&session));
    let [Waiter::Failed(context, _, error)] = ready.as_slice() else {
        panic!("expired owner must fail");
    };
    with(&mut engine, late, |cx| {
        cx.fail_context(context, error.clone())
    })
    .unwrap();
    assert!(engine.io.request_contexts.prepare_command_entry(7, late));
}

#[test]
fn a_session_write_accepted_through_the_classifier_resolves_without_order_ids() {
    let mut engine = engine();
    engine.venue.classified = true;
    let session = open(&mut engine, false);
    let token = session_write(&mut engine, &session, 1.);
    let request = release(&mut engine, token, 2.);
    respond(&mut engine, request, 3., "{\"listenKey\":\"k\"}");
    assert_eq!(engine.venue.facts, [Ok(())]);
    assert!(
        with(&mut engine, 4., |cx| cx
            .session_context(&session, policy(Operation::Mutation)))
        .is_ok()
    );
}

#[test]
fn a_classified_session_refusal_resolves_the_operation_without_a_resend_grant() {
    for table in [&REFUSED, &REFUSED_RESEND] {
        let mut engine = engine();
        engine.venue.refusal = Some(table);
        let session = open(&mut engine, false);
        let token = session_write(&mut engine, &session, 1.);
        let request = release(&mut engine, token, 2.);
        engine.handle(
            3.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 401,
                    headers: "",
                    body: "{\"code\":-2015}",
                },
            },
        );
        engine.drain_actions().for_each(drop);
        assert_eq!(engine.venue.facts, [Ok(())]);
        assert_eq!(
            engine.venue.grants,
            [false],
            "a session write is never resent"
        );
        assert!(
            with(&mut engine, 4., |cx| cx
                .session_context(&session, policy(Operation::Mutation)))
            .is_ok(),
            "a definitive refusal resolves the operation"
        );
    }
}

#[test]
fn a_held_command_past_its_deadline_cannot_reissue_in_the_old_generation() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let parked = owner(&mut engine, Scope::Command(7), 1_000, 1.);
    wait(&mut engine, &session, &parked, 1.).unwrap();
    assert_eq!(
        with(&mut engine, parked.deadline() + 1., |cx| cx
            .http_context(Scope::Command(7), policy(Operation::Read))),
        Err(Error::State)
    );
}

fn entry_root(engine: &mut Engine<Probe>, command: CommandId) -> crate::request_context::Context {
    let root = owner(engine, Scope::Command(command), 20_000, 1.);
    engine.io.request_contexts.retain_command_entry(&root);
    root
}

#[test]
fn execution_observers_and_session_waiters_exclude_each_other() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let parked = entry_root(&mut engine, 7);
    wait(&mut engine, &session, &parked, 1.).unwrap();
    assert_eq!(
        engine.io.request_contexts.begin_execution(
            9_000,
            &parked,
            policy(Operation::Read),
            crate::engine::Window::UntilTerminal,
            "k",
            2.,
        ),
        Err(Error::State),
        "a parked root cannot become an execution observer"
    );
    let observer = entry_root(&mut engine, 8);
    engine
        .io
        .request_contexts
        .begin_execution(
            9_001,
            &observer,
            policy(Operation::Read),
            crate::engine::Window::UntilTerminal,
            "k",
            2.,
        )
        .unwrap();
    assert_eq!(
        wait(&mut engine, &session, &observer, 3.),
        Err(Error::Revoked),
        "an observer root cannot park behind a session"
    );
}

#[cfg(feature = "http-plans")]
#[test]
fn a_planned_command_is_refused_a_session_wait_instead_of_parking() {
    use crate::request_context::plan::{CallBound, RetryWindow, StepBound};
    use crate::request_context::{PlanPolicy, PreflightPolicy, PreflightPurpose};
    let mut engine = engine();
    let session = open(&mut engine, true);
    let leg = with(&mut engine, 1., |cx| {
        let preflight = cx
            .issue_preflight(
                7,
                PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                },
            )
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps = [StepBound {
            wait_ms: 0,
            calls: &calls,
        }];
        let plan = cx
            .admit_plan(
                &preflight,
                &steps,
                PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        cx.plan_leg_context(&plan, &slot).unwrap()
    });
    assert!(matches!(leg.owner(), Owner::Command { id: 7, .. }));
    // The Plan sequences its own legs: the venue gets its pending back now.
    let refused = with(&mut engine, 1., |cx| {
        cx.session_wait(&session, &leg, Pending(leg.clone()))
    });
    assert!(matches!(refused, Err((Pending(back), Error::State)) if back == leg));
    assert!(engine.io.session_waits.entries.is_empty(), "nothing parked");
    assert!(
        with(&mut engine, 2., |cx| cx.session_ready(&session)).is_empty(),
        "readiness has no planned leg to re-dispatch"
    );
}

fn prepared_intent(
    engine: &mut Engine<Probe>,
    context: &crate::request_context::Context,
) -> crate::request_context::RoutingIntent {
    with(engine, 1., |cx| {
        cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/login".into(),
            Some("{\"apiKey\":\"body-key\"}".into()),
            Pending(context.clone()),
        )
    })
    .unwrap();
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { intent, .. } => Some(intent),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared login")
}

fn granted() -> Option<crate::request_context::PrivateEgress> {
    let slots = crate::credential_selection::Slots::new(Some(()), None, Default::default());
    let (selection, _) = slots
        .select(crate::credential_selection::CredentialRole::SessionAuth)
        .unwrap();
    selection.session_egress()
}

fn open_private_with(engine: &mut Engine<Probe>, repeat_safe: bool) -> Session {
    with(engine, 0., |cx| {
        cx.open_session(SessionPolicy {
            credential: None,
            repeat_safe,
            private_egress: granted(),
        })
    })
    .unwrap()
}

fn open_private(engine: &mut Engine<Probe>) -> Session {
    open_private_with(engine, true)
}

#[test]
fn only_a_primary_session_auth_selection_grants_private_egress() {
    use crate::credential_selection::{CredentialRole::*, PrimaryReason, Slots};
    assert!(granted().is_some());
    let both = Slots::new(Some(()), Some(()), Default::default());
    for role in [
        Read { scope: None },
        PrimaryRead {
            reason: PrimaryReason::Catalog,
            scope: None,
        },
        Mutation,
    ] {
        assert_eq!(both.select(role).unwrap().0.session_egress(), None);
    }
    assert_eq!(
        both.select(SessionAuth).unwrap().0.slot(),
        CredentialSlot::Primary,
        "session egress never routes through the read key's proxy"
    );
}

#[test]
fn a_keyless_login_carrying_credentials_in_its_body_must_not_route_public() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    let login = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Mutation))
    })
    .unwrap();
    assert_eq!((login.credential(), login.route()), (None, None));
    assert_eq!(
        prepared_intent(&mut engine, &login),
        crate::request_context::RoutingIntent::Primary,
        "the body carries the API key; Public/CORS egress would leak it"
    );
    let leg = with(&mut engine, 2., |cx| {
        cx.http_leg_context(&login, policy(Operation::Mutation))
    })
    .unwrap();
    assert_eq!(
        leg.egress(),
        login.egress(),
        "legs inherit the session egress"
    );
}

#[test]
fn a_keyless_session_without_private_egress_still_routes_public() {
    let mut engine = engine();
    let session = open(&mut engine, true);
    let bullet = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Read))
    })
    .unwrap();
    assert_eq!(
        prepared_intent(&mut engine, &bullet),
        crate::request_context::RoutingIntent::Public,
        "egress is declared, never inferred from method, URL or body"
    );
    assert_eq!(
        with(&mut engine, 1., |cx| cx.session_scoped_context(
            &session,
            Scope::Command(7),
            policy(Operation::Read)
        )),
        Err(Error::Invalid),
        "no private egress to scope"
    );
}

#[test]
fn work_carrying_a_session_token_rides_its_egress_without_gaining_authority() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    for (scope, operation) in [
        (Scope::Command(7), Operation::Mutation),
        (Scope::Command(8), Operation::Read),
        (Scope::Background, Operation::Read),
    ] {
        let context = with(&mut engine, 1., |cx| {
            cx.session_scoped_context(&session, scope, policy(operation))
        })
        .unwrap();
        assert_ne!(context.owner(), session.owner(), "routing, not ownership");
        assert_eq!(
            prepared_intent(&mut engine, &context),
            crate::request_context::RoutingIntent::Primary
        );
    }
    assert_eq!(
        with(&mut engine, 2., |cx| cx.session_scoped_context(
            &session,
            Scope::Background,
            policy(Operation::Mutation)
        )),
        Err(Error::Invalid),
        "session egress cannot authorize a write the scope does not own"
    );
    let signed = Policy {
        credential: Some(Credential {
            slot: CredentialSlot::Primary,
            reference: crate::signing::KeyRef("primary".into()),
        }),
        ..policy(Operation::Read)
    };
    assert_eq!(
        with(&mut engine, 2., |cx| cx.session_scoped_context(
            &session,
            Scope::Command(9),
            signed.clone()
        )),
        Err(Error::Invalid),
        "signed work keeps its credential's egress"
    );
    assert_eq!(
        with(&mut engine, 2., |cx| cx.open_session(SessionPolicy {
            credential: signed.credential.clone(),
            repeat_safe: true,
            private_egress: granted(),
        })),
        Err(Error::Invalid),
        "a signed session declares no unsigned egress"
    );
}

#[test]
fn forged_and_foreign_session_egress_is_refused() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    let Owner::Session { generation } = session.owner() else {
        unreachable!()
    };
    let forged = crate::request_context::Egress::Session(
        crate::request_context::SessionEgress::new(generation),
    );
    for operation in [Operation::Read, Operation::Mutation] {
        assert_eq!(
            with(&mut engine, 1., |cx| cx.http_context_with_egress(
                Scope::Command(7),
                policy(operation),
                forged
            )),
            Err(Error::Invalid),
            "only the session core mints session egress"
        );
    }
    let mut other = self::engine();
    let foreign = open_private(&mut other);
    assert_eq!(
        with(&mut engine, 1., |cx| cx.session_scoped_context(
            &foreign,
            Scope::Command(7),
            policy(Operation::Read)
        )),
        Err(Error::Revoked)
    );
}

#[test]
fn retirement_fences_sends_on_its_egress_but_not_sent_write_outcomes() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    let scoped = |engine: &mut Engine<Probe>, id| {
        with(engine, 1., |cx| {
            cx.session_scoped_context(&session, Scope::Command(id), policy(Operation::Mutation))
        })
        .unwrap()
    };
    let sent = scoped(&mut engine, 7);
    let sent = {
        let token = queue(&mut engine, &sent, 1.);
        release(&mut engine, token, 2.)
    };
    let issued = scoped(&mut engine, 8);
    let prepared = scoped(&mut engine, 9);
    let prepared = queue(&mut engine, &prepared, 2.);
    with(&mut engine, 3., |cx| cx.retire_session(&session)).unwrap();

    // Stale generation: neither new work nor a leg may use the old egress.
    assert_eq!(
        with(&mut engine, 3., |cx| cx.session_scoped_context(
            &session,
            Scope::Command(10),
            policy(Operation::Read)
        )),
        Err(Error::Revoked)
    );
    assert_eq!(
        with(&mut engine, 3., |cx| cx
            .http_leg_context(&issued, policy(Operation::Mutation))),
        Err(Error::Revoked)
    );
    let queued = with(&mut engine, 3., |cx| {
        cx.request_managed(
            issued.clone(),
            Method::Post,
            "https://venue.test/order".into(),
            None,
            Pending(issued.clone()),
        )
    });
    assert!(matches!(queued, Err((_, Error::Revoked))));
    engine.http_prepared(
        4.,
        prepared,
        Prepared {
            id: prepared.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(engine.drain_actions().all(|action| !matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. }))));
    assert_eq!(
        engine.venue.preparation_failures.len(),
        1,
        "the fenced command learns its write was never sent"
    );

    respond(&mut engine, sent, 5., "{\"orderId\":1}");
    assert!(
        engine
            .venue
            .responses
            .iter()
            .any(|(owner, _)| matches!(owner, Owner::Command { id: 7, .. })),
        "a write sent before retirement still delivers its outcome"
    );
}

#[test]
fn session_egress_exempts_no_write_from_its_journal_or_gate() {
    let mut engine = engine();
    let session = open_private_with(&mut engine, false);
    // A command write riding session egress keeps the full command journal.
    let command = with(&mut engine, 1., |cx| {
        cx.session_scoped_context(&session, Scope::Command(7), policy(Operation::Mutation))
    })
    .unwrap();
    let revision = engine.io.http_write_revision;
    let write = {
        let token = queue(&mut engine, &command, 1.);
        release(&mut engine, token, 2.)
    };
    assert!(engine.io.http_responses.contains(write));
    assert_ne!(engine.io.http_write_revision, revision);
    // A session write under its egress keeps the one-unresolved-write gate.
    let login = {
        let token = session_write(&mut engine, &session, 3.);
        release(&mut engine, token, 3.)
    };
    respond(&mut engine, login, 4., "{}");
    assert_eq!(
        with(&mut engine, 5., |cx| cx
            .session_context(&session, policy(Operation::Mutation))),
        Err(Error::State)
    );
}

#[test]
fn retirement_fences_claims_and_retries_but_not_streams_or_facts() {
    let mut engine = engine();
    engine.venue.command_write_fact = true;
    let session = open_private(&mut engine);
    engine.io.request_contexts.dispatch = Some((7, crate::classify::Intent::Setting));
    let write = with(&mut engine, 1., |cx| {
        cx.session_scoped_context(&session, Scope::Command(7), policy(Operation::Mutation))
    })
    .unwrap();
    engine.io.request_contexts.dispatch = None;
    let sent = {
        let token = queue(&mut engine, &write, 1.);
        release(&mut engine, token, 1.)
    };
    let retryable = Policy {
        retry: RetryPolicy::ReadFailure,
        ..policy(Operation::Read)
    };
    let read = with(&mut engine, 1., |cx| {
        cx.session_scoped_context(&session, Scope::Command(8), retryable)
    })
    .unwrap();
    let streamed = queue(&mut engine, &read, 1.);
    with(&mut engine, 1., |cx| {
        cx.stream_response(streamed.attempt().request, crate::json::ArrayAt::Root)
    });
    let streamed = release(&mut engine, streamed, 1.);
    let bound = with(&mut engine, 1., |cx| {
        cx.session_scoped_context(&session, Scope::Command(9), policy(Operation::Read))
    })
    .unwrap();
    let bound = queue(&mut engine, &bound, 1.);
    engine.http_prepared(
        1.,
        bound,
        Prepared {
            id: bound.attempt().request.0,
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
    engine.handle(
        2.,
        Event::HttpStart {
            request: streamed,
            status: 200,
            headers: "",
        },
    );
    with(&mut engine, 3., |cx| cx.retire_session(&session)).unwrap();

    assert!(
        engine.claim_http(3., permit).is_err(),
        "a bound send is fenced"
    );
    assert_eq!(
        with(&mut engine, 3., |cx| cx.http_retry_context(
            &read,
            crate::request_context::RetryEvidence::ReadFailure
        )),
        Err(Error::Revoked)
    );
    for data in [&b"[1,"[..], &b"2]"[..]] {
        engine.handle(
            4.,
            Event::HttpChunk {
                request: streamed,
                data,
            },
        );
    }
    engine.handle(4., Event::HttpEnd { request: streamed });
    assert_eq!(
        engine.venue.items, 2,
        "a stream already sent keeps delivering"
    );
    respond(&mut engine, sent, 5., "{}");
    assert_eq!(
        engine.venue.facts,
        [Ok(())],
        "the public write-fact path records a sent write after retirement"
    );
}

#[test]
fn session_egress_never_crosses_sessions_or_engines() {
    let mut engine = engine();
    let first = open_private(&mut engine);
    let second = open_private(&mut engine);
    let generation = |session: &Session| match session.owner() {
        Owner::Session { generation } => generation,
        _ => unreachable!(),
    };
    let login = with(&mut engine, 1., |cx| {
        cx.session_context(&first, policy(Operation::Mutation))
    })
    .unwrap();
    assert_eq!(
        engine.io.request_contexts.issue_session(
            1 << 40,
            generation(&second),
            policy(Operation::Mutation),
            1.,
            Some(&login),
        ),
        Err(Error::Invalid),
        "a session leg cannot move to another session"
    );
    let scoped = with(&mut engine, 1., |cx| {
        cx.session_scoped_context(&first, Scope::Command(7), policy(Operation::Read))
    })
    .unwrap();
    let other = engine
        .io
        .request_contexts
        .sessions
        .egress(generation(&second));
    assert!(other.is_some() && other != scoped.egress());
    assert_eq!(
        with(&mut engine, 1., |cx| cx.make_http_context(
            Scope::Command(7),
            policy(Operation::Read),
            Some(&scoped),
            other
        )),
        Err(Error::Invalid),
        "a child cannot swap its parent's session egress"
    );

    let mut foreign = self::engine();
    let theirs = open_private(&mut foreign);
    let foreign_scoped = with(&mut foreign, 1., |cx| {
        cx.session_scoped_context(&theirs, Scope::Command(7), policy(Operation::Read))
    })
    .unwrap();
    assert_eq!(
        with(&mut engine, 1., |cx| cx
            .http_leg_context(&foreign_scoped, policy(Operation::Read))),
        Err(Error::Revoked),
        "another engine's egress-bearing context is not ours"
    );

    // Retiring one session fences only its own egress.
    with(&mut engine, 2., |cx| cx.retire_session(&first)).unwrap();
    assert_eq!(
        with(&mut engine, 2., |cx| cx
            .http_leg_context(&scoped, policy(Operation::Read))),
        Err(Error::Revoked)
    );
    assert!(
        with(&mut engine, 2., |cx| cx.session_scoped_context(
            &second,
            Scope::Command(8),
            policy(Operation::Read)
        ))
        .is_ok()
    );
}

#[test]
fn a_session_callback_settles_the_commands_waiting_on_it() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    let waiter = with(&mut engine, 1., |cx| {
        cx.http_context(Scope::Command(7), policy(Operation::Read))
    })
    .unwrap();
    engine.venue.settle_in_response = Some((waiter, 9));
    let login = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 1.)
    };
    engine.handle(
        2.,
        Event::HttpResponse {
            request: login,
            response: Response {
                status: 401,
                headers: "",
                body: "{}",
            },
        },
    );
    let failed = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, .. }) => Some(command),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        engine.venue.facts,
        [Ok(())],
        "a session delivery is no command's authority, so it blocks none"
    );
    assert_eq!(failed, [7, 9], "managed and legacy waiters both settle");
}

#[test]
fn a_session_callback_settles_like_a_delivery_less_callback() {
    let mut engine = engine();
    let session = open_private(&mut engine);
    let write = with(&mut engine, 1., |cx| {
        cx.http_context(Scope::Command(11), policy(Operation::Mutation))
    })
    .unwrap();
    {
        let token = queue(&mut engine, &write, 1.);
        release(&mut engine, token, 1.);
    }
    engine.venue.refuse_in_response = Some(11);
    let login = {
        let token = session_write(&mut engine, &session, 1.);
        release(&mut engine, token, 1.)
    };
    respond(&mut engine, login, 2., "{}");
    assert_eq!(
        engine.venue.facts,
        [Err(Error::Invalid)],
        "a session Context is never command authority"
    );
    assert!(
        engine.io.request_contexts.commands.contains_key(&11),
        "a managed write is never sealed numerically"
    );
}

/// Session egress policy (#1833): a session opened without private egress
/// issues roots with no egress, so routing derives from the credential; its
/// legs inherit the root's egress and cannot widen it.
#[test]
fn session_roots_declare_no_egress_and_their_legs_inherit_it() {
    use crate::request_context::RoutingIntent;
    let mut engine = engine();
    let session = open(&mut engine, true);
    let root = with(&mut engine, 1., |cx| {
        cx.session_context(&session, policy(Operation::Read))
    })
    .unwrap();
    assert_eq!(root.egress(), None);
    assert_eq!(root.routing_intent(), RoutingIntent::Public);
    let leg = with(&mut engine, 2., |cx| {
        cx.http_leg_context(&root, policy(Operation::Read))
    })
    .unwrap();
    assert!(matches!(leg.owner(), Owner::Session { .. }));
    assert_eq!(leg.egress(), root.egress());
    assert_eq!(leg.routing_intent(), RoutingIntent::Public);
}
