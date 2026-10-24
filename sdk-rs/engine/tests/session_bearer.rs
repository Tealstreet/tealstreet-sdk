//! Runtime tests for `docs/plans/RUST_SESSION_BEARER_HOOK.md`.
//! The probe binds its private requests to its Session at issue and answers
//! the engine's Bearer hooks from its venue-held token cache.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
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
    ErrorKind, Event, HttpRequest, Method, Notice, Outcome, RequestId, Response, SignStep,
    SocketId, TimerId, Venue, VenueError,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The header limit exactly, so the engine's `Authorization` overflows it.
const CROWDED: &[(&str, &str)] = &[
    ("x-h0", "v"),
    ("x-h1", "v"),
    ("x-h2", "v"),
    ("x-h3", "v"),
    ("x-h4", "v"),
    ("x-h5", "v"),
    ("x-h6", "v"),
    ("x-h7", "v"),
    ("x-h8", "v"),
    ("x-h9", "v"),
    ("x-h10", "v"),
    ("x-h11", "v"),
    ("x-h12", "v"),
    ("x-h13", "v"),
    ("x-h14", "v"),
    ("x-h15", "v"),
    ("x-h16", "v"),
    ("x-h17", "v"),
    ("x-h18", "v"),
    ("x-h19", "v"),
    ("x-h20", "v"),
    ("x-h21", "v"),
    ("x-h22", "v"),
    ("x-h23", "v"),
    ("x-h24", "v"),
    ("x-h25", "v"),
    ("x-h26", "v"),
    ("x-h27", "v"),
    ("x-h28", "v"),
    ("x-h29", "v"),
    ("x-h30", "v"),
    ("x-h31", "v"),
];
const NOW: f64 = 1_000.25;
const KEY: &str = "account/primary";

/// The venue-held cache: `None` is the venue's NotReady.
struct Shared {
    token: Option<&'static str>,
    revision: u64,
    session: Option<Session>,
    retired: Option<Session>,
    /// A second live Session, opened at start when set.
    two_sessions: bool,
    second: Option<Session>,
    /// New requests bind the second Session.
    bind_second: bool,
    /// A fenced failure callback retires the second Session.
    retire_second_on_failure: bool,
    retire_timer: Option<TimerId>,
    /// Token the venue caches for the generation it opens on retire.
    next_generation_token: Option<&'static str>,
    bearer: bool,
    bind_retired: bool,
    /// Retire the Session from inside a response delivery.
    retire_on_response: bool,
    /// `start` issues one bound Background read, then retires its Session.
    retire_in_start: bool,
    /// The Session is granted private egress, and commands issue their root
    /// on it through `session_scoped_context`.
    session_egress: bool,
    recipe_authorization: bool,
    crowded: bool,
    /// The sibling leg binds the same Session as the root.
    sibling_bound: bool,
    signed: bool,
    egress: Option<Egress>,
    sibling: bool,
    retry: bool,
    #[cfg(feature = "http-plans")]
    preflight: bool,
    /// Session drains observed inside the Preflight owner's callback.
    #[cfg(feature = "http-plans")]
    scoped_drains: Option<usize>,
    /// This command id admits a Read Plan of `plan_legs` peer calls.
    #[cfg(feature = "http-plans")]
    plan_command: Option<u32>,
    #[cfg(feature = "http-plans")]
    plan_legs: usize,
    #[cfg(feature = "http-plans")]
    plan_wait_ms: u32,
    #[cfg(feature = "http-plans")]
    plan: Option<safe_cex_engine::request_context::Plan>,
    /// The Plan-ready callback tries to open, retire and use a Session.
    #[cfg(feature = "http-plans")]
    lifetimes_on_plan_ready: bool,
    #[cfg(feature = "http-plans")]
    scoped_refusals: Vec<Error>,
    #[cfg(feature = "http-plans")]
    plan_ready_calls: usize,
    /// An AmendOrder issues this many bound Mutation legs.
    write_legs: usize,
    /// A delivered write is refused `RefusedResend` and resent.
    resend: bool,
    /// Venue callbacks in progress, and the most ever nested.
    depth: usize,
    max_depth: usize,
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
            two_sessions: false,
            second: None,
            bind_second: false,
            retire_second_on_failure: false,
            retire_timer: None,
            next_generation_token: None,
            bearer: true,
            bind_retired: false,
            retire_on_response: false,
            retire_in_start: false,
            session_egress: false,
            recipe_authorization: false,
            crowded: false,
            sibling_bound: false,
            signed: false,
            egress: None,
            sibling: false,
            retry: false,
            #[cfg(feature = "http-plans")]
            preflight: false,
            #[cfg(feature = "http-plans")]
            scoped_drains: None,
            #[cfg(feature = "http-plans")]
            plan_command: None,
            #[cfg(feature = "http-plans")]
            plan_legs: 0,
            #[cfg(feature = "http-plans")]
            plan_wait_ms: 0,
            #[cfg(feature = "http-plans")]
            plan: None,
            #[cfg(feature = "http-plans")]
            lifetimes_on_plan_ready: false,
            #[cfg(feature = "http-plans")]
            scoped_refusals: Vec::new(),
            #[cfg(feature = "http-plans")]
            plan_ready_calls: 0,
            write_legs: 0,
            resend: false,
            depth: 0,
            max_depth: 0,
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
        [&self.session, &self.second].into_iter().any(|session| {
            session
                .as_ref()
                .is_some_and(|session| session.owner() == Owner::Session { generation })
        })
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
/// Held for the length of one venue callback.
struct Depth(Rc<RefCell<Shared>>);
impl Drop for Depth {
    fn drop(&mut self) {
        // Never a second panic while a failing callback still holds a borrow.
        if let Ok(mut shared) = self.0.try_borrow_mut() {
            shared.depth -= 1;
        }
    }
}
fn write_policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::DefinitiveRefusal,
    }
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
    fn enter(&self) -> Depth {
        let mut shared = self.shared.borrow_mut();
        shared.depth += 1;
        shared.max_depth = shared.max_depth.max(shared.depth);
        Depth(self.shared.clone())
    }
    /// A resend passes no Session: it inherits the leg's binding.
    fn issue_write(&self, cx: &mut Cx<'_, Pending>, context: Context, url: &str, bound: bool) {
        let session = bound
            .then(|| self.shared.borrow().session.clone())
            .flatten();
        cx.request_managed(
            context.clone(),
            session.as_ref(),
            Method::Post,
            format!("https://fixture/{url}"),
            Some("{}".into()),
            Pending { context },
        )
        .map_err(|(_, error)| error)
        .unwrap();
    }
    /// Bound Mutation legs of one command: the root, then sibling legs.
    fn amend(&self, cx: &mut Cx<'_, Pending>, command: u32) {
        let root = cx
            .http_context(Scope::Command(command), write_policy())
            .unwrap();
        for leg in 1..self.shared.borrow().write_legs {
            let context = cx.http_leg_context(&root, write_policy()).unwrap();
            self.issue_write(cx, context, &format!("write-{leg}"), true);
        }
        self.issue_write(cx, root, "write-0", true);
    }
    #[cfg(feature = "http-plans")]
    fn admit_plan(&self, cx: &mut Cx<'_, Pending>) {
        use safe_cex_engine::request_context::plan::{CallBound, RetryWindow, StepBound};
        use safe_cex_engine::request_context::{PlanPolicy, PreflightPolicy, PreflightPurpose};
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let (legs, wait_ms) = {
            let shared = self.shared.borrow();
            (shared.plan_legs, shared.plan_wait_ms)
        };
        let calls = (0..legs)
            .map(|_| CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            })
            .collect::<Vec<_>>();
        let plan = cx
            .admit_plan(
                &preflight,
                &[StepBound {
                    wait_ms,
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
        let session = self.shared.borrow().session.clone();
        for slot in cx.plan_slots(&plan).unwrap() {
            let context = cx.plan_leg_context(&plan, &slot).unwrap();
            cx.request_managed(
                context.clone(),
                session.as_ref(),
                Method::Get,
                "https://fixture/plan".into(),
                None,
                Pending { context },
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
        self.shared.borrow_mut().plan = Some(plan);
    }
    /// A retry passes no Session: it inherits the leg's binding.
    fn issue(&self, cx: &mut Cx<'_, Pending>, context: Context, retried: bool) {
        let mut shared = self.shared.borrow_mut();
        let session = match (shared.bearer && !retried, shared.bind_retired) {
            (false, _) => None,
            (true, _) if shared.bind_second => shared.second.clone(),
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
            } else if shared.crowded {
                CROWDED
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
                    private_egress: None,
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
        let private_egress = self.shared.borrow().session_egress.then(|| {
            Slots::new(Some(()), None, SlotPolicy::default())
                .select(CredentialRole::SessionAuth)
                .unwrap()
                .0
                .session_egress()
                .unwrap()
        });
        let session = cx
            .open_session(SessionPolicy {
                credential: None,
                repeat_safe: true,
                private_egress,
            })
            .unwrap();
        let timer = cx.set_timer(60_000.);
        let mut shared = self.shared.borrow_mut();
        if shared.two_sessions {
            shared.second = Some(
                cx.open_session(SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                    private_egress: None,
                })
                .unwrap(),
            );
        }
        shared.session = Some(session);
        shared.retire_timer = Some(timer);
        let retire_in_start = shared.retire_in_start;
        drop(shared);
        if retire_in_start {
            let context = cx
                .http_context(Scope::Background, policy(false, false))
                .unwrap();
            self.issue(cx, context, false);
            self.retire_now(cx);
        }
    }
    fn capture_http_reconciliation(
        &self,
        _: &Pending,
    ) -> Result<safe_cex_engine::request_context::ReconciliationData, Error> {
        safe_cex_engine::request_context::ReconciliationData::new("bearer-probe".into())
    }
    #[cfg(feature = "http-plans")]
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _: &safe_cex_engine::request_context::Plan,
    ) {
        let _depth = self.enter();
        let session = {
            let mut shared = self.shared.borrow_mut();
            shared.plan_ready_calls += 1;
            shared
                .lifetimes_on_plan_ready
                .then(|| shared.session.clone())
                .flatten()
        };
        if let Some(session) = session {
            let keyless = SessionPolicy {
                credential: None,
                repeat_safe: true,
                private_egress: None,
            };
            let refusals = [
                cx.open_session(keyless).err(),
                cx.session_context(&session, policy(false, false)).err(),
                cx.retire_session(&session).err(),
            ];
            self.shared
                .borrow_mut()
                .scoped_refusals
                .extend(refusals.into_iter().flatten());
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let _depth = self.enter();
        let command = match command {
            Command::AmendOrder { command, .. } => return self.amend(cx, command),
            Command::FetchAccount { command } => command,
            _ => return,
        };
        #[cfg(feature = "http-plans")]
        if self.shared.borrow().plan_command == Some(command) {
            self.admit_plan(cx);
            return;
        }
        #[cfg(feature = "http-plans")]
        if self.shared.borrow().preflight {
            self.preflight(cx);
            return;
        }
        let (retry, sibling, signed, egress, scoped) = {
            let shared = self.shared.borrow();
            let scoped = shared
                .session_egress
                .then(|| shared.session.clone())
                .flatten();
            (
                shared.retry,
                shared.sibling,
                shared.signed,
                shared.egress,
                scoped,
            )
        };
        let root = match (scoped, egress) {
            (Some(session), _) => {
                cx.session_scoped_context(&session, Scope::Command(command), policy(retry, signed))
            }
            (None, Some(egress)) => {
                cx.http_context_with_egress(Scope::Command(command), policy(retry, signed), egress)
            }
            (None, None) => cx.http_context(Scope::Command(command), policy(retry, signed)),
        }
        .unwrap();
        if sibling {
            let leg = cx.http_leg_context(&root, policy(false, false)).unwrap();
            let session = {
                let shared = self.shared.borrow();
                shared
                    .sibling_bound
                    .then(|| shared.session.clone())
                    .flatten()
            };
            cx.request_managed(
                leg.clone(),
                session.as_ref(),
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
        let _depth = self.enter();
        if self.shared.borrow().retire_timer == Some(timer) {
            self.retire_now(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        let _depth = self.enter();
        self.shared.borrow_mut().responses += 1;
        if pending.context.operation() == Operation::Mutation && self.shared.borrow().resend {
            use safe_cex_engine::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
            const TIMESTAMP: CodeTable = CodeTable {
                rows: &[Row {
                    key: Key::Int(10002),
                    verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
                    only: None,
                }],
                success: &[],
                unlisted: Verdict::Unknown(None),
                status: &[],
            };
            let native = Native {
                code: NativeCode::Int(10002),
                message: "invalid timestamp",
            };
            let (_, grant) = cx
                .http_write_failure(
                    &pending.context,
                    &TIMESTAMP,
                    Some(&native),
                    "native timestamp refusal",
                    None,
                )
                .unwrap();
            let resend = cx
                .http_resend_context(&pending.context, grant.expect("resend grant"))
                .unwrap();
            self.issue_write(cx, resend, "resend", false);
            return;
        }
        // The sibling leg's delivery only retires; the command stays open for
        // its bound root.
        if self.shared.borrow().retire_on_response {
            self.retire_now(cx);
            return;
        }
        let _ = cx.done_context(&pending.context, "[]".into());
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let _depth = self.enter();
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
        let _depth = self.enter();
        let second = {
            let mut shared = self.shared.borrow_mut();
            shared
                .retire_second_on_failure
                .then(|| shared.second.take())
                .flatten()
        };
        if let Some(second) = second {
            cx.retire_session(&second).unwrap();
        }
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

/// R3, two bound packets of one command: retire fails the owner once; the
/// owner's terminal retires its second packet, which then never releases.
#[test]
fn r3_one_owner_with_two_bound_packets_fails_once() {
    let (mut e, shared) = engine(|shared| {
        shared.sibling = true;
        shared.sibling_bound = true;
        shared.next_generation_token = Some("C");
    });
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let permits = issued
        .into_iter()
        .map(|(preparation, ..)| {
            let (permit, request) = prepared(&mut e, NOW + 1., preparation).expect("packet");
            assert_eq!(authorization(&request), Some("Bearer A"));
            permit
        })
        .collect::<Vec<_>>();
    retire(&mut e, &shared, NOW + 2.);
    e.drain_actions().for_each(drop);
    assert!(one_typed_read_failure(&shared, ErrorKind::InvalidInput));
    for permit in permits {
        assert!(!matches!(
            e.claim_http(NOW + 3., permit),
            Ok(Claim::Released(_))
        ));
    }
    e.drain_actions().for_each(drop);
    assert_eq!(shared.borrow().failures.len(), 1);
}

/// R3, cascade: a fenced failure callback that retires a second Session has
/// that Session's bound packet failed in the same flush, not a later event.
#[test]
fn r3_retire_inside_a_fenced_failure_callback_is_drained_in_the_same_flush() {
    let (mut e, shared) = engine(|shared| {
        shared.two_sessions = true;
        shared.retire_second_on_failure = true;
    });
    let first = ask(&mut e, 1);
    let first = prepared(&mut e, NOW + 1., first[0].0)
        .expect("bound packet")
        .0;
    shared.borrow_mut().bind_second = true;
    let second = ask(&mut e, 2);
    let second = prepared(&mut e, NOW + 1., second[0].0)
        .expect("bound packet")
        .0;
    retire(&mut e, &shared, NOW + 2.);
    e.drain_actions().for_each(drop);
    {
        let shared = shared.borrow();
        assert!(
            shared.second.is_none(),
            "the callback retired the second Session"
        );
        assert_eq!(shared.failures.len(), 2, "both owners failed in one flush");
        assert!(
            shared
                .failures
                .iter()
                .all(|failure| failure.outcome == Some(Outcome::NotSent))
        );
    }
    for permit in [first, second] {
        assert!(!matches!(
            e.claim_http(NOW + 3., permit),
            Ok(Claim::Released(_))
        ));
    }
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

/// R6, limits: a header set already at the limit cannot take the engine's
/// `Authorization`; the attempt fails before any packet is bound.
#[test]
fn r6_bearer_that_overflows_the_header_limit_is_refused() {
    let (mut e, shared) = engine(|shared| shared.crowded = true);
    let issued = ask(&mut e, 1);
    assert_eq!(issued.len(), 1);
    assert!(bind(route(&mut e, NOW + 1., issued[0].0)).is_none());
    let shared = shared.borrow();
    assert_eq!(shared.failures.len(), 1);
    assert_eq!(shared.failures[0].outcome, Some(Outcome::NotSent));
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

/// Session-egress control: a bound request on its Session's private egress
/// (`session_scoped_context`) is private, so it binds and prepares Primary.
#[test]
fn bound_request_on_session_private_egress_prepares_primary() {
    let (mut e, shared) = engine(|shared| shared.session_egress = true);
    let issued = ask(&mut e, 1);
    assert_eq!(shared.borrow().issue_errors, []);
    assert_eq!(issued.len(), 1);
    assert_eq!(issued[0].2, RoutingIntent::Primary);
    let (_, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound packet");
    assert_eq!(authorization(&request), Some("Bearer A"));
}

/// R7, Preflight: a refused binding fails the owning Preflight through its
/// admission, once and NotSent, and nothing is prepared.
#[cfg(feature = "http-plans")]
#[test]
fn r7_preflight_binding_a_retired_session_fails_the_preflight() {
    let (mut e, shared) = engine(|shared| {
        shared.preflight = true;
        shared.next_generation_token = Some("C");
    });
    retire(&mut e, &shared, NOW + 1.);
    e.drain_actions().for_each(drop);
    shared.borrow_mut().bind_retired = true;
    e.handle(
        NOW + 2.,
        Event::Command(Command::FetchAccount { command: 1 }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(shared.borrow().issue_errors, [Error::Revoked]);
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    let failed = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1, "the Preflight was not failed once");
    assert_eq!(failed[0].outcome, Some(Outcome::NotSent));
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

/// Every command terminal in `actions`, as `(command, Some(failure))` or
/// `(command, None)` for a success.
fn terminals(actions: &[Action]) -> Vec<(u32, Option<VenueError>)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                Some((*command, Some(error.clone())))
            }
            Action::Notice(Notice::CommandDone { command, .. }) => Some((*command, None)),
            _ => None,
        })
        .collect()
}
fn amend(e: &mut Engine<Probe>, command: u32) -> Vec<(Preparation, HttpRequest, RoutingIntent)> {
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    prepares(e)
}
fn released(e: &mut Engine<Probe>, now: f64, permit: Permit) -> bool {
    matches!(e.claim_http(now, permit), Ok(Claim::Released(_)))
}

/// R8: a Mutation command's first write is released and unanswered; the
/// Session is retired before its second bound write releases. The fence fails
/// the second write NotSent, and the command terminal still projects Unknown.
/// A same-leg retry or resend cannot follow an unanswered write (only a
/// definitive `RefusedResend` answer grants one), so the second write is a
/// sibling leg of the same command.
#[test]
fn r8_retire_after_an_unanswered_released_write_projects_unknown() {
    let (mut e, shared) = engine(|shared| {
        shared.write_legs = 2;
        shared.next_generation_token = Some("C");
    });
    let issued = amend(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let mut first = None;
    let mut second = None;
    for (preparation, request, intent) in issued {
        assert_eq!(intent, RoutingIntent::Primary);
        let packet = prepared(&mut e, NOW + 1., preparation).expect("bound write");
        assert_eq!(authorization(&packet.1), Some("Bearer A"));
        if request.url.ends_with("/write-0") {
            first = Some(packet.0);
        } else {
            second = Some(packet.0);
        }
    }
    assert!(released(&mut e, NOW + 2., first.expect("first write")));
    e.drain_actions().for_each(drop);
    retire(&mut e, &shared, NOW + 3.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let fenced = &shared.borrow().failures;
    assert_eq!(fenced.len(), 1);
    assert_eq!(fenced[0].outcome, Some(Outcome::NotSent), "the attempt");
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one command terminal: {terminals:?}");
    let (command, error) = &terminals[0];
    assert_eq!(*command, 1);
    assert_eq!(
        error.as_ref().and_then(|error| error.outcome),
        Some(Outcome::Unknown),
        "a released, unanswered write was relabelled by the Session retire"
    );
    assert!(!released(&mut e, NOW + 4., second.expect("second write")));
}

/// R8 control: the same writes with none released project NotSent, so R8's
/// Unknown comes from the released write.
#[test]
fn control_r8_fenced_writes_with_none_released_project_not_sent() {
    let (mut e, shared) = engine(|shared| {
        shared.write_legs = 2;
        shared.next_generation_token = Some("C");
    });
    let issued = amend(&mut e, 1);
    assert_eq!(issued.len(), 2);
    let mut first = None;
    let mut second = None;
    for (preparation, request, intent) in issued {
        assert_eq!(intent, RoutingIntent::Primary);
        let packet = prepared(&mut e, NOW + 1., preparation).expect("bound write");
        assert_eq!(authorization(&packet.1), Some("Bearer A"));
        if request.url.ends_with("/write-0") {
            first = Some(packet.0);
        } else {
            second = Some(packet.0);
        }
    }
    let first = first.expect("first write");
    retire(&mut e, &shared, NOW + 3.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let fenced = &shared.borrow().failures;
    assert!(!fenced.is_empty());
    assert_eq!(fenced[0].outcome, Some(Outcome::NotSent), "the attempt");
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one command terminal: {terminals:?}");
    let (command, error) = &terminals[0];
    assert_eq!(*command, 1);
    assert_eq!(
        error.as_ref().and_then(|error| error.outcome),
        Some(Outcome::NotSent),
        "nothing was released, so the command is NotSent"
    );
    assert!(!released(&mut e, NOW + 4., first));
    assert!(!released(&mut e, NOW + 4., second.expect("second write")));
}

/// Guard, not R8: the first write was answered (a definitive `RefusedResend`),
/// so nothing is unanswered. The resend inherits the binding and is fenced
/// before release. The Unknown terminal is `failure_projection`'s existing
/// conservative rule (released then unreleased on one leg); the Bearer adds
/// nothing to it.
#[test]
fn guard_fenced_resend_after_a_refusal_keeps_the_conservative_unknown() {
    let (mut e, shared) = engine(|shared| {
        shared.write_legs = 1;
        shared.resend = true;
        shared.next_generation_token = Some("C");
    });
    let issued = amend(&mut e, 1);
    let (permit, request) = prepared(&mut e, NOW + 1., issued[0].0).expect("bound write");
    assert!(released(&mut e, NOW + 2., permit));
    e.handle(
        NOW + 3.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    let resent = prepares(&mut e);
    assert_eq!(resent.len(), 1, "the refused write was resent");
    assert_eq!(resent[0].2, RoutingIntent::Primary);
    let (permit, request) = prepared(&mut e, NOW + 4., resent[0].0).expect("bound resend");
    assert!(request.url.ends_with("/resend"));
    assert_eq!(authorization(&request), Some("Bearer A"));
    retire(&mut e, &shared, NOW + 5.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(shared.borrow().failures.len(), 1);
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one command terminal: {terminals:?}");
    assert_eq!(
        terminals[0].1.as_ref().and_then(|error| error.outcome),
        Some(Outcome::Unknown)
    );
    assert!(!released(&mut e, NOW + 6., permit));
}

/// R9 guard: the Plan's root expiry and the Session retire land in the same
/// event. The Plan fails as expired, never relabelled by the retire.
#[cfg(feature = "http-plans")]
#[test]
fn r9_plan_expiry_and_retire_in_one_event_fail_the_plan_as_expired() {
    let (mut e, shared) = engine(|shared| {
        shared.plan_command = Some(1);
        shared.plan_legs = 1;
        shared.next_generation_token = Some("C");
    });
    let legs = ask(&mut e, 1);
    assert_eq!(legs.len(), 1);
    let (permit, request) = prepared(&mut e, NOW + 1., legs[0].0).expect("bound leg");
    assert_eq!(authorization(&request), Some("Bearer A"));
    let deadline = shared.borrow().plan.as_ref().expect("plan").deadline();
    retire(&mut e, &shared, deadline);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let terminals = terminals(&actions);
    assert_eq!(terminals.len(), 1, "one Plan terminal: {terminals:?}");
    let error = terminals[0].1.as_ref().expect("the Plan failed");
    assert_eq!(terminals[0].0, 1);
    assert_eq!(
        error.kind,
        Some(ErrorKind::Timeout),
        "the retire relabelled an expired Plan: {error:?}"
    );
    assert!(!released(&mut e, deadline + 1., permit));
    assert!(terminals_after(&mut e).is_empty());
}

#[cfg(feature = "http-plans")]
fn terminals_after(e: &mut Engine<Probe>) -> Vec<(u32, Option<VenueError>)> {
    terminals(&e.drain_actions().collect::<Vec<_>>())
}

/// R10: one Plan with two bound unreleased legs and one plain Command with a
/// bound request, all on one Session. The retire fails each owner once, sends
/// nothing, and never nests a venue callback.
#[cfg(feature = "http-plans")]
#[test]
fn r10_retire_fails_each_owner_once_without_http_or_reentry() {
    let (mut e, shared) = engine(|shared| {
        shared.plan_command = Some(1);
        shared.plan_legs = 2;
        shared.next_generation_token = Some("C");
    });
    let legs = ask(&mut e, 1);
    assert_eq!(legs.len(), 2, "two peer Plan legs");
    let command = ask(&mut e, 2);
    assert_eq!(command.len(), 1);
    let permits = legs
        .iter()
        .chain(&command)
        .map(|(preparation, ..)| {
            let (permit, request) = prepared(&mut e, NOW + 1., *preparation).expect("bound packet");
            assert_eq!(authorization(&request), Some("Bearer A"));
            permit
        })
        .collect::<Vec<_>>();
    shared.borrow_mut().max_depth = 0;
    retire(&mut e, &shared, NOW + 2.);
    let mut actions = e.drain_actions().collect::<Vec<_>>();
    for permit in permits {
        assert!(!released(&mut e, NOW + 3., permit));
    }
    actions.extend(e.drain_actions());
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "a fenced packet released"
    );
    let mut owners = terminals(&actions)
        .into_iter()
        .map(|(command, error)| {
            let error = error.expect("each owner fails");
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            command
        })
        .collect::<Vec<_>>();
    owners.sort_unstable();
    assert_eq!(owners, [1, 2], "one terminal per owner");
    assert_eq!(
        shared.borrow().max_depth,
        1,
        "a venue callback ran inside another"
    );
}

/// R14, refusal half: inside a Plan-ready scope, opening, using or retiring a
/// Session refuses `State`; the Session stays live and later work still binds.
#[cfg(feature = "http-plans")]
#[test]
fn r14_plan_ready_scope_refuses_session_lifetimes_and_keeps_the_session() {
    let (mut e, shared) = engine(|shared| {
        shared.plan_command = Some(1);
        shared.plan_legs = 1;
        shared.plan_wait_ms = 1_000;
        shared.lifetimes_on_plan_ready = true;
    });
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 1 }));
    let admitted = e.drain_actions().collect::<Vec<_>>();
    let timer = admitted
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1_000. => Some(*timer),
            _ => None,
        })
        .expect("the Plan's readiness timer");
    e.handle(NOW + 1_000., Event::TimerFired { timer });
    {
        let shared = shared.borrow();
        assert_eq!(shared.plan_ready_calls, 1);
        assert_eq!(shared.scoped_refusals, [Error::State; 3]);
        assert!(shared.retired.is_none() && shared.session.is_some());
        assert_eq!(shared.issue_errors, []);
    }
    let issued = ask(&mut e, 2);
    assert_eq!(issued[0].2, RoutingIntent::Primary);
    let (_, request) = prepared(&mut e, NOW + 1_001., issued[0].0).expect("bound packet");
    assert_eq!(authorization(&request), Some("Bearer A"));
}

/// Source gate: `start` runs no `finish_event_scope`, so a retire there is
/// fenced by the `flush_http_plans` tail before `start` returns.
#[test]
fn gate_retire_in_start_is_fenced_before_start_returns() {
    let mut probe = Shared::new();
    probe.retire_in_start = true;
    let shared = Rc::new(RefCell::new(probe));
    let mut e = support::seeded_engine(Probe {
        shared: shared.clone(),
    });
    let health = e.health().clone();
    e.start(NOW);
    assert!(
        shared.borrow().retired.is_some(),
        "start retired its Session"
    );
    assert_eq!(shared.borrow().issue_errors, []);
    assert!(
        one_typed_read_failure(&shared, ErrorKind::InvalidInput),
        "the fenced Background read failed inside start"
    );
    let left = e.drain_actions().collect::<Vec<_>>();
    let managed = left
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => Some(&**a),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [
        HttpAction::Prepare { preparation, .. },
        HttpAction::Cancel {
            preparation: cancelled,
            ..
        },
    ] = managed[..]
    else {
        panic!(
            "expected the queued Prepare and its Cancel, got {}",
            managed.len()
        );
    };
    assert_eq!(preparation, cancelled);
    assert!(route(&mut e, NOW + 1., *preparation).is_empty());
    assert_eq!(e.health(), &health);
}
