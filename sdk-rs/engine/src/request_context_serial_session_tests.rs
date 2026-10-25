// Original controls for RUST_SERIAL_SESSION_RESEND.md. No manufactured permit,
// grant or delivery: every positive starts at real command admission and release.
#[allow(clippy::result_large_err)]
mod serial_session {
    use super::*;
    use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
    use crate::request_context::{
        Plan, PreflightPolicy, PreflightPurpose, SecretHeaderValue, Session, SessionBearer,
        SessionHeaders, SessionPolicy, SessionResend,
        plan::{SerialCall, SerialPolicy, Slot},
    };

    const BODY: &str = "immutable-order-nonce-expiry-client-id";
    const TABLE: CodeTable = CodeTable {
        rows: &[Row {
            key: Key::Int(1000),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        }],
        success: &[Key::Int(0)],
        unlisted: Verdict::Unknown(None),
        status: &[],
    };
    fn key() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        }
    }
    fn call(operation: Operation, attempts: u8) -> SerialCall {
        SerialCall {
            operation,
            timeout_ms: 20_000,
            attempts,
        }
    }
    fn code(response: Response<'_>) -> Option<i64> {
        #[derive(serde::Deserialize)]
        struct Envelope {
            code: i64,
        }
        serde_json::from_str::<Envelope>(response.body)
            .ok()
            .map(|envelope| envelope.code)
    }
    #[derive(Default)]
    struct Probe {
        read: bool,
        attempts: u8,
        exempt: bool,
        immediate: bool,
        wrong_next: bool,
        use_read_evidence: bool,
        ordinary_first: bool,
        ordinary_second: bool,
        revision: u64,
        session: Option<Session>,
        timer: Option<TimerId>,
        park_timer: Option<TimerId>,
        retire: bool,
        foreign_probe: bool,
        foreign_result: Option<Result<(), Error>>,
        legacy_waits: Option<(Result<(), Error>, Result<(), Error>)>,
        plan: Option<Plan>,
        slot: Option<Slot>,
        context: Option<Context>,
        admitted: Option<Result<(), Error>>,
        selected: Vec<Result<(), Error>>,
        unchanged: Vec<bool>,
        data: usize,
        failures: usize,
        unproven: std::cell::Cell<usize>,
        read_fact_absent: bool,
        rejections: Option<usize>,
        lock: bool,
        double_continue: Option<Result<(), Error>>,
    }
    impl Probe {
        fn issue(&mut self, cx: &mut Cx<'_, Context>, context: Context) {
            self.context = Some(context.clone());
            let body = Some(BODY.into());
            let result = if context.credential().is_some() {
                cx.request_signed_managed(
                    context.clone(),
                    self.session.as_ref(),
                    Method::Post,
                    "https://venue.test/order".into(),
                    body,
                    Box::new(Recipe),
                    context,
                )
            } else {
                cx.request_managed(
                    context.clone(),
                    self.session.as_ref(),
                    Method::Post,
                    "https://venue.test/read".into(),
                    body,
                    context,
                )
            };
            result.map_err(|(_, error)| error).unwrap();
        }
        fn snapshot(cx: &Cx<'_, Context>) -> (crate::resource_ids::ResourceIds, usize) {
            (cx.io.core.next_id, cx.io.core.actions.len())
        }
        fn record(
            &mut self,
            cx: &mut Cx<'_, Context>,
            result: Result<Context, Error>,
            before: (crate::resource_ids::ResourceIds, usize),
        ) {
            self.unchanged.push(Self::snapshot(cx) == before);
            self.selected
                .push(result.as_ref().map(drop).map_err(|e| *e));
            match result {
                Ok(context) => self.issue(cx, context),
                Err(_) => {
                    cx.fail_plan(self.plan.as_ref().unwrap(), "authentication refused")
                        .unwrap();
                }
            }
        }
    }
    impl Venue for Probe {
        type Pending = Context;
        fn start(&mut self, cx: &mut Cx<'_, Context>) {
            let session = cx
                .open_session(SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                    private_egress: None,
                })
                .unwrap();
            assert!(cx.session_ready(&session).is_empty());
            self.session = Some(session);
            self.revision = 1;
            self.timer = Some(cx.set_timer(100.));
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(key()),
                })
                .unwrap();
            let admitted = cx.admit_serial_plan(
                &preflight,
                call(
                    if self.read {
                        Operation::Read
                    } else {
                        Operation::Mutation
                    },
                    self.attempts,
                ),
                SerialPolicy {
                    credential: key(),
                    pacing: None,
                    refusals_complete: true,
                },
            );
            self.admitted = Some(admitted.as_ref().map(drop).map_err(|e| *e));
            if let Ok((plan, slot)) = admitted {
                let context = cx.plan_leg_context(&plan, &slot).unwrap();
                self.plan = Some(plan);
                self.slot = Some(slot);
                self.issue(cx, context.clone());
                let before = Self::snapshot(cx);
                let plan = self.plan.as_ref().unwrap();
                let slot = self.slot.as_ref().unwrap();
                let wait = cx
                    .plan_session_wait(
                        plan,
                        slot,
                        None,
                        self.session.as_ref().unwrap(),
                        context.clone(),
                    )
                    .map_err(|(_, e)| e);
                let retry_wait = if self.read {
                    Some(
                        cx.plan_retry_session_wait(
                            plan,
                            slot,
                            &context,
                            crate::request_context::RetryEvidence::ReadFailure,
                            self.session.as_ref().unwrap(),
                            context.clone(),
                        )
                        .map_err(|(_, e)| e),
                    )
                } else {
                    None
                };
                assert_eq!(Self::snapshot(cx), before, "legacy waits spend nothing");
                self.legacy_waits = Some((wait, retry_wait.unwrap_or(Err(Error::State))));
            }
        }
        fn status_exempt_refusal(&self, _: &Context, response: Response<'_>) -> bool {
            self.exempt && code(response) == Some(1000)
        }
        fn session_bearer(&self, generation: u64) -> SessionBearer {
            if self
                .session
                .as_ref()
                .is_some_and(|s| s.owner() == crate::request_context::Owner::Session { generation })
            {
                SessionBearer::Ready {
                    headers: SessionHeaders::authorization(
                        SecretHeaderValue::new(format!("token-{}", self.revision)).unwrap(),
                    ),
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
        fn unproven_http_evidence(
            &self,
            _: &Context,
            _: Response<'_>,
            _: &mut crate::request_context::UnprovenEvidence,
        ) {
            self.unproven.set(self.unproven.get() + 1);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            self.data += 1;
            #[cfg(feature = "order-locks")]
            {
                self.lock = cx.io.core.order_locks.contains(7);
            }
            if code(response) == Some(1000) {
                let evidence = if self.read || self.use_read_evidence {
                    Some(SessionResend::Read)
                } else {
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
                    grant.map(SessionResend::Write)
                };
                let before = Self::snapshot(cx);
                let ordinary = if self.selected.is_empty() {
                    self.ordinary_first
                } else {
                    self.ordinary_second
                };
                let result = evidence.ok_or(Error::RetryForbidden).and_then(|evidence| {
                    if ordinary {
                        match evidence {
                            SessionResend::Write(grant) => {
                                cx.plan_resend_context(&plan, &slot, &context, grant)
                            }
                            SessionResend::Read => unreachable!(),
                        }
                    } else {
                        cx.serial_plan_session_resend(&plan, &slot, &context, evidence)
                    }
                });
                self.record(cx, result, before);
                return;
            }
            if self.read {
                let request = cx.io.core.http_delivery.as_ref().unwrap().attempt.request;
                self.read_fact_absent = !cx
                    .io
                    .core
                    .request_contexts
                    .command_writes(7)
                    .unwrap()
                    .holds_attempt(request);
            } else {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Accepted {
                        order_ids: vec!["id-1".into()],
                    },
                )
                .unwrap();
            }
            self.rejections = Some(
                cx.io
                    .core
                    .request_contexts
                    .command_writes(7)
                    .unwrap()
                    .rejections()
                    .count(),
            );
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            cx.done_plan(&plan, "[\"id-1\"]".into()).unwrap();
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            self.failures += 1;
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            if self.immediate {
                let before = Self::snapshot(cx);
                let next = call(
                    if self.wrong_next {
                        Operation::Mutation
                    } else {
                        Operation::Read
                    },
                    1,
                );
                let result = cx.continue_serial_plan(&plan, &slot, &context, next);
                self.selected
                    .push(result.as_ref().map(drop).map_err(|e| *e));
                self.unchanged.push(before == Self::snapshot(cx));
                if let Ok(next_slot) = result {
                    let before = Self::snapshot(cx);
                    self.double_continue = Some(
                        cx.continue_serial_plan(&plan, &slot, &context, next)
                            .map(drop),
                    );
                    assert_eq!(
                        Self::snapshot(cx),
                        before,
                        "duplicate continuation spends nothing"
                    );
                    self.slot = Some(next_slot.clone());
                    let next_context = cx.plan_leg_context(&plan, &next_slot).unwrap();
                    self.issue(cx, next_context);
                    return;
                }
            }
            let _ = cx.fail_plan(&plan, "transport failure");
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
            let session = self.session.as_ref().unwrap();
            if self.foreign_probe {
                let before = Self::snapshot(cx);
                self.foreign_result = Some(
                    cx.serial_plan_session_resend(
                        self.plan.as_ref().unwrap(),
                        self.slot.as_ref().unwrap(),
                        self.context.as_ref().unwrap(),
                        SessionResend::Read,
                    )
                    .map(drop),
                );
                assert_eq!(Self::snapshot(cx), before, "foreign custody spends nothing");
            }
            if self.retire {
                cx.retire_session(session).unwrap();
            } else {
                assert!(cx.session_ready(session).is_empty());
            }
            self.timer = Some(cx.set_timer(1.));
        }
        fn guard_pending_placements(&self) -> bool {
            true
        }
        fn guard_order_mutations(&self) -> bool {
            true
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("serial-session-fixture".into())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn start(read: bool, attempts: u8) -> Engine<Probe> {
        let mut e = crate::test_support::seeded_engine(Probe {
            read,
            attempts,
            exempt: true,
            ..Default::default()
        });
        e.start(0.);
        e.handle_command(
            0.,
            Command::PlaceOrders {
                command: 7,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .unwrap();
        e
    }
    fn prep(actions: &[Action]) -> Preparation {
        actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("real Prepare")
    }
    fn drain(e: &mut Engine<Probe>) -> Vec<Action> {
        e.drain_actions().collect()
    }
    fn bind(
        e: &mut Engine<Probe>,
        token: Preparation,
        now: f64,
    ) -> Option<(crate::request_context::Permit, HttpRequest)> {
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let mut actions = drain(e);
        if let Some((attempt, receipt)) = actions.iter().find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.receipt)),
            _ => None,
        }) {
            e.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Ok(crate::signing::SignResult::new(
                        vec![b"value".to_vec()],
                        receipt,
                    )),
                },
            );
            actions = drain(e);
        }
        for action in &actions {
            if let Action::SetTimer { timer, .. } = action {
                e.venue.park_timer = Some(*timer);
            }
        }
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
    fn release(e: &mut Engine<Probe>, token: Preparation, now: f64) -> HttpRequest {
        let (permit, request) = bind(e, token, now).expect("real Bind");
        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
        request
    }
    fn reply(e: &mut Engine<Probe>, token: Preparation, now: f64, status: u16, body: &str) {
        e.handle(
            now,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn auth(e: &mut Engine<Probe>, status: u16) -> (Preparation, HttpRequest, Vec<Action>) {
        assert_eq!(e.venue.admitted, Some(Ok(())), "serial admission");
        let first = prep(&drain(e));
        let packet = release(e, first, 0.);
        reply(e, first, 1., status, r#"{"code":1000}"#);
        let actions = drain(e);
        (first, packet, actions)
    }
    fn no_http(actions: &[Action]) {
        assert!(!actions.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(a) => matches!(
                a.as_ref(),
                HttpAction::Prepare { .. } | HttpAction::Bind { .. }
            ),
            _ => false,
        }));
    }
    fn failed(actions: &[Action]) -> &VenueError {
        actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .expect("one command failure")
    }
    fn seeded(read: bool, status: u16) -> (Engine<Probe>, Preparation, HttpRequest) {
        let mut e = start(read, 2);
        let (_, packet, actions) = auth(&mut e, status);
        assert_eq!(e.venue.data, 1, "exempt native DATA reaches codec");
        assert_eq!(
            e.venue.selected,
            vec![Ok(())],
            "select same call attempt two"
        );
        assert_eq!(e.venue.unproven.get(), 0);
        let second = prep(&actions);
        assert!(
            bind(&mut e, second, 2.).is_none(),
            "old Ready revision must park"
        );
        #[cfg(feature = "order-locks")]
        assert!(e.io.core.order_locks.contains(7), "lock survives the park");
        (e, second, packet)
    }
    fn wake(e: &mut Engine<Probe>, revision: u64, now: f64) -> Vec<Action> {
        e.venue.revision = revision;
        e.handle(
            now,
            Event::TimerFired {
                timer: e.venue.timer.unwrap(),
            },
        );
        drain(e)
    }
    #[test]
    fn mutation_resend_seeds_floor_and_preserves_body_and_journal() {
        let (mut e, _, first) = seeded(false, 200);
        no_http(&wake(&mut e, 1, 3.));
        #[cfg(feature = "order-locks")]
        assert!(e.venue.lock);
        let second = prep(&wake(&mut e, 2, 4.));
        let packet = release(&mut e, second, 4.);
        assert_eq!(packet.body, first.body);
        assert_ne!(packet.id, first.id);
        assert!(
            packet
                .headers
                .iter()
                .any(|(k, v)| *k == "Authorization" && v == "token-2")
        );
        reply(&mut e, second, 5., 200, r#"{"code":0}"#);
        let actions = drain(&mut e);
        assert_eq!(e.venue.rejections, Some(0));
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
                .count(),
            1
        );
    }
    #[test]
    fn unsigned_read_resend_has_no_write_fact() {
        let (mut e, _, _) = seeded(true, 200);
        let second = prep(&wake(&mut e, 2, 4.));
        release(&mut e, second, 4.);
        reply(&mut e, second, 5., 200, r#"{"code":0}"#);
        drain(&mut e);
        assert!(e.venue.read_fact_absent);
        assert_eq!(e.venue.rejections, Some(0));
    }
    #[test]
    fn parsed_408_refusal_reaches_data_and_resends_once() {
        seeded(false, 408);
    }
    #[test]
    fn parsed_503_refusal_reaches_data_and_resends_once() {
        seeded(false, 503);
    }
    #[test]
    fn second_auth_refusal_cannot_send_a_third_attempt() {
        let (mut e, _, _) = seeded(false, 200);
        let second = prep(&wake(&mut e, 2, 4.));
        release(&mut e, second, 4.);
        reply(&mut e, second, 5., 200, r#"{"code":1000}"#);
        let actions = drain(&mut e);
        assert_eq!(e.venue.selected, vec![Ok(()), Err(Error::RetryForbidden)]);
        assert!(e.venue.unchanged[1]);
        no_http(&actions);
        failed(&actions);
    }
    #[test]
    fn parked_attempt_expiry_seals_once_and_late_ready_is_inert() {
        let (mut e, _, _) = seeded(false, 200);
        let timer = e.venue.park_timer.expect("park retains its engine timer");
        e.handle(20_002., Event::TimerFired { timer });
        let actions = drain(&mut e);
        assert_eq!(failed(&actions).kind, Some(ErrorKind::Timeout));
        assert_eq!(failed(&actions).outcome, Some(crate::Outcome::NotSent));
        #[cfg(feature = "order-locks")]
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"));
        no_http(&actions);
        no_http(&wake(&mut e, 2, 20_003.));
        no_http(&wake(&mut e, 3, 20_004.));
    }
    #[test]
    fn retirement_while_parked_never_releases() {
        let (mut e, _, _) = seeded(false, 200);
        e.venue.retire = true;
        let actions = wake(&mut e, 2, 4.);
        no_http(&actions);
        failed(&actions);
    }
    #[test]
    fn attempts_one_refuses_without_spending() {
        let mut e = start(false, 1);
        auth(&mut e, 200);
        assert_eq!(e.venue.selected, vec![Err(Error::RetryForbidden)]);
        assert!(e.venue.unchanged[0]);
    }
    #[test]
    fn mutation_cannot_use_read_evidence() {
        let mut e = start(false, 2);
        e.venue.use_read_evidence = true;
        auth(&mut e, 200);
        assert_eq!(e.venue.selected, vec![Err(Error::Invalid)]);
        assert!(e.venue.unchanged[0]);
    }
    #[test]
    fn immediate_resend_spends_the_session_resend_credit() {
        let mut e = start(false, 2);
        e.venue.ordinary_first = true;
        let (_, _, actions) = auth(&mut e, 200);
        assert_eq!(e.venue.selected, vec![Ok(())]);
        let second = prep(&actions);
        release(&mut e, second, 2.);
        reply(&mut e, second, 3., 200, r#"{"code":1000}"#);
        drain(&mut e);
        assert_eq!(e.venue.selected[1], Err(Error::RetryForbidden));
        assert!(e.venue.unchanged[1]);
    }
    #[test]
    fn session_resend_spends_the_immediate_resend_credit() {
        let (mut e, _, _) = seeded(false, 200);
        e.venue.ordinary_second = true;
        let second = prep(&wake(&mut e, 2, 4.));
        release(&mut e, second, 4.);
        reply(&mut e, second, 5., 200, r#"{"code":1000}"#);
        drain(&mut e);
        assert_eq!(e.venue.selected[1], Err(Error::RetryForbidden));
        assert!(e.venue.unchanged[1]);
    }
    fn strict(status: u16, body: &str, exempt: bool) {
        let mut e = start(false, 2);
        e.venue.exempt = exempt;
        let token = prep(&drain(&mut e));
        release(&mut e, token, 0.);
        reply(&mut e, token, 1., status, body);
        let actions = drain(&mut e);
        assert_eq!(e.venue.data, 0);
        assert!(e.venue.selected.is_empty());
        assert_eq!(failed(&actions).outcome, Some(crate::Outcome::Unknown));
        assert_eq!(e.venue.unproven.get(), 1);
        no_http(&actions);
        #[cfg(feature = "order-locks")]
        assert!(e.io.core.order_locks.uncertain("BTCUSD"));
    }
    #[test]
    fn coded_408_without_auth_refusal_stays_unknown() {
        strict(408, r#"{"code":1001}"#, true);
    }
    #[test]
    fn coded_503_without_auth_refusal_stays_unknown() {
        strict(503, r#"{"code":1001}"#, true);
    }
    #[test]
    fn unparsed_408_stays_unknown() {
        strict(408, "gateway timeout", true);
    }
    #[test]
    fn html_503_mentioning_1000_stays_unknown() {
        strict(503, "<html>code 1000</html>", true);
    }
    #[test]
    fn unfinished_1000_json_stays_unknown() {
        strict(503, r#"{"code":1000"#, true);
    }
    #[test]
    fn trailing_junk_after_1000_json_stays_unknown() {
        strict(503, r#"{"code":1000}junk"#, true);
    }
    #[test]
    fn default_false_path_keeps_503_unknown() {
        strict(503, r#"{"code":1000}"#, false);
    }
    #[test]
    fn six_read_calls_continue_five_released_failures_without_delay() {
        let mut e = start(true, 1);
        e.venue.immediate = true;
        let mut actions = drain(&mut e);
        for i in 0..5 {
            let token = prep(&actions);
            release(&mut e, token, i as f64);
            e.handle(
                i as f64 + 0.5,
                Event::HttpFailed {
                    request: token.attempt().request,
                    error: "network unavailable",
                },
            );
            actions = drain(&mut e);
            assert_eq!(
                e.venue.selected.last(),
                Some(&Ok(())),
                "immediate released Read continuation"
            );
            assert!(e.venue.double_continue.unwrap().is_err());
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer {at_ms,..} if *at_ms == i as f64 + 1.5))
            );
        }
        let token = prep(&actions);
        release(&mut e, token, 7.);
        reply(&mut e, token, 8., 200, r#"{"code":0}"#);
        assert_eq!(e.venue.failures, 5);
        assert!(e.venue.read_fact_absent);
    }
    #[test]
    fn failed_read_cannot_continue_as_mutation() {
        let mut e = start(true, 1);
        e.venue.immediate = true;
        e.venue.wrong_next = true;
        let token = prep(&drain(&mut e));
        release(&mut e, token, 0.);
        e.handle(
            1.,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network",
            },
        );
        drain(&mut e);
        assert_eq!(e.venue.selected, vec![Err(Error::Invalid)]);
        assert!(e.venue.unchanged[0]);
    }
    #[test]
    fn foreign_timer_has_no_serial_data_authority() {
        let mut e = start(true, 1);
        let _ = drain(&mut e);
        e.venue.foreign_probe = true;
        wake(&mut e, 1, 1.);
        assert_eq!(e.venue.foreign_result, Some(Err(Error::Revoked)));
    }
    #[test]
    fn ordinary_read_admits_one_attempt_without_session_wait() {
        let e = start(true, 1);
        assert_eq!(e.venue.admitted, Some(Ok(())));
        let waits = e.venue.legacy_waits.unwrap();
        assert_eq!(waits.0, Err(Error::State));
        assert_eq!(waits.1, Err(Error::RetryForbidden));
    }
    #[test]
    fn preparation_failure_cannot_become_an_immediate_read_retry() {
        let mut e = start(true, 1);
        e.venue.immediate = true;
        let token = prep(&drain(&mut e));
        e.http_preparation_failed(
            1.,
            token,
            Error::Invalid,
            VenueError::new("provider refused"),
        );
        let actions = drain(&mut e);
        assert!(e.venue.selected.iter().all(Result::is_err));
        no_http(&actions);
        failed(&actions);
    }
    #[test]
    fn mutation_transport_failure_cannot_continue_as_read() {
        let mut e = start(false, 1);
        e.venue.immediate = true;
        let token = prep(&drain(&mut e));
        release(&mut e, token, 0.);
        e.handle(
            1.,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network",
            },
        );
        let actions = drain(&mut e);
        no_http(&actions);
        assert!(e.venue.selected.iter().all(Result::is_err));
        assert_eq!(failed(&actions).outcome, Some(crate::Outcome::Unknown));
    }
}
