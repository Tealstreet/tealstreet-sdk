//! Runtime tests for `docs/plans/RUST_SESSION_BEARER_HOOK.md`.
//! The probe binds its private requests to its Session at issue and answers
//! the engine's Bearer hooks from its venue-held token cache.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Credential, Egress, Error, HttpAction, Operation, Owner, Permit, Policy,
    Preparation, Prepared, RetryEvidence, RetryPolicy, RoutingIntent, Scope, SecretHeaderValue,
    Session, SessionBearer, SessionPolicy,
};
use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation as SignOperation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{
    AccountLookup, Action, Capabilities, Command, CredentialSlot, Cx, DispatchContext, Engine,
    ErrorKind, Event, HttpRequest, Method, Outcome, RequestId, Response, SignStep, SocketId,
    TimerId, Venue, VenueError,
};
use std::cell::RefCell;
use std::rc::Rc;

const NOW: f64 = 1_000.25;
const KEY: &str = "account/primary";

/// The venue-held cache: `None` is the venue's NotReady.
struct Shared {
    token: Option<&'static str>,
    revision: u64,
    session: Option<Session>,
    retired: Option<Session>,
    retire_timer: Option<TimerId>,
    /// Token the venue caches for the generation it opens on retire.
    next_generation_token: Option<&'static str>,
    bearer: bool,
    bind_retired: bool,
    /// Retire the Session from inside a response delivery.
    retire_on_response: bool,
    recipe_authorization: bool,
    signed: bool,
    egress: Option<Egress>,
    sibling: bool,
    retry: bool,
    #[cfg(feature = "http-plans")]
    preflight: bool,
    /// Session drains observed inside the Preflight owner's callback.
    #[cfg(feature = "http-plans")]
    scoped_drains: Option<usize>,
    hook_calls: usize,
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
            retired: None,
            retire_timer: None,
            next_generation_token: None,
            bearer: true,
            bind_retired: false,
            retire_on_response: false,
            recipe_authorization: false,
            signed: false,
            egress: None,
            sibling: false,
            retry: false,
            #[cfg(feature = "http-plans")]
            preflight: false,
            #[cfg(feature = "http-plans")]
            scoped_drains: None,
            hook_calls: 0,
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
    /// The venue maps a raw generation to its own cache: an old generation's
    /// packet is never served by a new generation's token.
    fn serves(&self, generation: u64) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.owner() == Owner::Session { generation })
    }
}

struct Recipe;
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request.body = Some(format!("{{\"stamp\":{}}}", context.stamp_ms()));
        Ok(SignRequest {
            receipt: SignReceipt::default(),
            operations: vec![SignOperation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef(KEY.into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"body".to_vec(),
                }],
            }],
            context: Default::default(),
        })
    }
    fn apply(&mut self, action: &mut Action, _: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        request.headers.push(("signature", "applied".into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        60_000.
    }
}

struct Pending {
    context: Context,
}
struct Probe {
    shared: Rc<RefCell<Shared>>,
}
fn policy(retry: bool, signed: bool) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: signed.then(|| Credential {
            slot: CredentialSlot::Primary,
            reference: KeyRef(KEY.into()),
        }),
        timeout_ms: 20_000,
        pacing: None,
        retry: if retry {
            RetryPolicy::ReadFailure
        } else {
            RetryPolicy::Never
        },
    }
}
const JSON: &[(&str, &str)] = &[("Content-Type", "application/json")];
const RECIPE_AUTHORIZATION: &[(&str, &str)] = &[
    ("authorization", "Bearer recipe"),
    ("Content-Type", "application/json"),
];
impl Probe {
    /// A retry passes no Session: it inherits the leg's binding.
    fn issue(&self, cx: &mut Cx<'_, Pending>, context: Context, retried: bool) {
        let mut shared = self.shared.borrow_mut();
        let session = match (shared.bearer && !retried, shared.bind_retired) {
            (false, _) => None,
            (true, false) => shared.session.clone(),
            (true, true) => shared.retired.clone(),
        };
        shared.issued.push(context.clone());
        let pending = Pending {
            context: context.clone(),
        };
        let url = "https://fixture/private".to_owned();
        let result = if shared.signed {
            cx.request_signed_managed(
                context,
                session.as_ref(),
                Method::Post,
                url,
                None,
                Box::new(Recipe),
                pending,
            )
        } else {
            let headers = if shared.recipe_authorization {
                RECIPE_AUTHORIZATION
            } else {
                JSON
            };
            cx.request_managed_with_headers(
                context,
                session.as_ref(),
                Method::Get,
                url,
                headers,
                None,
                pending,
            )
        };
        if let Err((_, error)) = result {
            shared.issue_errors.push(error);
        }
    }
    fn fail(&self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.shared.borrow_mut().failures.push(error.clone());
        let _ = cx.fail_context(&pending.context, error);
    }
    /// Retire the Session, optionally reopening a new generation in the same
    /// callback (outside any readiness scope).
    fn retire_now(&self, cx: &mut Cx<'_, Pending>) {
        let mut shared = self.shared.borrow_mut();
        let Some(session) = shared.session.take() else {
            return;
        };
        let waiters = cx.retire_session(&session).unwrap();
        assert!(waiters.is_empty());
        shared.retired = Some(session);
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
    #[cfg(feature = "http-plans")]
    fn preflight(&self, cx: &mut Cx<'_, Pending>) {
        use safe_cex_engine::request_context::plan::{CallTerms, RetryWindow};
        use safe_cex_engine::request_context::{PreflightPolicy, PreflightPurpose};
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let context = cx
            .preflight_read_context(
                &preflight,
                policy(false, false),
                CallTerms {
                    wait_ms: 0,
                    timeout_ms: 20_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                },
            )
            .unwrap();
        self.issue(cx, context, false);
        let session = self.shared.borrow().session.clone().unwrap();
        let drained = cx.session_ready(&session).len();
        self.shared.borrow_mut().scoped_drains = Some(drained);
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
    fn session_bearer(&self, generation: u64) -> SessionBearer {
        let mut shared = self.shared.borrow_mut();
        shared.hook_calls += 1;
        match shared.token {
            Some(token) if shared.serves(generation) => SessionBearer::Ready {
                value: SecretHeaderValue::new(format!("Bearer {token}")).unwrap(),
                revision: shared.revision,
            },
            _ => SessionBearer::NotReady,
        }
    }
    fn session_bearer_revision(&self, generation: u64) -> Option<u64> {
        let shared = self.shared.borrow();
        (shared.token.is_some() && shared.serves(generation)).then_some(shared.revision)
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
        #[cfg(feature = "http-plans")]
        if self.shared.borrow().preflight {
            self.preflight(cx);
            return;
        }
        let (retry, sibling, signed, egress) = {
            let shared = self.shared.borrow();
            (shared.retry, shared.sibling, shared.signed, shared.egress)
        };
        let root = match egress {
            Some(egress) => {
                cx.http_context_with_egress(Scope::Command(command), policy(retry, signed), egress)
            }
            None => cx.http_context(Scope::Command(command), policy(retry, signed)),
        }
        .unwrap();
        if sibling {
            let leg = cx.http_leg_context(&root, policy(false, false)).unwrap();
            cx.request_managed(
                leg.clone(),
                None,
                Method::Get,
                "https://fixture/public".into(),
                None,
                Pending { context: leg },
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
        self.issue(cx, root, false);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.shared.borrow().retire_timer == Some(timer) {
            self.retire_now(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.shared.borrow_mut().responses += 1;
        // The sibling leg's delivery only retires; the command stays open for
        // its bound root.
        if self.shared.borrow().retire_on_response {
            self.retire_now(cx);
            return;
        }
        let _ = cx.done_context(&pending.context, "[]".into());
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if self.shared.borrow().retry && self.shared.borrow().issued.len() == 1 {
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
fn bind(actions: Vec<Action>) -> Option<(Permit, HttpRequest)> {
    actions.into_iter().find_map(|a| match a {
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        _ => None,
    })
}
fn route(e: &mut Engine<Probe>, now: f64, preparation: Preparation) -> Vec<Action> {
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    e.drain_actions().collect()
}
fn sign_work(actions: &[Action]) -> Option<(RequestId, SignReceipt)> {
    actions.iter().find_map(|a| match a {
        Action::Sign { attempt, work, .. } => Some((*attempt, work.receipt)),
        _ => None,
    })
}
fn sign(e: &mut Engine<Probe>, now: f64, (attempt, receipt): (RequestId, SignReceipt)) {
    e.handle(
        now,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(vec![vec![7; 32]], receipt)),
        },
    );
}
/// Unsigned packets Bind at route readiness; signed ones after signing.
fn prepared(
    e: &mut Engine<Probe>,
    now: f64,
    preparation: Preparation,
) -> Option<(Permit, HttpRequest)> {
    let actions = route(e, now, preparation);
    match sign_work(&actions) {
        Some(work) => {
            sign(e, now, work);
            bind(e.drain_actions().collect())
        }
        None => bind(actions),
    }
}
fn authorization(request: &HttpRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
        .map(|(_, value)| value.as_str())
}
fn retire(e: &mut Engine<Probe>, shared: &Rc<RefCell<Shared>>, now: f64) {
    let timer = shared.borrow().retire_timer.unwrap();
    e.handle(now, Event::TimerFired { timer });
    assert!(
        shared.borrow().retired.is_some(),
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
/// Exactly one typed failure through the common preparation-failure path,
/// which marks every unreleased attempt NotSent, Reads included.
fn one_typed_read_failure(shared: &Rc<RefCell<Shared>>, kind: ErrorKind) -> bool {
    let shared = shared.borrow();
    shared.failures.len() == 1
        && shared.failures[0].outcome == Some(Outcome::NotSent)
        && shared.failures[0].kind == Some(kind)
}

/// R1: the Bearer is read at prepare, not snapshotted at issue.
fn r1(signed: bool) {
    let (mut e, shared) = engine(|shared| shared.signed = signed);
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 1);
    assert_eq!(shared.borrow().hook_calls, 0, "the hook ran at issue");
    assert_eq!(authorization(&issued[0].1), None);
    shared.borrow_mut().refresh(Some("B"));
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(
        authorization(&request),
        Some("Bearer B"),
        "the packet bound after the venue moved to B still carries the issue-time token"
    );
    assert_eq!(request.headers[0].0, "Authorization", "native header order");
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
}
#[test]
fn r1_bound_request_reads_the_token_just_in_time_at_prepare() {
    r1(false);
}
#[test]
fn r1_signed_bound_request_reads_the_token_at_the_signed_bind() {
    r1(true);
}

/// R2: a revision move between prepare and claim reprepares; one HTTP, new token.
fn r2(signed: bool) {
    let (mut e, shared) = engine(|shared| shared.signed = signed);
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
    assert_eq!(reprepared[0].2, RoutingIntent::Primary);
    assert_eq!(authorization(&reprepared[0].1), None);
    let (permit, request) = prepared(&mut e, NOW + 3., reprepared[0].0).expect("rebound packet");
    assert_eq!(authorization(&request), Some("Bearer B"));
    assert!(matches!(
        e.claim_http(NOW + 4., permit),
        Ok(Claim::Released(_))
    ));
    assert!(prepares(&mut e).is_empty());
    assert!(shared.borrow().failures.is_empty());
}
#[test]
fn r2_revision_moved_before_claim_reprepares_with_the_new_token() {
    r2(false);
}
#[test]
fn r2_signed_revision_moved_before_claim_reprepares_with_the_new_token() {
    r2(true);
}

/// R3: retiring the Session after prepare fences a command-owned bound packet;
/// the Read owner gets one typed NotSent failure, and a late response
/// reaches no callback. The new generation's token never serves it, even when
/// its text is unchanged.
#[test]
fn r3_session_retired_after_prepare_never_releases_and_fails_typed() {
    for next in ["C", "A"] {
        let (mut e, shared) = engine(|shared| shared.next_generation_token = Some(next));
        let issued = ask(&mut e, 1);
        let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
        let health = e.health().clone();
        retire(&mut e, &shared, NOW + 2.);
        e.drain_actions().for_each(drop);
        assert!(
            !matches!(e.claim_http(NOW + 3., permit), Ok(Claim::Released(_))),
            "a packet bound to a retired generation released"
        );
        e.drain_actions().for_each(drop);
        assert!(
            one_typed_read_failure(&shared, ErrorKind::InvalidInput),
            "the Read owner was not failed once, typed and NotSent"
        );
        // An engine preparation cause, never auth or HTTP failure evidence.
        assert_eq!(e.health(), &health);
        respond(&mut e, NOW + 4., &request);
        assert_eq!(
            shared.borrow().responses,
            0,
            "a late response was delivered"
        );
        assert!(prepares(&mut e).is_empty());
    }
}

/// R3, signed: a signing result arriving after the retire binds nothing.
#[test]
fn r3_signed_late_signing_result_after_retire_binds_nothing() {
    let (mut e, shared) = engine(|shared| {
        shared.signed = true;
        shared.next_generation_token = Some("C");
    });
    let issued = ask(&mut e, 1);
    let work = sign_work(&route(&mut e, NOW + 1., issued[0].0)).expect("signing work");
    retire(&mut e, &shared, NOW + 2.);
    e.drain_actions().for_each(drop);
    assert!(one_typed_read_failure(&shared, ErrorKind::InvalidInput));
    sign(&mut e, NOW + 3., work);
    assert!(bind(e.drain_actions().collect()).is_none());
    assert_eq!(shared.borrow().responses, 0);
    assert_eq!(shared.borrow().failures.len(), 1);
}

/// R3, nested: a retire inside another request's response delivery fails the
/// bound packet once after that callback, and it never releases.
#[test]
fn r3_retire_inside_a_response_delivery_fails_the_bound_packet_after_it() {
    let (mut e, shared) = engine(|shared| {
        shared.sibling = true;
        shared.retire_on_response = true;
        shared.next_generation_token = Some("C");
    });
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let mut bound = None;
    let mut sibling = None;
    for (preparation, ..) in issued {
        let (permit, request) = prepared(&mut e, NOW + 1., preparation).expect("packet");
        if authorization(&request).is_some() {
            bound = Some(permit);
        } else {
            sibling = Some((permit, request));
        }
    }
    let (permit, request) = sibling.expect("unbound sibling");
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
    respond(&mut e, NOW + 3., &request);
    assert!(shared.borrow().retired.is_some());
    assert!(one_typed_read_failure(&shared, ErrorKind::InvalidInput));
    assert!(!matches!(
        e.claim_http(NOW + 4., bound.expect("bound packet")),
        Ok(Claim::Released(_))
    ));
    e.drain_actions().for_each(drop);
    assert_eq!(shared.borrow().responses, 1);
    assert_eq!(shared.borrow().failures.len(), 1);
}

/// R4: a venue that is NotReady at prepare gets no HTTP; the Read owner gets
/// one typed NotSent failure.
#[test]
fn r4_venue_not_ready_at_prepare_sends_nothing_and_fails_typed() {
    for signed in [false, true] {
        let (mut e, shared) = engine(|shared| shared.signed = signed);
        let issued = ask(&mut e, 1);
        shared.borrow_mut().refresh(None);
        let health = e.health().clone();
        let actions = route(&mut e, NOW + 1., issued[0].0);
        assert!(
            sign_work(&actions).is_none() && bind(actions).is_none(),
            "provider work or a packet started while the venue's Bearer was NotReady"
        );
        assert!(one_typed_read_failure(&shared, ErrorKind::Internal));
        assert_eq!(e.health(), &health);
    }
}

/// R5: a retry context inherits the binding and reads the current header.
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
    assert_eq!(retried[0].2, RoutingIntent::Primary);
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

/// R6: a recipe-supplied `Authorization` header is refused, and the value
/// never appears in `Debug`.
#[test]
fn r6_recipe_authorization_header_is_refused() {
    let (mut e, shared) = engine(|shared| shared.recipe_authorization = true);
    let issued = ask(&mut e, 1);
    assert!(
        issued.is_empty() && shared.borrow().issue_errors == [Error::Invalid],
        "a recipe-supplied Authorization header was accepted"
    );
    let ready = SessionBearer::Ready {
        value: SecretHeaderValue::new("Bearer jwt-secret".into()).unwrap(),
        revision: 1,
    };
    assert!(!format!("{ready:?}").contains("jwt-secret"));
    assert_eq!(
        SecretHeaderValue::new(String::new()).err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        SecretHeaderValue::new("Bearer a\r\nX: b".into()).err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        SecretHeaderValue::new("x".repeat(8193)).err(),
        Some(Error::Invalid)
    );
}

/// R6 control: an unbound request keeps its own `Authorization` header, so
/// venues that send one today can still move to managed requests.
#[test]
fn control_r6_unbound_recipe_authorization_is_unchanged() {
    let (mut e, shared) = engine(|shared| {
        shared.bearer = false;
        shared.recipe_authorization = true;
    });
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 1);
    assert!(shared.borrow().issue_errors.is_empty());
    let (_, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("unbound packet");
    assert!(authorization(&request).is_some());
    assert_eq!(shared.borrow().hook_calls, 0);
}

/// R7: binding to a retired Session refuses Revoked and nothing is prepared.
#[test]
fn r7_binding_to_a_retired_session_is_revoked() {
    let (mut e, shared) = engine(|shared| shared.next_generation_token = Some("C"));
    retire(&mut e, &shared, NOW + 1.);
    e.drain_actions().for_each(drop);
    shared.borrow_mut().bind_retired = true;
    assert!(ask(&mut e, 1).is_empty());
    assert_eq!(shared.borrow().issue_errors, [Error::Revoked]);
    assert_eq!(shared.borrow().hook_calls, 0);
}

/// R12: a bound keyless request prepares Primary; its unbound sibling stays Public.
#[test]
fn r12_bound_keyless_request_prepares_primary_and_sibling_stays_public() {
    let (mut e, _) = engine(|shared| shared.sibling = true);
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let (bound, unbound): (Vec<_>, Vec<_>) = issued
        .iter()
        .partition(|(_, request, _)| request.url.ends_with("/private"));
    assert_eq!(unbound[0].2, RoutingIntent::Public);
    assert_eq!(
        bound[0].2,
        RoutingIntent::Primary,
        "a Bearer-bound keyless request prepares for public egress"
    );
}

/// R12: explicit public egress on a bound request refuses Invalid.
#[test]
fn r12_bound_request_on_explicit_public_egress_is_invalid() {
    for egress in [Egress::PublicCors, Egress::Direct] {
        let (mut e, shared) = engine(|shared| shared.egress = Some(egress));
        assert!(ask(&mut e, 1).is_empty());
        assert_eq!(shared.borrow().issue_errors, [Error::Invalid]);
    }
    let (mut e, _) = engine(|shared| shared.egress = Some(Egress::Primary));
    assert_eq!(ask(&mut e, 1)[0].2, RoutingIntent::Primary);
}

/// R14: a retained Preflight owner binds its own request; the binding takes
/// no session scope, and its callback drains no session work.
#[cfg(feature = "http-plans")]
#[test]
fn r14_preflight_owner_binds_its_own_request() {
    let (mut e, shared) = engine(|shared| shared.preflight = true);
    let issued = ask(&mut e, 1);
    assert_eq!(shared.borrow().issue_errors, []);
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].2, RoutingIntent::Primary);
    assert_eq!(shared.borrow().scoped_drains, Some(0));
    let (_, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(authorization(&request), Some("Bearer A"));
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
    assert_eq!(shared.borrow().hook_calls, 0);
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
