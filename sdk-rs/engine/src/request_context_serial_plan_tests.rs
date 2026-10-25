// Streamed serial Plan REDs (docs/plans/RUST_PLAN_SERIAL_STREAM.md). Each
// test name carries its control number; every control is driven through a
// genuine command callback, WriteAfterReads Preflight and Prepare/Bind/Claim.
#[allow(clippy::result_large_err)]
mod serial_plans {
    use super::*;
    use crate::request_context::{
        Plan, PreflightPolicy, PreflightPurpose,
        plan::{SerialCall, SerialPolicy, Slot},
    };
    use std::collections::VecDeque;

    const STEP_MS: f64 = 100.;

    /// A distinct read-role key, as Bitget signs in-job GETs.
    fn read_key() -> crate::request_context::Credential {
        crate::request_context::Credential {
            reference: crate::signing::KeyRef("read-fixture".into()),
            ..credential()
        }
    }
    fn credential() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        }
    }
    fn mutation() -> SerialCall {
        SerialCall {
            operation: Operation::Mutation,
            timeout_ms: 20_000,
            attempts: 1,
        }
    }
    fn read() -> SerialCall {
        SerialCall {
            operation: Operation::Read,
            timeout_ms: 20_000,
            attempts: 1,
        }
    }
    fn refused(message: &str) -> VenueError {
        VenueError::new(message.to_owned()).outcome(crate::Outcome::Rejected)
    }
    fn partial(message: &str, ids: Vec<String>) -> VenueError {
        let mut error = refused(message);
        error.partial = Some(Box::new(crate::Partial {
            order_ids: ids,
            total: None,
            failed_index: None,
        }));
        error
    }

    /// What the venue does with the current call once its DATA (or failure)
    /// is in custody. The body grammar is the venue's own fixture reply.
    enum Reply {
        Accepted(Vec<String>),
        Rejected,
        Mixed(Vec<String>),
        PartialUnknown(Vec<String>),
        Unproven,
        Resend,
        Read,
        Oversized,
        ServerError,
    }
    fn parse(body: &str) -> Reply {
        let ids = |rest: &str| {
            rest.split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect()
        };
        match body.split_once(':') {
            Some(("ok", rest)) => Reply::Accepted(ids(rest)),
            Some(("mixed", rest)) => Reply::Mixed(ids(rest)),
            Some(("partial-unknown", rest)) => Reply::PartialUnknown(ids(rest)),
            _ => match body {
                "reject" => Reply::Rejected,
                "unknown" => Reply::Unproven,
                "40009" => Reply::Resend,
                "read" => Reply::Read,
                "oversized" => Reply::Oversized,
                "503" => Reply::ServerError,
                other => panic!("fixture reply {other}"),
            },
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    enum OnRefusedContinue {
        #[default]
        Fail,
        FinishThenDone,
    }

    #[derive(Default)]
    struct Serial {
        // Terms.
        total: usize,
        first: Option<SerialCall>,
        next: VecDeque<SerialCall>,
        refusals_complete: bool,
        pacing: Option<crate::request_context::Pacing>,
        read_preflight: bool,
        no_credential: bool,
        code1: bool,
        partial_terminal: bool,
        on_refused: OnRefusedContinue,
        delay_ms: Option<u32>,
        retry_failed_reads: bool,
        read_key: bool,
        read_signers: Vec<Option<String>>,
        forge_resend: bool,
        read_failures: u32,
        probe_custody: bool,
        probe_second_admission: bool,
        timer: Option<TimerId>,
        idle_failures: bool,
        session: Option<crate::request_context::Session>,
        session_park: Option<Result<(), Error>>,
        session_retired: Option<usize>,
        // Native job state.
        job_ids: Vec<String>,
        current: Option<SerialCall>,
        // Engine handles.
        preflight: Option<crate::request_context::Preflight>,
        plan: Option<Plan>,
        slot: Option<Slot>,
        context: Option<Context>,
        // Observations.
        admitted: Option<Result<(), Error>>,
        second_admission: Option<Result<(), Error>>,
        continued: Vec<Result<(), Error>>,
        probes: Vec<(&'static str, Result<(), Error>, bool)>,
        resends: Vec<Result<(), Error>>,
        data: usize,
        failures: usize,
        ready: usize,
        terminal: Option<Result<(), Error>>,
        ids_per_step: Vec<crate::resource_ids::ResourceIds>,
        storage: Vec<(usize, usize, usize)>,
        #[cfg_attr(not(feature = "order-locks"), allow(dead_code))]
        lock_held: Vec<bool>,
        refusal_spent_nothing: Option<bool>,
        read_requests: Vec<RequestId>,
        journal_at_done: Option<(bool, usize)>,
    }

    fn queue(cx: &mut Cx<'_, Context>, context: Context, signed: bool) {
        let result = if signed {
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Box::new(Recipe),
                context,
            )
        } else {
            cx.request_managed(
                context.clone(),
                None,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                context,
            )
        };
        result.map(drop).map_err(|(_, error)| error).unwrap();
    }

    impl Serial {
        fn policy(&self) -> SerialPolicy {
            // A WriteAfterReads Preflight always holds a write credential, so the
            // reachable "no write credential" case is a Plan naming another one.
            let credential = if self.no_credential {
                crate::request_context::Credential {
                    reference: crate::signing::KeyRef("other".into()),
                    ..credential()
                }
            } else {
                credential()
            };
            SerialPolicy {
                credential,
                pacing: self.pacing,
                refusals_complete: self.refusals_complete,
            }
        }
        fn snapshot(
            cx: &Cx<'_, Context>,
        ) -> (
            crate::resource_ids::ResourceIds,
            usize,
            (usize, usize, usize),
        ) {
            (
                cx.io.next_id,
                cx.io.actions.len(),
                cx.io.request_contexts.plan_context_storage(),
            )
        }
        fn admit(&mut self, cx: &mut Cx<'_, Context>) {
            let policy = PreflightPolicy {
                purpose: if self.read_preflight {
                    PreflightPurpose::ReadOnly
                } else {
                    PreflightPurpose::WriteAfterReads
                },
                read_credential: self.read_key.then(read_key),
                write_credential: (!self.read_preflight).then(credential),
            };
            let Ok(preflight) = cx.begin_preflight(policy) else {
                self.admitted = Some(Err(Error::Revoked));
                return;
            };
            self.preflight = Some(preflight.clone());
            let first = self.first.unwrap_or_else(mutation);
            let before = Self::snapshot(cx);
            match cx.admit_serial_plan(&preflight, first, self.policy()) {
                Ok((plan, slot)) => {
                    self.admitted = Some(Ok(()));
                    if self.probe_second_admission {
                        let again = Self::snapshot(cx);
                        self.second_admission = Some(
                            cx.admit_serial_plan(&preflight, mutation(), self.policy())
                                .map(drop),
                        );
                        assert_eq!(Self::snapshot(cx), again);
                    }
                    if let Some(session) = self.session.clone() {
                        let token = cx
                            .http_context(
                                crate::request_context::Scope::Background,
                                crate::request_context::Policy {
                                    operation: Operation::Read,
                                    credential: None,
                                    timeout_ms: 1_000_000,
                                    pacing: None,
                                    retry: RetryPolicy::Never,
                                },
                            )
                            .unwrap();
                        let mid = Self::snapshot(cx);
                        self.session_park = Some(
                            cx.plan_session_wait(&plan, &slot, None, &session, token)
                                .map_err(|(_, error)| error),
                        );
                        assert_eq!(Self::snapshot(cx), mid, "a refused park spends nothing");
                    }
                    self.plan = Some(plan);
                    self.slot = Some(slot);
                    self.issue(cx, first);
                }
                Err(error) => {
                    self.refusal_spent_nothing = Some(Self::snapshot(cx) == before);
                    self.admitted = Some(Err(error));
                }
            }
        }
        fn issue(&mut self, cx: &mut Cx<'_, Context>, call: SerialCall) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            let context = cx.plan_leg_context(&plan, &slot).unwrap();
            self.context = Some(context.clone());
            self.current = Some(call);
            self.ids_per_step.push(cx.io.next_id);
            self.storage
                .push(cx.io.request_contexts.plan_context_storage());
            #[cfg(feature = "order-locks")]
            self.lock_held.push(cx.io.order_locks.contains(7));
            self.read_signers.extend(
                (call.operation == Operation::Read)
                    .then(|| context.credential().map(|c| c.reference.0.clone())),
            );
            let signed = context.credential().is_some();
            queue(cx, context, signed);
        }
        fn probe(&mut self, cx: &mut Cx<'_, Context>, name: &'static str, context: &Context) {
            let (Some(plan), Some(slot)) = (self.plan.clone(), self.slot.clone()) else {
                return;
            };
            let before = Self::snapshot(cx);
            let result = cx
                .continue_serial_plan(&plan, &slot, context, mutation())
                .map(drop);
            self.probes
                .push((name, result, Self::snapshot(cx) == before));
        }
        fn finish(&mut self, cx: &mut Cx<'_, Context>, context: &Context, done: bool) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            cx.finish_plan_call(&plan, &slot, context).unwrap();
            if done {
                let writes = cx.io.request_contexts.command_writes(7).unwrap();
                self.journal_at_done = Some((
                    self.read_requests
                        .iter()
                        .any(|id| writes.holds_attempt(*id)),
                    writes.rejections().count(),
                ));
            }
            self.terminal = Some(if done {
                cx.done_plan(&plan, format!("[{}]", self.job_ids.join(",")))
            } else {
                cx.fail_plan(&plan, "venue stopped")
            });
        }
        /// A resend without a proven-refusal grant: always refused.
        fn forged_resend(&mut self, cx: &mut Cx<'_, Context>, context: &Context) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            let attempt = cx
                .io
                .http_delivery
                .as_ref()
                .map_or(RequestId(0), |delivery| delivery.attempt.request);
            let before = Self::snapshot(cx);
            let result = cx
                .plan_resend_context(
                    &plan,
                    &slot,
                    context,
                    crate::request_context::ResendGrant { attempt },
                )
                .map(drop);
            self.resends.push(result);
            self.probes
                .push(("forged resend", result, Self::snapshot(cx) == before));
        }
        fn advance(&mut self, cx: &mut Cx<'_, Context>, context: &Context) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            if self.data >= self.total {
                self.finish(cx, context, true);
                return;
            }
            let next = self.next.pop_front().unwrap_or_else(mutation);
            if let Some(delay) = self.delay_ms {
                let result = cx.continue_serial_plan_after(&plan, &slot, context, delay, next);
                self.continued.push(result);
                if result.is_ok() {
                    self.current = Some(next);
                } else {
                    self.finish(cx, context, false);
                }
                return;
            }
            match cx.continue_serial_plan(&plan, &slot, context, next) {
                Ok(slot) => {
                    self.continued.push(Ok(()));
                    if self.probe_custody && self.data == 1 {
                        self.probe(cx, "stale previous delivery", context);
                    }
                    self.slot = Some(slot);
                    self.issue(cx, next);
                }
                Err(error) => {
                    self.continued.push(Err(error));
                    self.finish(
                        cx,
                        context,
                        self.on_refused == OnRefusedContinue::FinishThenDone,
                    );
                }
            }
        }
    }

    impl Venue for Serial {
        type Pending = Context;
        fn start(&mut self, cx: &mut Cx<'_, Context>) {
            if self.probe_custody {
                self.timer = Some(cx.set_timer(1.));
            }
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            self.admit(cx);
        }
        fn on_venue_action(&mut self, cx: &mut Cx<'_, Context>, _: CommandId, _: VenueAction) {
            self.admit(cx);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            let plan = self.plan.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            if self.probe_custody && self.data == 0 {
                self.probe(cx, "fact not yet recorded", &context);
            }
            self.data += 1;
            if self.data == 1
                && let Some(session) = self.session.clone()
            {
                self.session_retired = Some(cx.retire_session(&session).unwrap().len());
            }
            let stop = match parse(response.body) {
                Reply::Read => {
                    assert_eq!(self.current.unwrap().operation, Operation::Read);
                    self.read_failures = 0;
                    let request = cx.io.http_delivery.as_ref().unwrap().attempt.request;
                    self.read_requests.push(request);
                    false
                }
                Reply::Accepted(ids) => {
                    self.job_ids.extend(ids.iter().cloned());
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::Accepted { order_ids: ids },
                    )
                    .unwrap();
                    false
                }
                Reply::Rejected => {
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::Rejected(refused("row refused")),
                    )
                    .unwrap();
                    if self.code1 {
                        self.finish(cx, &context, false);
                        return;
                    }
                    false
                }
                Reply::Mixed(ids) => {
                    self.job_ids.extend(ids.iter().cloned());
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::KnownPartial(partial(
                            "some rows refused",
                            ids,
                        )),
                    )
                    .unwrap();
                    if self.partial_terminal {
                        self.finish(cx, &context, false);
                        return;
                    }
                    false
                }
                Reply::PartialUnknown(ids) => {
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::PartialUnknown(partial(
                            "rows unresolved",
                            ids,
                        )),
                    )
                    .unwrap();
                    true
                }
                Reply::Unproven => {
                    cx.http_write_unproven(&context, "reply does not prove the write")
                        .unwrap();
                    true
                }
                Reply::Oversized => {
                    let id = "x".repeat(1_025);
                    cx.http_write_result(
                        &context,
                        crate::request_context::WriteResult::Accepted {
                            order_ids: vec![id],
                        },
                    )
                    .unwrap();
                    true
                }
                Reply::ServerError => {
                    use crate::classify::{CodeTable, Key, Verdict};
                    const TABLE: CodeTable = CodeTable {
                        rows: &[],
                        success: &[Key::Int(0)],
                        unlisted: Verdict::Unknown(None),
                        status: &[],
                    };
                    cx.http_write_failure(&context, &TABLE, None, "503 Service Unavailable", None)
                        .unwrap();
                    self.forged_resend(cx, &context);
                    true
                }
                Reply::Resend => {
                    use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
                    const TABLE: CodeTable = CodeTable {
                        rows: &[Row {
                            key: Key::Int(40009),
                            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
                            only: None,
                        }],
                        success: &[Key::Int(0)],
                        unlisted: Verdict::Unknown(None),
                        status: &[],
                    };
                    let native = Native {
                        code: NativeCode::Int(40009),
                        message: "sign signature error",
                    };
                    let (_, grant) = cx
                        .http_write_failure(&context, &TABLE, Some(&native), "refused", None)
                        .unwrap();
                    let slot = self.slot.clone().unwrap();
                    let resend = grant
                        .ok_or(Error::RetryForbidden)
                        .and_then(|grant| cx.plan_resend_context(&plan, &slot, &context, grant));
                    self.resends
                        .push(resend.as_ref().map(drop).map_err(|error| *error));
                    match resend {
                        Ok(context) => {
                            self.context = Some(context.clone());
                            queue(cx, context, true);
                        }
                        Err(_) => self.finish(cx, &context, false),
                    }
                    return;
                }
            };
            if stop {
                let slot = self.slot.clone().unwrap();
                let before = Self::snapshot(cx);
                let result = cx
                    .continue_serial_plan(&plan, &slot, &context, mutation())
                    .map(drop);
                self.probes.push((
                    "after an uncertain fact",
                    result,
                    Self::snapshot(cx) == before,
                ));
                self.terminal = Some(cx.fail_plan(&plan, "uncertain write"));
                return;
            }
            self.advance(cx, &context);
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            self.failures += 1;
            if self.idle_failures {
                return;
            }
            if self.probe_custody {
                self.probe(cx, "failure callback", &context);
            }
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            if self.forge_resend {
                self.forged_resend(cx, &context);
            }
            if self.retry_failed_reads && self.read_failures < 3 {
                self.read_failures += 1;
                let delay = 250 * self.read_failures;
                let before = Self::snapshot(cx);
                let result =
                    cx.continue_serial_plan_after(&plan, &slot, &context, delay, mutation());
                self.probes.push((
                    "mutation next after a read failure",
                    result,
                    Self::snapshot(cx) == before,
                ));
                let result = cx.continue_serial_plan_after(&plan, &slot, &context, delay, read());
                self.continued.push(result);
                if result.is_ok() {
                    let before = Self::snapshot(cx);
                    let again =
                        cx.continue_serial_plan_after(&plan, &slot, &context, delay, read());
                    self.probes.push((
                        "second delayed continuation from one failure",
                        again,
                        Self::snapshot(cx) == before,
                    ));
                    self.current = Some(read());
                    return;
                }
            }
            // An unreleased call (a provider refusal) is stopped by the engine.
            if cx.finish_plan_failed_call(&plan, &slot, &context).is_ok() {
                self.terminal = Some(cx.fail_plan(&plan, "call failed"));
            }
        }
        fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Context>, plan: &Plan) {
            self.ready += 1;
            let slot = cx.plan_slots(plan).unwrap().remove(0);
            self.slot = Some(slot);
            let call = self.current.unwrap_or_else(mutation);
            self.issue(cx, call);
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
            if let Some(context) = self.context.clone() {
                self.probe(cx, "venue timer callback", &context);
            }
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {}
        // The Plan's placement holds the engine's BTCUSD order lock.
        fn guard_pending_placements(&self) -> bool {
            true
        }
        fn guard_order_mutations(&self) -> bool {
            true
        }
        // RED8's pacing controls name group 1; an undeclared group is never sendable.
        fn http_pacing(&self) -> &[crate::request_context::Pacing] {
            &[crate::request_context::Pacing {
                group: 1,
                minimum_gap_ms: 0.,
            }]
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("serial-native".into())
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

    // Engine driving: genuine Prepare, provider signing, Bind and Claim.
    fn command(engine: &mut Engine<Serial>, now: f64, wait_ms: Option<u32>) {
        engine.start(now);
        // A placement, so the engine holds a real BTCUSD order lock for the Plan.
        // A ReadOnly Preflight is refused under that lock, so its controls read.
        let command = if engine.venue.read_preflight {
            Command::Venue {
                command: 7,
                action: VenueAction::FetchBalanceInfo,
            }
        } else {
            Command::PlaceOrders {
                command: 7,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            }
        };
        engine.handle_command(now, command, wait_ms).unwrap();
    }
    fn prepared(actions: &[Action]) -> Option<Preparation> {
        actions.iter().find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
    }
    /// Returns the provider signing stamp when the call was signed.
    fn release(engine: &mut Engine<Serial>, token: Preparation, now: f64) -> Option<f64> {
        engine.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        let mut stamp = None;
        if actions.iter().any(|a| matches!(a, Action::Sign { .. })) {
            let (attempt, work) = work(&actions);
            stamp = work.receipt.stamp_ms;
            engine.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Ok(crate::signing::SignResult::new(
                        vec![b"value".to_vec()],
                        work.receipt,
                    )),
                },
            );
            actions = engine.drain_actions().collect();
        }
        let permit = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Bind { permit, .. } => Some(*permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("serial call binding");
        assert!(matches!(
            engine.claim_http(now, permit),
            Ok(Claim::Released(_))
        ));
        stamp
    }
    fn reply(engine: &mut Engine<Serial>, token: Preparation, now: f64, body: &str) {
        engine.handle(
            now,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    /// Release and answer each prepared call in turn; returns every action
    /// after the last reply and the clock.
    fn drive<'a>(
        engine: &mut Engine<Serial>,
        now: f64,
        bodies: impl IntoIterator<Item = &'a str>,
    ) -> (Vec<Action>, f64) {
        let actions = engine.drain_actions().collect::<Vec<_>>();
        drive_from(engine, actions, now, bodies)
    }
    /// As `drive`, starting from actions the caller already drained.
    fn drive_from<'a>(
        engine: &mut Engine<Serial>,
        mut actions: Vec<Action>,
        mut now: f64,
        bodies: impl IntoIterator<Item = &'a str>,
    ) -> (Vec<Action>, f64) {
        for body in bodies {
            let token = prepared(&actions).expect("the next serial call is prepared");
            release(engine, token, now);
            now += STEP_MS;
            reply(engine, token, now, body);
            actions = engine.drain_actions().collect();
        }
        (actions, now)
    }
    fn admitted(e: &Engine<Serial>) {
        assert_eq!(e.venue.admitted, Some(Ok(())), "serial admission");
    }
    fn done(actions: &[Action]) -> Option<String> {
        actions.iter().find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(result.clone()),
            _ => None,
        })
    }
    fn failed(actions: &[Action]) -> Option<VenueError> {
        actions.iter().find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.clone()),
            _ => None,
        })
    }
    fn engine(venue: Serial) -> Engine<Serial> {
        crate::test_support::seeded_engine(venue)
    }

    #[test]
    fn serial_01_six_hundred_calls_reserve_one_context_and_advance_from_data() {
        let mut e = engine(Serial {
            total: 600,
            ..Default::default()
        });
        let base = e.io.request_contexts.plan_context_storage();
        command(&mut e, 0., None);
        admitted(&e);
        let bodies = (0..600).map(|i| format!("ok:id-{i}")).collect::<Vec<_>>();
        let (actions, _) = drive(&mut e, 0., bodies.iter().map(String::as_str));
        assert_eq!(e.venue.data, 600);
        assert_eq!(e.venue.continued.len(), 599);
        assert!(e.venue.continued.iter().all(Result::is_ok));
        let steps = e
            .venue
            .ids_per_step
            .windows(3)
            .map(|w| (w[1], w[2]))
            .collect::<Vec<_>>();
        let mut first = e.venue.ids_per_step[1];
        let mut second = e.venue.ids_per_step[2];
        let per_call = (second.reserve(1).unwrap().start) - (first.reserve(1).unwrap().start);
        for (mut a, mut b) in steps {
            assert_eq!(
                b.reserve(1).unwrap().start - a.reserve(1).unwrap().start,
                per_call
            );
        }
        for storage in &e.venue.storage {
            assert!(
                storage.0 + storage.2 <= base.0 + base.2 + 1,
                "one reserved Context: {storage:?}"
            );
        }
        assert_eq!(
            done(&actions).as_deref(),
            Some(
                format!(
                    "[{}]",
                    (0..600)
                        .map(|i| format!("id-{i}"))
                        .collect::<Vec<_>>()
                        .join(",")
                )
                .as_str()
            )
        );
    }

    #[test]
    fn serial_01_previous_delivery_and_leg_are_stale_after_continuation() {
        let mut e = engine(Serial {
            total: 3,
            probe_custody: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        drive(&mut e, 0., ["ok:a", "ok:b", "ok:c"]);
        let stale = e
            .venue
            .probes
            .iter()
            .find(|(name, ..)| *name == "stale previous delivery")
            .unwrap();
        assert!(stale.1.is_err() && stale.2, "{stale:?}");
    }

    #[test]
    fn serial_02_full_registry_peers_survive_a_six_hundred_call_plan() {
        let mut e = engine(Serial {
            total: 600,
            ..Default::default()
        });
        e.start(0.);
        let peers = (0..255)
            .map(|_| {
                Cx {
                    now: 0.,
                    store: &mut e.store,
                    io: &mut e.io,
                }
                .http_context(
                    crate::request_context::Scope::Background,
                    crate::request_context::Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 600_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        e.handle_command(
            0.,
            Command::Venue {
                command: 7,
                action: VenueAction::FetchBalanceInfo,
            },
            None,
        )
        .unwrap();
        admitted(&e);
        let bodies = (0..600).map(|i| format!("ok:id-{i}")).collect::<Vec<_>>();
        let (actions, now) = drive(&mut e, 0., bodies.iter().map(String::as_str));
        assert!(done(&actions).is_some());
        let cx = Cx {
            now,
            store: &mut e.store,
            io: &mut e.io,
        };
        for peer in &peers {
            cx.validate_http_context(peer).unwrap();
        }
    }

    #[test]
    fn serial_03_continuation_outside_custody_refuses_and_spends_nothing() {
        let mut e = engine(Serial {
            total: 3,
            probe_custody: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions).unwrap();
        release(&mut e, token, 0.);
        let timer = e.venue.timer.expect("venue timer");
        e.handle(1., Event::TimerFired { timer });
        reply(&mut e, token, STEP_MS, "ok:a");
        actions = e.drain_actions().collect();
        let token = prepared(&actions).unwrap();
        let managed = e.io.managed_http[&token.attempt().request].timer;
        release(&mut e, token, STEP_MS);
        e.handle(STEP_MS + 20_000., Event::TimerFired { timer: managed });
        let names = e
            .venue
            .probes
            .iter()
            .map(|(name, ..)| *name)
            .collect::<Vec<_>>();
        for name in [
            "fact not yet recorded",
            "stale previous delivery",
            "venue timer callback",
            "failure callback",
        ] {
            assert!(names.contains(&name), "{name} probe ran: {names:?}");
        }
        for (name, result, unchanged) in &e.venue.probes {
            assert!(
                result.is_err() && *unchanged,
                "{name}: {result:?} unchanged={unchanged}"
            );
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn serial_04_unknown_stops_projects_every_id_and_converts_the_lock_to_the_fence() {
        let mut e = engine(Serial {
            total: 10,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "ok:b", "unknown"]);
        let after = e
            .venue
            .probes
            .iter()
            .find(|(name, ..)| *name == "after an uncertain fact")
            .unwrap();
        assert_eq!(after.1, Err(Error::State));
        assert!(after.2);
        let error = failed(&actions).expect("failure projection");
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["a", "b"]);
        assert!(
            prepared(&actions).is_none(),
            "nothing after an uncertain row is sent"
        );
        assert!(
            e.venue.lock_held.iter().all(|held| *held),
            "lock held for the Plan"
        );
        assert!(
            e.io.order_locks.uncertain("BTCUSD"),
            "the lock converts to the fence"
        );
    }

    #[test]
    fn serial_05a_unreleased_call_timeout_is_one_not_sent_terminal() {
        let mut e = engine(Serial {
            total: 10,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, now) = drive(&mut e, 0., ["ok:a"]);
        let token = prepared(&actions).unwrap();
        let timer = e.io.managed_http[&token.attempt().request].timer;
        e.handle(now + 20_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions).expect("one terminal");
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_ne!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["a"]);
        assert!(prepared(&actions).is_none());
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn serial_05b_released_call_timeout_is_unknown_with_the_fence() {
        let mut e = engine(Serial {
            total: 10,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, now) = drive(&mut e, 0., ["ok:a"]);
        let token = prepared(&actions).unwrap();
        let timer = e.io.managed_http[&token.attempt().request].timer;
        release(&mut e, token, now);
        e.handle(now + 20_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions).expect("one terminal");
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["a"]);
        assert!(e.io.order_locks.uncertain("BTCUSD"));
    }

    fn ten_thousand(stop_at_501: bool) -> (Engine<Serial>, Vec<Action>, Vec<String>) {
        let mut e = engine(Serial {
            total: if stop_at_501 { 600 } else { 500 },
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let expected = (0..10_000)
            .map(|i| format!("id-{i:05}"))
            .collect::<Vec<_>>();
        let mut bodies = expected
            .chunks(20)
            .map(|ids| format!("ok:{}", ids.join(",")))
            .collect::<Vec<_>>();
        if stop_at_501 {
            bodies.push("unknown".into());
        }
        let (actions, _) = drive(&mut e, 0., bodies.iter().map(String::as_str));
        (e, actions, expected)
    }

    #[test]
    fn serial_06_ten_thousand_ids_stream_untruncated_in_both_terminals() {
        let (_, actions, expected) = ten_thousand(true);
        let error = failed(&actions).expect("uncertain stop at call 501");
        assert_eq!(error.partial.as_ref().unwrap().order_ids, expected);
        assert!(!error.message.contains("truncated"));
        let (_, actions, expected) = ten_thousand(false);
        assert_eq!(done(&actions), Some(format!("[{}]", expected.join(","))));
    }

    #[test]
    fn serial_06_duplicate_ids_append_exactly_as_received() {
        let mut e = engine(Serial {
            total: 3,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:same", "ok:same", "unknown"]);
        assert_eq!(
            failed(&actions).unwrap().partial.unwrap().order_ids,
            ["same", "same"]
        );
    }

    fn refusals(refusals_complete: bool, code1: bool, bodies: &[&str]) -> Vec<Action> {
        let mut e = engine(Serial {
            total: bodies.len(),
            refusals_complete,
            code1,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        drive(&mut e, 0., bodies.iter().copied()).0
    }

    #[test]
    fn serial_07_refusals_complete_permits_done_and_never_overrides_failure() {
        assert_eq!(
            done(&refusals(true, false, &["reject", "reject"])).as_deref(),
            Some("[]")
        );
        assert_eq!(
            done(&refusals(true, false, &["ok:a", "reject"])).as_deref(),
            Some("[a]")
        );
        assert!(failed(&refusals(true, false, &["reject", "unknown"])).is_some());
        assert!(
            failed(&refusals(true, true, &["reject"])).is_some(),
            "code1 fail_plan stays a failure"
        );
        assert!(failed(&refusals(false, false, &["reject", "reject"])).is_some());
    }

    #[test]
    fn serial_07_a_not_sent_first_call_fails_with_the_flag_true() {
        let mut e = engine(Serial {
            total: 3,
            refusals_complete: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions).unwrap();
        let timer = e.io.managed_http[&token.attempt().request].timer;
        e.handle(20_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(failed(&actions).is_some());
        assert!(done(&actions).is_none());
    }

    #[test]
    fn serial_07_rejections_stay_bounded_across_thirty_thousand_refusals() {
        let mut e = engine(Serial {
            total: 30_000,
            refusals_complete: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let mut now = 0.;
        for _ in 0..29_999 {
            let token = prepared(&actions).unwrap();
            release(&mut e, token, now);
            now += STEP_MS;
            reply(&mut e, token, now, "reject");
            actions = e.drain_actions().collect();
        }
        let writes = e.io.request_contexts.command_writes(7).unwrap();
        let text = writes
            .rejections()
            .map(|(_, error)| error.message.len())
            .sum::<usize>();
        assert!(text <= 64 * 1024, "retained refusal text {text}");
        assert!(writes.rejections().count() < 29_999, "stream-and-retire");
        assert_eq!(writes.refusal_count(), 29_999, "the count stays exact");
        let (actions, _) = drive_from(&mut e, actions, now, ["reject"]);
        assert_eq!(done(&actions).as_deref(), Some("[]"));
    }

    #[test]
    fn serial_08_admission_refusals_spend_nothing() {
        for venue in [
            Serial {
                read_preflight: true,
                ..Default::default()
            },
            Serial {
                read_preflight: true,
                refusals_complete: true,
                ..Default::default()
            },
            Serial {
                no_credential: true,
                ..Default::default()
            },
            Serial {
                first: Some(SerialCall {
                    timeout_ms: 0,
                    ..mutation()
                }),
                ..Default::default()
            },
            Serial {
                first: Some(SerialCall {
                    attempts: 0,
                    ..mutation()
                }),
                ..Default::default()
            },
            Serial {
                first: Some(SerialCall {
                    attempts: 3,
                    ..mutation()
                }),
                ..Default::default()
            },
            Serial {
                pacing: Some(crate::request_context::Pacing {
                    group: 1,
                    minimum_gap_ms: 50.,
                }),
                ..Default::default()
            },
        ] {
            let mut e = engine(Serial { total: 1, ..venue });
            command(&mut e, 0., None);
            assert_eq!(e.venue.admitted, Some(Err(Error::Invalid)));
            assert_eq!(
                e.venue.refusal_spent_nothing,
                Some(true),
                "a refused admission spends nothing"
            );
        }
        let mut e = engine(Serial {
            total: 1,
            pacing: Some(crate::request_context::Pacing {
                group: 1,
                minimum_gap_ms: 0.,
            }),
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a"]);
        assert_eq!(
            done(&actions).as_deref(),
            Some("[a]"),
            "a zero pacing floor is admitted"
        );
        let mut e = engine(Serial {
            total: 1,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let preflight = e.venue.preflight.clone().unwrap();
        let policy = e.venue.policy();
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let before = Serial::snapshot(&cx);
        assert_eq!(
            cx.admit_serial_plan(&preflight, mutation(), policy)
                .map(drop),
            Err(Error::Revoked),
            "outside entry or ready custody"
        );
        assert_eq!(Serial::snapshot(&cx), before);
    }

    #[test]
    fn serial_09_thirty_thousand_call_cpu_control() {
        let mut e = engine(Serial {
            total: 30_000,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let started = std::time::Instant::now();
        let bodies = (0..30_000).map(|i| format!("ok:{i}")).collect::<Vec<_>>();
        let (actions, _) = drive(&mut e, 0., bodies.iter().map(String::as_str));
        eprintln!("serial_09: 30000 calls in {:?}", started.elapsed());
        assert!(done(&actions).is_some());
    }

    #[test]
    fn serial_10_a_refused_continue_leaves_the_call_settleable() {
        let mut e = engine(Serial {
            total: 3,
            on_refused: OnRefusedContinue::FinishThenDone,
            next: VecDeque::from([SerialCall {
                timeout_ms: 0,
                ..mutation()
            }]),
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a"]);
        assert!(e.venue.continued[0].is_err());
        assert_eq!(e.venue.terminal, Some(Ok(())));
        assert_eq!(done(&actions).as_deref(), Some("[a]"));
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn serial_11_an_oversized_id_is_unknown_and_stops() {
        let mut e = engine(Serial {
            total: 5,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "oversized"]);
        let error = failed(&actions).unwrap();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(error.partial.unwrap().order_ids, ["a"]);
        assert!(e.io.order_locks.uncertain("BTCUSD"));
    }

    #[test]
    fn serial_12_known_partial_continues_and_partial_unknown_stops() {
        let mut e = engine(Serial {
            total: 3,
            refusals_complete: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "mixed:b,c", "ok:d"]);
        assert_eq!(done(&actions).as_deref(), Some("[a,b,c,d]"));
        let mut e = engine(Serial {
            total: 3,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "partial-unknown:b"]);
        assert_eq!(
            failed(&actions).unwrap().outcome,
            Some(crate::Outcome::Unknown)
        );
    }

    #[test]
    fn serial_13_a_preflight_admits_at_most_one_plan() {
        let mut e = engine(Serial {
            total: 1,
            probe_second_admission: true,
            ..Default::default()
        });
        command(&mut e, 0., Some(100));
        admitted(&e);
        assert_eq!(e.venue.second_admission, Some(Err(Error::State)));
        let (actions, _) = drive(&mut e, 200., ["ok:a"]);
        assert_eq!(
            done(&actions).as_deref(),
            Some("[a]"),
            "no root deadline from the receipt"
        );
    }

    #[test]
    fn serial_14_sixty_thousand_refusals_with_mixed_batches_retire_evidence() {
        let mut e = engine(Serial {
            total: 60_000,
            refusals_complete: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let bodies = (0..60_000)
            .map(|i| if i % 100 == 0 { "mixed:" } else { "reject" })
            .collect::<Vec<_>>();
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let mut now = 0.;
        for body in &bodies[..59_999] {
            let token = prepared(&actions).unwrap();
            release(&mut e, token, now);
            now += STEP_MS;
            reply(&mut e, token, now, body);
            actions = e.drain_actions().collect();
        }
        let writes = e.io.request_contexts.command_writes(7).unwrap();
        let text = writes
            .rejections()
            .map(|(_, error)| error.message.len())
            .sum::<usize>();
        assert!(text <= 64 * 1024);
        assert!(writes.rejections().count() < 59_999);
        assert_eq!(
            writes.refusal_count(),
            59_999,
            "mixed batches share the count"
        );
        let (actions, _) = drive_from(&mut e, actions, now, ["reject"]);
        assert_eq!(done(&actions).as_deref(), Some("[]"));
    }

    #[test]
    fn serial_16_a_classic_partial_batch_fails_with_its_ids_kept() {
        let mut e = engine(Serial {
            total: 5,
            partial_terminal: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "mixed:b"]);
        assert_eq!(
            failed(&actions).unwrap().partial.unwrap().order_ids,
            ["a", "b"]
        );
    }

    #[test]
    fn serial_17_one_fresh_resend_then_a_third_attempt_is_refused() {
        let mut e = engine(Serial {
            total: 2,
            first: Some(SerialCall {
                attempts: 2,
                ..mutation()
            }),
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let first = prepared(&actions).unwrap();
        let first_stamp = release(&mut e, first, 0.).expect("signed");
        reply(&mut e, first, STEP_MS, "40009");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.resends, [Ok(())]);
        let second = prepared(&actions).expect("a fresh preparation");
        let second_stamp =
            release(&mut e, second, 2. * STEP_MS).expect("signed again by the provider");
        assert_ne!(
            second_stamp, first_stamp,
            "fresh signing, not replayed bytes"
        );
        reply(&mut e, second, 3. * STEP_MS, "40009");
        assert_eq!(e.venue.resends.len(), 2);
        assert!(e.venue.resends[1].is_err(), "a third attempt is refused");
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn serial_17_no_resend_after_a_5xx_or_transport_failure() {
        let mut e = engine(Serial {
            total: 3,
            first: Some(SerialCall {
                attempts: 2,
                ..mutation()
            }),
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["503"]);
        assert!(e.venue.resends[0].is_err(), "no grant after a 5xx");
        let error = failed(&actions).expect("fail_plan projects");
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert!(e.io.order_locks.uncertain("BTCUSD"));
        let mut e = engine(Serial {
            total: 3,
            first: Some(SerialCall {
                attempts: 2,
                ..mutation()
            }),
            forge_resend: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions).unwrap();
        release(&mut e, token, 0.);
        e.handle(
            STEP_MS,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "reset",
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (_, result, unchanged) = e
            .venue
            .probes
            .iter()
            .find(|(name, ..)| *name == "forged resend")
            .unwrap();
        assert!(
            result.is_err() && *unchanged,
            "no grant after a released transport failure"
        );
        assert_eq!(
            failed(&actions).unwrap().outcome,
            Some(crate::Outcome::Unknown)
        );
        assert!(e.io.order_locks.uncertain("BTCUSD"));
    }

    #[test]
    fn serial_17_read_calls_leave_the_journal_and_delays_use_the_plan_timer() {
        let mut e = engine(Serial {
            total: 3,
            next: VecDeque::from([read(), mutation()]),
            delay_ms: Some(250),
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, now) = drive(&mut e, 0., ["ok:a"]);
        assert!(prepared(&actions).is_none(), "the delay is not a live call");
        let timer =
            e.io.request_contexts
                .plan_timer(7)
                .expect("the Plan's one timer");
        assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { timer: t, at_ms } if *t == timer && *at_ms == now + 250.)));
        e.handle(now + 250., Event::TimerFired { timer });
        assert_eq!(e.venue.ready, 1);
        let (_, now) = drive(&mut e, now + 250., ["read"]);
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(now + 250., Event::TimerFired { timer });
        let (actions, _) = drive(&mut e, now + 250., ["ok:b"]);
        assert_eq!(done(&actions).as_deref(), Some("[a,b]"));
        assert_eq!(e.venue.read_requests.len(), 1);
        assert_eq!(
            e.venue.journal_at_done,
            Some((false, 0)),
            "no Read fact and no refusal in the journal"
        );
    }

    #[test]
    fn serial_17_read_failures_retry_at_250_500_750_then_succeed() {
        let mut e = engine(Serial {
            total: 2,
            first: Some(read()),
            retry_failed_reads: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let mut now = 0.;
        for n in 1..=3 {
            let token = prepared(&actions).expect("a fresh Read");
            release(&mut e, token, now);
            now += STEP_MS;
            e.handle(
                now,
                Event::HttpFailed {
                    request: token.attempt().request,
                    error: "reset",
                },
            );
            let set = e.drain_actions().collect::<Vec<_>>();
            let timer = e.io.request_contexts.plan_timer(7).unwrap();
            let delay = 250. * f64::from(n);
            assert!(set.iter().any(|a| matches!(a, Action::SetTimer { timer: t, at_ms } if *t == timer && *at_ms == now + delay)));
            now += delay;
            e.handle(now, Event::TimerFired { timer });
            actions = e.drain_actions().collect();
        }
        assert_eq!(e.venue.ready, 3);
        assert_eq!(e.venue.continued, [Ok(()), Ok(()), Ok(())]);
        for (name, result, unchanged) in &e.venue.probes {
            assert!(result.is_err() && *unchanged, "{name}: {result:?}");
        }
        assert_eq!(
            e.venue.probes.len(),
            6,
            "a Mutation next and a second delay, per failure"
        );
        let (actions, _) = drive_from(&mut e, actions, now, ["read", "ok:a"]);
        assert_eq!(done(&actions).as_deref(), Some("[a]"));
    }

    #[test]
    fn serial_17_a_read_timeout_at_equality_retries_without_corrupting_the_prefix() {
        let mut e = engine(Serial {
            total: 3,
            next: VecDeque::from([read()]),
            retry_failed_reads: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, now) = drive(&mut e, 0., ["ok:a"]);
        let token = prepared(&actions).unwrap();
        let timer = e.io.managed_http[&token.attempt().request].timer;
        release(&mut e, token, now);
        e.handle(now + 20_000., Event::TimerFired { timer });
        assert_eq!(
            e.venue.continued.last(),
            Some(&Ok(())),
            "failure-only ownership at the equality boundary"
        );
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(now + 20_250., Event::TimerFired { timer });
        let (actions, _) = drive(&mut e, now + 20_250., ["read", "ok:b"]);
        assert_eq!(done(&actions).as_deref(), Some("[a,b]"));
    }

    #[test]
    fn serial_15_an_empty_native_job_ends_without_a_plan() {
        struct Empty;
        impl Venue for Empty {
            type Pending = ();
            fn start(&mut self, _: &mut Cx<'_, ()>) {}
            fn on_venue_action(&mut self, cx: &mut Cx<'_, ()>, _: CommandId, _: VenueAction) {
                let preflight = cx
                    .begin_preflight(PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: Some(credential()),
                    })
                    .unwrap();
                let storage = cx.io.request_contexts.plan_context_storage();
                cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                assert_eq!(cx.io.request_contexts.plan_context_storage().2, storage.2);
                assert!(!cx.io.request_contexts.has_plans());
            }
            fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
            fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
                panic!("no attempt for an empty job")
            }
            fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
                panic!("no attempt for an empty job")
            }
            fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
            fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
            fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
            fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
            fn book(&self, _: &str) -> Option<&Book> {
                None
            }
            fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
                Box::new(std::iter::empty())
            }
        }
        let mut e = crate::test_support::seeded_engine(Empty);
        e.start(0.);
        e.handle_command(
            0.,
            Command::Venue {
                command: 7,
                action: VenueAction::FetchBalanceInfo,
            },
            None,
        )
        .unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")));
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    }

    // RED19 stops. A provider refusing the next call's credential stops the
    // chain at that call's own failure custody, even when the venue does nothing.
    #[test]
    fn serial_19_a_credential_refusal_stops_the_chain_with_every_streamed_id() {
        let mut e = engine(Serial {
            total: 5,
            idle_failures: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let (actions, now) = drive(&mut e, 0., ["ok:a", "ok:b"]);
        let token = prepared(&actions).expect("the third call is prepared");
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let signing = e.drain_actions().collect::<Vec<_>>();
        let (attempt, _) = work(&signing);
        e.handle(
            now,
            Event::Signed {
                attempt,
                result: Err(crate::signing::SignError::Refused),
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions).expect("one terminal at the refused call's custody");
        assert_eq!(error.partial.unwrap().order_ids, ["a", "b"]);
        assert_ne!(
            error.outcome,
            Some(crate::Outcome::Unknown),
            "nothing live was released"
        );
        assert!(
            prepared(&actions).is_none(),
            "nothing after the stop is sent"
        );
        assert!(
            e.io.request_contexts.plan_timer(7).is_none(),
            "the Plan is gone"
        );
    }

    #[test]
    fn serial_07_refusals_past_the_text_budget_are_marked_in_the_failure() {
        let bodies = vec!["reject"; 7_000];
        let actions = refusals(false, false, &bodies);
        let error = failed(&actions).expect("all-refused without the flag fails");
        assert_eq!(error.outcome, Some(crate::Outcome::Rejected));
        assert!(error.message.contains("[truncated: "), "{}", error.message);
    }

    #[test]
    fn serial_17_read_calls_sign_with_the_read_role_or_go_unsigned() {
        for (read_key, signer) in [(true, Some("read-fixture".to_owned())), (false, None)] {
            let mut e = engine(Serial {
                total: 3,
                read_key,
                next: VecDeque::from([read(), mutation()]),
                ..Default::default()
            });
            command(&mut e, 0., None);
            admitted(&e);
            let (actions, _) = drive(&mut e, 0., ["ok:a", "read", "ok:b"]);
            assert_eq!(done(&actions).as_deref(), Some("[a,b]"));
            assert_eq!(e.venue.read_signers, [signer], "never the write key");
        }
    }

    #[test]
    fn serial_17_a_provider_refused_read_cannot_retry() {
        let mut e = engine(Serial {
            total: 2,
            first: Some(read()),
            retry_failed_reads: true,
            read_key: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions).unwrap();
        e.http_prepared(
            0.,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let signing = e.drain_actions().collect::<Vec<_>>();
        let (attempt, _) = work(&signing);
        e.handle(
            0.,
            Event::Signed {
                attempt,
                result: Err(crate::signing::SignError::Refused),
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.failures, 1);
        assert_eq!(
            e.venue.continued,
            [Err(Error::Revoked)],
            "a Read that never reached the venue cannot retry"
        );
        assert!(prepared(&actions).is_none(), "no fresh Read");
        assert_eq!(
            failed(&actions).unwrap().outcome,
            Some(crate::Outcome::NotSent)
        );
    }

    #[test]
    fn serial_17_a_mutation_failure_never_takes_the_read_retry() {
        let mut e = engine(Serial {
            total: 3,
            retry_failed_reads: true,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions).unwrap();
        release(&mut e, token, 0.);
        e.handle(
            STEP_MS,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "reset",
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            matches!(e.venue.continued.first(), Some(Err(_))),
            "{:?}",
            e.venue.continued
        );
        assert!(prepared(&actions).is_none());
        assert_eq!(
            failed(&actions).unwrap().outcome,
            Some(crate::Outcome::Unknown)
        );
    }

    // A serial slot never parks on a Session, so retiring one mid-chain strands
    // nothing and the chain finishes on its own custody.
    #[test]
    fn serial_19_b_session_retirement_strands_nothing_in_a_serial_plan() {
        let mut e = engine(Serial {
            total: 3,
            ..Default::default()
        });
        e.start(0.);
        let session = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .open_session(crate::request_context::SessionPolicy {
            credential: None,
            repeat_safe: true,
            private_egress: None,
        })
        .unwrap();
        e.venue.session = Some(session);
        command(&mut e, 0., None);
        admitted(&e);
        assert_eq!(e.venue.session_park, Some(Err(Error::State)));
        let (actions, _) = drive(&mut e, 0., ["ok:a", "ok:b", "ok:c"]);
        assert_eq!(
            e.venue.session_retired,
            Some(0),
            "no serial call was parked"
        );
        assert_eq!(done(&actions).as_deref(), Some("[a,b,c]"));
    }

    #[test]
    fn serial_18_caller_wait_expiry_is_not_a_terminal() {
        let mut e = engine(Serial {
            total: 4,
            ..Default::default()
        });
        command(&mut e, 0., Some(150));
        admitted(&e);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let mut now = 0.;
        for (index, body) in ["ok:a", "ok:b", "ok:c", "ok:d"].into_iter().enumerate() {
            let token = prepared(&actions).unwrap();
            release(&mut e, token, now);
            now += STEP_MS;
            reply(&mut e, token, now, body);
            actions = e.drain_actions().collect();
            if index < 3 {
                assert!(
                    failed(&actions).is_none() && done(&actions).is_none(),
                    "no terminal at {now}"
                );
            }
        }
        assert!(now > 150.);
        assert_eq!(done(&actions).as_deref(), Some("[a,b,c,d]"));
        #[cfg(feature = "order-locks")]
        assert!(
            e.venue.lock_held.iter().all(|held| *held),
            "lock held across the expired wait"
        );
    }
}
