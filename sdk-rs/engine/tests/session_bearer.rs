//! Runtime REDs for `docs/plans/RUST_SESSION_BEARER_HOOK.md`.
//! The probe is an observation adapter: it emulates today's consumer, which
//! can only attach its cached Bearer as a static header at issue.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared,
    RetryEvidence, RetryPolicy, RoutingIntent, Scope, Session, SessionPolicy,
};
use safe_cex_engine::{
    AccountLookup, Action, Capabilities, Command, Cx, Engine, Event, HttpRequest, Method, Outcome,
    Response, SocketId, TimerId, Venue, VenueError,
};
use std::cell::RefCell;
use std::rc::Rc;

const NOW: f64 = 1_000.25;

/// The venue-held cache: `None` is the venue's NotReady.
struct Shared {
    token: Option<&'static str>,
    revision: u64,
    session: Option<Session>,
    retire_timer: Option<TimerId>,
    /// Token the venue caches for the generation it opens on retire.
    next_generation_token: Option<&'static str>,
    bearer: bool,
    sibling: bool,
    retry: bool,
    issued: Vec<Context>,
    issue_errors: Vec<Error>,
    failures: Vec<VenueError>,
    responses: usize,
}
impl Shared {
    fn new() -> Self {
        Self {
            token: Some("A"),
            revision: 1,
            session: None,
            retire_timer: None,
            next_generation_token: None,
            bearer: true,
            sibling: false,
            retry: false,
            issued: Vec::new(),
            issue_errors: Vec::new(),
            failures: Vec::new(),
            responses: 0,
        }
    }
    fn refresh(&mut self, token: Option<&'static str>) {
        self.token = token;
        self.revision += 1;
    }
}

struct Pending {
    context: Context,
    retried: bool,
}
struct Probe {
    shared: Rc<RefCell<Shared>>,
}
fn policy(retry: bool) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: if retry {
            RetryPolicy::ReadFailure
        } else {
            RetryPolicy::Never
        },
    }
}
/// Today's only attachment point: a static header snapshot taken at issue.
fn bearer_headers(token: &str) -> &'static [(&'static str, &'static str)] {
    let value: &'static str = Box::leak(format!("Bearer {token}").into_boxed_str());
    Box::leak(Box::new([
        ("Authorization", value),
        ("Content-Type", "application/json"),
    ]))
}
impl Probe {
    fn issue(&self, cx: &mut Cx<'_, Pending>, context: Context, retried: bool) {
        let mut shared = self.shared.borrow_mut();
        let headers = if shared.bearer {
            bearer_headers(shared.token.expect("probe issues only while ready"))
        } else {
            &[("Content-Type", "application/json")][..]
        };
        shared.issued.push(context.clone());
        let pending = Pending {
            context: context.clone(),
            retried,
        };
        let result = cx.request_managed_with_headers(
            context,
            Method::Get,
            "https://fixture/private".into(),
            headers,
            None,
            pending,
        );
        if let Err((_, error)) = result {
            shared.issue_errors.push(error);
        }
    }
    fn fail(&self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.shared.borrow_mut().failures.push(error.clone());
        let _ = cx.fail_context(&pending.context, error);
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            account: AccountLookup::Query,
            ..Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        let session = cx
            .open_session(SessionPolicy {
                credential: None,
                repeat_safe: true,
            })
            .unwrap();
        let timer = cx.set_timer(60_000.);
        let mut shared = self.shared.borrow_mut();
        shared.session = Some(session);
        shared.retire_timer = Some(timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::FetchAccount { command } = command else {
            return;
        };
        let (retry, sibling) = {
            let shared = self.shared.borrow();
            (shared.retry, shared.sibling)
        };
        let root = cx
            .http_context(Scope::Command(command), policy(retry))
            .unwrap();
        if sibling {
            let leg = cx.http_leg_context(&root, policy(false)).unwrap();
            cx.request_managed(
                leg.clone(),
                Method::Get,
                "https://fixture/public".into(),
                None,
                Pending {
                    context: leg,
                    retried: false,
                },
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
        self.issue(cx, root, false);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let mut shared = self.shared.borrow_mut();
        if shared.retire_timer != Some(timer) {
            return;
        }
        let session = shared.session.take().unwrap();
        let waiters = cx.retire_session(&session).unwrap();
        assert!(waiters.is_empty());
        if let Some(token) = shared.next_generation_token {
            shared.session = Some(
                cx.open_session(SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                })
                .unwrap(),
            );
            shared.token = Some(token);
            shared.revision = 1;
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.shared.borrow_mut().responses += 1;
        let _ = cx.done_context(&pending.context, "[]".into());
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if self.shared.borrow().retry && !pending.retried {
            let retry = cx
                .http_retry_context(&pending.context, RetryEvidence::ReadFailure)
                .unwrap();
            self.issue(cx, retry, true);
            return;
        }
        self.fail(cx, pending, VenueError::new(error));
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _: Error,
        error: VenueError,
    ) {
        self.fail(cx, pending, error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(configure: impl FnOnce(&mut Shared)) -> (Engine<Probe>, Rc<RefCell<Shared>>) {
    let mut shared = Shared::new();
    configure(&mut shared);
    let shared = Rc::new(RefCell::new(shared));
    let mut e = support::seeded_engine(Probe {
        shared: shared.clone(),
    });
    e.start(NOW);
    e.drain_actions().for_each(drop);
    (e, shared)
}
fn ask(e: &mut Engine<Probe>, command: u32) -> Vec<(Preparation, HttpRequest, RoutingIntent)> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command }));
    prepares(e)
}
fn prepares(e: &mut Engine<Probe>) -> Vec<(Preparation, HttpRequest, RoutingIntent)> {
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => Some((preparation, request, intent)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn prepared(
    e: &mut Engine<Probe>,
    now: f64,
    preparation: Preparation,
) -> Option<(Permit, HttpRequest)> {
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    e.drain_actions().find_map(|a| match a {
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        _ => None,
    })
}
fn authorization(request: &HttpRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|(name, _)| *name == "Authorization")
        .map(|(_, value)| value.as_str())
}
fn retire(e: &mut Engine<Probe>, shared: &Rc<RefCell<Shared>>, now: f64) {
    let timer = shared.borrow().retire_timer.unwrap();
    e.handle(now, Event::TimerFired { timer });
    assert_eq!(
        shared.borrow().token,
        Some("C"),
        "the probe retired its Session"
    );
}
fn respond(e: &mut Engine<Probe>, now: f64, request: &HttpRequest) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    e.drain_actions().for_each(drop);
}
fn not_sent(shared: &Rc<RefCell<Shared>>) -> bool {
    shared
        .borrow()
        .failures
        .iter()
        .any(|error| error.outcome == Some(Outcome::NotSent))
}

/// R1: the Bearer is read at prepare, not snapshotted at issue.
#[test]
fn r1_bound_request_reads_the_token_just_in_time_at_prepare() {
    let (mut e, shared) = engine(|_| {});
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 1);
    shared.borrow_mut().refresh(Some("B"));
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(
        authorization(&request),
        Some("Bearer B"),
        "the packet bound after the venue moved to B still carries the issue-time token"
    );
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
}

/// R2: a revision move between prepare and claim reprepares; one HTTP, new token.
#[test]
fn r2_revision_moved_before_claim_reprepares_with_the_new_token() {
    let (mut e, shared) = engine(|_| {});
    let issued = ask(&mut e, 1);
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(authorization(&request), Some("Bearer A"));
    shared.borrow_mut().refresh(Some("B"));
    assert!(
        !matches!(e.claim_http(NOW + 2., permit), Ok(Claim::Released(_))),
        "a packet prepared at revision 1 released after the venue moved to revision 2"
    );
    let reprepared = prepares(&mut e);
    assert_eq!(reprepared.len(), 1);
    let (permit, request) = prepared(&mut e, NOW + 3., reprepared[0].0).expect("rebound packet");
    assert_eq!(authorization(&request), Some("Bearer B"));
    assert!(matches!(
        e.claim_http(NOW + 4., permit),
        Ok(Claim::Released(_))
    ));
    assert!(prepares(&mut e).is_empty());
}

/// R3: retiring the Session after prepare fences a command-owned bound packet.
#[test]
fn r3_session_retired_after_prepare_never_releases_and_fails_not_sent() {
    let (mut e, shared) = engine(|shared| shared.next_generation_token = Some("C"));
    let issued = ask(&mut e, 1);
    let (permit, _) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    retire(&mut e, &shared, NOW + 2.);
    e.drain_actions().for_each(drop);
    assert!(
        !matches!(e.claim_http(NOW + 3., permit), Ok(Claim::Released(_))),
        "a packet bound to a retired generation released"
    );
    e.drain_actions().for_each(drop);
    assert!(not_sent(&shared), "the owner was not failed typed NotSent");
    assert!(prepares(&mut e).is_empty());
}

/// R4: a venue that is NotReady at prepare gets no HTTP and a typed NotSent.
#[test]
fn r4_venue_not_ready_at_prepare_sends_nothing_and_fails_not_sent() {
    let (mut e, shared) = engine(|_| {});
    let issued = ask(&mut e, 1);
    shared.borrow_mut().refresh(None);
    assert!(
        prepared(&mut e, NOW + 1., issued[0].0).is_none(),
        "a packet was bound while the venue's Bearer was NotReady"
    );
    assert!(not_sent(&shared));
}

/// R5: a retry context reads the current header at its own preparation.
#[test]
fn r5_retry_reads_the_current_token_and_keeps_lineage() {
    let (mut e, shared) = engine(|shared| shared.retry = true);
    let issued = ask(&mut e, 1);
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request: request.id,
            error: "reset",
        },
    );
    let retried = prepares(&mut e);
    assert_eq!(retried.len(), 1);
    {
        let shared = shared.borrow();
        assert_eq!(shared.issued.len(), 2);
        assert_eq!(shared.issued[1].leg(), shared.issued[0].leg());
        assert_eq!(shared.issued[1].owner(), shared.issued[0].owner());
    }
    shared.borrow_mut().refresh(Some("B"));
    let (_, request) = prepared(&mut e, NOW + 4., retried[0].0).expect("retry packet");
    assert_eq!(
        authorization(&request),
        Some("Bearer B"),
        "the retry carries the token cached when the retry was issued"
    );
}

/// R6: a recipe-supplied `Authorization` header is refused.
#[test]
fn r6_recipe_authorization_header_is_refused() {
    let (mut e, shared) = engine(|_| {});
    let issued = ask(&mut e, 1);
    assert!(
        issued.is_empty() && shared.borrow().issue_errors == [Error::Invalid],
        "a recipe-supplied Authorization header was accepted"
    );
}

/// R12: a bound keyless request prepares Primary; its unbound sibling stays Public.
#[test]
fn r12_bound_keyless_request_prepares_primary_and_sibling_stays_public() {
    let (mut e, _) = engine(|shared| shared.sibling = true);
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let (bound, unbound): (Vec<_>, Vec<_>) = issued
        .iter()
        .partition(|(_, request, _)| authorization(request).is_some());
    assert_eq!(unbound[0].2, RoutingIntent::Public);
    assert_eq!(
        bound[0].2,
        RoutingIntent::Primary,
        "a Bearer-carrying keyless request prepares for public egress"
    );
}

/// R12 control: unbound sibling legs of the same root prepare Public today.
#[test]
fn control_unbound_sibling_leg_prepares_public() {
    let (mut e, shared) = engine(|shared| {
        shared.bearer = false;
        shared.sibling = true;
    });
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 2);
    assert!(issued.iter().all(|(_, request, intent)| {
        *intent == RoutingIntent::Public && authorization(request).is_none()
    }));
    assert!(shared.borrow().issue_errors.is_empty());
}

/// Control: an unbound request prepares, binds, releases and delivers today.
#[test]
fn control_unbound_request_releases_and_delivers() {
    let (mut e, shared) = engine(|shared| shared.bearer = false);
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 1);
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(authorization(&request), None);
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
    respond(&mut e, NOW + 3., &request);
    assert_eq!(shared.borrow().responses, 1);
    assert!(shared.borrow().failures.is_empty());
}

/// R13 control: a packet released before retire still delivers to its owner.
#[test]
fn control_r13_released_before_retire_still_delivers_its_response() {
    let (mut e, shared) = engine(|shared| shared.next_generation_token = Some("C"));
    let issued = ask(&mut e, 1);
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
    retire(&mut e, &shared, NOW + 3.);
    respond(&mut e, NOW + 4., &request);
    assert_eq!(shared.borrow().responses, 1);
    assert!(shared.borrow().failures.is_empty());
}
