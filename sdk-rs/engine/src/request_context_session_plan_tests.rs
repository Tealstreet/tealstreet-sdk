// Independent complete Plans on a real login Session root, with per-call auth.
#[allow(clippy::result_large_err)]
mod session_plan_calls {
    use super::*;
    use crate::book::Book;
    use crate::request_context::plan::{CallAuth, CallBound, RetryWindow, Slot, StepBound};
    use crate::request_context::{
        Egress, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, SecretHeaderValue, Session,
        SessionBearer, SessionHeaders, SessionPolicy, SessionResend, WriteResult,
    };
    use crate::signing::{self, Identity, KeyRef, SignError, SignRequest, SignResult};
    use std::collections::VecDeque;

    const S: CallAuth = CallAuth::Session;
    const P: CallAuth = CallAuth::Preflight;

    /// The declared topology: three serial cancels, an entry with its
    /// protective siblings, or one concurrent mixed or Session-pair step.
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Shape {
        #[default]
        Cancels,
        Protective,
        Mixed,
        SessionPair,
    }
    /// One declaration fault, refused at admission before any ID.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Fault {
        NoSessionRoot,
        NoWriteCredential,
        Dependent,
        Read,
        Fallback,
        FewerSteps,
        MoreSteps,
        FewerCalls,
        MoreCalls,
    }
    /// How the venue answers its own lost parent.
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Venue {
        #[default]
        Continue,
        RetireChildren,
        SignedSessionLeg,
        UnsignedPreflightLeg,
        ChangedResend,
        RetireBetween,
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
            let operation = if self.login {
                signing::Operation::Present {
                    key: self.key.clone(),
                }
            } else {
                signing::Operation::Sign {
                    algorithm: signing::Algorithm::HmacSha256,
                    key: self.key.clone(),
                    payload: vec![signing::PayloadPart::Bytes {
                        bytes: b"signed-leg".to_vec(),
                    }],
                }
            };
            let work = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![operation],
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

    fn primary() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: KeyRef("primary.credential".into()),
        }
    }
    fn call(attempts: u16) -> CallBound<'static> {
        CallBound {
            timeout_ms: 20_000,
            attempts,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }
    }
    const ONE: [CallBound<'static>; 1] = [CallBound {
        timeout_ms: 20_000,
        attempts: 2,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    }];
    const TWO: [CallBound<'static>; 2] = [
        CallBound {
            timeout_ms: 20_000,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        },
        CallBound {
            timeout_ms: 20_000,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        },
    ];

    enum Pending {
        Login(Context),
        Leg(Context, Slot, usize),
        Later(Context),
    }
    #[derive(Default)]
    struct Probe {
        shape: Shape,
        fault: Option<Fault>,
        venue: Venue,
        session: Option<Session>,
        plan: Option<Plan>,
        auth: Vec<CallAuth>,
        ordinal: usize,
        live: usize,
        admitted: Option<Result<(), Error>>,
        issues: Vec<(usize, Result<(), Error>)>,
        resends: Vec<(usize, Result<(), Error>)>,
        settled: Vec<(usize, Result<(), Error>)>,
        before: Option<(crate::resource_ids::ResourceIds, bool)>,
        after: Option<(crate::resource_ids::ResourceIds, bool)>,
        observed: Vec<(usize, Context)>,
        work: Vec<SignRequest>,
        failures: Vec<VenueError>,
        revision: u64,
        resent: Vec<usize>,
        done: Option<Result<(), Error>>,
        signed: Vec<u64>,
    }
    impl Probe {
        fn steps(&self) -> Vec<StepBound<'static>> {
            let step = |calls: &'static [CallBound<'static>]| StepBound { wait_ms: 0, calls };
            match self.shape {
                Shape::Cancels => vec![step(&ONE), step(&ONE), step(&ONE)],
                Shape::Protective => vec![step(&ONE), step(&TWO)],
                Shape::Mixed | Shape::SessionPair => vec![step(&TWO)],
            }
        }
        fn declared(&self) -> Vec<Vec<CallAuth>> {
            match self.shape {
                Shape::Cancels => vec![vec![S], vec![S], vec![S]],
                Shape::Protective => vec![vec![P], vec![P, P]],
                Shape::Mixed => vec![vec![P, S]],
                Shape::SessionPair => vec![vec![S, S]],
            }
        }
        fn admit(&mut self, cx: &mut Cx<'_, Pending>, preflight: &crate::request_context::Preflight) {
            let mut steps = self.steps();
            let mut declared = self.declared();
            let mut independent = true;
            let mut operation = Operation::Mutation;
            let fallback = [StepBound {
                wait_ms: 0,
                calls: &ONE,
            }];
            let with_fallback = [CallBound {
                after_refusal: &fallback,
                ..call(2)
            }];
            match self.fault {
                Some(Fault::Dependent) => independent = false,
                Some(Fault::Read) => operation = Operation::Read,
                Some(Fault::Fallback) => {
                    steps = vec![StepBound {
                        wait_ms: 0,
                        calls: &with_fallback,
                    }];
                    declared = vec![vec![S]];
                }
                Some(Fault::FewerSteps) => {
                    declared.pop();
                }
                Some(Fault::MoreSteps) => declared.push(vec![S]),
                Some(Fault::FewerCalls) => {
                    declared.last_mut().unwrap().pop();
                }
                Some(Fault::MoreCalls) => declared.last_mut().unwrap().push(S),
                _ => {}
            }
            let auth: Vec<&[CallAuth]> = declared.iter().map(Vec::as_slice).collect();
            let write = (self.fault != Some(Fault::NoWriteCredential)).then(primary);
            self.before = Some((cx.io.core.next_id, cx.io.core.signing.available_ids(1)));
            let result = cx.admit_session_plan(
                preflight,
                &steps,
                &auth,
                PlanPolicy {
                    independent_calls: independent,
                    operation,
                    credential: write,
                    pacing: None,
                    retry: RetryPolicy::DefinitiveRefusal,
                },
            );
            self.after = Some((cx.io.core.next_id, cx.io.core.signing.available_ids(1)));
            self.admitted = Some(result.as_ref().map(drop).map_err(|error| *error));
            let Ok(plan) = result else {
                let _ = cx.fail_preflight(preflight, "refused declaration");
                return;
            };
            self.auth = declared.concat();
            self.plan = Some(plan);
            self.issue_available(cx);
        }
        /// Issue (or, under a lost parent, retire) every available Slot.
        fn issue_available(&mut self, cx: &mut Cx<'_, Pending>) {
            let plan = self.plan.clone().unwrap();
            for slot in cx.plan_slots(&plan).unwrap() {
                let ordinal = self.ordinal;
                self.ordinal += 1;
                if self.venue == Venue::RetireChildren && ordinal > 0 {
                    let result = cx.retire_unissued_plan_call(&plan, &slot);
                    self.settled.push((ordinal, result));
                    continue;
                }
                if self.venue == Venue::RetireBetween && ordinal == 1 {
                    cx.retire_session(self.session.as_ref().unwrap()).unwrap();
                }
                let context = match cx.plan_leg_context(&plan, &slot) {
                    Ok(context) => context,
                    Err(error) => {
                        self.issues.push((ordinal, Err(error)));
                        continue;
                    }
                };
                self.send(cx, context, slot, ordinal, false);
            }
            self.finish_if_idle(cx);
        }
        fn send(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            context: Context,
            slot: Slot,
            ordinal: usize,
            resend: bool,
        ) {
            self.observed.push((ordinal, context.clone()));
            let pending = Pending::Leg(context.clone(), slot, ordinal);
            let session = self.auth[ordinal] == S;
            let signed = match self.venue {
                Venue::SignedSessionLeg => true,
                Venue::UnsignedPreflightLeg => false,
                _ => !session,
            };
            let url = format!("https://venue.test/leg-{ordinal}");
            let result = if signed {
                cx.request_signed_managed(
                    context,
                    self.session.as_ref(),
                    Method::Post,
                    url,
                    None,
                    Box::new(Recipe {
                        key: KeyRef("primary.signing".into()),
                        login: false,
                        prepared: None,
                    }),
                    pending,
                )
            } else {
                let changed = resend && self.venue == Venue::ChangedResend;
                cx.request_managed(
                    context,
                    self.session.as_ref(),
                    Method::Post,
                    url,
                    Some(if changed {
                        format!("changed-{ordinal}")
                    } else {
                        format!("immutable-{ordinal}")
                    }),
                    pending,
                )
            };
            let result = result.map(drop).map_err(|(_, error)| error);
            if result.is_ok() {
                self.live += 1;
            }
            self.issues.push((ordinal, result));
        }
        fn finish_if_idle(&mut self, cx: &mut Cx<'_, Pending>) {
            let plan = self.plan.clone().unwrap();
            if self.live == 0 && cx.plan_slots(&plan).is_ok_and(|slots| slots.is_empty()) {
                self.done = Some(cx.done_plan(&plan, "[]".into()));
            }
        }
        fn settle_failed(&mut self, cx: &mut Cx<'_, Pending>, context: &Context, slot: &Slot, ordinal: usize) {
            self.live -= 1;
            let plan = self.plan.clone().unwrap();
            let result = cx.finish_plan_failed_call(&plan, slot, context);
            self.settled.push((ordinal, result));
            if self.venue == Venue::RetireChildren && ordinal == 0 {
                // A lost parent's children were never sent; retire, never issue.
                self.issue_available(cx);
                return;
            }
            self.issue_available(cx);
        }
    }
    const REFUSAL: crate::classify::CodeTable = crate::classify::CodeTable {
        rows: &[
            crate::classify::Row {
                key: crate::classify::Key::Int(1000),
                verdict: crate::classify::Verdict::RefusedResend(crate::ErrorKind::AuthInvalid),
                only: None,
            },
            crate::classify::Row {
                key: crate::classify::Key::Int(2000),
                verdict: crate::classify::Verdict::Refused(crate::ErrorKind::InvalidInput),
                only: None,
            },
        ],
        success: &[],
        unlisted: crate::classify::Verdict::Unknown(None),
        status: &[],
    };
    impl crate::Venue for Probe {
        type Pending = Pending;
        fn guard_order_mutations(&self) -> bool {
            true
        }
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            let session = cx
                .open_session(SessionPolicy {
                    credential: Some(primary()),
                    repeat_safe: true,
                    private_egress: None,
                })
                .unwrap();
            let context = cx
                .session_context(
                    &session,
                    Policy {
                        operation: Operation::Mutation,
                        credential: Some(primary()),
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/login".into(),
                None,
                Box::new(Recipe {
                    key: primary().reference,
                    login: true,
                    prepared: None,
                }),
                Pending::Login(context),
            )
            .map_err(|(_, error)| error)
            .unwrap();
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
            let write = (self.fault != Some(Fault::NoWriteCredential)).then(primary);
            let policy = PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: write,
            };
            let preflight = if self.fault == Some(Fault::NoSessionRoot) {
                cx.begin_preflight(policy)
            } else {
                cx.begin_preflight_with_session(policy, self.session.as_ref().unwrap())
            };
            let preflight = match preflight {
                Ok(preflight) => preflight,
                Err(error) => panic!("real Session root entry: {error:?}"),
            };
            self.admit(cx, &preflight);
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
            let (context, slot, ordinal) = match pending {
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
                Pending::Leg(context, slot, ordinal) => (context, slot, ordinal),
            };
            let plan = self.plan.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            let code = match response.body {
                "auth" => 1000,
                "refused" => 2000,
                _ => {
                    cx.http_write_result(
                        &context,
                        WriteResult::Accepted {
                            order_ids: vec![format!("id-{ordinal}")],
                        },
                    )
                    .unwrap();
                    self.live -= 1;
                    let result = cx.finish_plan_call(&plan, &slot, &context);
                    self.settled.push((ordinal, result));
                    self.issue_available(cx);
                    return;
                }
            };
            let native = crate::classify::Native {
                code: crate::classify::NativeCode::Int(code),
                message: "refused",
            };
            let (_, grant) = cx
                .http_write_failure(&context, &REFUSAL, Some(&native), "refused", None)
                .unwrap();
            let Some(grant) = grant else {
                // A refusal in DATA settles the call; its fact stays journaled.
                self.live -= 1;
                let result = cx.finish_plan_call(&plan, &slot, &context);
                self.settled.push((ordinal, result));
                self.issue_available(cx);
                return;
            };
            let resend = if self.auth[ordinal] == S {
                cx.plan_session_resend(&plan, &slot, &context, SessionResend::Write(grant))
            } else {
                cx.plan_resend_context(&plan, &slot, &context, grant)
            };
            self.resends
                .push((ordinal, resend.as_ref().map(drop).map_err(|error| *error)));
            match resend {
                Ok(context) => {
                    self.live -= 1;
                    self.resent.push(ordinal);
                    self.send(cx, context, slot, ordinal, true);
                    self.revision += 1;
                    let _ = cx.session_ready(self.session.as_ref().unwrap());
                }
                Err(_) => self.settle_failed(cx, &context, &slot, ordinal),
            }
        }
        fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
            self.failures.push(error);
            match pending {
                Pending::Leg(context, slot, ordinal) => {
                    self.settle_failed(cx, &context, &slot, ordinal)
                }
                Pending::Login(context) | Pending::Later(context) => {
                    let _ = cx.fail_context(&context, "login failed");
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
            self.failures.push(error);
            match pending {
                Pending::Leg(_, _, _) => {
                    let _ = cx.fail_plan(self.plan.as_ref().unwrap(), "unsent leg");
                }
                Pending::Login(context) | Pending::Later(context) => {
                    let _ = cx.fail_context(&context, "unsent");
                }
            }
        }
        fn capture_http_reconciliation(
            &self,
            _: &Pending,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("session-plan-fixture".into())
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, error: &str) {
            panic!("untyped failure: {error}");
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


    /// What one engine turn released and announced.
    #[derive(Default)]
    struct Flow {
        packets: Vec<crate::HttpRequest>,
        notices: Vec<Notice>,
    }
    impl Flow {
        fn leg(&self, ordinal: usize) -> &crate::HttpRequest {
            self.packets
                .iter()
                .find(|packet| packet.url.ends_with(&format!("/leg-{ordinal}")))
                .unwrap_or_else(|| panic!("leg {ordinal} was released"))
        }
        fn terminal(&self) -> Option<Result<String, VenueError>> {
            self.notices.iter().find_map(|notice| match notice {
                Notice::CommandFailed { command: 7, error } => Some(Err(error.clone())),
                Notice::CommandDone { command: 7, result } => Some(Ok(result.clone())),
                _ => None,
            })
        }
    }
    fn pump(e: &mut Engine<Probe>, now: f64) -> Flow {
        e.prepare_dispatch(now);
        let mut queue: VecDeque<_> = e.drain_actions().collect();
        let mut flow = Flow::default();
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        if let Some(managed) = e.io.core.managed_http.get(&request.id) {
                            assert_eq!(
                                managed.lifecycle.context().credential().is_some(),
                                managed.signing_attempt.is_some()
                            );
                        }
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
                        flow.packets.push(request);
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
                    e.venue.signed.push(attempt.0);
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
                Action::Notice(notice) => flow.notices.push(notice),
                _ => {}
            }
            queue.extend(e.drain_actions());
        }
        flow
    }
    fn answer(e: &mut Engine<Probe>, packet: &crate::HttpRequest, now: f64, body: &'static str) -> Flow {
        e.handle(
            now,
            Event::HttpResponse {
                request: packet.id,
                response: Response {
                    status: if body == "ok" { 200 } else { 400 },
                    headers: "",
                    body,
                },
            },
        );
        pump(e, now)
    }
    fn lose(e: &mut Engine<Probe>, packet: &crate::HttpRequest, now: f64) -> Flow {
        e.handle(
            now,
            Event::HttpFailed {
                request: packet.id,
                error: "network",
            },
        );
        pump(e, now)
    }
    fn command(e: &mut Engine<Probe>, command: u32, now: f64) -> Flow {
        e.handle_command(
            now,
            Command::CancelOrdersWithSymbols {
                command,
                orders: vec![],
            },
            None,
        )
        .unwrap();
        pump(e, now)
    }
    fn ready(probe: Probe) -> Engine<Probe> {
        let mut e = crate::test_support::seeded_engine(probe);
        e.set_identities(signing::Identities {
            primary: Some(Identity::new(
                primary().reference,
                Some(KeyRef("primary.signing".into())),
            )),
            secondary: None,
        })
        .unwrap();
        e.start(0.);
        let login = pump(&mut e, 0.);
        answer(&mut e, &login.packets[0], 0.5, "ok");
        assert_eq!(e.venue.work.len(), 1, "the real login presented once");
        e
    }
    fn admitted(probe: Probe) -> (Engine<Probe>, Flow) {
        let mut e = ready(probe);
        let flow = command(&mut e, 7, 1.);
        assert_eq!(e.venue.admitted, Some(Ok(())), "Session Plan admission");
        (e, flow)
    }
    fn probe(shape: Shape, venue: Venue) -> Probe {
        Probe {
            shape,
            venue,
            ..Default::default()
        }
    }
    fn on_session_egress(context: &Context) -> bool {
        context.credential().is_none() && matches!(context.egress(), Some(Egress::Session(_)))
    }
    fn bearer(packet: &crate::HttpRequest) -> Option<&str> {
        packet
            .headers
            .iter()
            .find(|(name, _)| *name == "Cookie")
            .map(|(_, value)| value.as_str())
    }
    fn later_signed_work_releases(e: &mut Engine<Probe>) {
        let before = e.venue.work.len();
        let flow = command(e, 8, 10.);
        assert_eq!(flow.packets.len(), 1, "ordinary signed work still releases");
        assert_eq!(e.venue.work.len(), before + 1);
        assert!(!e.io.core.signing.refused());
    }

    /// Original 1: the cancel shape continues past an Unknown cancel.
    #[test]
    fn session_cancels_continue_past_a_lost_cancel_and_project_unknown() {
        let (mut e, first) = admitted(probe(Shape::Cancels, Venue::Continue));
        assert_eq!(first.packets.len(), 1);
        let second = lose(&mut e, first.leg(0), 2.);
        assert!(second.terminal().is_none(), "an Unknown cancel stops nothing");
        let third = answer(&mut e, second.leg(1), 3., "ok");
        assert!(third.terminal().is_none());
        let end = answer(&mut e, third.leg(2), 4., "refused");
        let error = end.terminal().unwrap_or_else(|| panic!("one terminal {:?} {:?} {:?} {}", e.venue.done, e.venue.settled, e.venue.failures, e.venue.live)).unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["id-1"]);
        assert_eq!(e.venue.observed.len(), 3);
        assert!(e.venue.observed.iter().all(|(_, context)| on_session_egress(context)));
        assert_eq!(e.venue.work.len(), 1, "no provider work after the login");
        assert!(e.venue.settled.iter().all(|(_, result)| result.is_ok()));
        later_signed_work_releases(&mut e);
        assert_eq!(
            e.venue.signed[1],
            e.venue.signed[0] + 1,
            "Session legs reserved no signing IDs"
        );
    }

    /// Original 2: protective siblings continue past a lost sibling.
    #[test]
    fn signed_protective_siblings_continue_past_a_lost_take_profit() {
        let (mut e, entry) = admitted(probe(Shape::Protective, Venue::Continue));
        assert_eq!(e.venue.work.len(), 2, "the entry signed exactly once");
        let children = answer(&mut e, entry.leg(0), 2., "ok");
        assert_eq!(children.packets.len(), 2, "both siblings are in flight");
        assert_eq!(e.venue.work.len(), 4, "each sibling signed exactly once");
        for (_, context) in &e.venue.observed {
            assert_eq!(context.credential(), Some(&primary()));
            assert!(context.egress().is_none(), "a signed leg has route None");
        }
        for packet in entry.packets.iter().chain(&children.packets) {
            assert_eq!(bearer(packet), Some("cookie-1"), "the root's bearer");
        }
        assert!(lose(&mut e, children.leg(1), 3.).terminal().is_none());
        let end = answer(&mut e, children.leg(2), 4., "ok");
        let error = end.terminal().expect("one terminal").unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["id-0", "id-2"]);
    }

    /// Original 3: a lost parent's children are retired unsent.
    #[test]
    fn a_lost_parent_orphans_its_children_unsent() {
        let (mut e, entry) = admitted(probe(Shape::Protective, Venue::RetireChildren));
        let end = lose(&mut e, entry.leg(0), 2.);
        assert!(end.packets.is_empty(), "nothing more is sent");
        let error = end.terminal().expect("one terminal").unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert!(error.partial.as_ref().is_none_or(|partial| partial.order_ids.is_empty()));
        assert_eq!(
            e.venue.settled,
            [(0, Ok(())), (1, Ok(())), (2, Ok(()))],
            "the venue retired both unissued children"
        );
        assert_eq!(e.venue.work.len(), 2);
    }

    fn refuses_before_ids(fault: Fault) {
        let mut e = ready(Probe {
            fault: Some(fault),
            ..probe(Shape::Mixed, Venue::Continue)
        });
        let flow = command(&mut e, 7, 1.);
        assert_eq!(e.venue.admitted, Some(Err(Error::Invalid)), "{fault:?}");
        assert_eq!(e.venue.before, e.venue.after, "{fault:?} spent an ID");
        assert!(flow.packets.is_empty());
        assert_eq!(e.venue.work.len(), 1);
    }
    /// Original 4: declaration refusals before IDs.
    #[test]
    fn declaration_faults_refuse_before_any_id() {
        for fault in [
            Fault::NoSessionRoot,
            Fault::NoWriteCredential,
            Fault::Dependent,
            Fault::Read,
            Fault::Fallback,
        ] {
            refuses_before_ids(fault);
        }
    }
    /// Original 7: an auth declaration that does not mirror the steps.
    #[test]
    fn an_auth_length_mismatch_at_either_level_refuses_before_any_id() {
        for fault in [
            Fault::FewerSteps,
            Fault::MoreSteps,
            Fault::FewerCalls,
            Fault::MoreCalls,
        ] {
            refuses_before_ids(fault);
        }
    }
    /// Original 4: recipe gates per leg.
    #[test]
    fn a_session_leg_refuses_a_signing_recipe_and_a_preflight_leg_an_unsigned_one() {
        let (e, flow) = admitted(probe(Shape::Cancels, Venue::SignedSessionLeg));
        assert_eq!(e.venue.issues, [(0, Err(Error::Invalid))]);
        assert!(flow.packets.is_empty());
        assert_eq!(e.venue.work.len(), 1);
        let (e, flow) = admitted(probe(Shape::Protective, Venue::UnsignedPreflightLeg));
        assert_eq!(e.venue.issues, [(0, Err(Error::Invalid))]);
        assert!(flow.packets.is_empty());
        assert_eq!(e.venue.work.len(), 1);
    }

    /// Original 5: a retired root ends unreleased legs NotSent without a latch.
    #[test]
    fn retiring_the_root_between_legs_ends_the_rest_unsent_without_a_latch() {
        let (mut e, first) = admitted(probe(Shape::Cancels, Venue::RetireBetween));
        let end = answer(&mut e, first.leg(0), 2., "ok");
        assert!(end.packets.is_empty(), "no leg rides a retired root");
        let error = end.terminal().expect("one terminal").unwrap_err();
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["id-0"]);
        assert_eq!(e.io.core.health.auth_failures, 0);
        later_signed_work_releases(&mut e);
    }

    /// Original 6: a Session resend keeps its body and rereads the bearer; a
    /// signed resend repeats its signing.
    #[test]
    fn session_and_signed_resends_keep_their_own_auth() {
        let (mut e, sent) = admitted(probe(Shape::Mixed, Venue::Continue));
        assert_eq!(e.venue.work.len(), 2);
        let signed = answer(&mut e, sent.leg(0), 2., "auth");
        assert_eq!(e.venue.work.len(), 3, "the signed resend signed again");
        let session = answer(&mut e, sent.leg(1), 2., "auth");
        assert_eq!(e.venue.resends, [(0, Ok(())), (1, Ok(()))]);
        assert_eq!(e.venue.work.len(), 3, "the Session resend did no provider work");
        let retry = session.leg(1);
        assert_eq!(sent.leg(1).body, retry.body);
        assert_eq!(bearer(sent.leg(1)), Some("cookie-1"));
        assert_eq!(bearer(retry), Some("cookie-3"), "the resend rereads the bearer");
        answer(&mut e, signed.leg(0), 3., "ok");
        let end = answer(&mut e, retry, 3., "ok");
        assert_eq!(end.terminal(), Some(Ok("[]".into())));
    }
    #[test]
    fn a_changed_session_resend_body_refuses() {
        let (mut e, sent) = admitted(probe(Shape::Cancels, Venue::ChangedResend));
        let flow = answer(&mut e, sent.leg(0), 2., "auth");
        assert_eq!(e.venue.resends, [(0, Ok(()))]);
        assert_eq!(e.venue.issues.last(), Some(&(0, Err(Error::Invalid))));
        assert!(flow.packets.is_empty());
    }

    /// Original 8: one concurrent step, each leg with its own auth.
    #[test]
    fn a_concurrent_mixed_step_gives_each_leg_its_own_auth() {
        let (_, sent) = admitted(probe(Shape::Mixed, Venue::Continue));
        assert_eq!(sent.packets.len(), 2, "both legs are in flight");
        let (e, _) = admitted(probe(Shape::Mixed, Venue::Continue));
        assert_eq!(e.venue.issues, [(0, Ok(())), (1, Ok(()))]);
        let (_, signed) = &e.venue.observed[0];
        let (_, session) = &e.venue.observed[1];
        assert_eq!(signed.credential(), Some(&primary()));
        assert!(signed.egress().is_none());
        assert!(on_session_egress(session));
        assert_eq!(e.venue.work.len(), 2, "only the signed leg did provider work");
        for packet in &sent.packets {
            assert_eq!(bearer(packet), Some("cookie-1"));
        }
        let (e, _) = admitted(probe(Shape::Mixed, Venue::SignedSessionLeg));
        assert_eq!(
            e.venue.issues,
            [(0, Ok(())), (1, Err(Error::Invalid))],
            "the Session leg's gate holds while its signed sibling is admitted"
        );
    }

    /// Original 9: each Session leg freezes its own wire. Leg 1 commits after
    /// leg 0, so a single owner slot would refuse leg 0's unchanged resend.
    #[test]
    fn session_resends_are_fingerprinted_per_leg() {
        let (mut e, sent) = admitted(probe(Shape::SessionPair, Venue::Continue));
        assert_ne!(sent.leg(0).body, sent.leg(1).body);
        let again = answer(&mut e, sent.leg(0), 2., "auth");
        assert_eq!(e.venue.resends, [(0, Ok(()))]);
        assert_eq!(again.packets.len(), 1, "only leg 0 resent");
        assert_eq!(again.leg(0).body, sent.leg(0).body);
        let other = answer(&mut e, sent.leg(1), 3., "auth");
        assert_eq!(e.venue.resends, [(0, Ok(())), (1, Ok(()))]);
        assert_eq!(other.leg(1).body, sent.leg(1).body);
        answer(&mut e, again.leg(0), 4., "ok");
        let end = answer(&mut e, other.leg(1), 4., "ok");
        assert_eq!(end.terminal(), Some(Ok("[]".into())));
    }
    /// A changed body is refused for that leg's resend, before release; the
    /// Plan then ends by the existing issue-refusal rule, its sibling Unknown.
    #[test]
    fn a_changed_resend_on_one_session_leg_is_refused_for_that_leg() {
        let (mut e, sent) = admitted(probe(Shape::SessionPair, Venue::ChangedResend));
        let flow = answer(&mut e, sent.leg(1), 2., "auth");
        assert_eq!(e.venue.resends, [(1, Ok(()))]);
        assert_eq!(e.venue.issues.last(), Some(&(1, Err(Error::Invalid))));
        assert!(flow.packets.is_empty(), "the changed body never reaches the wire");
        let error = flow.terminal().expect("one terminal").unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "leg 0 is in flight");
        assert!(answer(&mut e, sent.leg(0), 3., "ok").terminal().is_none());
    }
}
