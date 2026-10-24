mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{PrimaryReason, SlotPolicy, Slots};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, HttpAction, Owner, Prepared, RetryEvidence, RetryPolicy,
    RoutingIntent, Scope,
};
use safe_cex_engine::selected_read::{
    AdmittedRead, BackgroundReadError, CommandReadError, LocalRefusal, ReadPolicy, ReadRequest,
    ReadRole, SelectedRead,
};
use safe_cex_engine::signing::{
    Identity, KeyRef, Operation as SignOperation, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, Command, CredentialSlot, Cx, DispatchContext, Engine,
    ErrorKind, Event, Method, Notice, Response, SignStep, SocketId, Store, TimerId, Venue,
    VenueError,
};
use std::sync::{Arc, Mutex};

const NOW: f64 = 1_000.25;
const HEADERS: &[(&str, &str)] = &[("X-Demo", "1"), ("Content-Type", "application/json")];
fn identity(slot: &str) -> Identity {
    Identity {
        credential: KeyRef(format!("ref:{slot}:credential")),
        signing: KeyRef(format!("ref:{slot}:signing")),
        passphrase: Some(KeyRef(format!("ref:{slot}:passphrase"))),
        stable_id: Some("account".into()),
        extra: Default::default(),
    }
}

struct Recipe {
    identity: Identity,
    invalid: bool,
}
impl SignStep for Recipe {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.invalid {
            &[("Bad\nName", "value")]
        } else {
            HEADERS
        }
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &self.identity,
            Vec::new(),
            Default::default(),
            SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        request.headers.push((
            "X-Selected",
            String::from_utf8(result.values[0].clone()).unwrap(),
        ));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

#[derive(Default)]
struct Observed {
    contexts: Vec<Context>,
    identities: Vec<Identity>,
    entry: Vec<CommandEntryError>,
    queue: Vec<(Error, u8, Context)>,
    retry: Vec<(Error, u8, Context)>,
    local: Vec<Result<(), Error>>,
    background_missing: usize,
    scoped_refusals: Vec<BackgroundReadError>,
    response_markers: Vec<u8>,
}
struct Config {
    primary: bool,
    secondary: bool,
    force_primary: bool,
    background: bool,
    command_from_start: bool,
    hold: bool,
    invalid_headers: bool,
    diagnostic: bool,
    timeout: u32,
    retry: RetryPolicy,
    retry_failure: bool,
    method: Method,
    stream: bool,
    /// Issue under the delivered command's existing scope, not an entry root.
    scoped: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            primary: true,
            secondary: true,
            force_primary: false,
            background: false,
            command_from_start: false,
            hold: false,
            invalid_headers: false,
            diagnostic: false,
            timeout: 20_000,
            retry: RetryPolicy::Never,
            retry_failure: false,
            method: Method::Get,
            stream: false,
            scoped: false,
        }
    }
}
struct Probe {
    config: Config,
    slots: Slots<Identity>,
    held: Vec<AdmittedRead>,
    observed: Arc<Mutex<Observed>>,
}
struct Pending {
    read: AdmittedRead,
    marker: u8,
}
impl Probe {
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, background: bool) {
        self.issue_for(cx, background, None);
    }
    fn issue_for(&mut self, cx: &mut Cx<'_, Pending>, background: bool, command: Option<u32>) {
        let role = if self.config.force_primary {
            ReadRole::Primary {
                reason: PrimaryReason::DegradedRead,
                scope: Some(AccountReadScope::Balance),
            }
        } else {
            ReadRole::Read {
                scope: Some(AccountReadScope::Balance),
            }
        };
        let selected = SelectedRead::select(&self.slots, role);
        let policy = ReadPolicy {
            timeout_ms: self.config.timeout,
            pacing: None,
            retry: self.config.retry,
        };
        let read = if let Some(command) = command.filter(|_| self.config.scoped) {
            match SelectedRead::scoped(cx, selected, Scope::Command(command), policy) {
                Ok(read) => read,
                Err(refusal) => {
                    self.observed.lock().unwrap().scoped_refusals.push(refusal);
                    return;
                }
            }
        } else if background {
            match SelectedRead::background(cx, selected, policy) {
                Ok(read) => read,
                Err(BackgroundReadError::Selection(_)) => {
                    self.observed.lock().unwrap().background_missing += 1;
                    return;
                }
                Err(BackgroundReadError::Admission(e)) => panic!("unexpected background {e:?}"),
            }
        } else {
            match SelectedRead::command(
                cx,
                selected,
                policy,
                LocalRefusal {
                    diagnostic: self.config.diagnostic.then(|| "native diagnostic".into()),
                    error: VenueError::new("native credential refusal")
                        .kind(ErrorKind::AuthInvalid),
                },
            ) {
                Ok(read) => read,
                Err(CommandReadError::Entry(error)) => {
                    self.observed.lock().unwrap().entry.push(error);
                    return;
                }
                Err(CommandReadError::LocalRefusal { terminal, .. }) => {
                    self.observed.lock().unwrap().local.push(terminal);
                    return;
                }
            }
        };
        self.observed
            .lock()
            .unwrap()
            .contexts
            .push(read.context().clone());
        if self.config.hold {
            self.held.push(read);
        } else {
            self.queue(cx, read, 73);
        }
    }
    fn queue(&mut self, cx: &mut Cx<'_, Pending>, read: AdmittedRead, marker: u8) {
        let observed = self.observed.clone();
        let invalid = self.config.invalid_headers;
        let result = read.queue(
            cx,
            ReadRequest {
                method: self.config.method,
                url: "https://fixture/read".into(),
                body: None,
                stream: self
                    .config
                    .stream
                    .then_some(safe_cex_engine::json::ArrayAt::Field("rows")),
            },
            move |identity| {
                observed.lock().unwrap().identities.push(identity.clone());
                Box::new(Recipe {
                    identity: identity.clone(),
                    invalid,
                })
            },
            |read| Pending { read, marker },
        );
        if let Err((pending, error)) = result {
            self.observed.lock().unwrap().queue.push((
                error,
                pending.marker,
                pending.read.context().clone(),
            ));
            self.reject(cx, pending, error.venue_error());
        }
    }
    fn reject(&self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        if matches!(pending.read.context().owner(), Owner::Command { .. }) {
            cx.fail_context(pending.read.context(), error).unwrap();
        } else {
            cx.finish_http_context(pending.read.context()).unwrap();
        }
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.config.background || self.config.command_from_start {
            self.issue(cx, self.config.background);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let id = match command {
            Command::FetchAccount { command } => Some(command),
            _ => None,
        };
        self.issue_for(cx, false, id);
    }
    fn on_item(&mut self, _: &mut Cx<'_, Pending>, pending: &mut Pending, _: &str) {
        pending.marker += 1;
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.observed
            .lock()
            .unwrap()
            .response_markers
            .push(pending.marker);
        if matches!(pending.read.context().owner(), Owner::Command { .. }) {
            cx.validate_read_http_delivery(pending.read.context(), pending.read.context())
                .unwrap();
            cx.done_context(pending.read.context(), "[]".into())
                .unwrap();
        } else {
            cx.finish_http_context(pending.read.context()).unwrap();
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if !self.config.retry_failure {
            self.reject(cx, pending, VenueError::new(error));
            return;
        }
        let Pending { read, marker } = pending;
        match read.retry(cx, RetryEvidence::ReadFailure, marker) {
            Ok((read, marker)) => self.queue(cx, read, marker),
            Err(refusal) => {
                self.observed.lock().unwrap().retry.push((
                    refusal.error,
                    refusal.pending,
                    refusal.read.context().clone(),
                ));
                self.reject(
                    cx,
                    Pending {
                        read: refusal.read,
                        marker: refusal.pending,
                    },
                    refusal.error.venue_error(),
                );
            }
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.reject(cx, pending, error);
    }
    fn account_auth_outcome(
        &self,
        _: &Pending,
        _: &Response<'_>,
        _: f64,
        _: &Store,
    ) -> Option<AuthOutcome> {
        Some(AuthOutcome::Accepted)
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(config: Config) -> (Engine<Probe>, Arc<Mutex<Observed>>) {
    let slots = Slots::new(
        config.primary.then(|| identity("primary")),
        config.secondary.then(|| identity("secondary")),
        SlotPolicy::default(),
    );
    let observed = Arc::new(Mutex::new(Observed::default()));
    (
        support::seeded_engine(Probe {
            config,
            slots,
            held: Vec::new(),
            observed: observed.clone(),
        }),
        observed,
    )
}
fn ask(e: &mut Engine<Probe>, id: u32) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: id }));
    e.drain_actions().collect()
}
fn prepare(
    actions: Vec<Action>,
) -> (
    safe_cex_engine::request_context::Preparation,
    safe_cex_engine::HttpRequest,
    RoutingIntent,
) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::Http(_)))
    );
    actions
        .into_iter()
        .find_map(|a| match a {
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
        .expect("genuine preparation")
}
fn release(e: &mut Engine<Probe>, actions: Vec<Action>) -> safe_cex_engine::RequestId {
    let (preparation, _, _) = prepare(actions);
    e.http_prepared(
        NOW + 1.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let (attempt, work) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert!(matches!(work.operations[0], SignOperation::Present { .. }));
    e.handle(
        NOW + 2.,
        Event::Signed {
            attempt,
            result: Ok(SignResult {
                values: vec![b"remote-key".to_vec(), vec![7; 32]],
                receipt: work.receipt,
                headers: Vec::new(),
            }),
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(e.claim_http(NOW + 2., permit).is_ok());
    request.id
}

#[test]
fn read_role_selects_identity_egress_and_ordered_headers_without_method_selection() {
    for method in [Method::Get, Method::Post] {
        for primary in [false, true] {
            let (mut e, observed) = engine(Config {
                method,
                force_primary: primary,
                ..Config::default()
            });
            let (_, request, intent) = prepare(ask(&mut e, 7));
            assert_eq!(
                intent,
                if primary {
                    RoutingIntent::Primary
                } else {
                    RoutingIntent::Secondary
                }
            );
            assert_eq!(request.method, method);
            assert_eq!(
                request.headers,
                HEADERS
                    .iter()
                    .map(|&(k, v)| (k, v.into()))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                observed.lock().unwrap().identities[0],
                identity(if primary { "primary" } else { "secondary" })
            );
        }
    }
}
#[test]
fn genuine_entry_capacity_preserves_exact_error_and_emits_one_terminal() {
    let (mut e, observed) = engine(Config {
        hold: true,
        ..Config::default()
    });
    for id in 1..=256 {
        assert!(ask(&mut e, id).is_empty());
    }
    let actions = ask(&mut e, 257);
    let mut expected = Error::Capacity.venue_error();
    expected.outcome = None;
    assert_eq!(
        actions,
        vec![Action::Notice(Notice::CommandFailed {
            command: 257,
            error: expected
        })]
    );
    assert_eq!(
        observed.lock().unwrap().entry,
        vec![CommandEntryError::Rejected(Error::Capacity)]
    );
    assert_eq!(observed.lock().unwrap().contexts.len(), 256);
}
#[test]
fn duplicate_entry_does_not_queue_or_finalize_the_previous_owner() {
    let (mut e, observed) = engine(Config {
        hold: true,
        ..Config::default()
    });
    assert!(ask(&mut e, 7).is_empty());
    let duplicate = ask(&mut e, 7);
    assert!(!duplicate.iter().any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })
        || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))
        || matches!(a, Action::Notice(Notice::CommandDone { command:7,.. } | Notice::CommandFailed { command:7,.. }))));
    assert!(observed.lock().unwrap().entry.is_empty());
    assert_eq!(observed.lock().unwrap().contexts.len(), 1);
}
#[test]
fn local_missing_selection_preserves_optional_diagnostic_order_and_native_typed_error() {
    for diagnostic in [false, true] {
        let (mut e, observed) = engine(Config {
            primary: false,
            secondary: false,
            diagnostic,
            ..Config::default()
        });
        let actions = ask(&mut e, 7);
        let mut expected = Vec::new();
        if diagnostic {
            expected.push(Action::Notice(Notice::CommandError {
                command: 7,
                message: "native diagnostic".into(),
            }));
        }
        expected.push(Action::Notice(Notice::CommandFailed {
            command: 7,
            error: VenueError::new("native credential refusal").kind(ErrorKind::AuthInvalid),
        }));
        assert_eq!(actions, expected);
        assert_eq!(observed.lock().unwrap().local, vec![Ok(())]);
        assert!(observed.lock().unwrap().identities.is_empty());
    }
}
#[test]
fn background_missing_selection_creates_no_command_or_io() {
    let (mut e, observed) = engine(Config {
        primary: false,
        secondary: false,
        background: true,
        ..Config::default()
    });
    e.start(NOW);
    assert!(e.drain_actions().next().is_none());
    assert_eq!(observed.lock().unwrap().background_missing, 1);
}
#[test]
fn command_helper_outside_genuine_callback_is_revoked_without_work() {
    let (mut e, observed) = engine(Config {
        command_from_start: true,
        ..Config::default()
    });
    e.start(NOW);
    assert!(e.drain_actions().next().is_none());
    assert_eq!(
        observed.lock().unwrap().entry,
        vec![CommandEntryError::Revoked]
    );
}
#[test]
fn queue_refusal_returns_exact_pending_wrapper_and_error_for_typed_settlement() {
    let (mut e, observed) = engine(Config {
        invalid_headers: true,
        ..Config::default()
    });
    let actions = ask(&mut e, 7);
    assert_eq!(observed.lock().unwrap().queue.len(), 1);
    let state = observed.lock().unwrap();
    assert_eq!(
        state.queue[0],
        (Error::Invalid, 73, state.contexts[0].clone())
    );
    assert_eq!(
        actions,
        vec![Action::Notice(Notice::CommandFailed {
            command: 7,
            error: Error::Invalid.venue_error()
        })]
    );
}
#[test]
fn retry_forbidden_returns_original_wrapper_and_continuation_without_new_io() {
    let (mut e, observed) = engine(Config {
        retry_failure: true,
        ..Config::default()
    });
    let actions = ask(&mut e, 7);
    let request = release(&mut e, actions);
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request,
            error: "network",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let state = observed.lock().unwrap();
    assert_eq!(
        state.retry,
        vec![(Error::RetryForbidden, 73, state.contexts[0].clone())]
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
            .count(),
        1
    );
    assert!(!actions.iter().any(
        |a| matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{..}))
    ));
}
#[test]
fn allowed_retry_keeps_original_generation_identity_egress_and_fractional_deadline() {
    let (mut e, observed) = engine(Config {
        retry_failure: true,
        retry: RetryPolicy::ReadFailure,
        ..Config::default()
    });
    let actions = ask(&mut e, 7);
    let request = release(&mut e, actions);
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request,
            error: "network",
        },
    );
    let (_, request, intent) = prepare(e.drain_actions().collect());
    assert_eq!(intent, RoutingIntent::Secondary);
    assert_eq!(request.timeout_ms, Some(19_997));
    let state = observed.lock().unwrap();
    assert_eq!(
        state.identities,
        vec![identity("secondary"), identity("secondary")]
    );
    assert_eq!(state.contexts[0].deadline(), NOW + 20_000.);
}
#[test]
fn selected_account_evidence_is_tagged_to_the_same_slot_and_scope() {
    let (mut e, _) = engine(Config::default());
    let actions = ask(&mut e, 7);
    let request = release(&mut e, actions);
    e.handle(
        NOW + 3.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::AccountAuth {
            credential: CredentialSlot::Secondary,
            scope: AccountReadScope::Balance,
            outcome: AuthOutcome::Accepted
        })
    )));
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            .count(),
        1
    );
}

#[test]
fn provider_refusal_preserves_exact_typed_error_without_transport() {
    let (mut e, _) = engine(Config::default());
    let (preparation, _, _) = prepare(ask(&mut e, 7));
    e.http_prepared(
        NOW + 1.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let attempt = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, .. } => Some(attempt),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW + 2.,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![&SignError::Refused.venue_error()]
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })
        || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
}

#[test]
fn original_fractional_expiry_denies_delayed_preparation_and_cannot_complete_twice() {
    let (mut e, _) = engine(Config {
        timeout: 20,
        ..Config::default()
    });
    let (preparation, _, _) = prepare(ask(&mut e, 7));
    e.http_prepared(
        NOW + 20.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.kind),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![Some(ErrorKind::Timeout)]
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    e.http_prepared(
        NOW + 21.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandFailed { command: 7, .. } | Notice::CommandDone { command: 7, .. }
        )
    )));
}

#[test]
fn invalid_missing_selection_policy_rejects_before_native_diagnostic_or_second_terminal() {
    let (mut e, observed) = engine(Config {
        primary: false,
        secondary: false,
        diagnostic: true,
        timeout: 0,
        ..Config::default()
    });
    let actions = ask(&mut e, 7);
    let mut error = Error::Invalid.venue_error();
    error.outcome = None;
    assert_eq!(
        actions,
        vec![Action::Notice(Notice::CommandFailed { command: 7, error })]
    );
    assert_eq!(
        observed.lock().unwrap().entry,
        vec![CommandEntryError::Rejected(Error::Invalid)]
    );
    assert!(observed.lock().unwrap().local.is_empty());
}

#[test]
fn background_read_keeps_selected_identity_without_command_completion() {
    let (mut e, observed) = engine(Config {
        background: true,
        ..Config::default()
    });
    e.start(NOW);
    let actions = e.drain_actions().collect();
    let request = release(&mut e, actions);
    assert!(matches!(
        observed.lock().unwrap().contexts[0].owner(),
        Owner::Background { .. }
    ));
    e.handle(
        NOW + 3.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
}

#[test]
fn secondary_only_read_is_valid_but_primary_only_role_never_falls_back() {
    let (mut e, _) = engine(Config {
        primary: false,
        ..Config::default()
    });
    assert_eq!(prepare(ask(&mut e, 7)).2, RoutingIntent::Secondary);
    let (mut e, observed) = engine(Config {
        primary: false,
        force_primary: true,
        ..Config::default()
    });
    assert_eq!(
        ask(&mut e, 7),
        vec![Action::Notice(Notice::CommandFailed {
            command: 7,
            error: VenueError::new("native credential refusal").kind(ErrorKind::AuthInvalid),
        })]
    );
    assert_eq!(observed.lock().unwrap().local, vec![Ok(())]);
    assert!(observed.lock().unwrap().identities.is_empty());
}

#[test]
fn streaming_descriptor_registers_rows_on_the_same_native_pending_and_context() {
    let (mut e, observed) = engine(Config {
        stream: true,
        ..Config::default()
    });
    let actions = ask(&mut e, 7);
    let request = release(&mut e, actions);
    e.handle(
        NOW + 3.,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 3.,
        Event::HttpChunk {
            request,
            data: br#"{"rows":[{"id":1},{"id":2}]}"#,
        },
    );
    e.handle(NOW + 3., Event::HttpEnd { request });
    assert_eq!(observed.lock().unwrap().response_markers, vec![75]);
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            .count(),
        1
    );
}

/// R1 (#1992): a read issued under the delivered command's own scope (not an
/// entry root) carries the Selection's slot, identity and egress, and its
/// queued request is tagged with the Selection's account evidence.
#[test]
fn scoped_read_issues_under_the_command_with_the_selections_slot_and_evidence() {
    for (secondary, slot) in [
        (true, CredentialSlot::Secondary),
        (false, CredentialSlot::Primary),
    ] {
        let (mut e, observed) = engine(Config {
            scoped: true,
            secondary,
            ..Config::default()
        });
        let (_, _, intent) = prepare(ask(&mut e, 9));
        assert_eq!(
            intent,
            if secondary {
                RoutingIntent::Secondary
            } else {
                RoutingIntent::Primary
            }
        );
        let observed = observed.lock().unwrap();
        let [context] = &observed.contexts[..] else {
            panic!("one scoped context");
        };
        assert!(matches!(context.owner(), Owner::Command { id: 9, .. }));
        assert_eq!(context.credential().map(|c| c.slot), Some(slot));
        assert_eq!(
            observed.identities,
            vec![identity(if secondary { "secondary" } else { "primary" })]
        );
        assert!(observed.scoped_refusals.is_empty());
    }
}

/// R2 (#1992): a missing selection or a refused admission issues no context,
/// emits no HTTP and settles nothing; the caller keeps the refusal.
#[test]
fn scoped_read_refusals_issue_nothing_and_settle_nothing() {
    for (primary, secondary, timeout) in [(false, false, 20_000), (true, true, 0)] {
        let (mut e, observed) = engine(Config {
            scoped: true,
            primary,
            secondary,
            timeout,
            ..Config::default()
        });
        let actions = ask(&mut e, 9);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::ManagedHttp(_) | Action::Http(_) | Action::Sign { .. }
        )));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
        )));
        let observed = observed.lock().unwrap();
        assert!(observed.contexts.is_empty());
        let [refusal] = &observed.scoped_refusals[..] else {
            panic!("one scoped refusal");
        };
        if timeout == 0 {
            assert!(matches!(
                refusal,
                BackgroundReadError::Admission(Error::Invalid)
            ));
        } else {
            assert!(matches!(refusal, BackgroundReadError::Selection(_)));
        }
    }
}
