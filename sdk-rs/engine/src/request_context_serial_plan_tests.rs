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
        command_wait: bool,
        code1: bool,
        partial_terminal: bool,
        on_refused: OnRefusedContinue,
        delay_ms: Option<u32>,
        retry_failed_reads: bool,
        probe_custody: bool,
        probe_second_admission: bool,
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
    }

    fn queue(cx: &mut Cx<'_, Context>, context: Context, signed: bool) {
        let result = if signed {
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Box::new(Recipe),
                context,
            )
        } else {
            cx.request_managed(
                context.clone(),
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
            SerialPolicy {
                credential: credential(),
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
                read_credential: None,
                write_credential: (!self.read_preflight && !self.no_credential).then(credential),
            };
            let preflight = if self.command_wait {
                cx.begin_preflight_with_command_wait(policy)
            } else {
                cx.begin_preflight(policy)
            };
            let Ok(preflight) = preflight else {
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
                    self.plan = Some(plan);
                    self.slot = Some(slot);
                    self.issue(cx, first);
                }
                Err(error) => {
                    assert_eq!(
                        Self::snapshot(cx),
                        before,
                        "a refused admission spends nothing"
                    );
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
            queue(cx, context, call.operation == Operation::Mutation);
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
            self.terminal = Some(if done {
                cx.done_plan(&plan, format!("[{}]", self.job_ids.join(",")))
            } else {
                cx.fail_plan(&plan, "venue stopped")
            });
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
                cx.set_timer(1.);
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
            let stop = match parse(response.body) {
                Reply::Read => {
                    assert_eq!(self.current.unwrap().operation, Operation::Read);
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
            if self.probe_custody {
                self.probe(cx, "failure callback", &context);
            }
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            if self.retry_failed_reads {
                let before = Self::snapshot(cx);
                let result = cx.continue_serial_plan_after(&plan, &slot, &context, 250, mutation());
                self.probes.push((
                    "mutation next after a read failure",
                    result,
                    Self::snapshot(cx) == before,
                ));
                let result = cx.continue_serial_plan_after(&plan, &slot, &context, 250, read());
                self.continued.push(result);
                if result.is_ok() {
                    self.current = Some(read());
                    return;
                }
            }
            cx.finish_plan_failed_call(&plan, &slot, &context).unwrap();
            self.terminal = Some(cx.fail_plan(&plan, "call failed"));
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
        engine
            .handle_command(
                now,
                Command::Venue {
                    command: 7,
                    action: VenueAction::FetchBalanceInfo,
                },
                wait_ms,
            )
            .unwrap();
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
    fn release(engine: &mut Engine<Serial>, token: Preparation, now: f64) {
        engine.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        if actions.iter().any(|a| matches!(a, Action::Sign { .. })) {
            let (attempt, work) = work(&actions);
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
        mut now: f64,
        bodies: impl IntoIterator<Item = &'a str>,
    ) -> (Vec<Action>, f64) {
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
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
        let timer =
            e.io.timers
                .keys()
                .copied()
                .find(|t| e.io.request_contexts.plan_timer(7) != Some(*t));
        if let Some(timer) = timer {
            e.handle(0.5, Event::TimerFired { timer });
        }
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
        let (actions, _) = drive(&mut e, now, ["reject"]);
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
            assert!(
                matches!(e.venue.admitted, Some(Err(Error::Invalid))),
                "{:?}",
                e.venue.admitted
            );
        }
        let mut e = engine(Serial {
            total: 1,
            ..Default::default()
        });
        command(&mut e, 0., None);
        admitted(&e);
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let preflight = e.venue.preflight.clone().unwrap();
        let before = Serial::snapshot(&cx);
        assert!(
            cx.admit_serial_plan(&preflight, mutation(), e.venue.policy())
                .is_err(),
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
    fn serial_13_write_after_reads_command_wait_binds_the_entry_receipt() {
        let mut e = engine(Serial {
            total: 1,
            command_wait: true,
            probe_second_admission: true,
            ..Default::default()
        });
        command(&mut e, 0., Some(100));
        admitted(&e);
        let preflight = e.venue.preflight.clone().unwrap();
        assert_eq!(
            e.io.request_contexts
                .preflight_receipt_cap(&preflight)
                .unwrap()
                .map(|cap| cap.1),
            Some(100.)
        );
        assert_eq!(e.venue.second_admission, Some(Err(Error::State)));
        let (actions, _) = drive(&mut e, 200., ["ok:a"]);
        assert_eq!(
            done(&actions).as_deref(),
            Some("[a]"),
            "no root deadline taken from the receipt"
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
        let (actions, _) = drive(&mut e, now, ["reject"]);
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
        release(&mut e, first, 0.);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        reply(&mut e, first, STEP_MS, "40009");
        actions.extend(e.drain_actions());
        assert_eq!(e.venue.resends, [Ok(())]);
        let second = prepared(&actions).expect("a fresh preparation, not replayed bytes");
        assert_ne!(second.attempt().request, first.attempt().request);
        release(&mut e, second, STEP_MS);
        reply(&mut e, second, 2. * STEP_MS, "40009");
        assert_eq!(e.venue.resends.len(), 2);
        assert!(e.venue.resends[1].is_err(), "a third attempt is refused");
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
        e.handle(now + 250., Event::TimerFired { timer });
        assert_eq!(e.venue.ready, 1);
        let (actions, now) = drive(&mut e, now + 250., ["read"]);
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(now + 250., Event::TimerFired { timer });
        let _ = actions;
        let (actions, _) = drive(&mut e, now + 250., ["ok:b"]);
        assert_eq!(done(&actions).as_deref(), Some("[a,b]"));
    }

    #[test]
    fn serial_17_a_read_failure_retries_once_through_the_plan_timer() {
        let mut e = engine(Serial {
            total: 2,
            first: Some(read()),
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
        assert_eq!(e.venue.continued, [Ok(())]);
        let (name, result, unchanged) = &e.venue.probes[0];
        assert!(result.is_err() && *unchanged, "{name}");
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(STEP_MS + 250., Event::TimerFired { timer });
        let (actions, _) = drive(&mut e, STEP_MS + 250., ["read"]);
        let _ = actions;
        assert_eq!(e.venue.ready, 1);
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

    #[test]
    fn serial_18_caller_wait_expiry_is_not_a_terminal() {
        let mut e = engine(Serial {
            total: 4,
            ..Default::default()
        });
        command(&mut e, 0., Some(150));
        admitted(&e);
        let (actions, _) = drive(&mut e, 0., ["ok:a", "ok:b", "ok:c", "ok:d"]);
        assert_eq!(done(&actions).as_deref(), Some("[a,b,c,d]"));
    }
}
