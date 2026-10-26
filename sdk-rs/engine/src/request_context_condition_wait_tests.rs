// Condition wait REDs (docs/plans/RUST_CONDITION_WAIT.md). Each test name
// carries its RED number; every owner enters through a genuine command
// callback, and every call runs real Prepare/Sign/Bind/Claim.
#[allow(clippy::result_large_err)]
mod condition_waits {
    use super::*;
    use crate::request_context::{
        ConditionOwner, ConditionWait, Owner, Plan, PreflightPolicy, PreflightPurpose,
        plan::{SerialCall, SerialPolicy, Slot},
    };

    const STEP_MS: f64 = 100.;
    const NATIVE: &str = "Bitget account detection did not complete";

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
    fn native() -> VenueError {
        VenueError::new(NATIVE).outcome(crate::Outcome::NotSent)
    }
    fn command_of(owner: Owner) -> CommandId {
        match owner {
            Owner::Command { id, .. } => id,
            _ => 0,
        }
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    enum OnReady {
        #[default]
        Admit,
        Repark(Option<u32>),
        Fail,
    }
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    enum OnExpired {
        #[default]
        Fail,
        Decline,
        /// Fail the expired owner and wake every other parked Preflight.
        FanOut,
    }
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    enum Release {
        /// Wake every parked owner, in park order, from the borrowed callback.
        #[default]
        All,
        None,
    }

    #[derive(Default)]
    struct Cond {
        // Terms.
        park: bool,
        bound: Option<u32>,
        bounds: Vec<Option<u32>>,
        read_preflight: bool,
        balance_command: bool,
        total: usize,
        serial_parks: usize,
        serial_bound: Option<u32>,
        on_ready: OnReady,
        on_expired: OnExpired,
        release: Release,
        release_in_park: bool,
        release_on_timer: bool,
        park_next: Option<SerialCall>,
        issued: Vec<Operation>,
        park_on_timer: bool,
        last_data: Option<(CommandId, Context)>,
        second_waits: Vec<Result<(), Error>>,
        timer_parks: Vec<(Result<(), Error>, bool)>,
        delayed_before_park: bool,
        stop_after_release: bool,
        // State.
        entries: Vec<(CommandId, crate::request_context::Preflight)>,
        parked: Vec<CommandId>,
        plans: Vec<(CommandId, Plan, Slot)>,
        data: Vec<(CommandId, usize)>,
        waits: Vec<ConditionWait>,
        job_ids: Vec<String>,
        // Observations.
        parks: Vec<Result<(), Error>>,
        serial_waits: Vec<Result<(), Error>>,
        releases: Vec<Result<(), Error>>,
        admissions: Vec<Result<(), Error>>,
        ready: Vec<CommandId>,
        ready_in_callback: bool,
        in_callback: bool,
        plan_ready: usize,
        expired: Vec<CommandId>,
        probes: Vec<(&'static str, Result<(), Error>, bool)>,
        storage: Vec<(usize, usize, usize)>,
        foreign: Option<Context>,
        entry_refusals: usize,
    }

    fn queue(cx: &mut Cx<'_, Context>, context: Context) {
        let result = if context.credential().is_some() {
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

    type Snapshot = (
        crate::resource_ids::ResourceIds,
        usize,
        (usize, usize, usize),
    );
    fn snapshot(cx: &Cx<'_, Context>) -> Snapshot {
        (
            cx.io.core.next_id,
            cx.io.core.actions.len(),
            cx.io.core.request_contexts.plan_context_storage(),
        )
    }

    impl Cond {
        fn preflight(&self, id: CommandId) -> Option<crate::request_context::Preflight> {
            self.entries
                .iter()
                .find(|(entry, _)| *entry == id)
                .map(|(_, preflight)| preflight.clone())
        }
        fn plan(&self, id: CommandId) -> Option<(Plan, Slot)> {
            self.plans
                .iter()
                .rev()
                .find(|(entry, ..)| *entry == id)
                .map(|(_, plan, slot)| (plan.clone(), slot.clone()))
        }
        fn park_bound(&mut self) -> Option<u32> {
            if self.bounds.is_empty() {
                self.bound
            } else {
                self.bounds.remove(0)
            }
        }
        fn admit(&mut self, cx: &mut Cx<'_, Context>, id: CommandId) {
            let preflight = self.preflight(id).expect("entered owner");
            let admitted = cx.admit_serial_plan(
                &preflight,
                mutation(),
                SerialPolicy {
                    credential: Some(credential()),
                    pacing: None,
                    refusals_complete: false,
                },
            );
            self.admissions
                .push(admitted.as_ref().map(drop).map_err(|error| *error));
            if let Ok((plan, slot)) = admitted {
                self.plans.push((id, plan, slot));
                self.issue(cx, id);
            }
        }
        fn issue(&mut self, cx: &mut Cx<'_, Context>, id: CommandId) {
            let (plan, slot) = self.plan(id).expect("admitted Plan");
            self.storage
                .push(cx.io.core.request_contexts.plan_context_storage());
            if let Ok(context) = cx.plan_leg_context(&plan, &slot) {
                self.issued.push(context.operation());
                queue(cx, context);
            }
        }
        /// The borrowed release: wake parked Preflights in order, or a Plan.
        fn release_all(&mut self, cx: &mut Cx<'_, Context>) {
            if self.release == Release::None {
                return;
            }
            for wait in std::mem::take(&mut self.waits) {
                self.releases.push(cx.release_condition_wait(&wait));
            }
            for id in std::mem::take(&mut self.parked) {
                if let Some(preflight) = self.preflight(id) {
                    self.releases.push(cx.request_preflight_ready(&preflight));
                    if self.stop_after_release {
                        self.releases.push(cx.fail_preflight(&preflight, native()));
                    }
                }
            }
        }
        fn park_serial(&mut self, cx: &mut Cx<'_, Context>, id: CommandId, context: &Context) {
            let (plan, slot) = self.plan(id).expect("admitted Plan");
            let next = self.park_next.unwrap_or_else(mutation);
            let result =
                cx.serial_plan_condition_wait(&plan, &slot, context, next, self.serial_bound);
            self.serial_waits
                .push(result.as_ref().map(drop).map_err(|error| *error));
            if let Ok(wait) = result {
                self.second_waits.push(
                    cx.serial_plan_condition_wait(
                        &plan,
                        &slot,
                        context,
                        mutation(),
                        self.serial_bound,
                    )
                    .map(drop),
                );
                self.waits.push(wait);
            } else {
                let _ = cx.finish_plan_call(&plan, &slot, context);
                let _ = cx.fail_plan(&plan, "condition wait refused");
            }
        }
    }

    impl Venue for Cond {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
            let id = match &command {
                Command::PlaceOrders { command, .. } => *command,
                _ => return,
            };
            self.enter(cx, id);
        }
        fn on_venue_action(&mut self, cx: &mut Cx<'_, Context>, id: CommandId, _: VenueAction) {
            self.enter(cx, id);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            if self.foreign.as_ref() == Some(&context) {
                let _ = cx.finish_http_context(&context);
                self.release_all(cx);
                return;
            }
            let id = command_of(context.owner());
            let Some((plan, slot)) = self.plan(id) else {
                return;
            };
            let _ = cx.validate_plan_delivery(&plan, &context);
            let ids = response
                .body
                .strip_prefix("ok:")
                .unwrap_or_default()
                .split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if response.body == "unknown" {
                let _ = cx.http_write_unproven(&context, "reply does not prove the write");
            } else {
                self.job_ids.extend(ids.iter().cloned());
                let _ = cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Accepted { order_ids: ids },
                );
            }
            let seen = match self.data.iter_mut().find(|(entry, _)| *entry == id) {
                Some((_, seen)) => {
                    *seen += 1;
                    *seen
                }
                None => {
                    self.data.push((id, 1));
                    1
                }
            };
            if response.body == "unknown" || self.delayed_before_park {
                let refused = if self.delayed_before_park {
                    cx.continue_serial_plan_after(&plan, &slot, &context, 1_000, mutation())
                        .map(|()| "with a delayed continuation armed")
                        .unwrap_or("after a refused delayed continuation")
                } else {
                    "while a fact is uncertain"
                };
                let before = snapshot(cx);
                let result = cx
                    .serial_plan_condition_wait(&plan, &slot, &context, mutation(), None)
                    .map(drop);
                self.probes.push((refused, result, snapshot(cx) == before));
                if !self.delayed_before_park {
                    let _ = cx.fail_plan(&plan, "uncertain write");
                }
                return;
            }
            self.last_data = Some((id, context.clone()));
            if seen <= self.serial_parks {
                self.park_serial(cx, id, &context);
                if self.release_in_park {
                    self.in_callback = true;
                    self.release_all(cx);
                    self.in_callback = false;
                }
                return;
            }
            if seen >= self.total.max(1) {
                let _ = cx.finish_plan_call(&plan, &slot, &context);
                let _ = cx.done_plan(&plan, format!("[{}]", self.job_ids.join(",")));
                // The next queued owner wakes when this job ends.
                self.release_all(cx);
                return;
            }
            if let Ok(slot) = cx.continue_serial_plan(&plan, &slot, &context, mutation()) {
                self.plans.push((id, plan, slot));
                self.issue(cx, id);
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            let id = command_of(context.owner());
            if let Some((plan, slot)) = self.plan(id)
                && cx.finish_plan_failed_call(&plan, &slot, &context).is_ok()
            {
                let _ = cx.fail_plan(&plan, "call failed");
            }
        }
        fn on_http_preflight_ready(
            &mut self,
            cx: &mut Cx<'_, Context>,
            preflight: &crate::request_context::Preflight,
        ) {
            let id = command_of(preflight.owner());
            self.ready.push(id);
            self.ready_in_callback |= self.in_callback;
            match self.on_ready {
                OnReady::Admit => self.admit(cx, id),
                OnReady::Repark(bound) => {
                    self.on_ready = OnReady::Admit;
                    let result = cx.park_preflight_for(preflight, bound);
                    self.parks.push(result);
                    if result.is_ok() {
                        self.parked.push(id);
                    }
                }
                OnReady::Fail => {
                    let _ = cx.fail_preflight(preflight, native());
                }
            }
        }
        fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Context>, plan: &Plan) {
            self.plan_ready += 1;
            self.ready_in_callback |= self.in_callback;
            let id = command_of(plan.owner());
            if let Some(slot) = cx.plan_slots(plan).ok().and_then(|mut slots| slots.pop()) {
                self.plans.push((id, plan.clone(), slot));
                self.issue(cx, id);
            }
        }
        fn on_condition_wait_expired(
            &mut self,
            cx: &mut Cx<'_, Context>,
            owner: &ConditionOwner,
            _: VenueError,
        ) {
            let id = match owner {
                ConditionOwner::Preflight(preflight) => command_of(preflight.owner()),
                ConditionOwner::Plan(plan) => command_of(plan.owner()),
            };
            self.expired.push(id);
            if self.on_expired == OnExpired::Decline {
                return;
            }
            match owner {
                ConditionOwner::Preflight(preflight) => {
                    let _ = cx.fail_preflight(preflight, native());
                }
                ConditionOwner::Plan(plan) => {
                    let _ = cx.fail_plan(plan, native());
                }
            }
            if self.on_expired == OnExpired::FanOut {
                self.parked.retain(|parked| *parked != id);
                self.on_ready = OnReady::Fail;
                self.release_all(cx);
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, _: TimerId) {
            if self.park_on_timer
                && let Some((id, context)) = self.last_data.clone()
                && let Some((plan, slot)) = self.plan(id)
            {
                let before = snapshot(cx);
                let result = cx
                    .serial_plan_condition_wait(&plan, &slot, &context, mutation(), None)
                    .map(drop);
                self.timer_parks.push((result, snapshot(cx) == before));
            }
            if self.release_on_timer {
                self.release_all(cx);
            }
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {}
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
            crate::request_context::ReconciliationData::new("condition-native".into())
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

    impl Cond {
        fn enter(&mut self, cx: &mut Cx<'_, Context>, id: CommandId) {
            let policy = PreflightPolicy {
                purpose: if self.read_preflight {
                    PreflightPurpose::ReadOnly
                } else {
                    PreflightPurpose::WriteAfterReads
                },
                read_credential: None,
                write_credential: (!self.read_preflight).then(credential),
            };
            let Ok(preflight) = cx.begin_preflight(policy) else {
                self.entry_refusals += 1;
                return;
            };
            self.entries.push((id, preflight.clone()));
            if !self.park {
                self.admit(cx, id);
                return;
            }
            let bound = self.park_bound();
            let result = cx.park_preflight_for(&preflight, bound);
            self.parks.push(result);
            if result.is_ok() {
                self.parked.push(id);
                if self.release_in_park {
                    self.in_callback = true;
                    self.release_all(cx);
                    self.in_callback = false;
                }
                if self.release_on_timer {
                    cx.set_timer(1_000.);
                }
            }
        }
    }

    // Engine driving.
    fn engine(venue: Cond) -> Engine<Cond> {
        let mut e = crate::test_support::seeded_engine(venue);
        e.start(0.);
        e
    }
    fn place(e: &mut Engine<Cond>, now: f64, id: CommandId) {
        let command = if e.venue.balance_command {
            Command::Venue {
                command: id,
                action: VenueAction::FetchBalanceInfo,
            }
        } else {
            Command::PlaceOrders {
                command: id,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            }
        };
        let _ = e.handle_command(now, command, None);
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
    fn release(engine: &mut Engine<Cond>, token: Preparation, now: f64) {
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
        if let Some(permit) = actions.iter().find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Bind { permit, .. } => Some(*permit),
                _ => None,
            },
            _ => None,
        }) {
            let _ = engine.claim_http(now, permit);
        }
    }
    fn reply(engine: &mut Engine<Cond>, token: Preparation, now: f64, body: &str) {
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
    /// Release and answer each prepared call in turn, from already drained
    /// actions; returns every action after the last reply and the clock.
    fn drive<'a>(
        engine: &mut Engine<Cond>,
        mut actions: Vec<Action>,
        mut now: f64,
        bodies: impl IntoIterator<Item = &'a str>,
    ) -> (Vec<Action>, f64) {
        for body in bodies {
            let Some(token) = prepared(&actions) else {
                break;
            };
            release(engine, token, now);
            now += STEP_MS;
            reply(engine, token, now, body);
            actions = engine.drain_actions().collect();
        }
        (actions, now)
    }
    /// A foreign Background read whose `on_response` is the borrowed callback
    /// that releases parked owners. Returns every action from `now` on.
    fn borrowed_read(engine: &mut Engine<Cond>, now: f64) -> Vec<Action> {
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        let mut cx = Cx {
            now,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        let context = cx
            .http_context(
                crate::request_context::Scope::Background,
                crate::request_context::Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        engine.venue.foreign = Some(context.clone());
        let mut cx = Cx {
            now,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        queue(&mut cx, context);
        let own = engine.drain_actions().collect::<Vec<_>>();
        let token = prepared(&own).expect("the foreign read is prepared");
        release(engine, token, now);
        reply(engine, token, now + 1., "foreign");
        actions.extend(engine.drain_actions());
        actions
    }
    fn timer_at(actions: &[Action], at: f64) -> Option<TimerId> {
        actions.iter().find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
    }
    fn terminals(actions: &[Action], id: CommandId) -> Vec<Result<String, VenueError>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                    Some(Err(error.clone()))
                }
                _ => None,
            })
            .collect()
    }
    fn native_failure(result: &[Result<String, VenueError>]) -> bool {
        matches!(result, [Err(error)]
            if error.message == NATIVE && error.outcome == Some(crate::Outcome::NotSent))
    }

    // Fixture control (GREEN): the borrowed read really reaches `on_response`.
    #[test]
    fn condition_00_the_borrowed_read_fixture_reaches_on_response() {
        let mut e = engine(Cond {
            release: Release::None,
            ..Default::default()
        });
        let actions = borrowed_read(&mut e, 0.);
        assert!(e.venue.foreign.is_some());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }

    #[test]
    fn condition_01_a_parked_preflight_wakes_from_a_borrowed_read_and_admits() {
        let mut e = engine(Cond {
            park: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(())], "a WriteAfterReads Preflight parks");
        assert!(prepared(&actions).is_none(), "a parked owner holds no I/O");
        let actions = borrowed_read(&mut e, 1_000.);
        assert_eq!(e.venue.ready, [7], "ready in the same flush");
        assert_eq!(e.venue.admissions, [Ok(())]);
        let (actions, _) = drive(&mut e, actions, 1_000., ["ok:a"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a]".to_owned())]);
    }

    #[test]
    fn condition_02_a_bounded_park_expires_once_with_the_native_message() {
        let mut e = engine(Cond {
            park: true,
            bound: Some(10_000),
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(())]);
        let timer = timer_at(&actions, 10_000.).expect("one engine timer at now + bound");
        e.handle(10_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.expired, [7]);
        assert!(native_failure(&terminals(&actions, 7)), "{actions:?}");
        let late = borrowed_read(&mut e, 10_001.);
        assert!(e.venue.ready.is_empty(), "a late release is a no-op");
        assert!(terminals(&late, 7).is_empty());
    }

    #[test]
    fn condition_03_a_serial_plan_parks_between_calls_and_continues_on_release() {
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (actions, now) = drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(
            e.venue.serial_waits,
            [Ok(())],
            "a definitive fact parks the Plan"
        );
        assert!(prepared(&actions).is_none(), "no live call while parked");
        let parked = e.io.core.request_contexts.plan_context_storage();
        let actions = borrowed_read(&mut e, now + 1_000.);
        assert_eq!(e.venue.plan_ready, 1, "released into on_http_plan_ready");
        let (actions, _) = drive(&mut e, actions, now + 1_000., ["ok:b"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a,b]".to_owned())]);
        assert!(
            e.venue
                .storage
                .iter()
                .all(|s| s.0 + s.2 <= parked.0 + parked.2 + 1)
        );
    }

    #[test]
    fn condition_04_parking_a_serial_plan_is_refused_without_custody() {
        // Positive control first: a park in exact custody is accepted.
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        for (venue, body) in [
            (Cond::default(), "unknown"),
            (
                Cond {
                    delayed_before_park: true,
                    total: 3,
                    ..Default::default()
                },
                "ok:a",
            ),
        ] {
            let mut e = engine(venue);
            place(&mut e, 0., 7);
            let actions = e.drain_actions().collect::<Vec<_>>();
            drive(&mut e, actions, 0., [body]);
            let (name, result, unchanged) = e.venue.probes[0];
            assert!(result.is_err() && unchanged, "{name}: {result:?}");
        }
        // Outside custody: a borrowed timer callback while the next call is live.
        let mut e = engine(Cond {
            total: 3,
            park_on_timer: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        drive(&mut e, actions, 0., ["ok:a"]);
        let timer = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .set_timer(1.);
        e.handle(2., Event::TimerFired { timer });
        assert!(
            matches!(e.venue.timer_parks.as_slice(), [(Err(_), true)]),
            "a borrowed callback cannot park: {:?}",
            e.venue.timer_parks
        );
        // Outside custody: from the test, with no delivery loaned.
        let (plan, slot) = e.venue.plan(7).unwrap();
        let context = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .plan_leg_context(&plan, &slot);
        assert!(
            context.is_err(),
            "a parked Plan issues nothing outside its ready custody"
        );
    }

    #[test]
    fn condition_24_a_parked_plan_issues_the_next_call_it_named_on_release() {
        let mut e = engine(Cond {
            total: 3,
            serial_parks: 1,
            park_next: Some(SerialCall {
                operation: Operation::Read,
                ..mutation()
            }),
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (_, now) = drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        borrowed_read(&mut e, now + 1_000.);
        assert_eq!(
            e.venue.issued.last(),
            Some(&Operation::Read),
            "a replan re-reads after release: {:?}",
            e.venue.issued
        );
    }

    #[test]
    fn condition_05_stale_double_and_second_waits_are_refused() {
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        assert_eq!(
            e.venue.second_waits,
            [Err(Error::State)],
            "one wait at a time per owner"
        );
        let wait = e.venue.waits[0].clone();
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(cx.release_condition_wait(&wait), Ok(()));
        let before = snapshot(&cx);
        assert_eq!(
            cx.release_condition_wait(&wait),
            Err(Error::Revoked),
            "double release"
        );
        assert_eq!(snapshot(&cx), before);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (actions, _) = drive(&mut e, actions, 1., ["ok:b"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a,b]".to_owned())]);
        let mut cx = Cx {
            now: 2.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(
            cx.release_condition_wait(&wait),
            Err(Error::Revoked),
            "a sealed generation's handle"
        );
    }

    #[test]
    fn condition_06_a_parked_plan_expires_with_no_fence() {
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            serial_bound: Some(500),
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (actions, now) = drive(&mut e, actions, 0., ["ok:"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        let timer = timer_at(&actions, now + 500.).expect("the Plan's one timer");
        e.handle(now + 500., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.expired, [7]);
        let result = terminals(&actions, 7);
        assert!(matches!(&result[..], [Err(error)] if error.message.starts_with(NATIVE)));
        #[cfg(feature = "order-locks")]
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"), "nothing was live");
    }

    // RED7's stops have no engine entry for a parked owner today (shutdown and
    // caller cancel); this pins the one that exists: a parked owner's own
    // failure-only cleanup from a loan-free callback ends it with one terminal.
    #[test]
    fn condition_07_a_stop_ends_a_parked_preflight_with_one_terminal() {
        let mut e = engine(Cond {
            park: true,
            release: Release::None,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks, [Ok(())]);
        let preflight = e.venue.preflight(7).unwrap();
        Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .fail_preflight(&preflight, native())
        .unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(native_failure(&terminals(&actions, 7)));
    }

    #[test]
    fn condition_08_computed_bounds_expire_at_one_shared_deadline() {
        let mut e = engine(Cond {
            park: true,
            bounds: vec![Some(10_000), Some(4_000), Some(1_000)],
            ..Default::default()
        });
        place(&mut e, 0., 7);
        place(&mut e, 6_000., 8);
        place(&mut e, 9_000., 9);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(()), Ok(()), Ok(())]);
        let timers = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 10_000. => Some(*timer),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(timers.len(), 3, "every bound lands on the shared deadline");
        for timer in timers {
            e.handle(10_000., Event::TimerFired { timer });
        }
        assert_eq!(e.venue.expired, [7, 8, 9]);
    }

    #[test]
    fn condition_09_an_unbounded_park_outlives_the_caller_wait() {
        let mut e = engine(Cond {
            park: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(())]);
        assert!(timer_at(&actions, 120_000.).is_none(), "no engine bound");
        let actions = borrowed_read(&mut e, 130_000.);
        assert_eq!(
            e.venue.ready,
            [7],
            "still parked after the 120 s caller wait"
        );
        let (actions, _) = drive(&mut e, actions, 130_000., ["ok:a"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a]".to_owned())]);
    }

    #[test]
    fn condition_10_a_mid_plan_expiry_keeps_the_accepted_ids() {
        let mut e = engine(Cond {
            total: 3,
            serial_parks: 2,
            serial_bound: Some(500),
            release: Release::None,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (actions, now) = drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        let timer = timer_at(&actions, now + 500.).unwrap();
        e.handle(now + 500., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let result = terminals(&actions, 7);
        let [Err(error)] = &result[..] else {
            panic!("one failure: {result:?}");
        };
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["a"]);
        assert_ne!(error.outcome, Some(crate::Outcome::NotSent));
    }

    #[test]
    fn condition_11_unbounded_parks_end_through_release_in_queue_order() {
        let mut e = engine(Cond {
            park: true,
            ..Default::default()
        });
        for id in [7, 8, 9] {
            place(&mut e, 0., id);
        }
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks, [Ok(()), Ok(()), Ok(())]);
        let actions = borrowed_read(&mut e, 1_000.);
        assert_eq!(e.venue.ready, [7, 8, 9], "FIFO wakes");
        let mut done = Vec::new();
        // All three admit in one flush, so their calls are prepared together.
        let tokens = actions
            .iter()
            .filter_map(|a| prepared(std::slice::from_ref(a)))
            .collect::<Vec<_>>();
        for token in tokens {
            release(&mut e, token, 1_000.);
            reply(&mut e, token, 1_000. + STEP_MS, "ok:x");
            let after = e.drain_actions().collect::<Vec<_>>();
            done.extend(
                [7, 8, 9]
                    .into_iter()
                    .filter(|id| !terminals(&after, *id).is_empty()),
            );
        }
        assert_eq!(done, [7, 8, 9]);
        assert!(
            !e.io.core.request_contexts.has_plans(),
            "no owner, wait or timer leaks"
        );
    }

    #[test]
    fn condition_12_detection_expiry_fans_out_in_queue_order() {
        let mut e = engine(Cond {
            park: true,
            bounds: vec![Some(10_000), None, None],
            on_expired: OnExpired::FanOut,
            ..Default::default()
        });
        for id in [7, 8, 9] {
            place(&mut e, 0., id);
        }
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(()), Ok(()), Ok(())]);
        let timer = timer_at(&actions, 10_000.).unwrap();
        e.handle(10_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.expired, [7]);
        assert_eq!(
            e.venue.ready,
            [8, 9],
            "all wake in this flush, in queue order"
        );
        for id in [7, 8, 9] {
            assert!(native_failure(&terminals(&actions, id)), "{id}");
        }
    }

    #[test]
    fn condition_13_a_repark_from_ready_custody_is_progress() {
        let mut e = engine(Cond {
            park: true,
            on_ready: OnReady::Repark(None),
            ..Default::default()
        });
        place(&mut e, 0., 7);
        e.drain_actions().for_each(drop);
        let actions = borrowed_read(&mut e, 1_000.);
        assert_eq!(
            e.venue.parks,
            [Ok(()), Ok(())],
            "parked again from ready custody"
        );
        assert!(
            terminals(&actions, 7).is_empty(),
            "no made-no-progress failure"
        );
        let actions = borrowed_read(&mut e, 2_000.);
        let (actions, _) = drive(&mut e, actions, 2_000., ["ok:a"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a]".to_owned())]);
    }

    #[test]
    fn condition_14_the_257th_parked_command_is_refused_before_any_send() {
        let mut e = engine(Cond {
            park: true,
            balance_command: true,
            ..Default::default()
        });
        for id in 1..=256 {
            place(&mut e, 0., id);
        }
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks.len(), 256);
        assert!(e.venue.parks.iter().all(Result::is_ok), "256 owners park");
        place(&mut e, 0., 257);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let result = terminals(&actions, 257);
        assert!(
            matches!(&result[..], [Err(error)] if error.outcome == Some(crate::Outcome::NotSent)),
            "{result:?}"
        );
        assert!(prepared(&actions).is_none());
        assert_eq!(e.venue.parks.len(), 256, "the parked 256 are unaffected");
    }

    #[test]
    fn condition_15_sequential_waits_reuse_one_context() {
        let mut e = engine(Cond {
            total: 3,
            serial_parks: 2,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (_, now) = drive(&mut e, actions, 0., ["ok:a"]);
        let actions = borrowed_read(&mut e, now + 10.);
        let (_, now) = drive(&mut e, actions, now + 10., ["ok:b"]);
        let actions = borrowed_read(&mut e, now + 10.);
        let (actions, _) = drive(&mut e, actions, now + 10., ["ok:c"]);
        assert_eq!(e.venue.serial_waits, [Ok(()), Ok(())]);
        assert_eq!(terminals(&actions, 7), [Ok("[a,b,c]".to_owned())]);
        let first = e.venue.storage[0];
        assert!(
            e.venue
                .storage
                .iter()
                .all(|s| s.0 + s.2 == first.0 + first.2)
        );
    }

    #[test]
    fn condition_16_a_release_from_the_parking_callback_does_not_reenter() {
        let mut e = engine(Cond {
            park: true,
            release_in_park: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        assert_eq!(e.venue.parks, [Ok(())]);
        assert!(!e.venue.ready_in_callback, "no re-entry");
        assert_eq!(e.venue.ready, [7], "ready after the callback, same flush");
    }

    #[test]
    fn condition_17_a_declining_expiry_hook_seals_once_not_sent() {
        let mut e = engine(Cond {
            park: true,
            bound: Some(1_000),
            on_expired: OnExpired::Decline,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.parks, [Ok(())]);
        let timer = timer_at(&actions, 1_000.).unwrap();
        e.handle(1_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let result = terminals(&actions, 7);
        assert!(
            matches!(&result[..], [Err(error)] if error.outcome == Some(crate::Outcome::NotSent)),
            "{result:?}"
        );
        assert!(!e.io.core.request_contexts.has_plans());
    }

    #[test]
    fn condition_17_a_declining_serial_expiry_seals_once_with_the_streamed_ids() {
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            serial_bound: Some(500),
            on_expired: OnExpired::Decline,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (actions, now) = drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        let timer = timer_at(&actions, now + 500.).expect("the Plan's one timer");
        e.handle(now + 500., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let result = terminals(&actions, 7);
        let [Err(error)] = &result[..] else {
            panic!("one engine seal: {result:?}")
        };
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["a"]);
        assert_ne!(
            error.outcome,
            Some(crate::Outcome::Unknown),
            "nothing was live"
        );
        #[cfg(feature = "order-locks")]
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"), "no fence");
        assert!(!e.io.core.request_contexts.has_plans());
    }

    /// The order-lock scope limit (64 active) binds Bitget's queued placements
    /// before the 256-owner limit; this is existing entry behaviour.
    #[cfg(feature = "order-locks")]
    #[test]
    fn condition_23_the_65th_queued_placement_is_refused_before_any_send() {
        let mut e = engine(Cond {
            park: true,
            ..Default::default()
        });
        for id in 1..=64 {
            place(&mut e, 0., id);
        }
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks.len(), 64);
        assert!(
            e.venue.parks.iter().all(Result::is_ok),
            "64 placements park"
        );
        place(&mut e, 0., 65);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let result = terminals(&actions, 65);
        assert!(
            // The order-lock capacity refusal (#2095): Internal, NotSent, nothing sent.
            matches!(&result[..], [Err(error)] if error.message == crate::order_locks::CAPACITY_REACHED
                && error.kind == Some(ErrorKind::Internal)
                && error.outcome == Some(crate::Outcome::NotSent)),
            "{result:?}"
        );
        assert!(prepared(&actions).is_none());
        assert_eq!(e.venue.parks.len(), 64, "the parked 64 are unaffected");
        for id in 1..=64 {
            assert!(e.io.core.order_locks.contains(id));
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn condition_18_a_parked_owner_holds_its_lock_until_its_terminal() {
        let mut e = engine(Cond {
            park: true,
            bounds: vec![Some(1_000), None],
            release: Release::None,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        place(&mut e, 10., 8);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            e.venue.parks,
            [Ok(()), Ok(())],
            "the later command parks behind"
        );
        assert!(e.io.core.order_locks.contains(7) && e.io.core.order_locks.contains(8));
        let timer = timer_at(&actions, 1_000.).unwrap();
        e.handle(1_000., Event::TimerFired { timer });
        assert!(
            !e.io.core.order_locks.contains(7),
            "released through its terminal"
        );
        assert!(e.io.core.order_locks.contains(8));
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"), "no fence");
    }

    #[test]
    fn condition_19_releases_from_a_timer_and_foreign_handles_are_revoked() {
        let mut e = engine(Cond {
            total: 2,
            serial_parks: 1,
            release_on_timer: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        let actions = e.drain_actions().collect::<Vec<_>>();
        drive(&mut e, actions, 0., ["ok:a"]);
        assert_eq!(e.venue.serial_waits, [Ok(())]);
        let wait = e.venue.waits[0].clone();
        let mut other = engine(Cond::default());
        let mut cx = Cx {
            now: 1.,
            store: &mut other.store,
            io: &mut other.io,
        };
        assert_eq!(
            cx.release_condition_wait(&wait),
            Err(Error::Revoked),
            "another engine"
        );
        let timer = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .set_timer(1.);
        e.handle(2., Event::TimerFired { timer });
        assert_eq!(e.venue.releases, [Ok(())], "a timer callback releases");
        assert_eq!(e.venue.plan_ready, 1);
    }

    #[test]
    fn condition_20_zero_and_read_only_bounds_are_invalid() {
        let mut e = engine(Cond {
            park: true,
            bound: Some(1_000),
            ..Default::default()
        });
        place(&mut e, 0., 7);
        assert_eq!(e.venue.parks, [Ok(())], "positive control");
        for venue in [
            Cond {
                park: true,
                bound: Some(0),
                ..Default::default()
            },
            Cond {
                park: true,
                bound: Some(1_000),
                read_preflight: true,
                balance_command: true,
                ..Default::default()
            },
        ] {
            let mut e = engine(venue);
            place(&mut e, 0., 7);
            assert_eq!(e.venue.parks, [Err(Error::Invalid)]);
        }
    }

    #[test]
    fn condition_21_a_stop_racing_a_queued_release_gives_one_terminal() {
        let mut e = engine(Cond {
            park: true,
            stop_after_release: true,
            ..Default::default()
        });
        place(&mut e, 0., 7);
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks, [Ok(())]);
        let actions = borrowed_read(&mut e, 1_000.);
        assert!(
            native_failure(&terminals(&actions, 7)),
            "the native message passes through"
        );
        assert!(e.venue.ready.is_empty(), "no ready after the stop");
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn condition_22_same_symbol_placements_queue_with_their_own_scopes() {
        let mut e = engine(Cond {
            park: true,
            bounds: vec![None, None],
            ..Default::default()
        });
        place(&mut e, 0., 7);
        place(&mut e, 10., 8);
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.parks, [Ok(()), Ok(())]);
        assert!(e.io.core.order_locks.contains(7) && e.io.core.order_locks.contains(8));
        e.venue.parked.retain(|id| *id == 7);
        let actions = borrowed_read(&mut e, 100.);
        e.venue.parked.push(8);
        let (actions, now) = drive(&mut e, actions, 100., ["ok:a"]);
        assert_eq!(terminals(&actions, 7), [Ok("[a]".to_owned())]);
        assert!(
            !e.io.core.order_locks.contains(7),
            "the first releases only its own scope"
        );
        assert_eq!(e.venue.ready, [7, 8], "the job's end wakes the next");
        let (actions, _) = drive(&mut e, actions, now, ["ok:b"]);
        assert_eq!(terminals(&actions, 8), [Ok("[a,b]".to_owned())]);
        assert!(!e.io.core.order_locks.contains(8));
        assert!(!e.io.core.order_locks.uncertain("BTCUSD"));
    }
}
