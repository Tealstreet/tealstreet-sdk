// The probe pending is a whole Context, returned by refused managed requests.
#![allow(clippy::result_large_err)]
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
    fact_on_failure: Fact,
    classified: bool,
    refusal: Option<&'static crate::classify::CodeTable>,
    finish_first: bool,
    retire_in_response: Option<Session>,
    command_fact: bool,
    items: usize,
    facts: Vec<Result<(), Error>>,
    responses: Vec<(Owner, String)>,
    failures: Vec<Owner>,
    preparation_failures: Vec<(Error, Option<crate::Outcome>)>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            fact: Fact::None,
            fact_on_failure: Fact::None,
            classified: false,
            refusal: None,
            finish_first: false,
            retire_in_response: None,
            command_fact: false,
            items: 0,
            facts: vec![],
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
        if let Some(session) = self.retire_in_response.take() {
            cx.retire_session(&session).unwrap();
            return;
        }
        let session = matches!(pending.0.owner(), Owner::Session { .. });
        if !session && self.command_fact && pending.0.operation() == Operation::Mutation {
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
            self.facts.push(
                cx.http_write_failure(&pending.0, table, Some(&native), "refused", None)
                    .map(drop),
            );
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
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

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
        assert!(
            with(&mut engine, 4., |cx| cx
                .session_context(&session, policy(Operation::Mutation)))
            .is_ok(),
            "a definitive refusal resolves the operation"
        );
    }
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
    engine.venue.command_fact = true;
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
