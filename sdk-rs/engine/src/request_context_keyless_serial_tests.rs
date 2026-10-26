// Keyless serial originals: the entry stub is intentionally inert. Suffixes
// after the entry assertion are source gates until genuine admission exists.
#[allow(clippy::result_large_err)]
mod keyless_serial {
    use super::*;
    use crate::book::Book;
    use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
    use crate::request_context::{
        Plan, PreflightPolicy, PreflightPurpose, SecretHeaderValue, Session, SessionBearer,
        SessionHeaders, SessionPolicy, SessionResend,
        plan::{CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound},
    };

    const CANCEL: &str = r#"{"order_id":"one"}"#;
    const LEVERAGE: &str = r#"{"leverage":3}"#;
    const TABLE: CodeTable = CodeTable {
        rows: &[Row {
            key: Key::Int(1000),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        }],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    };
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Mode {
        #[default]
        Chain,
        Ordinary,
        Retired,
        Foreign,
        ReadOnly,
        ReadKey,
        Mixed,
        MixedAtEntry,
        Complete,
        EmptyDone,
        EmptyFail,
        RetireBetween,
        WrongBearer,
        NoBearer,
        Signed,
        Read,
        Auth,
        ReadAuth,
        AuthChangedBody,
        SignedResend,
        #[cfg(feature = "sign-meta")]
        Selected,
        ReadWithSecondary,
        NoSessionEgress,
        CredentialSession,
        Forbidden,
        LegacyAttach,
        #[cfg(feature = "condition-waits")]
        ParkRetire,
    }
    enum Pending {
        Login(Context),
        Call(Context),
        Later,
    }
    #[derive(Default)]
    struct Probe {
        mode: Mode,
        session: Option<Session>,
        foreign: Option<Session>,
        revision: u64,
        timer: Option<TimerId>,
        plan: Option<Plan>,
        expiry_plan: Option<Plan>,
        retirements: usize,
        slot: Option<Slot>,
        entry: Option<Result<(), Error>>,
        admitted: Option<Result<(), Error>>,
        selection: Option<Result<(), Error>>,
        issue: Option<Result<(), Error>>,
        before: Option<crate::resource_ids::ResourceIds>,
        after: Option<crate::resource_ids::ResourceIds>,
        calls: usize,
        selected: usize,
        cleanup: bool,
        preflight: Option<crate::request_context::Preflight>,
        errors: Vec<VenueError>,
        read_fact_absent: bool,
        legacy_unchanged: bool,
    }
    fn credential() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("read-ref-must-not-fall-back".into()),
        }
    }
    fn call(operation: Operation) -> SerialCall {
        SerialCall {
            auth: Default::default(),
            operation,
            timeout_ms: 20_000,
            attempts: 2,
        }
    }
    impl Probe {
        fn issue(&mut self, cx: &mut Cx<'_, Pending>, context: Context) {
            assert!(
                context.credential().is_none(),
                "keyless context cannot sign"
            );
            if self.mode == Mode::ReadWithSecondary {
                let generation = cx
                    .io
                    .core
                    .request_contexts
                    .validate_session(self.session.as_ref().unwrap())
                    .unwrap();
                assert!(
                    matches!(context.egress(), Some(crate::request_context::Egress::Session(egress)) if egress.generation() == generation),
                    "actual Read Context keeps the root Session egress"
                );
            }
            let session = if self.mode == Mode::NoBearer {
                None
            } else if self.mode == Mode::WrongBearer {
                self.foreign.as_ref()
            } else {
                self.session.as_ref()
            };
            let url = if self.calls == 0 {
                "https://venue.test/cancel"
            } else {
                "https://venue.test/leverage"
            };
            let body = if self.mode == Mode::AuthChangedBody && self.selected > 0 {
                LEVERAGE
            } else if self.calls == 0 {
                CANCEL
            } else {
                LEVERAGE
            };
            self.before = Some(cx.io.core.next_id);
            #[cfg(feature = "sign-meta")]
            if self.mode == Mode::Selected {
                let slots = crate::credential_selection::Slots::new(
                    None,
                    cx.io.core.identities.secondary.clone(),
                    Default::default(),
                );
                let (selection, _) = slots
                    .select(crate::credential_selection::CredentialRole::Read { scope: None })
                    .unwrap();
                assert_eq!(selection.slot(), crate::CredentialSlot::Secondary);
                let result = cx.request_selected_managed(
                    context.clone(),
                    &selection,
                    session,
                    Method::Post,
                    url.into(),
                    Some(body.into()),
                    Box::new(ForbiddenRecipe),
                    Pending::Call(context),
                );
                self.after = Some(cx.io.core.next_id);
                self.issue = Some(result.map(drop).map_err(|(_, e)| e));
                return;
            }
            let result = if matches!(self.mode, Mode::Signed | Mode::Forbidden)
                || (self.mode == Mode::SignedResend && self.selected > 0)
            {
                cx.request_signed_managed(
                    context.clone(),
                    session,
                    Method::Post,
                    url.into(),
                    Some(body.into()),
                    if matches!(self.mode, Mode::Forbidden | Mode::SignedResend) {
                        Box::new(ForbiddenRecipe)
                    } else {
                        Box::new(Recipe)
                    },
                    Pending::Call(context),
                )
            } else {
                cx.request_managed(
                    context.clone(),
                    session,
                    Method::Post,
                    url.into(),
                    Some(body.into()),
                    Pending::Call(context),
                )
            };
            if self.mode == Mode::LegacyAttach
                && let Ok(id) = &result
            {
                let before = (cx.io.core.next_id, cx.io.core.actions.len());
                cx.sign_http(*id, Box::new(ForbiddenRecipe));
                self.legacy_unchanged = before == (cx.io.core.next_id, cx.io.core.actions.len());
            }
            self.after = Some(cx.io.core.next_id);
            self.issue = Some(result.map(drop).map_err(|(_, e)| e));
        }
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            let intent =
                crate::signing::SetupFacts::new(crate::signing::KeyPresence::Empty).primary_login();
            let slots = crate::credential_selection::Slots::new(intent, None, Default::default());
            let (selection, intent) = slots
                .select(crate::credential_selection::CredentialRole::SessionAuth)
                .unwrap();
            assert_eq!(*intent, crate::signing::PrimaryLogin::Empty);
            assert!(cx.io.core.identities.primary.is_none());
            let session = cx
                .open_session(SessionPolicy {
                    credential: (self.mode == Mode::CredentialSession).then(credential),
                    repeat_safe: true,
                    private_egress: (!matches!(
                        self.mode,
                        Mode::NoSessionEgress | Mode::CredentialSession
                    ))
                    .then(|| selection.session_egress())
                    .flatten(),
                })
                .unwrap();
            if self.mode == Mode::CredentialSession {
                self.session = Some(session);
                return;
            }
            let context = cx
                .session_context(
                    &session,
                    Policy {
                        operation: Operation::Mutation,
                        credential: (self.mode == Mode::CredentialSession).then(credential),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: crate::request_context::RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/login".into(),
                Some(r#"{"api_key":""}"#.into()),
                Pending::Login(context),
            )
            .map_err(|(_, e)| e)
            .unwrap();
            self.session = Some(session);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if command.completion_id() == Some(9) {
                let preflight = cx
                    .begin_preflight(PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: Some(credential()),
                    })
                    .unwrap();
                let (plan, _) = cx
                    .admit_serial_plan(
                        &preflight,
                        call(Operation::Mutation),
                        SerialPolicy {
                            credential: Some(credential()),
                            pacing: None,
                            refusals_complete: true,
                        },
                    )
                    .unwrap();
                self.expiry_plan = Some(plan);
                return;
            }
            if command.completion_id() == Some(8) {
                let context = cx
                    .admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: Some(crate::request_context::Credential {
                            slot: crate::CredentialSlot::Primary,
                            reference: crate::signing::KeyRef("fixture".into()),
                        }),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: crate::request_context::RetryPolicy::Never,
                    })
                    .unwrap();
                cx.request_signed_managed(
                    context,
                    None,
                    Method::Post,
                    "https://venue.test/later".into(),
                    None,
                    Box::new(Recipe),
                    Pending::Later,
                )
                .map_err(|(_, error)| error)
                .unwrap();
                return;
            }
            if self.mode == Mode::Retired {
                cx.retire_session(self.session.as_ref().unwrap()).unwrap();
            }
            let policy = PreflightPolicy {
                purpose: if self.mode == Mode::ReadOnly {
                    PreflightPurpose::ReadOnly
                } else {
                    PreflightPurpose::WriteAfterReads
                },
                read_credential: (self.mode == Mode::ReadKey).then(credential),
                write_credential: (self.mode == Mode::MixedAtEntry).then(credential),
            };
            self.before = Some(cx.io.core.next_id);
            let result = if self.mode == Mode::Ordinary {
                cx.begin_preflight(policy)
            } else {
                cx.begin_preflight_with_session(
                    policy,
                    if self.mode == Mode::Foreign {
                        self.foreign.as_ref().unwrap()
                    } else {
                        self.session.as_ref().unwrap()
                    },
                )
            };
            self.after = Some(cx.io.core.next_id);
            self.entry = Some(result.as_ref().map(drop).map_err(|e| match e {
                crate::request_context::CommandEntryError::Rejected(reason) => *reason,
                crate::request_context::CommandEntryError::Revoked => Error::Revoked,
            }));
            let Ok(preflight) = result else {
                return;
            };
            self.preflight = Some(preflight.clone());
            #[cfg(feature = "condition-waits")]
            if self.mode == Mode::ParkRetire {
                cx.park_preflight_for(&preflight, None).unwrap();
                self.timer = Some(cx.set_timer(1.));
                return;
            }
            if self.mode == Mode::EmptyDone {
                self.selection = Some(cx.done_empty_plan(&preflight, "[]".into()));
                self.cleanup = true;
                return;
            }
            if self.mode == Mode::EmptyFail {
                self.selection = Some(cx.fail_preflight(&preflight, "no work"));
                self.cleanup = true;
                return;
            }
            if self.mode == Mode::Complete {
                let calls = [CallBound {
                    timeout_ms: 20_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                }];
                let steps = [StepBound {
                    wait_ms: 0,
                    calls: &calls,
                }];
                let policy = crate::request_context::PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: None,
                    pacing: None,
                    retry: crate::request_context::RetryPolicy::Never,
                };
                let before = cx.io.core.next_id;
                self.selection = Some(cx.admit_plan(&preflight, &steps, policy.clone()).map(drop));
                assert_eq!(self.selection, Some(Err(Error::Invalid)));
                assert_eq!(cx.io.core.next_id, before);
                assert_eq!(
                    cx.admit_physical_plan(&preflight, &steps, policy.clone())
                        .map(drop),
                    Err(Error::Invalid),
                    "owned physical Plan route"
                );
                assert_eq!(cx.io.core.next_id, before);
                #[cfg(feature = "http-mutation-barriers")]
                {
                    assert_eq!(
                        cx.admit_plan_with_barriers(
                            &preflight,
                            &steps,
                            policy,
                            &[crate::request_context::plan::MutationBarrierBound { step: 0 }]
                        )
                        .map(drop),
                        Err(Error::Invalid)
                    );
                    assert_eq!(cx.io.core.next_id, before);
                }
            }
            let operation = match self.mode {
                Mode::Read | Mode::ReadAuth | Mode::ReadWithSecondary => Operation::Read,
                #[cfg(feature = "sign-meta")]
                Mode::Selected => Operation::Read,
                _ => Operation::Mutation,
            };
            let result = cx.admit_serial_plan(
                &preflight,
                call(operation),
                SerialPolicy {
                    credential: (self.mode == Mode::Mixed).then(credential),
                    pacing: None,
                    refusals_complete: true,
                },
            );
            self.admitted = Some(result.as_ref().map(drop).map_err(|e| *e));
            if let Ok((plan, slot)) = result {
                let context = cx.plan_leg_context(&plan, &slot).unwrap();
                self.plan = Some(plan);
                self.slot = Some(slot);
                self.issue(cx, context);
            }
        }
        fn session_bearer(&self, generation: u64) -> SessionBearer {
            if self
                .session
                .as_ref()
                .is_some_and(|s| s.owner() == crate::request_context::Owner::Session { generation })
                && self.revision > 0
            {
                SessionBearer::Ready {
                    headers: SessionHeaders::new(vec![(
                        "Cookie",
                        SecretHeaderValue::new(format!("cookie-{}", self.revision)).unwrap(),
                    )])
                    .unwrap(),
                    revision: self.revision,
                }
            } else {
                SessionBearer::NotReady
            }
        }
        fn session_bearer_revision(&self, generation: u64) -> Option<u64> {
            self.session
                .as_ref()
                .filter(|s| s.owner() == crate::request_context::Owner::Session { generation })
                .map(|_| self.revision)
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            response: Response<'_>,
        ) {
            let context = match pending {
                Pending::Login(context) => {
                    assert_eq!(response.body, "login-ok");
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::Accepted { order_ids: vec![] },
                    )
                    .unwrap();
                    cx.finish_http_context(&context).unwrap();
                    self.revision = 1;
                    assert!(cx.session_ready(self.session.as_ref().unwrap()).is_empty());
                    return;
                }
                Pending::Call(context) => context,
                Pending::Later => panic!("later signing proof stops at actual provider staging"),
            };
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            if response.body == "auth" {
                let evidence = if context.operation() == Operation::Read {
                    let request = cx.io.core.http_delivery.as_ref().unwrap().attempt.request;
                    self.read_fact_absent = !cx
                        .io
                        .core
                        .request_contexts
                        .command_writes(7)
                        .unwrap()
                        .holds_attempt(request);
                    Ok(SessionResend::Read)
                } else {
                    let (_, grant) = cx
                        .http_write_failure(
                            &context,
                            &TABLE,
                            Some(&Native {
                                code: NativeCode::Int(1000),
                                message: "auth",
                            }),
                            "auth",
                            None,
                        )
                        .unwrap();
                    grant.map(SessionResend::Write).ok_or(Error::RetryForbidden)
                };
                self.before = Some(cx.io.core.next_id);
                let next = evidence.and_then(|evidence| {
                    cx.serial_plan_session_resend(&plan, &slot, &context, evidence)
                });
                self.after = Some(cx.io.core.next_id);
                self.selected += 1;
                self.selection = Some(next.as_ref().map(drop).map_err(|e| *e));
                if let Ok(next) = next {
                    self.issue(cx, next);
                    self.timer = Some(cx.set_timer(1.));
                } else {
                    cx.fail_plan(&plan, "authentication attempts exhausted")
                        .unwrap();
                }
                return;
            }
            if context.operation() == Operation::Mutation {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Accepted { order_ids: vec![] },
                )
                .unwrap();
            }
            self.calls += 1;
            if self.calls == 1 {
                if self.mode == Mode::RetireBetween {
                    cx.retire_session(self.session.as_ref().unwrap()).unwrap();
                }
                self.before = Some(cx.io.core.next_id);
                let next =
                    cx.continue_serial_plan(&plan, &slot, &context, call(Operation::Mutation));
                self.after = Some(cx.io.core.next_id);
                self.selection = Some(next.as_ref().map(drop).map_err(|e| *e));
                if let Ok(slot) = next {
                    self.slot = Some(slot.clone());
                    let context = cx.plan_leg_context(&plan, &slot).unwrap();
                    self.issue(cx, context);
                }
            } else {
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "[]".into()).unwrap();
            }
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            reason: Error,
            error: VenueError,
        ) {
            assert!(matches!(pending, Pending::Call(_)));
            self.issue = Some(Err(reason));
            self.errors.push(error.clone());
            cx.fail_plan(self.plan.as_ref().unwrap(), error).unwrap();
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
            panic!("unexpected transport failure");
        }
        fn capture_http_reconciliation(
            &self,
            _: &Pending,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("keyless-serial".into())
        }
        fn reconcile_uncertain(&mut self, _: &mut Cx<'_, Pending>, _: Option<&[String]>) {}
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            if self.timer == Some(timer) {
                #[cfg(feature = "condition-waits")]
                if self.mode == Mode::ParkRetire {
                    cx.retire_session(self.session.as_ref().unwrap()).unwrap();
                    let before = cx.io.core.next_id;
                    self.selection =
                        Some(cx.request_preflight_ready(self.preflight.as_ref().unwrap()));
                    assert_eq!(cx.io.core.next_id, before);
                    return;
                }
                self.revision += 1;
                assert!(cx.session_ready(self.session.as_ref().unwrap()).is_empty());
            }
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {
            self.retirements += 1;
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn local_session_refusal(e: &mut Engine<Probe>) {
        assert_eq!(e.health().auth_failures, 0);
        assert!(!e.io.core.signing.refused());
        let actions: Vec<_> = e.drain_actions().collect();
        no_sign(&actions);
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].outcome, Some(crate::Outcome::NotSent));
        assert_ne!(failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_ne!(failures[0].kind, Some(ErrorKind::AuthExpired));
        later_signed_stages(e);
    }
    fn later_signed_stages(e: &mut Engine<Probe>) {
        e.handle_command(
            30_000.,
            Command::CancelOrdersWithSymbols {
                command: 8,
                orders: vec![],
            },
            None,
        )
        .unwrap();
        let actions: Vec<_> = e.drain_actions().collect();
        let preparation = actions
            .into_iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("ordinary signed work still prepares");
        e.http_prepared(
            30_000.,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let work = actions
            .iter()
            .find_map(|action| match action {
                Action::Sign { work, .. } => Some(work),
                _ => None,
            })
            .expect("ordinary provider work still stages");
        assert!(
            matches!(work.operations.as_slice(), [crate::signing::Operation::Present { key }] if key.0 == "fixture")
        );
        assert!(!e.io.core.signing.refused());
        assert_eq!(e.health().auth_failures, 0);
    }
    fn no_sign(actions: &[Action]) {
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Sign { .. })),
            "no provider signing work"
        );
    }
    fn drive(e: &mut Engine<Probe>, actions: Vec<Action>, now: f64) -> Vec<HttpRequest> {
        no_sign(&actions);
        let mut packets = Vec::new();
        for action in actions {
            if let Action::ManagedHttp(action) = action {
                match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        let managed =
                            e.io.core
                                .managed_http
                                .get(&preparation.attempt().request)
                                .unwrap();
                        assert!(
                            managed.lifecycle.context().credential().is_none(),
                            "Prepare has no signing credential"
                        );
                        assert!(
                            managed.signing_attempt.is_none() && managed.recipe.is_none(),
                            "Prepare has no signing intent"
                        );
                        assert!(request.signer_headers.is_empty());
                        #[cfg(feature = "sign-meta")]
                        assert!(
                            managed.role.is_none(),
                            "no provider role on keyless Prepare"
                        );
                        e.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: preparation.attempt().request.0,
                                expires_at: None,
                            },
                        );
                        let next = e.drain_actions().collect();
                        packets.extend(drive(e, next, now));
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                        packets.push(request);
                    }
                    HttpAction::BindGroup { .. } => panic!("serial calls never group"),
                    _ => {}
                }
            }
        }
        packets
    }
    fn reply(e: &mut Engine<Probe>, request: RequestId, body: &str) {
        reply_at(e, request, body, 1.);
    }
    fn reply_at(e: &mut Engine<Probe>, request: RequestId, body: &str, now: f64) {
        e.handle(
            now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn ready(mode: Mode) -> Engine<Probe> {
        let mut e = crate::test_support::seeded_engine(Probe {
            mode,
            ..Default::default()
        });
        let secondary = mode == Mode::ReadWithSecondary;
        #[cfg(feature = "sign-meta")]
        let secondary = secondary || mode == Mode::Selected;
        if secondary {
            e.set_identities(crate::signing::Identities {
                primary: None,
                secondary: Some(crate::signing::Identity::new(
                    crate::signing::KeyRef("secondary.credential".into()),
                    Some(crate::signing::KeyRef("secondary.signing".into())),
                )),
            })
            .unwrap();
        }
        e.start(0.);
        if mode == Mode::CredentialSession {
            assert!(e.drain_actions().next().is_none());
            return e;
        }
        let actions = e.drain_actions().collect();
        let login = drive(&mut e, actions, 0.).pop().unwrap();
        assert_eq!(login.body.as_deref(), Some(r#"{"api_key":""}"#));
        reply(&mut e, login.id, "login-ok");
        let actions: Vec<_> = e.drain_actions().collect();
        no_sign(&actions);
        e
    }
    fn enter(e: &mut Engine<Probe>) {
        e.handle_command(
            1.,
            Command::CancelOrdersWithSymbols {
                command: 7,
                orders: vec![],
            },
            None,
        )
        .unwrap();
    }
    fn admitted(mode: Mode) -> Engine<Probe> {
        let mut e = ready(mode);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Ok(())), "genuine keyless Session entry");
        assert_eq!(e.venue.admitted, Some(Ok(())), "keyless serial admission");
        e
    }
    #[test]
    fn empty_login_cancel_then_leverage_without_any_provider_work() {
        let mut e = admitted(Mode::Chain);
        let actions = e.drain_actions().collect();
        let cancel = drive(&mut e, actions, 1.).pop().unwrap();
        assert_eq!(cancel.body.as_deref(), Some(CANCEL));
        assert_eq!(cancel.headers, vec![("Cookie", "cookie-1".into())]);
        reply(&mut e, cancel.id, "ok");
        let actions = e.drain_actions().collect();
        let leverage = drive(&mut e, actions, 1.).pop().unwrap();
        assert_eq!(leverage.body.as_deref(), Some(LEVERAGE));
        reply(&mut e, leverage.id, "ok");
        let actions: Vec<_> = e.drain_actions().collect();
        no_sign(&actions);
        assert_eq!(e.venue.calls, 2);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
                .count(),
            1
        );
    }
    #[test]
    fn ordinary_keyless_preflight_stays_invalid_without_resource_spend() {
        let mut e = ready(Mode::Ordinary);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
        no_sign(&e.drain_actions().collect::<Vec<_>>());
    }
    #[test]
    fn foreign_engine_session_with_equal_generation_cannot_admit() {
        let foreign = ready(Mode::Chain);
        let mut e = ready(Mode::Foreign);
        assert_eq!(
            e.venue.session.as_ref().unwrap().owner(),
            foreign.venue.session.as_ref().unwrap().owner()
        );
        e.venue.foreign = foreign.venue.session.clone();
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.before, e.venue.after);
        local_session_refusal(&mut e);
    }
    #[test]
    fn retired_session_cannot_admit() {
        let mut e = ready(Mode::Retired);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.before, e.venue.after);
        local_session_refusal(&mut e);
    }
    #[test]
    fn read_key_and_readonly_do_not_enter_the_session_exception() {
        for mode in [Mode::ReadKey, Mode::ReadOnly] {
            let mut e = ready(mode);
            enter(&mut e);
            assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
            assert_eq!(e.venue.before, e.venue.after);
        }
    }
    #[test]
    fn complete_plan_refusal_preserves_serial_opportunity() {
        let e = admitted(Mode::Complete);
        assert_eq!(e.venue.selection, Some(Err(Error::Invalid)));
    }
    #[test]
    fn mixed_credential_cannot_admit() {
        let mut e = ready(Mode::Mixed);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Ok(())), "genuine entry before mismatch");
        assert_eq!(e.venue.admitted, Some(Err(Error::Invalid)));
    }
    #[test]
    fn entry_without_plan_can_finish_empty_without_retiring_session() {
        for mode in [Mode::EmptyDone, Mode::EmptyFail] {
            let mut e = ready(mode);
            enter(&mut e);
            assert_eq!(e.venue.entry, Some(Ok(())));
            assert_eq!(e.venue.selection, Some(Ok(())));
            assert!(e.venue.cleanup);
            assert_eq!(
                e.venue.revision, 1,
                "empty command cleanup leaves Session revision unchanged"
            );
            assert!(
                e.io.core
                    .request_contexts
                    .validate_session(e.venue.session.as_ref().unwrap())
                    .is_ok()
            );
        }
    }
    #[test]
    fn signing_recipe_under_keyless_root_refuses_before_ids() {
        let e = admitted(Mode::Signed);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[test]
    fn missing_and_foreign_bearer_refuse_before_ids() {
        for mode in [Mode::NoBearer, Mode::WrongBearer] {
            let foreign = ready(Mode::Chain);
            let mut e = ready(mode);
            e.venue.foreign = foreign.venue.session.clone();
            enter(&mut e);
            assert_eq!(e.venue.entry, Some(Ok(())), "genuine keyless entry");
            assert_eq!(e.venue.admitted, Some(Ok(())));
            assert_eq!(e.venue.issue, Some(Err(Error::Revoked)));
            assert_eq!(e.venue.before, e.venue.after);
            local_session_refusal(&mut e);
        }
    }
    #[test]
    fn session_retirement_between_calls_blocks_continuation_before_ids() {
        let mut e = admitted(Mode::RetireBetween);
        let actions = e.drain_actions().collect();
        let request = drive(&mut e, actions, 1.).pop().unwrap();
        reply(&mut e, request.id, "ok");
        assert_eq!(e.venue.selection, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.before, e.venue.after);
        no_sign(&e.drain_actions().collect::<Vec<_>>());
        later_signed_stages(&mut e);
    }
    #[test]
    fn keyless_read_uses_same_session_without_secondary_fallback() {
        let mut e = admitted(Mode::Read);
        let actions = e.drain_actions().collect();
        let request = drive(&mut e, actions, 1.).pop().unwrap();
        assert_eq!(request.headers, vec![("Cookie", "cookie-1".into())]);
    }
    #[test]
    fn auth_resend_waits_for_new_cookie_preserving_body_and_unsigned_work() {
        let mut e = admitted(Mode::Auth);
        let actions = e.drain_actions().collect();
        let first = drive(&mut e, actions, 1.).pop().unwrap();
        reply(&mut e, first.id, "auth");
        assert_eq!(e.venue.selection, Some(Ok(())));
        let actions = e.drain_actions().collect();
        let packets = drive(&mut e, actions, 1.);
        assert!(packets.is_empty(), "old revision cannot bind");
        e.handle(
            2.,
            Event::TimerFired {
                timer: e.venue.timer.unwrap(),
            },
        );
        let actions = e.drain_actions().collect();
        let second = drive(&mut e, actions, 2.)
            .pop()
            .expect("new revision releases resend");
        assert_eq!(second.body, first.body, "immutable resend body");
        assert_ne!(second.id, first.id);
        assert_eq!(second.headers, vec![("Cookie", "cookie-2".into())]);
        reply_at(&mut e, second.id, "auth", 3.);
        assert_eq!(e.venue.selection, Some(Err(Error::RetryForbidden)));
        assert_eq!(e.venue.before, e.venue.after);
        let actions = e.drain_actions().collect();
        assert!(drive(&mut e, actions, 2.).is_empty(), "no third send");
    }
    include!("request_context_keyless_serial_source_tests.rs");
}
