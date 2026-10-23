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
    ) -> Result<(Plan, ExecutionHandle), Error> {
        cx.begin_planned_execution(preflight, steps, plan_policy(), Window::UntilTerminal)
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
    }

    impl Shared {
        fn found(&mut self, cx: &mut Cx<'_, Context>) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential()),
                })
                .unwrap();
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
            if self.invalid_steps {
                let ids = cx.io.next_id;
                let actions = cx.io.actions.len();
                let contexts = cx.io.request_contexts.plan_context_storage();
                self.refusals.push(share(cx, &preflight, &[]).unwrap_err());
                assert_eq!(cx.io.next_id, ids, "refusal spends no identity");
                assert_eq!(cx.io.actions.len(), actions);
                assert_eq!(cx.io.request_contexts.plan_context_storage(), contexts);
                assert_eq!(cx.io.request_contexts.execution_count(), 0);
                cx.fail_preflight(&preflight, "refused admission").unwrap();
                return;
            }
            let (plan, execution) = share(cx, &preflight, &steps).expect("shared plan execution");
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
            let root = cx.admit_http_command(follower_policy(timeout_ms)).unwrap();
            let joined = cx.join_execution(self.execution.as_ref().unwrap(), &root, "same");
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
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            cx.http_write_result(
                &context,
                crate::request_context::WriteResult::Accepted {
                    order_ids: vec!["native-1".into()],
                },
            )
            .unwrap();
            cx.validate_http_write_effects(&context).unwrap();
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            cx.done_plan(&plan, "placed".into()).unwrap();
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
}
