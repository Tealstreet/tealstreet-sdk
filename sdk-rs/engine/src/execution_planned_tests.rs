mod planned_execution {
    use super::*;
    use crate::engine::{ExecutionHandle, Window};
    use crate::request_context::{
        Operation, Plan, PlanPolicy, Preflight, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    const FOUNDER: CommandId = 7;
    const FOLLOWER: CommandId = 8;
    const SHORT_FOLLOWER: CommandId = 9;
    const LATE: CommandId = 10;

    fn credential() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        }
    }

    fn plan_policy() -> PlanPolicy {
        PlanPolicy {
            operation: Operation::Mutation,
            credential: Some(credential()),
            pacing: None,
            retry: RetryPolicy::DefinitiveRefusal,
        }
    }

    fn follower_policy(timeout_ms: u32) -> Policy {
        Policy {
            operation: Operation::Mutation,
            credential: Some(credential()),
            timeout_ms,
            pacing: None,
            retry: RetryPolicy::DefinitiveRefusal,
        }
    }

    /// The composition under test: one atomic admission of the founder's
    /// retained Preflight as a Plan-backed execution that followers can join.
    fn share(
        cx: &mut Cx<'_, Context>,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        read: bool,
    ) -> Result<(Plan, ExecutionHandle, Option<Context>), Error> {
        let policy = if read {
            read_plan_policy()
        } else {
            plan_policy()
        };
        cx.begin_planned_execution(preflight, steps, policy, Window::UntilTerminal, "same")
            .map(|(plan, execution, founder)| (plan, execution, Some(founder)))
    }

    fn read_plan_policy() -> PlanPolicy {
        PlanPolicy {
            operation: Operation::Read,
            credential: None,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }

    #[derive(Default)]
    struct Shared {
        plan: Option<Plan>,
        slot: Option<Slot>,
        execution: Option<ExecutionHandle>,
        joined: Vec<Result<crate::engine::Joined, Error>>,
        refusals: Vec<Error>,
        invalid_steps: bool,
        probe_handle: bool,
        /// A Read Plan with Read followers instead of a write.
        read: bool,
        founder: Option<Context>,
        withheld: Vec<Error>,
        delivered: usize,
        own_root_send: bool,
        /// Two serial writes; the second is queued from the first's DATA.
        two_steps: bool,
        /// The first write is definitively refused; the second is due later.
        reject_first: bool,
        /// The only write is definitively refused and the Plan completes.
        reject_only: bool,
    }

    impl Shared {
        fn queue_next(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.clone().unwrap();
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            let leg = cx.plan_leg_context(&plan, &slot).unwrap();
            cx.request_signed_managed(
                leg.clone(),
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Box::new(Recipe),
                leg,
            )
            .unwrap();
            self.slot = Some(slot);
        }
    }

    impl Shared {
        fn found(&mut self, cx: &mut Cx<'_, Context>) {
            let preflight = cx
                .begin_preflight(if self.read {
                    PreflightPolicy {
                        purpose: PreflightPurpose::ReadOnly,
                        read_credential: None,
                        write_credential: None,
                    }
                } else {
                    PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: Some(credential()),
                    }
                })
                .unwrap();
            let calls = [CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            let step = StepBound {
                wait_ms: 0,
                calls: &calls,
            };
            let two = [
                StepBound {
                    wait_ms: 0,
                    calls: &calls,
                },
                StepBound {
                    // A refused first write waits before the second is due.
                    wait_ms: if self.reject_first { 500 } else { 0 },
                    calls: &calls,
                },
            ];
            let steps: &[StepBound<'_>] = if self.two_steps {
                &two
            } else {
                std::slice::from_ref(&step)
            };
            if self.invalid_steps {
                let ids = cx.io.next_id;
                let actions = cx.io.actions.len();
                let contexts = cx.io.request_contexts.plan_context_storage();
                self.refusals
                    .push(share(cx, &preflight, &[], self.read).unwrap_err());
                assert_eq!(cx.io.next_id, ids, "refusal spends no identity");
                assert_eq!(cx.io.actions.len(), actions);
                assert_eq!(cx.io.request_contexts.plan_context_storage(), contexts);
                assert_eq!(cx.io.request_contexts.execution_count(), 0);
                cx.fail_preflight(&preflight, "refused admission").unwrap();
                return;
            }
            let (plan, execution, founder) =
                share(cx, &preflight, steps, self.read).expect("shared plan execution");
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            let leg = cx.plan_leg_context(&plan, &slot).unwrap();
            if self.read {
                cx.request_managed(
                    leg.clone(),
                    Method::Get,
                    "https://venue.test/read".into(),
                    None,
                    leg,
                )
                .unwrap();
            } else {
                cx.request_signed_managed(
                    leg.clone(),
                    Method::Post,
                    "https://venue.test/write".into(),
                    None,
                    Box::new(Recipe),
                    leg,
                )
                .unwrap();
            }
            self.founder = founder;
            if self.probe_handle {
                self.refusals
                    .push(cx.abort_execution(&execution, "direct").unwrap_err());
                self.refusals.push(
                    cx.complete_execution(&execution, |_| Ok("direct".into()))
                        .unwrap_err(),
                );
                self.refusals.push(
                    cx.http_leg_context(execution.context(), follower_policy(1_000))
                        .unwrap_err(),
                );
            }
            self.plan = Some(plan);
            self.slot = Some(slot);
            self.execution = Some(execution);
        }

        fn follow(&mut self, cx: &mut Cx<'_, Context>, timeout_ms: u32) {
            let policy = if self.read {
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }
            } else {
                follower_policy(timeout_ms)
            };
            let root = cx.admit_http_command(policy).unwrap();
            let joined = cx.join_execution(self.execution.as_ref().unwrap(), &root, "same");
            if joined.is_ok() && self.own_root_send {
                // A member's only physical work is the Plan's.
                let sent = cx.request_signed_managed(
                    root.clone(),
                    Method::Post,
                    "https://venue.test/own-root".into(),
                    None,
                    Box::new(Recipe),
                    root.clone(),
                );
                self.refusals.push(
                    sent.map(|_| ())
                        .map_err(|(_, e)| e)
                        .err()
                        .unwrap_or(Error::Duplicate),
                );
            }
            self.joined.push(joined);
        }
    }

    impl Venue for Shared {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {}
        fn on_venue_action(
            &mut self,
            cx: &mut Cx<'_, Context>,
            command: CommandId,
            _: VenueAction,
        ) {
            match command {
                FOUNDER => self.found(cx),
                FOLLOWER | LATE.. => self.follow(cx, 60_000),
                SHORT_FOLLOWER => self.follow(cx, 100),
                _ => unreachable!(),
            }
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
            self.delivered += 1;
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            if let Err(error) = cx.validate_plan_delivery(&plan, &context) {
                // Nothing may be published once no observer survives.
                self.withheld.push(error);
                return;
            }
            if self.read {
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "candles".into()).unwrap();
                return;
            }
            if self.reject_only {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Rejected(
                        VenueError::new("insufficient margin").outcome(crate::Outcome::Rejected),
                    ),
                )
                .unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "placed".into()).unwrap();
                return;
            }
            if self.reject_first && self.delivered == 1 {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Rejected(
                        VenueError::new("insufficient margin").outcome(crate::Outcome::Rejected),
                    ),
                )
                .unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                return;
            }
            cx.http_write_result(
                &context,
                crate::request_context::WriteResult::Accepted {
                    order_ids: vec![format!("native-{}", self.delivered)],
                },
            )
            .unwrap();
            cx.validate_http_write_effects(&context).unwrap();
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            if self.two_steps && self.delivered == 1 {
                self.queue_next(cx);
                return;
            }
            cx.done_plan(&plan, "placed".into()).unwrap();
        }
        fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Context>, _: &Plan) {
            if self.reject_first && self.delivered == 1 {
                self.queue_next(cx);
            }
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("planned-execution".into())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
            panic!("engine timer leaked to venue");
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn command(engine: &mut Engine<Shared>, now: f64, command: CommandId) {
        engine.handle(
            now,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::FetchBalanceInfo,
            }),
        );
    }

    fn prepared(actions: &[Action]) -> Vec<Preparation> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn release(engine: &mut Engine<Shared>, token: Preparation, now: f64) {
        engine.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
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
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("planned execution binding");
        assert!(matches!(
            engine.claim_http(now, permit),
            Ok(Claim::Released(_))
        ));
    }

    fn terminals(actions: &[Action], command: CommandId) -> Vec<Result<String, VenueError>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                    Some(Err(error.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Founder plus `followers`, with the single write prepared and released.
    fn shared_write(followers: &[CommandId]) -> (Engine<Shared>, Preparation, Vec<Action>) {
        let mut e = crate::test_support::seeded_engine(Shared::default());
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        for &follower in followers {
            command(&mut e, 0., follower);
        }
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let tokens = prepared(&actions);
        assert_eq!(tokens.len(), 1, "followers never prepare a second write");
        release(&mut e, tokens[0], 0.);
        actions.extend(e.drain_actions());
        (e, tokens[0], actions)
    }

    #[test]
    fn followers_receive_the_founders_one_write_and_single_projection() {
        let (mut e, token, _) = shared_write(&[FOLLOWER]);
        assert_eq!(
            e.venue.joined,
            vec![Ok(crate::engine::Joined::Live(Vec::new()))]
        );
        e.handle(
            1_000.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        for owner in [FOUNDER, FOLLOWER] {
            assert_eq!(
                terminals(&actions, owner),
                vec![Ok("placed".into())],
                "owner {owner}"
            );
        }
        assert!(prepared(&actions).is_empty());
        assert!(!e.io.request_contexts.is_observer(FOLLOWER));
    }

    #[test]
    fn an_unresolved_write_projects_unknown_to_every_observer() {
        let (mut e, token, _) = shared_write(&[FOLLOWER]);
        e.handle(
            1_000.,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network",
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let founder = terminals(&actions, FOUNDER);
        assert_eq!(founder.len(), 1);
        let error = founder[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
        assert_eq!(
            terminals(&actions, FOLLOWER),
            founder,
            "one shared projection"
        );
    }

    #[test]
    fn an_observer_expiring_early_leaves_the_shared_write_running() {
        let (mut e, token, actions) = shared_write(&[SHORT_FOLLOWER, FOLLOWER]);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 100. => Some(*timer),
                _ => None,
            })
            .expect("short observer deadline");
        e.handle(100., Event::TimerFired { timer });
        let expired = e.drain_actions().collect::<Vec<_>>();
        let short = terminals(&expired, SHORT_FOLLOWER);
        assert_eq!(short.len(), 1);
        assert_eq!(short[0].clone().unwrap_err().kind, Some(ErrorKind::Timeout));
        assert!(terminals(&expired, FOUNDER).is_empty());
        assert!(terminals(&expired, FOLLOWER).is_empty());
        e.handle(
            1_000.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOUNDER), vec![Ok("placed".into())]);
        assert_eq!(terminals(&actions, FOLLOWER), vec![Ok("placed".into())]);
        assert!(terminals(&actions, SHORT_FOLLOWER).is_empty());
    }

    #[test]
    fn the_plan_root_bounds_every_observer_and_is_never_renewed() {
        let (mut e, token, _) = shared_write(&[FOLLOWER]);
        let deadline = e.venue.plan.as_ref().unwrap().deadline();
        let root = e.venue.execution.as_ref().unwrap().context().deadline();
        assert_eq!(
            root, deadline,
            "the execution bound is the Plan-minted root"
        );
        command(&mut e, 500., LATE);
        assert_eq!(e.venue.plan.as_ref().unwrap().deadline(), deadline);
        assert_eq!(
            e.venue.execution.as_ref().unwrap().context().deadline(),
            root
        );
        e.handle(
            1_000.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, LATE), vec![Ok("placed".into())]);
        command(&mut e, 2_000., LATE + 1);
        assert_eq!(
            e.venue.joined.last(),
            Some(&Err(Error::Revoked)),
            "no join after the seal"
        );
    }

    #[test]
    fn a_planned_execution_has_no_direct_completion_or_extra_legs() {
        let mut e = crate::test_support::seeded_engine(Shared {
            probe_handle: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        assert_eq!(e.venue.refusals.len(), 3);
        assert!(
            e.venue
                .refusals
                .iter()
                .all(|error| *error != Error::Capacity)
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(prepared(&actions).len(), 1);
        assert!(terminals(&actions, FOUNDER).is_empty());
    }

    #[test]
    fn a_refused_admission_creates_no_plan_execution_or_identity() {
        let mut e = crate::test_support::seeded_engine(Shared {
            invalid_steps: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        assert_eq!(e.venue.refusals, vec![Error::Invalid]);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOUNDER).len(), 1);
    }

    fn detach_founder(engine: &mut Engine<Shared>, now: f64) {
        let founder = engine.venue.founder.clone().expect("founder observer root");
        Cx {
            now,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .detach_execution_observer(&founder, "founder left")
        .unwrap();
    }

    fn respond(engine: &mut Engine<Shared>, token: Preparation, now: f64) {
        engine.handle(
            now,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
    }

    #[test]
    fn the_plan_is_owned_by_its_execution_not_the_founder() {
        let (e, _, _) = shared_write(&[]);
        let plan = e.venue.plan.as_ref().unwrap();
        assert!(matches!(
            plan.owner(),
            crate::request_context::Owner::Execution { .. }
        ));
        assert_eq!(
            plan.owner(),
            e.venue.execution.as_ref().unwrap().context().owner()
        );
        assert!(
            e.venue.founder.is_some(),
            "the founder is an explicit observer"
        );
    }

    #[test]
    fn a_detached_founder_leaves_the_shared_write_to_live_peers() {
        let (mut e, token, _) = shared_write(&[FOLLOWER]);
        detach_founder(&mut e, 500.);
        let detached = e.drain_actions().collect::<Vec<_>>();
        let founder = terminals(&detached, FOUNDER);
        assert_eq!(founder.len(), 1);
        assert_eq!(founder[0].clone().unwrap_err().message, "founder left");
        respond(&mut e, token, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOLLOWER), vec![Ok("placed".into())]);
        assert!(terminals(&actions, FOUNDER).is_empty());
        assert!(e.venue.withheld.is_empty(), "a live peer may publish");
    }

    #[test]
    fn the_last_observer_leaving_withholds_publication_of_a_sent_write() {
        let (mut e, token, _) = shared_write(&[]);
        detach_founder(&mut e, 500.);
        e.drain_actions().for_each(drop);
        respond(&mut e, token, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        // The orphaned Plan failed at the event boundary, so the late reply
        // reaches no venue codec or Store at all.
        assert_eq!(e.venue.delivered, 0, "no codec or Store work for an orphan");
        assert!(e.venue.withheld.is_empty());
        assert!(terminals(&actions, FOUNDER).is_empty());
    }

    #[test]
    fn the_last_observer_leaving_before_release_sends_nothing() {
        let mut e = crate::test_support::seeded_engine(Shared::default());
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions)[0];
        detach_founder(&mut e, 1.);
        let detached = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&detached, FOUNDER).len(), 1);
        e.http_prepared(
            2.,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        assert!(
            !e.drain_actions().any(|a| matches!(
                a,
                Action::ManagedHttp(a) if matches!(*a, HttpAction::Bind { .. })
            )),
            "an orphaned Plan never binds its unsent write"
        );
    }

    #[test]
    fn read_followers_receive_the_plan_result() {
        let mut e = crate::test_support::seeded_engine(Shared {
            read: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., FOLLOWER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let tokens = prepared(&actions);
        assert_eq!(tokens.len(), 1);
        e.http_prepared(
            0.,
            tokens[0],
            Prepared {
                id: tokens[0].attempt().request.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("read binding");
        assert!(matches!(e.claim_http(0., permit), Ok(Claim::Released(_))));
        respond(&mut e, tokens[0], 100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        for owner in [FOUNDER, FOLLOWER] {
            assert_eq!(
                terminals(&actions, owner),
                vec![Ok("candles".into())],
                "owner {owner}"
            );
        }
    }

    #[test]
    fn a_follower_cannot_send_on_its_own_root() {
        let mut e = crate::test_support::seeded_engine(Shared {
            own_root_send: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., FOLLOWER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.refusals, vec![Error::Revoked]);
        assert_eq!(
            prepared(&actions).len(),
            1,
            "only the Plan's write is prepared"
        );
    }

    #[test]
    fn the_last_observer_expiring_after_release_is_never_told_not_sent() {
        let (mut e, _, actions) = shared_write(&[SHORT_FOLLOWER]);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 100. => Some(*timer),
                _ => None,
            })
            .expect("short observer deadline");
        detach_founder(&mut e, 50.);
        e.drain_actions().for_each(drop);
        e.handle(100., Event::TimerFired { timer });
        let expired = e.drain_actions().collect::<Vec<_>>();
        let short = terminals(&expired, SHORT_FOLLOWER);
        assert_eq!(short.len(), 1);
        let error = short[0].clone().unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_eq!(
            error.outcome,
            Some(crate::Outcome::Unknown),
            "the write was released"
        );
    }

    fn short_timer(actions: &[Action]) -> TimerId {
        actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 100. => Some(*timer),
                _ => None,
            })
            .expect("short observer deadline")
    }

    #[test]
    fn an_observer_expiring_during_a_released_write_is_told_unknown() {
        let (mut e, token, actions) = shared_write(&[SHORT_FOLLOWER, FOLLOWER]);
        e.handle(
            100.,
            Event::TimerFired {
                timer: short_timer(&actions),
            },
        );
        let expired = e.drain_actions().collect::<Vec<_>>();
        let short = terminals(&expired, SHORT_FOLLOWER);
        assert_eq!(short.len(), 1);
        let error = short[0].clone().unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
        respond(&mut e, token, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOLLOWER), vec![Ok("placed".into())]);
        assert!(terminals(&actions, SHORT_FOLLOWER).is_empty());
    }

    #[test]
    fn an_early_expiry_carries_the_accepted_prefix_and_unknown_remainder() {
        let mut e = crate::test_support::seeded_engine(Shared {
            two_steps: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., SHORT_FOLLOWER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = short_timer(&actions);
        let first = prepared(&actions)[0];
        release(&mut e, first, 0.);
        respond(&mut e, first, 10.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let second = prepared(&actions)[0];
        release(&mut e, second, 20.);
        e.handle(100., Event::TimerFired { timer });
        let expired = e.drain_actions().collect::<Vec<_>>();
        let short = terminals(&expired, SHORT_FOLLOWER);
        assert_eq!(short.len(), 1);
        let error = short[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
        assert_eq!(
            error.partial.map(|partial| partial.order_ids),
            Some(vec!["native-1".to_owned()])
        );
        respond(&mut e, second, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOUNDER), vec![Ok("placed".into())]);
        assert!(terminals(&actions, SHORT_FOLLOWER).is_empty());
    }

    #[test]
    fn detaching_before_release_with_a_live_peer_never_claims_not_sent() {
        let mut e = crate::test_support::seeded_engine(Shared::default());
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., FOLLOWER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepared(&actions)[0];
        detach_founder(&mut e, 1.);
        let detached = e.drain_actions().collect::<Vec<_>>();
        let founder = terminals(&detached, FOUNDER);
        assert_eq!(founder.len(), 1);
        let error = founder[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
        release(&mut e, token, 2.);
        respond(&mut e, token, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminals(&actions, FOLLOWER), vec![Ok("placed".into())]);
    }

    #[test]
    fn an_early_observer_after_a_refused_first_write_is_told_unknown_while_more_may_send() {
        let mut e = crate::test_support::seeded_engine(Shared {
            two_steps: true,
            reject_first: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., SHORT_FOLLOWER);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = short_timer(&actions);
        let first = prepared(&actions)[0];
        release(&mut e, first, 0.);
        respond(&mut e, first, 10.);
        let refused = e.drain_actions().collect::<Vec<_>>();
        assert!(
            prepared(&refused).is_empty(),
            "the second write is not yet due"
        );
        e.handle(100., Event::TimerFired { timer });
        let expired = e.drain_actions().collect::<Vec<_>>();
        let short = terminals(&expired, SHORT_FOLLOWER);
        assert_eq!(short.len(), 1);
        let error = short[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
        let due = expired
            .iter()
            .chain(refused.iter())
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms >= 500. && *at_ms < 1_000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("second write's readiness");
        e.handle(due.1, Event::TimerFired { timer: due.0 });
        let actions = e.drain_actions().collect::<Vec<_>>();
        let second = prepared(&actions)[0];
        release(&mut e, second, due.1);
        respond(&mut e, second, 1_000.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            terminals(&actions, FOUNDER).len(),
            1,
            "the peer completes once"
        );
        assert!(terminals(&actions, SHORT_FOLLOWER).is_empty());
    }

    #[test]
    fn an_observer_expired_when_preparation_failure_seals_keeps_the_accepted_prefix() {
        let mut e = crate::test_support::seeded_engine(Shared {
            two_steps: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., SHORT_FOLLOWER);
        let first = prepared(&e.drain_actions().collect::<Vec<_>>())[0];
        release(&mut e, first, 0.);
        respond(&mut e, first, 10.);
        let second = prepared(&e.drain_actions().collect::<Vec<_>>())[0];
        // The host callback seals the Plan before any execution sweep runs.
        e.http_preparation_failed(
            101.,
            second,
            Error::Invalid,
            VenueError::new("route denied"),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let founder = terminals(&actions, FOUNDER);
        let short = terminals(&actions, SHORT_FOLLOWER);
        assert_eq!((founder.len(), short.len()), (1, 1));
        let ids = |error: &VenueError| error.partial.as_ref().map(|p| p.order_ids.clone());
        let native = Some(vec!["native-1".to_owned()]);
        assert_eq!(ids(&founder[0].clone().unwrap_err()), native);
        let expired = short[0].clone().unwrap_err();
        assert_eq!(expired.kind, Some(ErrorKind::Timeout));
        assert_eq!(ids(&expired), native, "{expired:?}");
    }

    #[test]
    fn every_observer_gets_the_rejection_reports_once_with_the_terminal() {
        let mut e = crate::test_support::seeded_engine(Shared {
            reject_only: true,
            ..Default::default()
        });
        e.start(0.);
        command(&mut e, 0., FOUNDER);
        command(&mut e, 0., FOLLOWER);
        let token = prepared(&e.drain_actions().collect::<Vec<_>>())[0];
        release(&mut e, token, 0.);
        respond(&mut e, token, 10.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        for owner in [FOUNDER, FOLLOWER] {
            let reports = actions
                .iter()
                .filter(|a| {
                    matches!(a, Action::Notice(Notice::CommandError { command, message })
                    if *command == owner && message.contains("insufficient margin"))
                })
                .count();
            assert_eq!(reports, 1, "owner {owner}");
            let terminal = terminals(&actions, owner);
            assert_eq!(terminal.len(), 1);
            let error = terminal[0].clone().unwrap_err();
            assert_eq!(
                error.outcome,
                Some(crate::Outcome::Rejected),
                "owner {owner}"
            );
        }
    }
}
