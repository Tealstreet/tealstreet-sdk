mod plan_expiry_cleanup {
    use super::*;
    use crate::model::OrderStatus;
    use crate::request_context::{
        HttpOwnerRetirement, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, StepBound},
    };

    #[derive(Default)]
    struct Cleanup {
        plan: Option<Plan>,
        slots: Vec<crate::request_context::plan::Slot>,
        count: usize,
        cleaned: Vec<usize>,
        retired_after: Vec<usize>,
        panic_cleanup: bool,
    }
    impl Cleanup {
        fn failure(
            &mut self,
            cx: &mut Cx<'_, (usize, Context)>,
            pending: (usize, Context),
            error: VenueError,
        ) {
            let (index, context) = pending;
            assert_eq!(error.kind, Some(ErrorKind::Timeout));
            let plan = self.plan.as_ref().unwrap();
            if cx.now >= plan.deadline() {
                assert!(
                    cx.plan_slots(plan).is_err(),
                    "expired root cannot select another slot"
                );
            }
            assert!(
                cx.request_signed_managed(
                    context.clone(), None,
                    Method::Post,
                    "https://venue.test/forbidden".into(),
                    None,
                    Box::new(Recipe),
                    (index, context)
                )
                .is_err(),
                "expired cleanup cannot send again"
            );
            let previous = cx.io.http_delivery.clone().unwrap();
            let mut foreign_delivery = previous.clone();
            foreign_delivery.attempt.request =
                RequestId(foreign_delivery.attempt.request.0 + 1_000_000);
            cx.io.http_delivery = Some(foreign_delivery);
            assert_eq!(cx.fail_plan(plan, "wrong attempt"), Err(Error::Revoked));
            cx.io.http_delivery = Some(previous);
            let Owner::Command { id, generation } = plan.owner() else {
                unreachable!()
            };
            let forged = Plan::new(
                &crate::request_context::Preflight::new(&std::sync::Arc::new(()), id, generation),
                plan.deadline(),
            );
            assert_eq!(cx.fail_plan(&forged, "wrong issuer"), Err(Error::Revoked));
            self.cleaned.push(index);
            let id = format!("native-{index}");
            if index == 0 {
                cx.store.remove_orders(&[&id]);
            } else {
                cx.store.set_order_status(&id, OrderStatus::Open);
            }
            assert!(!self.panic_cleanup, "cleanup panic control");
            cx.fail_plan(plan, error).unwrap();
        }
    }
    impl Venue for Cleanup {
        type Pending = (usize, Context);
        fn start(&mut self, _: &mut Cx<'_, Self::Pending>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, _: Command) {
            let credential = crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            };
            let owner = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential.clone()),
                })
                .unwrap();
            let calls = (0..self.count)
                .map(|_| CallBound {
                    timeout_ms: 20_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                })
                .collect::<Vec<_>>();
            let plan = cx
                .admit_plan(
                    &owner,
                    &[StepBound {
                        wait_ms: 0,
                        calls: &calls,
                    }],
                    PlanPolicy {
                        independent_calls: false,
                        operation: Operation::Mutation,
                        credential: Some(credential),
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            for (index, slot) in cx.plan_slots(&plan).unwrap().into_iter().enumerate() {
                let context = cx.plan_leg_context(&plan, &slot).unwrap();
                self.slots.push(slot);
                let mut order = fixture_order(&format!("native-{index}"));
                order.status = match index {
                    0 => OrderStatus::Creating,
                    1 => OrderStatus::Canceling,
                    _ => OrderStatus::Updating,
                };
                cx.store.upsert_order(order);
                cx.request_signed_managed(
                    context.clone(), None,
                    Method::Post,
                    "https://venue.test/write".into(),
                    None,
                    Box::new(Recipe),
                    (index, context),
                )
                .unwrap();
            }
            self.plan = Some(plan);
        }
        fn on_venue_action(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            command: CommandId,
            action: crate::VenueAction,
        ) {
            self.on_command(cx, Command::Venue { command, action });
        }
        fn on_http_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: VenueError,
        ) {
            self.failure(cx, pending, error);
        }
        fn on_sign_error(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: VenueError,
        ) {
            assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
            self.failure(cx, pending, error);
        }
        fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {
            self.retired_after.push(self.cleaned.len());
        }
        fn capture_http_reconciliation(
            &self,
            _: &Self::Pending,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("native-correlation".into())
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            _: Response<'_>,
        ) {
            let (index, context) = pending;
            let plan = self.plan.as_ref().unwrap();
            cx.validate_plan_delivery(plan, &context).unwrap();
            cx.http_write_result(
                &context,
                crate::request_context::WriteResult::Accepted {
                    order_ids: vec!["accepted-prefix".into()],
                },
            )
            .unwrap();
            cx.finish_plan_call(plan, &self.slots[index], &context)
                .unwrap();
            cx.store.remove_orders(&[&format!("native-{index}")]);
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Self::Pending>, _: Self::Pending, _: &str) {
            panic!("untyped failure");
        }
        fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Self::Pending>, _: TimerId) {
            panic!("engine timer reached native venue");
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    #[derive(Clone, Copy, Debug)]
    enum Phase {
        Preparation,
        Signing,
        Bound,
        Released,
    }
    fn setup(count: usize, phase: Phase) -> (Engine<Cleanup>, Vec<Preparation>) {
        let mut e = crate::test_support::seeded_engine(Cleanup {
            count,
            ..Default::default()
        });
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::Venue {
                command: 7,
                action: crate::VenueAction::FetchBalanceInfo,
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let tokens = actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), count);
        for token in &tokens {
            if matches!(phase, Phase::Preparation) {
                continue;
            }
            e.http_prepared(
                0.,
                *token,
                Prepared {
                    id: token.attempt().request.0,
                    expires_at: None,
                },
            );
            let actions = e.drain_actions().collect::<Vec<_>>();
            let (attempt, work) = work(&actions);
            if matches!(phase, Phase::Signing) {
                continue;
            }
            e.handle(
                0.,
                Event::Signed {
                    attempt,
                    result: Ok(crate::signing::SignResult {
                        values: vec![b"value".to_vec()],
                        receipt: work.receipt,
                        headers: vec![],
                    }),
                },
            );
            let actions = e.drain_actions().collect::<Vec<_>>();
            let permit = bound(&actions);
            if matches!(phase, Phase::Released) {
                e.claim_http(0., permit).unwrap();
            }
        }
        e.drain_actions().for_each(drop);
        (e, tokens)
    }
    fn assert_cleaned(e: &mut Engine<Cleanup>, released: bool) {
        assert_eq!(e.venue.cleaned, (0..e.venue.count).collect::<Vec<_>>());
        assert_eq!(e.venue.retired_after, vec![e.venue.count]);
        assert!(
            e.store.order("native-0").is_none(),
            "Creating row survived Plan expiry"
        );
        for index in 1..e.venue.count {
            assert_eq!(
                e.store.order(&format!("native-{index}")).unwrap().status,
                OrderStatus::Open
            );
        }
        let actions = e.drain_actions().collect::<Vec<_>>();
        let errors = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
        assert_eq!(
            errors[0].outcome,
            Some(if released {
                crate::Outcome::Unknown
            } else {
                crate::Outcome::NotSent
            })
        );
        assert!(!actions.iter().any(
            |a| matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{..}))
        ));
        assert!(e.io.managed_http.is_empty());
    }
    #[test]
    fn plan_timer_cleans_every_outstanding_phase_before_retirement() {
        for phase in [
            Phase::Preparation,
            Phase::Signing,
            Phase::Bound,
            Phase::Released,
        ] {
            let (mut e, tokens) = setup(3, phase);
            let timer = e.io.request_contexts.plan_timer(7).unwrap();
            e.handle(20_000., Event::TimerFired { timer });
            assert_cleaned(&mut e, matches!(phase, Phase::Released));
            for token in tokens {
                e.http_prepared(
                    20_001.,
                    token,
                    Prepared {
                        id: 900,
                        expires_at: None,
                    },
                );
                e.handle(
                    20_001.,
                    Event::HttpFailed {
                        request: token.attempt().request,
                        error: "late",
                    },
                );
            }
            assert_eq!(e.venue.cleaned.len(), 3);
            assert_eq!(e.venue.retired_after, vec![3]);
            assert!(
                !e.drain_actions()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
            );
        }
    }
    #[test]
    fn http_timer_first_at_root_cleans_parallel_peers_before_retirement() {
        let (mut e, tokens) = setup(3, Phase::Released);
        let timer = e.io.managed_http[&tokens[0].attempt().request].timer;
        e.handle(20_000., Event::TimerFired { timer });
        assert_cleaned(&mut e, true);
    }
    #[test]
    fn host_failure_first_at_root_cleans_parallel_unsent_peers() {
        let (mut e, tokens) = setup(3, Phase::Preparation);
        e.http_preparation_failed(
            20_000.,
            tokens[0],
            Error::TimedOut,
            VenueError::new("host deadline"),
        );
        assert_cleaned(&mut e, false);
    }
    #[test]
    fn ordinary_pre_root_failure_keeps_the_existing_typed_cleanup_path() {
        let (mut e, tokens) = setup(1, Phase::Preparation);
        e.http_preparation_failed(
            10.,
            tokens[0],
            Error::TimedOut,
            VenueError::new("host deadline"),
        );
        assert_cleaned(&mut e, false);
    }
    #[test]
    fn mixed_unsent_and_released_peers_keep_unknown_until_all_cleanup_finishes() {
        let (mut e, tokens) = setup(3, Phase::Preparation);
        let token = tokens[2];
        e.http_prepared(
            0.,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let (attempt, work) = work(&e.drain_actions().collect::<Vec<_>>());
        e.handle(
            0.,
            Event::Signed {
                attempt,
                result: Ok(crate::signing::SignResult {
                    values: vec![b"value".to_vec()],
                    receipt: work.receipt,
                    headers: vec![],
                }),
            },
        );
        let permit = bound(&e.drain_actions().collect::<Vec<_>>());
        e.claim_http(0., permit).unwrap();
        e.drain_actions().for_each(drop);
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(20_000., Event::TimerFired { timer });
        assert_cleaned(&mut e, true);
    }
    #[test]
    fn root_cleanup_preserves_accepted_prefix_before_unresolved_parallel_writes() {
        let (mut e, tokens) = setup(3, Phase::Released);
        e.handle(
            1.,
            Event::HttpResponse {
                request: tokens[0].attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "accepted",
                },
            },
        );
        e.drain_actions().for_each(drop);
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(20_000., Event::TimerFired { timer });
        assert_eq!(e.venue.cleaned, vec![1, 2]);
        assert_eq!(e.venue.retired_after, vec![2]);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let errors = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].outcome, Some(crate::Outcome::Unknown));
        assert_eq!(
            errors[0].partial.as_ref().unwrap().order_ids,
            vec!["accepted-prefix"]
        );
    }
    #[test]
    fn cleanup_panic_poisons_the_engine_instead_of_leaving_a_deferred_seal() {
        let (mut e, _) = setup(1, Phase::Preparation);
        e.venue.panic_cleanup = true;
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || e.handle(20_000., Event::TimerFired { timer })
            ))
            .is_err()
        );
        assert!(e.failed());
    }
}
