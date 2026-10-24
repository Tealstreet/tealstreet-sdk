//! N7 (RUST_PLAN_SESSION_WAIT): a retained Preflight read parked on a login
//! Session before issue, resumed by `session_ready` under its own generation.
use super::*;
use crate::book::Book;
use crate::request_context::plan::RetryWindow;
use crate::request_context::{
    Claim, HttpAction, PreflightPolicy, PreflightPurpose, Preparation, Prepared, RetryPolicy,
    SessionPolicy, Waiter,
};
use crate::{
    Action, Command, Engine, ErrorKind, Event, Method, Notice, Response, SocketId, TimerId, Venue,
};

const COMMAND: u32 = 7;

#[derive(Default)]
struct Reads {
    capped: bool,
    preflight: Option<Preflight>,
    read: Option<Context>,
    park_on: Option<Session>,
    park_result: Option<Result<(), Error>>,
    /// Read issue, completion and a second park, attempted while parked.
    parked_refusals: Vec<Error>,
    /// Refusals tried in genuine entry custody before the real park.
    stale: Option<Session>,
    entry_refusals: Vec<(Error, bool)>,
    park_retry_on: Option<Session>,
    retry_parks: Vec<Result<(), Error>>,
    repark_on: Option<Session>,
    repark_retry: bool,
    /// (pending, error, ran in this Preflight's own ready custody)
    ended: Vec<(u32, VenueError, bool)>,
    /// (pending, error, Preflight still live)
    expired: Vec<(u32, VenueError, bool)>,
    park_in_ready: Option<Session>,
    ready_park_result: Option<Result<(), Error>>,
}

fn preflight_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}
fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}
fn terms() -> CallTerms {
    CallTerms {
        wait_ms: 0,
        timeout_ms: 20_000,
        attempts: 2,
        retry_window: RetryWindow::FreshAttempt,
    }
}
fn queue(cx: &mut Cx<'_, u32>, read: Context) -> Result<RequestId, (u32, Error)> {
    cx.request_managed(
        read,
        None,
        Method::Get,
        "https://venue.test/read".into(),
        None,
        0,
    )
}

impl Reads {
    fn live(&self, cx: &Cx<'_, u32>) -> bool {
        self.preflight
            .as_ref()
            .is_some_and(|handle| cx.io.request_contexts.preflight_state(handle).is_ok())
    }
    fn entry_refusals(&mut self, cx: &mut Cx<'_, u32>, handle: &Preflight, live: &Session) {
        let stale = self.stale.clone().unwrap();
        let mutation = Policy {
            operation: Operation::Mutation,
            ..read_policy()
        };
        let mismatched = CallTerms {
            timeout_ms: 10_000,
            ..terms()
        };
        let attempts: [(&Session, Policy, CallTerms); 3] = [
            (&stale, read_policy(), terms()),
            (live, mutation, terms()),
            (live, read_policy(), mismatched),
        ];
        for (session, policy, terms) in attempts {
            let before = (cx.io.next_id, cx.io.timers.len(), cx.io.actions.len());
            let error = cx
                .preflight_session_wait(handle, policy, terms, session, 9)
                .map_err(|(_, error)| error)
                .unwrap_err();
            let after = (cx.io.next_id, cx.io.timers.len(), cx.io.actions.len());
            self.entry_refusals.push((error, before == after));
        }
    }
}

impl Venue for Reads {
    type Pending = u32;
    fn start(&mut self, _: &mut Cx<'_, u32>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, u32>, _: Command) {
        let handle = if self.capped {
            cx.begin_preflight_with_command_wait(preflight_policy())
                .unwrap()
        } else {
            cx.begin_preflight(preflight_policy()).unwrap()
        };
        self.preflight = Some(handle.clone());
        if self.park_in_ready.is_some() {
            cx.park_preflight(&handle).unwrap();
            cx.request_preflight_ready(&handle).unwrap();
            return;
        }
        if let Some(session) = self.park_on.clone() {
            if self.stale.is_some() {
                self.entry_refusals(cx, &handle, &session);
            }
            let parked = cx
                .preflight_session_wait(&handle, read_policy(), terms(), &session, 1)
                .map_err(|(_, error)| error);
            self.park_result = Some(parked);
            if parked.is_ok() {
                self.parked_refusals = [
                    cx.preflight_read_context(&handle, read_policy(), terms())
                        .err(),
                    cx.done_preflight(&handle, "early".into()).err(),
                    cx.preflight_session_wait(&handle, read_policy(), terms(), &session, 2)
                        .err()
                        .map(|(_, error)| error),
                ]
                .into_iter()
                .flatten()
                .collect();
            }
            return;
        }
        let read = cx
            .preflight_read_context(&handle, read_policy(), terms())
            .unwrap();
        queue(cx, read.clone()).unwrap();
        self.read = Some(read);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, u32>, _: u32, _: Response<'_>) {
        let handle = self.preflight.clone().unwrap();
        let read = self.read.clone().unwrap();
        cx.finish_preflight_call(&handle, &read).unwrap();
        cx.done_preflight(&handle, "read".into()).unwrap();
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, u32>, _: u32, _: &str) {
        if let Some(session) = self.park_retry_on.clone() {
            let handle = self.preflight.clone().unwrap();
            let read = self.read.clone().unwrap();
            self.retry_parks.push(
                cx.preflight_retry_session_wait(
                    &handle,
                    &read,
                    RetryEvidence::ReadFailure,
                    &session,
                    3,
                )
                .map_err(|(_, error)| error),
            );
        }
    }
    fn on_preflight_session_wait_ended(
        &mut self,
        cx: &mut Cx<'_, u32>,
        handle: &Preflight,
        pending: u32,
        error: VenueError,
    ) {
        let own = cx
            .io
            .http_preflight_ready
            .as_ref()
            .is_some_and(|ready| ready.owner() == handle.owner());
        self.ended.push((pending, error, own));
        if let Some(session) = self.repark_on.take() {
            let parked = if self.repark_retry {
                let read = self.read.clone().unwrap();
                cx.preflight_retry_session_wait(
                    handle,
                    &read,
                    RetryEvidence::ReadFailure,
                    &session,
                    pending,
                )
            } else {
                cx.preflight_session_wait(handle, read_policy(), terms(), &session, pending)
            };
            parked.map_err(|(_, error)| error).unwrap();
        }
    }
    fn on_session_wait_expired(&mut self, cx: &mut Cx<'_, u32>, pending: u32, error: VenueError) {
        let live = self.live(cx);
        self.expired.push((pending, error, live));
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, u32>, handle: &Preflight) {
        if let Some(session) = self.park_in_ready.take() {
            self.ready_park_result = Some(
                cx.preflight_session_wait(handle, read_policy(), terms(), &session, 4)
                    .map_err(|(_, error)| error),
            );
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, u32>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, u32>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(venue: Reads) -> Engine<Reads> {
    let mut e = crate::test_support::seeded_engine(venue);
    e.start(0.);
    e
}
fn cx(e: &mut Engine<Reads>, now: f64) -> Cx<'_, u32> {
    Cx {
        now,
        store: &mut e.store,
        io: &mut e.io,
    }
}
fn open(e: &mut Engine<Reads>, now: f64) -> Session {
    cx(e, now)
        .open_session(SessionPolicy {
            credential: None,
            repeat_safe: true,
            private_egress: None,
        })
        .unwrap()
}
fn command(e: &mut Engine<Reads>, now: f64, wait_ms: u32) -> Vec<Action> {
    e.handle_command(
        now,
        Command::FetchCandles {
            command: COMMAND,
            symbol: "BTCUSD".into(),
            interval: crate::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        },
        Some(wait_ms),
    )
    .unwrap();
    e.drain_actions().collect()
}
fn flush(e: &mut Engine<Reads>, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::TimerFired {
            timer: TimerId(u64::MAX - 1),
        },
    );
    e.drain_actions().collect()
}
fn prepares(actions: &[Action]) -> Vec<Preparation> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn terminals(actions: &[Action]) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == COMMAND => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == COMMAND => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn release(e: &mut Engine<Reads>, token: Preparation, now: f64) {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("read binding");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
}
/// Park on `session` in genuine entry custody; nothing is issued.
fn parked(venue: Reads, wait_ms: u32) -> (Engine<Reads>, Session, Vec<Action>) {
    let mut e = engine(venue);
    let session = open(&mut e, 0.);
    e.venue.park_on = Some(session.clone());
    let actions = command(&mut e, 0., wait_ms);
    assert_eq!(
        e.venue.park_result,
        Some(Ok(())),
        "the read parks in entry custody"
    );
    assert!(
        prepares(&actions).is_empty(),
        "nothing is issued while parked"
    );
    assert!(terminals(&actions).is_empty());
    (e, session, actions)
}

/// N7: a parked read spends nothing and refuses every other progress; the
/// Session's drain issues a fresh read under the same Preflight generation,
/// queued in that drain, with one HTTP, and its DATA completes the command.
#[test]
fn a_parked_preflight_read_resumes_as_a_fresh_read_under_its_generation() {
    let (mut e, session, _) = parked(Reads::default(), 60_000);
    assert_eq!(e.venue.parked_refusals, [Error::Revoked; 3]);
    let handle = e.venue.preflight.clone().unwrap();
    let mut cx = cx(&mut e, 30_000.);
    let waiters = cx.session_ready(&session);
    let Some(Waiter::Ready(read, pending)) = waiters.into_iter().next() else {
        panic!("a live Preflight resumes Ready");
    };
    assert_eq!(pending, 1);
    assert_eq!(
        read.owner(),
        handle.owner(),
        "the same Preflight generation"
    );
    assert!(read.deadline() > 30_000., "the window starts at resume");
    queue(&mut cx, read.clone()).unwrap();
    e.venue.read = Some(read);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let issued = prepares(&actions);
    assert_eq!(issued.len(), 1, "exactly one HTTP");
    release(&mut e, issued[0], 30_001.);
    e.handle(
        30_002.,
        Event::HttpResponse {
            request: issued[0].attempt().request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    let terminals = terminals(&e.drain_actions().collect::<Vec<_>>());
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].as_deref().ok(), Some("read"));
}

/// N7 retry twin: a failed read's declared retry parks in the failure
/// callback, the Preflight stays alive, and the resume selects exactly one
/// more attempt within the declared budget.
#[test]
fn a_failed_preflight_reads_retry_parks_and_resumes_as_one_more_attempt() {
    let mut e = engine(Reads::default());
    let session = open(&mut e, 0.);
    e.venue.park_retry_on = Some(session.clone());
    let actions = command(&mut e, 0., 60_000);
    eprintln!("{actions:?}");
    let first = prepares(&actions)[0];
    release(&mut e, first, 0.);
    e.handle(
        1.,
        Event::HttpFailed {
            request: first.attempt().request,
            error: "login expired",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        e.venue.retry_parks,
        [Ok(())],
        "the retry parks in the failure callback"
    );
    assert!(terminals(&actions).is_empty(), "the Preflight stays alive");
    assert!(
        prepares(&actions).is_empty(),
        "nothing is selected while parked"
    );
    let mut cx = cx(&mut e, 2.);
    let Some(Waiter::Ready(read, pending)) = cx.session_ready(&session).into_iter().next() else {
        panic!("the parked retry resumes");
    };
    assert_eq!(
        (read, pending),
        (e.venue.read.clone().unwrap(), 3),
        "the same read resumes"
    );
    let mut cx = Cx {
        now: 2.,
        store: &mut e.store,
        io: &mut e.io,
    };
    queue(&mut cx, e.venue.read.clone().unwrap()).unwrap();
    let issued = prepares(&e.drain_actions().collect::<Vec<_>>());
    assert_eq!(issued.len(), 1, "exactly one more attempt");
    release(&mut e, issued[0], 3.);
    e.handle(
        4.,
        Event::HttpFailed {
            request: issued[0].attempt().request,
            error: "again",
        },
    );
    assert_eq!(
        e.venue.retry_parks[1],
        Err(Error::RetryForbidden),
        "the declared budget is spent"
    );
}

/// N7 bound: a retained Preflight's read window is unbounded, so its wait ends
/// at the caller's receipt cap. One Timeout terminal comes first, then the
/// pending returns once, and nothing is sent.
#[test]
fn a_parked_preflight_read_expires_at_the_receipt_cap() {
    let (mut e, _, admitted) = parked(
        Reads {
            capped: true,
            ..Default::default()
        },
        1_000,
    );
    let handle = e.venue.preflight.clone().unwrap();
    let cap =
        e.io.request_contexts
            .preflight_receipt_cap(&handle)
            .unwrap()
            .expect("caller receipt cap")
            .1;
    let timer = admitted
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == cap => Some(*timer),
            _ => None,
        })
        .expect("the parked wait's timer at the receipt cap");
    e.handle(cap, Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(prepares(&actions).is_empty());
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one Preflight terminal");
    assert_eq!(
        terminals[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    assert_eq!(e.venue.expired.len(), 1, "the pending returns once");
    let (pending, error, live) = &e.venue.expired[0];
    assert_eq!((*pending, *live), (1, false), "the terminal comes first");
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Timeout), Some(crate::Outcome::NotSent))
    );
    assert!(e.venue.ended.is_empty());
}

/// N7 retire: the wait ends after the retiring callback, in this Preflight's
/// own ready custody, where the venue re-parks on its replacement Session.
#[test]
fn a_retired_session_ends_a_preflight_wait_in_ready_custody_for_a_repark() {
    let (mut e, old, _) = parked(Reads::default(), 60_000);
    let replacement = open(&mut e, 1.);
    e.venue.repark_on = Some(replacement.clone());
    let returned = cx(&mut e, 1.).retire_session(&old).unwrap();
    assert!(
        returned.is_empty(),
        "a parked read is not a plain owner waiter"
    );
    let actions = flush(&mut e, 2.);
    assert!(prepares(&actions).is_empty() && terminals(&actions).is_empty());
    assert_eq!(e.venue.ended.len(), 1, "ended once");
    let (pending, error, own) = &e.venue.ended[0];
    assert_eq!(
        (*pending, *own),
        (1, true),
        "in the Preflight's own ready custody"
    );
    assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
    assert!(e.venue.expired.is_empty());
    let handle = e.venue.preflight.clone().unwrap();
    let Some(Waiter::Ready(read, _)) = cx(&mut e, 3.)
        .session_ready(&replacement)
        .into_iter()
        .next()
    else {
        panic!("the re-parked read resumes on the new Session");
    };
    assert_eq!(read.owner(), handle.owner());
}

/// N7 retire, no progress: an ended hook that neither re-parks nor finishes
/// fails the Preflight once with the ended error; nothing is sent.
#[test]
fn an_ended_preflight_wait_without_progress_fails_once() {
    let (mut e, old, _) = parked(Reads::default(), 60_000);
    let _ = cx(&mut e, 1.).retire_session(&old).unwrap();
    let actions = flush(&mut e, 2.);
    assert_eq!(e.venue.ended.len(), 1);
    assert!(prepares(&actions).is_empty());
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one terminal");
    assert_eq!(
        terminals[0].as_ref().unwrap_err().message,
        "Session retired"
    );
}

/// N7 sealed: a Preflight failed while its read is parked returns the
/// pending once with no custody, and its Session's drain finds nothing.
#[test]
fn a_sealed_preflight_returns_its_parked_pending_once() {
    let (mut e, session, _) = parked(Reads::default(), 60_000);
    let handle = e.venue.preflight.clone().unwrap();
    cx(&mut e, 1.)
        .fail_preflight(&handle, "venue gave up")
        .unwrap();
    let mut actions = e.drain_actions().collect::<Vec<_>>();
    actions.extend(flush(&mut e, 2.));
    assert!(prepares(&actions).is_empty());
    assert_eq!(terminals(&actions).len(), 1);
    assert!(e.venue.ended.is_empty());
    assert_eq!(e.venue.expired.len(), 1);
    let (pending, error, live) = &e.venue.expired[0];
    assert_eq!((*pending, *live), (1, false));
    assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
    assert!(cx(&mut e, 3.).session_ready(&session).is_empty());
}

/// N7 custody: a stale Session, a non-Read policy or mismatched terms refuse
/// in entry custody before any effect; outside custody the park is Revoked.
#[test]
fn preflight_session_wait_refusals_spend_nothing() {
    let mut e = engine(Reads::default());
    let stale = open(&mut e, 0.);
    let _ = cx(&mut e, 0.).retire_session(&stale).unwrap();
    e.venue.stale = Some(stale);
    let session = open(&mut e, 0.);
    e.venue.park_on = Some(session.clone());
    command(&mut e, 0., 60_000);
    assert_eq!(
        e.venue.entry_refusals,
        [
            (Error::Revoked, true),
            (Error::Invalid, true),
            (Error::Invalid, true)
        ]
    );
    assert_eq!(e.venue.park_result, Some(Ok(())));
    let handle = e.venue.preflight.clone().unwrap();
    let mut cx = cx(&mut e, 1.);
    let before = (cx.io.next_id, cx.io.timers.len());
    let outside = cx.preflight_session_wait(&handle, read_policy(), terms(), &session, 5);
    assert_eq!(outside.map_err(|(_, error)| error), Err(Error::Revoked));
    assert_eq!((cx.io.next_id, cx.io.timers.len()), before);
}

/// N7 with #1896: parking inside the Preflight's own ready callback is
/// progress, so readiness does not fail it, and the drain resumes it.
#[test]
fn parking_in_preflight_ready_custody_is_progress() {
    let mut e = engine(Reads::default());
    let session = open(&mut e, 0.);
    e.venue.park_in_ready = Some(session.clone());
    let mut actions = command(&mut e, 0., 60_000);
    actions.extend(flush(&mut e, 1.));
    assert_eq!(e.venue.ready_park_result, Some(Ok(())));
    assert!(
        terminals(&actions).is_empty(),
        "a parked read is not a stalled activation"
    );
    assert!(prepares(&actions).is_empty());
    let Some(Waiter::Ready(_, pending)) = cx(&mut e, 2.).session_ready(&session).into_iter().next()
    else {
        panic!("the read parked in ready custody resumes");
    };
    assert_eq!(pending, 4);
}

/// N7 cap (codex-zs5e83 review): a read resumed before the caller's receipt
/// cap keeps that cap. Its window ends at the cap, so a claim after it cannot
/// release, and the original receipt still bounds the command.
#[test]
fn a_resumed_capped_read_cannot_release_after_the_receipt_cap() {
    let (mut e, session, _) = parked(
        Reads {
            capped: true,
            ..Default::default()
        },
        1_000,
    );
    let handle = e.venue.preflight.clone().unwrap();
    let cap =
        e.io.request_contexts
            .preflight_receipt_cap(&handle)
            .unwrap()
            .unwrap()
            .1;
    let mut cx = cx(&mut e, cap - 1.);
    let Some(Waiter::Ready(read, _)) = cx.session_ready(&session).into_iter().next() else {
        panic!("resume while the original cap is live");
    };
    assert!(
        read.deadline() <= cap,
        "the resumed read keeps the receipt cap"
    );
    queue(&mut cx, read).unwrap();
    let issued = prepares(&e.drain_actions().collect::<Vec<_>>());
    assert_eq!(issued.len(), 1);
    e.http_prepared(
        cap + 1.,
        issued[0],
        Prepared {
            id: issued[0].attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e.drain_actions().find_map(|a| match a {
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind { permit, .. } => Some(permit),
            _ => None,
        },
        _ => None,
    });
    if let Some(permit) = permit {
        assert!(
            !matches!(e.claim_http(cap + 1., permit), Ok(Claim::Released(_))),
            "no release after the original caller receipt"
        );
    }
}

/// N7 cap, without a park: a capped Preflight's ordinary read is bounded by
/// the same receipt. An uncapped Preflight keeps its declared window.
#[test]
fn a_capped_preflight_read_window_ends_at_the_receipt_cap() {
    for capped in [true, false] {
        let mut e = engine(Reads {
            capped,
            ..Default::default()
        });
        command(&mut e, 0., 1_000);
        let read = e.venue.read.clone().unwrap();
        if capped {
            let handle = e.venue.preflight.clone().unwrap();
            let cap =
                e.io.request_contexts
                    .preflight_receipt_cap(&handle)
                    .unwrap()
                    .unwrap()
                    .1;
            assert!(read.deadline() <= cap);
        } else {
            assert!(read.deadline() > 1_000., "the declared 2 x 20 s window");
        }
    }
}
