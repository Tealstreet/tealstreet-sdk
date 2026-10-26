// Genuine login/command/Prepare/Bind/Claim originals for explicit serial auth.
#[allow(clippy::result_large_err)]
mod serial_call_auth {
    use super::*;
    use crate::book::Book;
    use crate::request_context::plan::{CallAuth, SerialCall, SerialPolicy, Slot};
    use crate::request_context::{
        Egress, Plan, PreflightPolicy, PreflightPurpose, SecretHeaderValue, Session, SessionBearer,
        SessionHeaders, SessionPolicy, WriteResult,
    };
    use crate::signing::{self, Identity, KeyRef, SignError, SignRequest, SignResult};
    use std::collections::VecDeque;

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Mode {
        #[default]
        Mixed,
        MixedResend,
        UnsignedOnly,
        ForeignRef,
        SecondarySession,
        WrongWrite,
        SigningRefAsCredential,
        EmptyMixed,
        KeylessPreflight,
        NoSessionRoot,
        SignedSessionCall,
        UnsignedPreflightCall,
        RetireBetween,
        Resend,
        ChangedResend,
        RetiredResend,
        ForbiddenRecipe,
        #[cfg(feature = "sign-meta")]
        SelectedRecipe,
    }
    enum Pending {
        Login(Context),
        Call(Context, Slot, usize),
        Later(Context),
    }
    struct Recipe {
        key: KeyRef,
        login: bool,
        prepared: Option<SignRequest>,
    }
    impl crate::SignStep for Recipe {
        fn freshness_ms(&self) -> f64 {
            20_000.
        }
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<SignRequest, SignError> {
            let work = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![if self.login {
                    signing::Operation::Present {
                        key: self.key.clone(),
                    }
                } else {
                    signing::Operation::Sign {
                        algorithm: signing::Algorithm::HmacSha256,
                        key: self.key.clone(),
                        payload: vec![signing::PayloadPart::Bytes {
                            bytes: b"frozen-create".to_vec(),
                        }],
                    }
                }],
            };
            work.validate()?;
            self.prepared = Some(work.clone());
            Ok(work)
        }
        fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
            self.prepared
                .take()
                .ok_or(SignError::InvalidResult)?
                .validate_result(result)?;
            if self.login {
                let Action::Http(http) = outbound else {
                    return Err(SignError::InvalidRequest);
                };
                http.body = Some(format!(
                    "api_key={}",
                    std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?
                ));
            }
            Ok(())
        }
    }
    struct ForbiddenRecipe;
    impl crate::SignStep for ForbiddenRecipe {
        fn freshness_ms(&self) -> f64 {
            panic!("Session call cannot inspect a signing recipe")
        }
        fn nonce_claim(&self) -> Option<&crate::nonce::NonceClaim> {
            panic!("Session call cannot inspect nonce authority")
        }
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<SignRequest, SignError> {
            panic!("Session call cannot prepare provider work")
        }
        fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
            panic!("Session call cannot apply signing")
        }
    }

    fn primary() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: KeyRef("primary.credential".into()),
        }
    }
    fn secondary() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Secondary,
            reference: KeyRef("secondary.credential".into()),
        }
    }
    fn terms(auth: CallAuth, operation: Operation) -> SerialCall {
        SerialCall {
            auth,
            operation,
            timeout_ms: 20_000,
            attempts: 2,
        }
    }
    #[derive(Default)]
    struct Probe {
        mode: Mode,
        session: Option<Session>,
        plan: Option<Plan>,
        entry: Option<Result<(), Error>>,
        admitted: Option<Result<(), Error>>,
        issue: Option<Result<(), Error>>,
        selected: Option<Result<(), Error>>,
        before: Option<crate::resource_ids::ResourceIds>,
        after: Option<crate::resource_ids::ResourceIds>,
        observed: Vec<Context>,
        work: Vec<SignRequest>,
        errors: Vec<VenueError>,
        revision: u64,
        resends: usize,
        auth_at_retire: Option<u32>,
    }
    impl Probe {
        fn issue(&mut self, cx: &mut Cx<'_, Pending>, slot: Slot, stage: usize) {
            let context = cx
                .plan_leg_context(self.plan.as_ref().unwrap(), &slot)
                .unwrap();
            self.issue_context(cx, context, slot, stage);
        }
        fn issue_context(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            context: Context,
            slot: Slot,
            stage: usize,
        ) {
            self.observed.push(context.clone());
            let signed = stage == 3 || self.mode == Mode::SignedSessionCall;
            self.before = Some(cx.io.core.next_id);
            let pending = Pending::Call(context.clone(), slot, stage);
            #[cfg(feature = "sign-meta")]
            if self.mode == Mode::SelectedRecipe {
                let slots = crate::credential_selection::Slots::new(
                    None,
                    cx.io.core.identities.secondary.clone(),
                    Default::default(),
                );
                let (selection, _) = slots
                    .select(crate::credential_selection::CredentialRole::Read { scope: None })
                    .unwrap();
                let result = cx.request_selected_managed(
                    context,
                    &selection,
                    self.session.as_ref(),
                    Method::Post,
                    "https://venue.test/selected".into(),
                    None,
                    Box::new(ForbiddenRecipe),
                    pending,
                );
                self.after = Some(cx.io.core.next_id);
                self.issue = Some(result.map(drop).map_err(|(_, error)| error));
                return;
            }
            let result = if self.mode == Mode::ForbiddenRecipe {
                cx.request_signed_managed(
                    context,
                    self.session.as_ref(),
                    Method::Post,
                    "https://venue.test/forbidden".into(),
                    None,
                    Box::new(ForbiddenRecipe),
                    pending,
                )
            } else if signed {
                cx.request_signed_managed(
                    context,
                    self.session.as_ref(),
                    Method::Post,
                    format!("https://venue.test/{stage}"),
                    Some(if self.mode == Mode::MixedResend && self.resends > 0 {
                        "fresh-signature-create".into()
                    } else {
                        "frozen-create".into()
                    }),
                    Box::new(Recipe {
                        key: KeyRef("primary.signing".into()),
                        login: false,
                        prepared: None,
                    }),
                    pending,
                )
            } else {
                cx.request_managed(
                    context,
                    self.session.as_ref(),
                    Method::Post,
                    format!("https://venue.test/{stage}"),
                    Some(if self.mode == Mode::ChangedResend && self.resends > 0 {
                        "changed-call".into()
                    } else {
                        format!("immutable-call-{stage}")
                    }),
                    pending,
                )
            };
            self.after = Some(cx.io.core.next_id);
            self.issue = Some(result.map(drop).map_err(|(_, error)| error));
        }
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn guard_order_mutations(&self) -> bool {
            true
        }
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            let empty = matches!(self.mode, Mode::EmptyMixed | Mode::KeylessPreflight);
            let credential = if empty {
                None
            } else if self.mode == Mode::SecondarySession {
                Some(secondary())
            } else if self.mode == Mode::ForeignRef {
                Some(crate::request_context::Credential {
                    reference: KeyRef("foreign.credential".into()),
                    ..primary()
                })
            } else {
                Some(primary())
            };
            let slots = crate::credential_selection::Slots::new(
                crate::signing::SetupFacts::new(crate::signing::KeyPresence::Empty).primary_login(),
                None,
                Default::default(),
            );
            let (selection, _) = slots
                .select(crate::credential_selection::CredentialRole::SessionAuth)
                .unwrap();
            let session = cx
                .open_session(SessionPolicy {
                    credential: credential.clone(),
                    repeat_safe: true,
                    private_egress: empty.then(|| selection.session_egress()).flatten(),
                })
                .unwrap();
            let context = cx
                .session_context(
                    &session,
                    Policy {
                        operation: Operation::Mutation,
                        credential: credential.clone(),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let result = if let Some(credential) = credential {
                cx.request_signed_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/login".into(),
                    None,
                    Box::new(Recipe {
                        key: credential.reference,
                        login: true,
                        prepared: None,
                    }),
                    Pending::Login(context),
                )
            } else {
                cx.request_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/login".into(),
                    Some("api_key=".into()),
                    Pending::Login(context),
                )
            };
            result.map_err(|(_, error)| error).unwrap();
            self.session = Some(session);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if command.completion_id() == Some(8) {
                let context = cx
                    .admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: Some(primary()),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .unwrap();
                cx.request_signed_managed(
                    context.clone(),
                    None,
                    Method::Post,
                    "https://venue.test/later".into(),
                    None,
                    Box::new(Recipe {
                        key: primary().reference,
                        login: true,
                        prepared: None,
                    }),
                    Pending::Later(context),
                )
                .map_err(|(_, error)| error)
                .unwrap();
                return;
            }
            let write = if matches!(self.mode, Mode::UnsignedOnly | Mode::KeylessPreflight) {
                None
            } else if self.mode == Mode::WrongWrite {
                Some(crate::request_context::Credential {
                    reference: KeyRef("foreign.credential".into()),
                    ..primary()
                })
            } else if self.mode == Mode::SigningRefAsCredential {
                Some(crate::request_context::Credential {
                    reference: KeyRef("primary.signing".into()),
                    ..primary()
                })
            } else {
                Some(primary())
            };
            let policy = PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: write.clone(),
            };
            self.before = Some(cx.io.core.next_id);
            let result = if self.mode == Mode::NoSessionRoot {
                cx.begin_preflight(policy)
            } else {
                cx.begin_preflight_with_session(policy, self.session.as_ref().unwrap())
            };
            self.after = Some(cx.io.core.next_id);
            self.entry = Some(result.as_ref().map(drop).map_err(|error| match error {
                crate::request_context::CommandEntryError::Rejected(reason) => *reason,
                crate::request_context::CommandEntryError::Revoked => Error::Revoked,
            }));
            let Ok(preflight) = result else { return };
            let auth = if matches!(
                self.mode,
                Mode::KeylessPreflight | Mode::UnsignedPreflightCall
            ) {
                CallAuth::Preflight
            } else {
                CallAuth::Session
            };
            let operation = if matches!(self.mode, Mode::Mixed | Mode::MixedResend) {
                Operation::Read
            } else {
                Operation::Mutation
            };
            self.before = Some(cx.io.core.next_id);
            let result = cx.admit_serial_plan(
                &preflight,
                terms(auth, operation),
                SerialPolicy {
                    credential: write,
                    pacing: None,
                    refusals_complete: true,
                },
            );
            self.after = Some(cx.io.core.next_id);
            self.admitted = Some(result.as_ref().map(drop).map_err(|error| *error));
            if let Ok((plan, slot)) = result {
                self.plan = Some(plan);
                self.issue(cx, slot, 0);
            }
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            response: Response<'_>,
        ) {
            let (context, slot, stage) = match pending {
                Pending::Login(context) => {
                    cx.http_write_result(&context, WriteResult::Accepted { order_ids: vec![] })
                        .unwrap();
                    cx.finish_http_context(&context).unwrap();
                    self.revision = 1;
                    let _ = cx.session_ready(self.session.as_ref().unwrap());
                    return;
                }
                Pending::Later(context) => {
                    cx.finish_http_context(&context).unwrap();
                    return;
                }
                Pending::Call(context, slot, stage) => (context, slot, stage),
            };
            let plan = self.plan.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            if response.body == "auth" {
                use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
                const TABLE: CodeTable = CodeTable {
                    rows: &[Row {
                        key: Key::Int(1000),
                        verdict: Verdict::RefusedResend(crate::ErrorKind::AuthInvalid),
                        only: None,
                    }],
                    success: &[],
                    unlisted: Verdict::Unknown(None),
                    status: &[],
                };
                let native = Native {
                    code: NativeCode::Int(1000),
                    message: "authentication refused",
                };
                let (_, grant) = cx
                    .http_write_failure(
                        &context,
                        &TABLE,
                        Some(&native),
                        "authentication refused",
                        None,
                    )
                    .unwrap();
                if self.mode == Mode::RetiredResend {
                    self.auth_at_retire = Some(cx.io.core.health.auth_failures);
                    cx.retire_session(self.session.as_ref().unwrap()).unwrap();
                }
                self.before = Some(cx.io.core.next_id);
                let result = grant.ok_or(Error::RetryForbidden).and_then(|grant| {
                    cx.serial_plan_session_resend(
                        &plan,
                        &slot,
                        &context,
                        crate::request_context::SessionResend::Write(grant),
                    )
                });
                self.after = Some(cx.io.core.next_id);
                self.selected = Some(result.as_ref().map(drop).map_err(|error| *error));
                match result {
                    Ok(context) => {
                        self.resends += 1;
                        self.issue_context(cx, context, slot, stage);
                        self.revision += 1;
                        let _ = cx.session_ready(self.session.as_ref().unwrap());
                    }
                    Err(error) => {
                        let _ = cx.fail_plan(&plan, error.venue_error());
                    }
                }
                return;
            }
            if context.operation() == Operation::Mutation {
                cx.http_write_result(&context, WriteResult::Accepted { order_ids: vec![] })
                    .unwrap();
            }
            if self.mode == Mode::RetireBetween {
                cx.retire_session(self.session.as_ref().unwrap()).unwrap();
            }
            let last = if matches!(self.mode, Mode::Mixed | Mode::MixedResend) {
                3
            } else {
                1
            };
            if stage == last {
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "[]".into()).unwrap();
                return;
            }
            let next = stage + 1;
            let auth = if matches!(self.mode, Mode::Mixed | Mode::MixedResend) && next == 3 {
                CallAuth::Preflight
            } else {
                CallAuth::Session
            };
            let operation = if matches!(self.mode, Mode::Mixed | Mode::MixedResend) && next == 2 {
                Operation::Read
            } else {
                Operation::Mutation
            };
            self.before = Some(cx.io.core.next_id);
            let result = cx.continue_serial_plan(&plan, &slot, &context, terms(auth, operation));
            self.after = Some(cx.io.core.next_id);
            self.selected = Some(result.as_ref().map(drop).map_err(|error| *error));
            match result {
                Ok(slot) => self.issue(cx, slot, next),
                Err(error) => {
                    let _ = cx.fail_plan(&plan, error.venue_error());
                }
            }
        }
        fn session_bearer(&self, generation: u64) -> SessionBearer {
            if self.session.as_ref().is_some_and(|session| {
                session.owner() == (crate::request_context::Owner::Session { generation })
            }) && self.revision > 0
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
                .filter(|session| {
                    session.owner() == (crate::request_context::Owner::Session { generation })
                })
                .map(|_| self.revision)
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            _: Error,
            error: VenueError,
        ) {
            self.errors.push(error.clone());
            match pending {
                Pending::Call(_, _, _) => {
                    let _ = cx.fail_plan(self.plan.as_ref().unwrap(), error);
                }
                Pending::Login(context) | Pending::Later(context) => {
                    let _ = cx.fail_context(&context, error);
                }
            }
        }
        fn capture_http_reconciliation(
            &self,
            _: &Pending,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("serial-call-auth-fixture".into())
        }

        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
            match pending {
                Pending::Call(_, _, _) => {
                    cx.fail_plan(self.plan.as_ref().unwrap(), error).unwrap();
                }
                _ => panic!("no login or provider transport failures in this fixture"),
            }
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
    fn drive(e: &mut Engine<Probe>, now: f64) -> Vec<crate::HttpRequest> {
        e.prepare_dispatch(now);
        let mut queue: VecDeque<_> = e.drain_actions().collect();
        let mut packets = vec![];
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        let managed = &e.io.core.managed_http[&request.id];
                        assert_eq!(
                            managed.lifecycle.context().credential().is_some(),
                            managed.signing_attempt.is_some()
                        );
                        e.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        let claim = e.claim_http(now, permit);
                        assert!(matches!(claim, Ok(Claim::Released(_))), "{claim:?}");
                        packets.push(request);
                    }
                    HttpAction::BindGroup { .. } => panic!("no grouped calls"),
                    _ => {}
                },
                Action::Sign { attempt, work, .. } => {
                    let values = work
                        .operations
                        .iter()
                        .map(|operation| match operation {
                            signing::Operation::Present { .. } => b"fixture-api-key".to_vec(),
                            signing::Operation::Sign { .. } => vec![0x44; 32],
                            _ => panic!("no filler public-address operation"),
                        })
                        .collect();
                    e.venue.work.push(work.clone());
                    e.handle(
                        now,
                        Event::Signed {
                            attempt,
                            result: Ok(SignResult {
                                receipt: work.receipt,
                                values,
                                headers: vec![],
                            }),
                        },
                    );
                }
                _ => {}
            }
            queue.extend(e.drain_actions());
        }
        packets
    }
    fn reply(e: &mut Engine<Probe>, request: RequestId, now: f64) {
        e.handle(
            now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "ok",
                },
            },
        );
    }
    fn ready(mode: Mode) -> Engine<Probe> {
        let mut e = crate::test_support::seeded_engine(Probe {
            mode,
            ..Default::default()
        });
        let empty = matches!(mode, Mode::EmptyMixed | Mode::KeylessPreflight);
        e.set_identities(signing::Identities {
            primary: (!empty).then(|| {
                Identity::new(primary().reference, Some(KeyRef("primary.signing".into())))
            }),
            secondary: Some(Identity::new(
                secondary().reference,
                Some(KeyRef("secondary.signing".into())),
            )),
        })
        .unwrap();
        e.start(0.);
        let login = drive(&mut e, 0.).pop().expect("actual released login");
        reply(&mut e, login.id, 0.5);
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.work.len(), usize::from(!empty));
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
        assert_eq!(
            e.venue.entry,
            Some(Ok(())),
            "real installed login Session entry"
        );
        assert_eq!(
            e.venue.admitted,
            Some(Ok(())),
            "explicit call auth admission"
        );
        e
    }
    #[test]
    fn mixed_inspect_cancel_verify_then_signed_create_uses_one_root() {
        let mut e = admitted(Mode::Mixed);
        for stage in 0..4 {
            let packet = drive(&mut e, 1. + stage as f64)
                .pop()
                .expect("one real physical call");
            let context = &e.venue.observed[stage];
            if stage < 3 {
                assert!(context.credential().is_none());
                assert!(matches!(context.egress(), Some(Egress::Session(_))));
                assert_eq!(
                    context.routing_intent(),
                    crate::request_context::RoutingIntent::Primary
                );
                assert_eq!(
                    e.venue.work.len(),
                    1,
                    "only the real login used provider work"
                );
            } else {
                assert_eq!(context.credential(), Some(&primary()));
                assert_eq!(e.venue.work.len(), 2);
            }
            reply(&mut e, packet.id, 1.5 + stage as f64);
        }
        assert!(e.venue.errors.is_empty());
        assert!(!e.io.core.signing.refused());
    }
    #[test]
    fn installed_unsigned_cancel_then_leverage_never_uses_a_signer_or_secondary() {
        let mut e = admitted(Mode::UnsignedOnly);
        for stage in 0..2 {
            let packet = drive(&mut e, 1. + stage as f64).pop().unwrap();
            let context = &e.venue.observed[stage];
            assert!(context.credential().is_none());
            assert!(matches!(context.egress(), Some(Egress::Session(_))));
            assert_eq!(
                context.routing_intent(),
                crate::request_context::RoutingIntent::Primary
            );
            reply(&mut e, packet.id, 1.5 + stage as f64);
        }
        assert_eq!(e.venue.work.len(), 1);
    }
    fn later_signed_stages(e: &mut Engine<Probe>) {
        let before = e.venue.work.len();
        e.handle_command(
            10.,
            Command::CancelOrdersWithSymbols {
                command: 8,
                orders: vec![],
            },
            None,
        )
        .unwrap();
        assert_eq!(
            drive(e, 10.).len(),
            1,
            "ordinary signed work still releases"
        );
        assert_eq!(e.venue.work.len(), before + 1);
        assert!(!e.io.core.signing.refused());
    }

    fn entry_refuses(mode: Mode) {
        let mut e = ready(mode);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
        assert!(drive(&mut e, 2.).is_empty());
        assert!(!e.io.core.signing.refused());
        assert_eq!(e.io.core.health.auth_failures, 0);
        later_signed_stages(&mut e);
    }
    #[test]
    fn foreign_session_credential_refuses_before_entry_ids() {
        entry_refuses(Mode::ForeignRef)
    }
    #[test]
    fn a_secondary_login_session_cannot_authorize_a_primary_root() {
        entry_refuses(Mode::SecondarySession)
    }
    #[test]
    fn mismatched_write_credential_refuses_before_entry_ids() {
        entry_refuses(Mode::WrongWrite)
    }
    #[test]
    fn a_signing_ref_cannot_replace_the_credential_ref() {
        entry_refuses(Mode::SigningRefAsCredential)
    }
    #[test]
    fn an_empty_login_session_cannot_carry_a_write_credential() {
        entry_refuses(Mode::EmptyMixed)
    }
    #[test]
    fn a_session_declaration_without_a_session_root_refuses_before_ids() {
        let mut e = ready(Mode::NoSessionRoot);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Ok(())));
        assert_eq!(e.venue.admitted, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[test]
    fn a_keyless_root_cannot_declare_preflight_auth() {
        let mut e = ready(Mode::KeylessPreflight);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Ok(())));
        assert_eq!(e.venue.admitted, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[test]
    fn a_session_call_refuses_a_signing_recipe_before_ids() {
        let e = admitted(Mode::SignedSessionCall);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[test]
    fn a_preflight_call_refuses_an_unsigned_recipe_before_ids() {
        let e = admitted(Mode::UnsignedPreflightCall);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[test]
    fn session_auth_resend_keeps_body_and_rereads_new_cookie_without_provider_work() {
        let mut e = admitted(Mode::Resend);
        let first = drive(&mut e, 1.).pop().unwrap();
        auth_reply(&mut e, first.id, 2.);
        assert_eq!(e.venue.selected, Some(Ok(())));
        let ids = e.io.core.next_id;
        auth_reply(&mut e, first.id, 2.);
        assert_eq!(e.io.core.next_id, ids, "late DATA is inert");
        let next = drive(&mut e, 2.).pop().unwrap();
        assert_ne!(first.id, next.id);
        assert_eq!(first.body, next.body);
        assert_eq!(first.headers, vec![("Cookie", "cookie-1".into())]);
        assert_eq!(next.headers, vec![("Cookie", "cookie-2".into())]);
        assert_eq!(e.venue.work.len(), 1);
        reply(&mut e, next.id, 3.);
    }
    #[test]
    fn a_mixed_roots_preflight_create_resend_repeats_signing_without_session_fingerprint() {
        let mut e = admitted(Mode::MixedResend);
        for stage in 0..3 {
            let packet = drive(&mut e, 1. + stage as f64).pop().unwrap();
            reply(&mut e, packet.id, 1.5 + stage as f64);
        }
        let create = drive(&mut e, 4.).pop().unwrap();
        assert_eq!(e.venue.work.len(), 2);
        auth_reply(&mut e, create.id, 5.);
        assert_eq!(e.venue.selected, Some(Ok(())));
        let retry = drive(&mut e, 5.).pop().unwrap();
        assert_ne!(
            create.body, retry.body,
            "provider-signed bytes are not the Session codec fingerprint"
        );
        assert_eq!(e.venue.work.len(), 3);
        assert_eq!(
            e.venue.observed.last().unwrap().credential(),
            Some(&primary())
        );
        reply(&mut e, retry.id, 6.);
    }
    #[test]
    fn a_released_session_cancel_transport_failure_keeps_write_evidence_and_unknown() {
        let mut e = admitted(Mode::UnsignedOnly);
        let cancel = drive(&mut e, 1.).pop().unwrap();
        assert!(e.venue.observed[0].credential().is_none());
        assert!(e.io.core.request_contexts.command_writes(7).is_some());
        #[cfg(feature = "order-locks")]
        assert!(e.io.core.order_locks.contains(7));
        e.handle(
            2.,
            Event::HttpFailed {
                request: cancel.id,
                error: "network",
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let terminal = actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .expect("shared journal seals the released Mutation");
        assert_eq!(terminal.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(e.venue.work.len(), 1);
        assert!(e.venue.selected.is_none());
    }

    #[test]
    fn changed_session_resend_body_refuses_before_ids() {
        let mut e = admitted(Mode::ChangedResend);
        let first = drive(&mut e, 1.).pop().unwrap();
        auth_reply(&mut e, first.id, 2.);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
        assert!(drive(&mut e, 2.).is_empty());
        assert_eq!(e.venue.work.len(), 1);
    }
    #[test]
    fn retiring_the_installed_session_before_resend_refuses_without_spend() {
        let mut e = admitted(Mode::RetiredResend);
        let first = drive(&mut e, 1.).pop().unwrap();
        auth_reply(&mut e, first.id, 2.);
        assert_eq!(e.venue.selected, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.before, e.venue.after);
        assert!(drive(&mut e, 2.).is_empty());
        assert!(!e.io.core.signing.refused());
        assert_eq!(
            e.io.core.health.auth_failures,
            e.venue.auth_at_retire.unwrap() + 1,
            "the journal terminal preserves the earlier genuine auth refusal"
        );
        later_signed_stages(&mut e);
    }
    #[test]
    fn session_call_refuses_before_nonce_or_freshness_inspection() {
        let e = admitted(Mode::ForbiddenRecipe);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    #[cfg(feature = "sign-meta")]
    #[test]
    fn session_call_refuses_selected_secondary_recipe_before_ids() {
        let e = admitted(Mode::SelectedRecipe);
        assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
    }
    fn auth_reply(e: &mut Engine<Probe>, request: RequestId, now: f64) {
        e.handle(
            now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 401,
                    headers: "",
                    body: "auth",
                },
            },
        );
    }

    #[test]
    fn retiring_the_installed_session_between_calls_spends_nothing_and_does_not_latch() {
        let mut e = admitted(Mode::RetireBetween);
        let packet = drive(&mut e, 1.).pop().unwrap();
        reply(&mut e, packet.id, 2.);
        assert_eq!(e.venue.selected, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.before, e.venue.after);
        assert!(!e.io.core.signing.refused());
        assert_eq!(e.io.core.health.auth_failures, 0);
        e.handle_command(
            3.,
            Command::CancelOrdersWithSymbols {
                command: 8,
                orders: vec![],
            },
            None,
        )
        .unwrap();
        assert_eq!(
            drive(&mut e, 3.).len(),
            1,
            "later ordinary signed work releases"
        );
        assert_eq!(e.venue.work.len(), 2);
    }
}
